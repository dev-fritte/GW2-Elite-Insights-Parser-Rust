//! `ei`: reads an EVTC log (`.evtc`, `.zevtc`, `.evtc.zip`) and writes a JSON summary to stdout
//! or to a file.
//!
//! ```text
//! ei <log> [--content <dir>] [--out <file>] [--pretty]
//! ```
//!
//! `--content` is the `Content` directory of the C# parser (`MapList.json`, `SpecList.json`). Without it
//! profession names of newer specialisations and the region of logs without a world id stay unknown.

use std::path::PathBuf;
use std::process::ExitCode;

use ei_core::api::Api;
use ei_core::pipeline::{analyze, to_json};
use evtc_ingest::read_log;

const USAGE: &str = "usage: ei <log> [--content <dir>] [--out <file>] [--pretty]";

struct Args {
    log: PathBuf,
    content: Option<PathBuf>,
    out: Option<PathBuf>,
    pretty: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut log = None;
    let mut content = None;
    let mut out = None;
    let mut pretty = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--content" => {
                content = Some(PathBuf::from(args.next().ok_or("--content needs a path")?));
            }
            "--out" => out = Some(PathBuf::from(args.next().ok_or("--out needs a path")?)),
            "--pretty" => pretty = true,
            "-h" | "--help" => return Err(USAGE.to_string()),
            _ if arg.starts_with('-') => return Err(format!("unknown option {arg}\n{USAGE}")),
            _ if log.is_none() => log = Some(PathBuf::from(arg)),
            _ => return Err(format!("unexpected argument {arg}\n{USAGE}")),
        }
    }
    Ok(Args {
        log: log.ok_or(USAGE)?,
        content,
        out,
        pretty,
    })
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_args()?;
    let api = match args.content.as_deref() {
        Some(dir) => Api::load(dir)?,
        None => Api::default(),
    };
    let data = read_log(&args.log)?;
    let analysis = analyze(&data, &api)?;
    let json = to_json(&analysis);
    let text = if args.pretty {
        serde_json::to_string_pretty(&json)?
    } else {
        serde_json::to_string(&json)?
    };
    match args.out {
        Some(path) => std::fs::write(path, text)?,
        None => println!("{text}"),
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
