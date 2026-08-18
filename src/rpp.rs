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

pub struct Project<'a> {
    pub date: Timestamp,
    pub usages: FileUsages<'a>,
}

pub fn parse_project<'a>(e: &'a Element) -> Result<Project<'a>, String> {
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
                // "METRONOME" => {
                //     for path in iter_metronome_paths(child) {
                //         file_usages.push((Usage::Metronome, path));
                //     }
                // }
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
