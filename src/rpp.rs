use std::path::Path;

use jiff::Timestamp;
use rpp_parser::parser::{Child, Element};

fn iter_metronome_paths<'a>(e: &'a Element) -> impl Iterator<Item = &'a Path> {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Usage {
    Metronome,
    RecordPath,
}

pub type FileUsages<'a> = Vec<(Usage, &'a Path)>;

pub fn parse_project<'a>(e: &'a Element) -> Result<FileUsages<'a>, String> {
    if e.tag != "REAPER_PROJECT" {
        return Err(format!("expected project tag: {}", e.tag));
    }

    let attr: &[&str; 3] = e
        .attr
        .as_slice()
        .try_into()
        .map_err(|_| format!("expected 3 project attrs: {:?}", e.attr))?;

    let date = {
        let num = attr[2]
            .parse::<i64>()
            .map_err(|_| format!("project date is malformed: {:?}", attr))?;

        Timestamp::from_second(num).map_err(|_| format!("invalid project date: {:?}", attr))?
    };

    let mut file_usages: FileUsages = vec![];

    for child in &e.children {
        match child {
            Child::Line(items) if let Some((key, vals)) = items.split_first() => match *key {
                "RECORD_PATH" => {
                    let [record_path, _] = vals.try_into().map_err(|_| {
                        format!("expected project record path to have exactly 2 values: {vals:?}")
                    })?;
                    file_usages.push((Usage::RecordPath, Path::new(record_path)));
                }
                _ => (),
            },
            Child::Element(child) => match child.tag {
                "METRONOME" => {
                    for path in iter_metronome_paths(child) {
                        file_usages.push((Usage::Metronome, path));
                    }
                }
                _ => (),
            },
            _ => (),
        }
    }

    Ok(file_usages)
}
