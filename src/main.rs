use std::{fs, path::Path};

use walkdir::WalkDir;

mod argparse;

fn iter_rpp_files(dir: &Path) -> impl Iterator<Item = walkdir::DirEntry> {
    WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|x| {
            x.file_type().is_file()
                && x.path()
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("rpp"))
        })
}

fn main() {
    let args = match argparse::parse() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error: {}.", e);
            std::process::exit(1);
        }
    };

    println!("{:#?}", args);

    for entry in iter_rpp_files(&args.path) {
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
    }
}
