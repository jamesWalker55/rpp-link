use std::path::{Path, PathBuf};

const HELP: &str = "\
App

USAGE:
  app [OPTIONS] [INPUT]

FLAGS:
  -h, --help            Prints help information

ARGS:
  <INPUT>               Input directory path
";

#[derive(Debug)]
pub struct Args {
    pub path: PathBuf,
}

pub fn parse() -> Result<Args, pico_args::Error> {
    let mut pargs = pico_args::Arguments::from_env();

    // Help has a higher priority and should be handled separately.
    if pargs.contains(["-h", "--help"]) {
        print!("{}", HELP);
        std::process::exit(0);
    }

    let args = Args {
        path: pargs.free_from_fn(parse_existing_path)?,
    };

    // It's up to the caller what to do with the remaining arguments.
    let remaining = pargs.finish();
    if !remaining.is_empty() {
        return Err(pico_args::Error::ArgumentParsingFailed {
            cause: format!("unused arguments left: {:?}.", remaining),
        });
    }

    Ok(args)
}

fn parse_existing_path(s: &str) -> Result<PathBuf, &'static str> {
    let p = Path::new(s);
    if !p.exists() {
        return Err("path does not exist");
    }
    Ok(p.to_path_buf())
}
