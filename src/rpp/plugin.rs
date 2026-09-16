use std::{path::PathBuf, sync::LazyLock};

use base64_simd::STANDARD as base64;
use rpp_parser::parser::{Child, Element};
use smallvec::{SmallVec, smallvec};

/// Parse a `FXCHAIN` element
fn find_fxchain_plugins<'a>(e: &'a Element<'a>, out: &mut Vec<&'a Element<'a>>) {
    for child in &e.children {
        let Child::Element(child) = child else {
            continue;
        };
        match child.tag {
            // both vst2 and vst3
            "VST" | "CLAP" => out.push(child),
            "CONTAINER" => find_fxchain_plugins(child, out),
            "JS" | "PARMENV" | "PROGRAMENV" | "IN_PINS" | "OUT_PINS" | "JS_SER" | "JS_PINMAP"
            | "COMMENT" | "VIDEO_EFFECT" => (),
            _ => {
                eprintln!("unhandled plugin type! {:?}", child.tag);
                eprintln!("{:?}", e);
                todo!("unhandled plugin type! {:?}", child.tag)
            }
        }
    }
}

fn extract_plugin_strings<'a>(plugin: &'a Element<'a>) -> Option<Vec<String>> {
    fn b64_extract_strings(children: &[Child]) -> Vec<String> {
        let lines = children.iter().filter_map(|x| {
            if let Child::Line(items) = x {
                if items.len() != 1 {
                    eprintln!("warning: plugin data contains space");
                }
                items.first()
            } else {
                eprintln!("warning: non-data found in plugin");
                None
            }
        });

        let mut data: Vec<u8> = vec![];
        for line in lines {
            let res = base64.decode_append(line.as_bytes(), &mut data);
            if res.is_err() {
                eprintln!("warning: failed to decode plugin data");
                return vec![];
            }
        }

        data.utf8_chunks()
            .map(|chunk| chunk.valid())
            .flat_map(|s| s.split(|c: char| c.is_control() && c != '\n' && c != '\r'))
            .filter(|s| s.chars().count() >= 5)
            .map(|x| x.to_string())
            .collect()
    }

    let result = match plugin.tag {
        // both vst2 and vst3
        "VST" => b64_extract_strings(&plugin.children),
        "CLAP" => {
            // find the <STATE> element
            let state = plugin
                .children
                .iter()
                .filter_map(|x| {
                    if let Child::Element(y) = x {
                        Some(y)
                    } else {
                        None
                    }
                })
                .find(|x| x.tag == "STATE");

            let Some(state) = state else {
                eprintln!("warning: failed to find CLAP plugin data");
                return None;
            };

            b64_extract_strings(&state.children)
        }
        _ => return None,
    };

    Some(result)
}
fn extract_plugin_path(e: &Element) -> SmallVec<[PathBuf; 1]> {
    if e.tag == "VST"
        && e.attr
            .get(1)
            .map(|ident| *ident == "reasamplomatic.dll")
            .unwrap_or(false)
    {
        let Some(strings) = extract_plugin_strings(e) else {
            eprintln!("failed to get sample path from fx reasamplomatic");
            return smallvec![];
        };
        strings.get(0).map(PathBuf::from).into_iter().collect()
    } else if e.tag == "VST"
        && e.attr
            .get(1)
            .map(|ident| *ident == "NadIR.vst3")
            .unwrap_or(false)
    {
        let Some(config) = extract_plugin_strings(e)
            .and_then(|list| list.into_iter().filter(|x| x.len() >= 100).next())
        else {
            eprintln!("failed to get sample path from fx NadIR");
            return smallvec![];
        };

        // config is a JSON, but it may have garbage data before/after it, so use regex instead
        static PATTERN: LazyLock<regex::Regex> =
            LazyLock::new(|| regex::Regex::new(r#""IRPath[01]" *: *("(?:[^"\\]|\\.)*")"#).unwrap());

        PATTERN
            .captures_iter(&config)
            .map(|c| c.get(1).expect("group 1"))
            // ignore built-in files
            .filter(|mat| !mat.as_str().starts_with("\"${documents}"))
            .map(|mat| serde_json::from_str::<String>(mat.as_str()).expect("malformed json string"))
            .map(|text| PathBuf::from(text))
            .collect()
    } else if e.tag == "VST"
        && e.attr
            .get(1)
            .map(|ident| *ident == "Sitala.dll")
            .unwrap_or(false)
    {
        let Some(config) = extract_plugin_strings(e)
            .and_then(|list| list.into_iter().filter(|x| x.len() >= 100).next())
        else {
            eprintln!("failed to get sample path from fx Sitala");
            return smallvec![];
        };

        // config is a JSON, but it may have garbage data before/after it, so use regex instead
        static PATTERN: LazyLock<regex::Regex> = LazyLock::new(|| {
            regex::Regex::new(r#"<sound slot="\d+" location=("(?:[^"\\]|\\.)*")"#).unwrap()
        });

        PATTERN
            .captures_iter(&config)
            .map(|c| c.get(1).expect("group 1"))
            .map(|mat| serde_json::from_str::<String>(mat.as_str()).expect("malformed json string"))
            .filter_map(|path| {
                if let Ok(path) = urlencoding::decode(path.as_str())
                    && path.starts_with("file:///")
                {
                    Some(PathBuf::from(&path["file:///".len()..]))
                } else {
                    eprintln!(
                        "malformed sample path in fx Sitala config: {}",
                        path.as_str()
                    );
                    None
                }
            })
            .collect()
    } else {
        // let Some(strings) = extract_plugin_strings(e) else {
        //     return None;
        // };
        // println!("  <{} {:?}> {:?}", e.tag, e.attr, strings);
        // // TODO
        smallvec![]
    }
}

pub fn extract_fxchain_paths(e: &Element) -> SmallVec<[PathBuf; 2]> {
    let plugins = {
        let mut out = vec![];
        find_fxchain_plugins(e, &mut out);
        out
    };

    plugins
        .into_iter()
        .flat_map(|plugin| extract_plugin_path(plugin))
        .collect()
}
