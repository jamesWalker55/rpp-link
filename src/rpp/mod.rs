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

#[cfg(test)]
#[test]
fn test_scan_collect() {
    use std::collections::HashMap;
    use std::collections::HashSet;
    use std::fs;
    use std::path::PathBuf;

    use normalize_path::NormalizePath as _;

    fn iter_rpp_files(dir: &Path) -> impl Iterator<Item = walkdir::DirEntry> {
        walkdir::WalkDir::new(dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|x| {
                x.file_type().is_file()
                    && x.path()
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("rpp"))
            })
    }

    let mut dir_usage: HashMap<PathBuf, i64> = Default::default();
    let mut plugin_usage: HashMap<String, i64> = Default::default();

    fn path_depth_3(x: &Path) -> PathBuf {
        use std::path::Component;

        let mut normal_count = 0;

        x.components()
            .take_while(|comp| {
                match comp {
                    Component::Normal(_) => {
                        if normal_count >= 3 {
                            false
                        } else {
                            normal_count += 1;
                            true
                        }
                    }
                    // Always keep prefix (e.g., "D:") and root directory (e.g., "/")
                    _ => true,
                }
            })
            .collect()
    }

    let project_dirs: [&Path; _] = [
        Path::new(r"D:\Audio Projects"),
        Path::new(r"D:\Audio Projects (Reaper)"),
    ];
    for entry in project_dirs.iter().flat_map(|dir| iter_rpp_files(dir)) {
        let text = match fs::read_to_string(entry.path()) {
            Ok(x) => x,
            Err(err) => {
                eprintln!("failed to read {}: {}", entry.path().display(), err);
                continue;
            }
        };
        let proj = match rpp_parser::parser::parse_element(&text) {
            Ok(x) => x,
            Err(err) => {
                eprintln!(
                    "failed to parse {}: {}",
                    entry.path().display(),
                    err.code.description()
                );
                continue;
            }
        };

        let mut used_dirs: HashSet<PathBuf> = Default::default();
        let mut used_plugins: HashSet<String> = Default::default();

        for child in &proj.children {
            match child {
                Child::Line(items) if let Some((key, vals)) = items.split_first() => {
                    // nothing to check
                }
                Child::Element(child) => match child.tag {
                    "METRONOME" => {
                        for path in iter_metronome_paths(child)
                            .filter(|x| x.is_absolute())
                            .map(|x| x.normalize())
                        {
                            used_dirs.insert(path_depth_3(&path));
                        }
                    }
                    "MASTERFXLIST" => {
                        let plugins = {
                            let mut out = vec![];
                            plugin::collect_plugins(child, &mut out);
                            out
                        };

                        for plugin in plugins {
                            used_plugins.insert(plugin.ident().to_string());

                            for path in plugin
                                .extract_paths()
                                .into_iter()
                                .filter(|x| x.is_absolute())
                                .map(|x| x.normalize())
                            {
                                used_dirs.insert(path_depth_3(&path));
                            }
                        }
                    }
                    "TRACK" => {
                        for path in iter_track_item_paths(child)
                            .filter(|x| x.is_absolute())
                            .map(|x| x.normalize())
                        {
                            used_dirs.insert(path_depth_3(&path));
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
                            let plugins = {
                                let mut out = vec![];
                                plugin::collect_plugins(fxchain, &mut out);
                                out
                            };

                            for plugin in plugins {
                                used_plugins.insert(plugin.ident().to_string());

                                for path in plugin
                                    .extract_paths()
                                    .into_iter()
                                    .filter(|x| x.is_absolute())
                                    .map(|x| x.normalize())
                                {
                                    used_dirs.insert(path_depth_3(&path));
                                }
                            }
                        }
                    }
                    _ => (),
                },
                _ => (),
            }
        }

        for path in used_dirs {
            *dir_usage.entry(path).or_default() += 1;
        }
        for ident in used_plugins {
            *plugin_usage.entry(ident).or_default() += 1;
        }
    }

    // fn asdf<T: std::fmt::Display>(usage: HashMap<T, i64>, outpath: &Path) -> std::io::Result<()> {
    //     let usage = dir_usage;

    //     use std::fs::File;
    //     use std::io::{BufWriter, Write};

    //     let mut usage = usage.into_iter().collect::<Vec<_>>();
    //     usage.sort_by_key(|x| -x.1);

    //     // Create the file and wrap it in a BufWriter for efficient I/O
    //     let file = File::create(outpath)?;
    //     let mut writer = BufWriter::new(file);

    //     // Write each sorted entry to the file in the format `NUM: T`
    //     for (item, count) in usage {
    //         writeln!(writer, "{}: {}", count, item)?;
    //     }

    //     Ok(())
    // }

    {
        let usage = dir_usage;
        let outpath = Path::new("dir_usage.txt");

        use std::fs::File;
        use std::io::{BufWriter, Write};

        let mut usage = usage.into_iter().collect::<Vec<_>>();
        usage.sort_by_key(|x| -x.1);

        // Create the file and wrap it in a BufWriter for efficient I/O
        let file = File::create(outpath).unwrap();
        let mut writer = BufWriter::new(file);

        // Write each sorted entry to the file in the format `NUM: T`
        for (item, count) in usage {
            writeln!(writer, "{}: {}", count, item.display()).unwrap();
        }
    }
    {
        let usage = plugin_usage;
        let outpath = Path::new("plugin_usage.txt");

        use std::fs::File;
        use std::io::{BufWriter, Write};

        let mut usage = usage.into_iter().collect::<Vec<_>>();
        usage.sort_by_key(|x| -x.1);

        // Create the file and wrap it in a BufWriter for efficient I/O
        let file = File::create(outpath).unwrap();
        let mut writer = BufWriter::new(file);

        // Write each sorted entry to the file in the format `NUM: T`
        for (item, count) in usage {
            writeln!(writer, "{}: {}", count, item).unwrap();
        }
    }
}
