use std::io::{self, Write};
use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

use acdc_term::Render;

/// Parses files
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// List of files to parse
    #[arg(required = true)]
    files: Vec<PathBuf>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    for file in &args.files {
        parse_file(file)?;
    }
    Ok(())
}

fn parse_file(file: &PathBuf) -> Result<()> {
    let doc = acdc_parser::parse_file(file)?;
    let mut stdout = io::stdout();
    doc.render(&mut stdout)?;
    stdout.flush()?;
    Ok(())
}
