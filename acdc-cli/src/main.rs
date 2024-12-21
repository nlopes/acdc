use std::{
    io::{self, BufReader, Write},
    path::PathBuf,
};

use acdc_core::{Config, Doctype, Processable, SafeMode};
use anyhow::Result;
use clap::{Parser, ValueEnum};
use serde::Deserialize;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{fmt, EnvFilter};

#[derive(Debug, ValueEnum, Clone)]
enum Backend {
    #[cfg(feature = "html")]
    Html,
    #[cfg(feature = "terminal")]
    Terminal,
}

/// Parses files
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// List of files to parse
    #[arg(required = true, conflicts_with = "tck_mode")]
    files: Vec<PathBuf>,

    /// backend output format
    #[arg(long, value_enum, conflicts_with = "tck_mode", default_value_t = Backend::Html)]
    backend: Backend,

    /// document type to use when converting document
    #[arg(long, value_enum, conflicts_with = "tck_mode", default_value_t = Doctype::Article)]
    doctype: Doctype,

    /// safe mode to use when converting document
    #[arg(long, value_enum, conflicts_with = "tck_mode", default_value_t = SafeMode::Unsafe)]
    safe_mode: SafeMode,

    /// Run in TCK compatible mode, taking a single `AsciiDoc` document from `stdin` and
    /// outputting JSON to `stdout`
    #[arg(long)]
    tck_mode: bool,
}

fn main() -> Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer())
        .init();

    let args = Args::parse();

    if args.tck_mode {
        handle_tck_mode()?;
    } else {
        handle_normal_mode(&args)?;
    }

    Ok(())
}

#[derive(Debug, Deserialize)]
struct TckInput {
    contents: String,
    path: String,
    r#type: String,
}

#[tracing::instrument]
fn handle_tck_mode() -> Result<()> {
    let stdin = io::stdin();
    let mut reader = BufReader::new(stdin.lock());
    let tck_input: TckInput = serde_json::from_reader(&mut reader)?;
    tracing::debug!(
        path = tck_input.path,
        r#type = tck_input.r#type,
        "processing TCK input",
    );
    let doc = acdc_parser::parse(&tck_input.contents)?;
    let mut stdout = io::stdout();
    serde_json::to_writer(&stdout, &doc)?;
    stdout.flush()?;
    Ok(())
}

#[tracing::instrument]
fn handle_normal_mode(args: &Args) -> Result<()> {
    let config = Config {
        doctype: args.doctype.clone(),
        safe_mode: args.safe_mode.clone(),
    };
    match args.backend {
        Backend::Html => {
            acdc_html::Processor::new(config).process_files(&args.files)?;
        }

        #[cfg(feature = "terminal")]
        Backend::Terminal => acdc_terminal::Processor::new(config).process_files(&args.files)?,
    };
    Ok(())
}
