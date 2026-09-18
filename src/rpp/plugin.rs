use std::{path::PathBuf, sync::LazyLock};

use base64_simd::STANDARD as base64;
use rpp_parser::parser::{Child, Element};
use smallvec::{SmallVec, smallvec};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginKind {
    VST,
    CLAP,
    JS,
}

#[derive(Debug, Clone)]
pub struct Plugin<'a> {
    kind: PluginKind,
    display_name: &'a str,
    ident: &'a str,
    element: Element<'a>,
}

impl<'a> Plugin<'a> {
    pub fn kind(&self) -> PluginKind {
        self.kind
    }

    pub fn display_name(&self) -> &str {
        self.display_name
    }

    pub fn ident(&self) -> &str {
        self.ident
    }

    fn extract_raw_strings_from_data(&self) -> Result<Vec<String>, &'static str> {
        match self.kind {
            // both vst2 and vst3
            PluginKind::VST => Ok(b64_extract_strings(&self.element.children)),
            PluginKind::CLAP => {
                // find the <STATE> element
                let state = self
                    .element
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
                    return Err("failed to find CLAP plugin data");
                };

                Ok(b64_extract_strings(&state.children))
            }
            // js plugins don't have string data
            PluginKind::JS => Ok(vec![]),
        }
    }

    /// Plugin-specific logic for extracting paths
    pub fn extract_paths(&self) -> SmallVec<[PathBuf; 1]> {
        if matches!(self.kind, PluginKind::VST) && self.ident == "reasamplomatic.dll" {
            let strings = match self.extract_raw_strings_from_data() {
                Ok(x) => x,
                Err(err) => {
                    eprintln!("failed to get sample path from fx reasamplomatic: {err}");
                    return smallvec![];
                }
            };
            strings.get(0).map(PathBuf::from).into_iter().collect()
        } else if matches!(self.kind, PluginKind::VST) && self.ident == "NadIR.vst3" {
            let config = match self.extract_raw_strings_from_data().and_then(|list| {
                list.into_iter()
                    .filter(|x| x.len() >= 100)
                    .next()
                    .ok_or("can't find raw config string")
            }) {
                Ok(x) => x,
                Err(err) => {
                    eprintln!("failed to get sample path from fx NadIR: {err}");
                    return smallvec![];
                }
            };

            // config is a JSON, but it may have garbage data before/after it, so use regex instead
            static PATTERN: LazyLock<regex::Regex> = LazyLock::new(|| {
                regex::Regex::new(r#""IRPath[01]" *: *("(?:[^"\\]|\\.)*")"#).unwrap()
            });

            PATTERN
                .captures_iter(&config)
                .map(|c| c.get(1).expect("group 1"))
                // ignore built-in files
                .filter(|mat| !mat.as_str().starts_with("\"${documents}"))
                .map(|mat| {
                    serde_json::from_str::<String>(mat.as_str()).expect("malformed json string")
                })
                .map(|text| PathBuf::from(text))
                .collect()
        } else if matches!(self.kind, PluginKind::VST) && self.ident == "Sitala.dll" {
            let config = match self.extract_raw_strings_from_data().and_then(|list| {
                list.into_iter()
                    .filter(|x| x.len() >= 100)
                    .next()
                    .ok_or("can't find raw config string")
            }) {
                Ok(x) => x,
                Err(err) => {
                    eprintln!("failed to get sample path from fx Sitala: {err}");
                    return smallvec![];
                }
            };

            // config is a XML, but it may have garbage data before/after it, so use regex instead
            static PATTERN: LazyLock<regex::Regex> = LazyLock::new(|| {
                regex::Regex::new(r#"<sound slot="\d+" location=("(?:[^"\\]|\\.)*")"#).unwrap()
            });

            PATTERN
                .captures_iter(&config)
                .map(|c| c.get(1).expect("group 1"))
                .map(|mat| {
                    serde_json::from_str::<String>(mat.as_str()).expect("malformed json string")
                })
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
}

/// Scan a `FXCHAIN` element for all plugin instances
pub fn collect_plugins<'a>(e: &'a Element<'a>, out: &mut Vec<Plugin<'a>>) {
    for child in &e.children {
        let Child::Element(child) = child else {
            continue;
        };
        let plugin = match child.tag {
            // both vst2 and vst3
            "VST" => Plugin {
                kind: PluginKind::VST,
                display_name: child.attr.first().expect("VST plugin has no name"),
                ident: child.attr.get(1).expect("VST plugin has no ident"),
                element: child.clone(),
            },
            "CLAP" => Plugin {
                kind: PluginKind::CLAP,
                display_name: child.attr.first().expect("CLAP plugin has no name"),
                ident: child.attr.get(1).expect("CLAP plugin has no ident"),
                element: child.clone(),
            },
            "JS" => Plugin {
                kind: PluginKind::JS,
                display_name: child.attr.get(1).expect("JS plugin has no ident"),
                ident: child.attr.first().expect("JS plugin has no name"),
                element: child.clone(),
            },
            "CONTAINER" => {
                collect_plugins(child, out);
                continue;
            }
            "PARMENV" | "PROGRAMENV" | "IN_PINS" | "OUT_PINS" | "JS_SER" | "JS_PINMAP"
            | "COMMENT" | "VIDEO_EFFECT" => {
                continue;
            }
            _ => {
                eprintln!("unhandled plugin type! {:?}", child.tag);
                eprintln!("{:?}", e);
                todo!("unhandled plugin type! {:?}", child.tag)
            }
        };
        out.push(plugin);
    }
}

pub fn extract_fxchain_paths(e: &Element) -> SmallVec<[PathBuf; 2]> {
    let plugins = {
        let mut out = vec![];
        collect_plugins(e, &mut out);
        out
    };

    plugins
        .into_iter()
        .flat_map(|plugin| plugin.extract_paths())
        .collect()
}
