use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap},
    fmt::Display,
    path::{Path, PathBuf},
};

use base64_simd::STANDARD as base64;
use bitflags::bitflags;
use jiff::Timestamp;
use rpp_parser::parser::{Child, Element};
use smallvec::{SmallVec, smallvec};

fn iter_metronome_paths<'a>(e: &'a Element<'a>) -> impl Iterator<Item = &'a Path> {
    e.children
        .iter()
        .filter_map(|child| {
            if let Child::Line(items) = child {
                items.split_first()
            } else {
                None
            }
        })
        .filter_map(|(key, vals)| {
            if *key == "SAMPLES" {
                Some(vals.iter().filter(|x| !x.is_empty()).map(Path::new))
            } else {
                None
            }
        })
        .flatten()
}

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
            "JS" | "PARMENV" | "PROGRAMENV" => (),
            _ => todo!("unhandled plugin type! {:?}", child.tag),
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
            .flat_map(|s| s.split(|c: char| c.is_control()))
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

fn get_source_path<'a>(e: &'a Element<'a>) -> Option<&'a Path> {
    let Ok([source_type]) = TryInto::<[&str; 1]>::try_into(e.attr.as_slice()) else {
        eprintln!("item source attr has more than 1 value: {:?}", e.attr);
        return None;
    };
    match source_type {
        "MIDI" | "MIDIPOOL" => None,
        "FLAC" | "WAVE" | "RPP_PROJECT" => {
            let source_file = e
                .children
                .iter()
                .filter_map(|child| {
                    if let Child::Line(line) = child
                        && let Ok([key, val]) = TryInto::<[&str; 2]>::try_into(line.as_slice())
                        && key == "FILE"
                    {
                        Some(val)
                    } else {
                        None
                    }
                })
                .next();
            if let Some(source_file) = source_file {
                Some(Path::new(source_file))
            } else {
                eprintln!("failed to find source path in <{}>", source_type);
                None
            }
        }
        "SECTION" => {
            // find the inner source
            e.children
                .iter()
                .filter_map(|child| {
                    if let Child::Element(child) = child
                        && child.tag == "SOURCE"
                    {
                        get_source_path(child)
                    } else {
                        None
                    }
                })
                .next()
                .or_else(|| {
                    eprintln!("failed to find source path in <SECTION>");
                    None
                })
        }
        _ => {
            todo!("unhandled source type! {:?}", source_type)
        }
    }
}

fn iter_track_item_paths<'a>(e: &'a Element<'a>) -> impl Iterator<Item = &'a Path> {
    e.children
        .iter()
        .filter_map(|x| {
            if let Child::Element(x) = x
                && x.tag == "ITEM"
            {
                Some(x)
            } else {
                None
            }
        })
        .flat_map(|item| {
            item.children.iter().filter_map(|x| {
                if let Child::Element(x) = x
                    && x.tag == "SOURCE"
                {
                    Some(x)
                } else {
                    None
                }
            })
        })
        .filter_map(|source| get_source_path(source))
}

fn extract_plugin_path(e: &Element) -> Option<PathBuf> {
    if e.tag == "VST"
        && e.attr
            .get(1)
            .map(|ident| *ident == "reasamplomatic.dll")
            .unwrap_or(false)
    {
        // reasamplomatic
        let Some(strings) = extract_plugin_strings(e) else {
            eprintln!("failed to get sample path from reasamplomatic fx");
            return None;
        };
        strings.get(0).map(PathBuf::from)
    } else {
        let Some(strings) = extract_plugin_strings(e) else {
            return None;
        };
        println!("  <{} {:?}> {:?}", e.tag, e.attr, strings);
        // TODO
        None
    }
}

fn extract_fxchain_paths(e: &Element) -> SmallVec<[PathBuf; 2]> {
    let plugins = {
        let mut out = vec![];
        find_fxchain_plugins(e, &mut out);
        out
    };

    plugins
        .into_iter()
        .filter_map(|plugin| extract_plugin_path(plugin))
        .collect()
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
    pub struct Usages: u8 {
        const ITEM = 0b00000001;
        // /// Reasamplomatic5000
        // const RS5K = 0b00000010;
        const PLUGIN = 0b00000100;
        const RECORD_PATH = 0b01000000;
        const METRONOME = 0b10000000;
    }
}

impl Display for Usages {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.contains(Self::ITEM) {
            write!(f, "I")?;
        } else {
            write!(f, " ")?;
        }
        if self.contains(Self::PLUGIN) {
            write!(f, "P")?;
        } else {
            write!(f, " ")?;
        }
        if self.contains(Self::RECORD_PATH) {
            write!(f, "R")?;
        } else {
            write!(f, " ")?;
        }
        if self.contains(Self::METRONOME) {
            write!(f, "M")?;
        } else {
            write!(f, " ")?;
        }
        Ok(())
    }
}

pub type PathUsages<'a> = BTreeMap<Cow<'a, Path>, Usages>;

pub struct Project<'a> {
    pub date: Timestamp,
    pub usages: PathUsages<'a>,
}

pub fn parse_project<'a>(e: &'a Element<'a>) -> Result<Project<'a>, String> {
    if e.tag != "REAPER_PROJECT" {
        return Err(format!("expected project tag: {}", e.tag));
    }

    let date = {
        let num = e
            .attr
            .get(2)
            .ok_or_else(|| format!("can't find project date: {:?}", e.attr))?
            .parse::<i64>()
            .map_err(|_| format!("project date is malformed: {:?}", e.attr))?;

        Timestamp::from_second(num).map_err(|_| format!("invalid project date: {:?}", e.attr))?
    };

    let mut file_usages: PathUsages = PathUsages::new();

    for child in &e.children {
        match child {
            Child::Line(items) if let Some((key, vals)) = items.split_first() => match *key {
                "RECORD_PATH" => {
                    let [record_path, _] = vals.try_into().map_err(|_| {
                        format!("expected project record path to have exactly 2 values: {vals:?}")
                    })?;
                    file_usages
                        .entry(Path::new(record_path).into())
                        .or_default()
                        .insert(Usages::RECORD_PATH);
                }
                _ => (),
            },
            Child::Element(child) => match child.tag {
                // "METRONOME" => {
                //     for path in iter_metronome_paths(child) {
                //         file_usages.push((Usage::Metronome, path));
                //     }
                // }
                "MASTERFXLIST" => {
                    for path in extract_fxchain_paths(child) {
                        file_usages
                            .entry(path.into())
                            .or_default()
                            .insert(Usages::PLUGIN);
                    }
                }
                "TRACK" => {
                    for path in iter_track_item_paths(child) {
                        file_usages
                            .entry(path.into())
                            .or_default()
                            .insert(Usages::ITEM);
                    }

                    let fxchain = child
                        .children
                        .iter()
                        .filter_map(|x| {
                            if let Child::Element(x) = x
                                && x.tag == "FXCHAIN"
                            {
                                Some(x)
                            } else {
                                None
                            }
                        })
                        .next();
                    if let Some(fxchain) = fxchain {
                        for path in extract_fxchain_paths(fxchain) {
                            file_usages
                                .entry(path.into())
                                .or_default()
                                .insert(Usages::PLUGIN);
                        }
                    }
                }
                _ => (),
            },
            _ => (),
        }
    }

    Ok(Project {
        date,
        usages: file_usages,
    })
}
