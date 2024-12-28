use std::path::PathBuf;

use acdc_backends_common::{Config, Doctype, Processable, SafeMode, Source};
use anyhow::Result;
use clap::{Parser, ValueEnum};
use tracing_subscriber::prelude::*;
use tracing_subscriber::{fmt, EnvFilter};

#[derive(Debug, ValueEnum, Clone)]
enum Backend {
    #[cfg(feature = "html")]
    Html,

    #[cfg(feature = "tck")]
    Tck,

    #[cfg(feature = "terminal")]
    Terminal,
}

/// Parses files
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// List of files to parse
    #[arg(conflicts_with = "stdin")]
    files: Vec<PathBuf>,

    /// backend output format
    #[arg(long, value_enum, default_value_t = Backend::Html)]
    backend: Backend,

    /// document type to use when converting document
    #[arg(long, value_enum, default_value_t = Doctype::Article)]
    doctype: Doctype,

    /// safe mode to use when converting document
    #[arg(long, value_enum, default_value_t = SafeMode::Unsafe)]
    safe_mode: SafeMode,

    /// input from stdin
    #[arg(long, conflicts_with = "files")]
    stdin: bool,
}

fn main() -> Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer())
        .init();

    let args = Args::parse();

    let mut config = Config {
        doctype: args.doctype.clone(),
        safe_mode: args.safe_mode.clone(),
        source: Source::Files(args.files.clone()),
    };

    if args.stdin {
        tracing::debug!("Reading from stdin");
        config.source = Source::Stdin;
    }

    match args.backend {
        Backend::Html => {
            run_processor(&args, acdc_html::Processor::new(config))?;
        }

        #[cfg(feature = "tck")]
        Backend::Tck => {
            config.source = Source::Stdin;
            acdc_tck::Processor::new(config).run()?;
        }

        #[cfg(feature = "terminal")]
        Backend::Terminal => {
            run_processor(&args, acdc_terminal::Processor::new(config))?;
        }
    };

    Ok(())
}

#[tracing::instrument(skip(processor))]
fn run_processor<P: Processable>(args: &Args, processor: P) -> Result<(), P::Error> {
    if args.stdin {
        let output = processor.output()?;
        println!("{output}");
    } else if args.files.is_empty() {
        tracing::error!("You must pass at least one file to this processor");
        std::process::exit(1);
    } else {
        processor.run()?;
    }

    Ok(())
}
