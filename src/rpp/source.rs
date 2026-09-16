use std::path::Path;

use rpp_parser::parser::{Child, Element};

use super::utils;

/// Find the path of a `<SOURCE>` element
pub fn get_source_path<'a>(e: &'a Element<'a>) -> Option<&'a Path> {
    let Some([mut source_type]) = utils::attr_as_arr(&e.attr) else {
        eprintln!("item source attr has more than 1 value: {:?}", e.attr);
        return None;
    };
    if source_type.starts_with("_OFFLINE_") {
        source_type = &source_type["_OFFLINE_".len()..];
    }
    match source_type {
        "MIDI" | "MIDIPOOL" | "CLICK" | "EMPTY" => None,
        "FLAC" | "WAVE" | "VORBIS" | "OPUS" | "WAVE_SLICE" | "REX" | "RPP_PROJECT" | "VIDEO" => {
            let res = e
                .children
                .iter()
                .filter_map(|child| {
                    if let Some([key, val]) = utils::child_as_arr(child)
                        && key == "FILE"
                    {
                        Some(Path::new(val))
                    } else {
                        None
                    }
                })
                .next();
            if res.is_none() {
                eprintln!("failed to find source path in <{}>", source_type);
                eprintln!("{:?}", e);
                panic!("failed to find source path in <{}>", source_type);
            }
            res
        }
        "MP3" => {
            let res = e
                .children
                .iter()
                .filter_map(|child| {
                    if let Some([key, val, _]) = utils::child_as_arr(child)
                        && key == "FILE"
                    {
                        Some(Path::new(val))
                    } else {
                        None
                    }
                })
                .next();
            if res.is_none() {
                eprintln!("failed to find source path in <{}>", source_type);
                eprintln!("{:?}", e);
                panic!("failed to find source path in <{}>", source_type);
            }
            res
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
                    eprintln!("{:?}", e);
                    panic!("failed to find source path in <SECTION>");
                })
        }
        _ => {
            eprintln!("unhandled source type! {:?}", source_type);
            eprintln!("{:?}", e);
            todo!("unhandled source type! {:?}", source_type)
        }
    }
}
