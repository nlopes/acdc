use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

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
        acdc_term::parse_file(file)?;
    }
    Ok(())
}
