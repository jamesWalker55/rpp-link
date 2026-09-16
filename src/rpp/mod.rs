mod plugin;
mod source;
mod utils;

use std::{borrow::Cow, collections::BTreeMap, fmt::Display, path::Path};

use bitflags::bitflags;
use jiff::Timestamp;
use rpp_parser::parser::{Child, Element};

pub use crate::rpp::plugin::{Plugin, PluginKind};

/// Find all paths in a `<METRONOME>` element
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
        .filter_map(|source| source::get_source_path(source))
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
                    for path in plugin::extract_fxchain_paths(child) {
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
                        for path in plugin::extract_fxchain_paths(fxchain) {
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
