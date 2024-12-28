use std::path::PathBuf;

use acdc_backends_common::{Config, Doctype, Processable, SafeMode};
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
}

fn main() -> Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer())
        .init();

    let args = Args::parse();

    let config = Config {
        doctype: args.doctype.clone(),
        safe_mode: args.safe_mode.clone(),
        files: args.files.clone(),
    };
    match args.backend {
        Backend::Html => {
            if args.files.is_empty() {
                tracing::error!("You must pass files to this processor");
            }
            acdc_html::Processor::new(config).run()?;
        }

        #[cfg(feature = "tck")]
        Backend::Tck => {
            acdc_tck::Processor::new(config).run()?;
        }

        #[cfg(feature = "terminal")]
        Backend::Terminal => {
            if args.files.is_empty() {
                tracing::error!("You must pass files to this processor");
            }
            acdc_terminal::Processor::new(config).run()?;
        }
    };

    Ok(())
}
