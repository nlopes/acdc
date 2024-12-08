use std::{
    io::{self, BufReader, Read},
    path::PathBuf,
};

use anyhow::Result;
use clap::{ArgGroup, Parser};

/// Parses files
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
#[command(group(
    ArgGroup::new("input")
        .required(true)
        .args(["tck_mode", "files"])
))]
struct Args {
    /// List of files to parse
    #[arg(required = true, conflicts_with = "tck_mode")]
    files: Vec<PathBuf>,

    /// Run in TCK compatible mode, taking a single `AsciiDoc` document from `stdin` and
    /// outputting JSON to `stdout`
    #[arg(long)]
    tck_mode: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    if args.tck_mode {
        handle_tck_mode()?;
    } else {
        todo!();
        // for file in &args.files {
        //     let doc = acdc_parser::parse_file(file)?;
        // }
    }

    Ok(())
}

fn handle_tck_mode() -> Result<()> {
    let stdin = io::stdin();
    let mut reader = BufReader::new(stdin.lock());
    let mut input = String::new();
    reader.read_to_string(&mut input)?;

    let doc = acdc_parser::parse(&input)?;
    serde_json::to_writer_pretty(io::stdout(), &doc)?;
    Ok(())
}
