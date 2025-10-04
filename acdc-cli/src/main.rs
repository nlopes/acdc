use std::path::PathBuf;

use acdc_converters_common::{GeneratorMetadata, Options, Processable};
use acdc_core::{Doctype, SafeMode, Source};
use acdc_parser::{AttributeValue, DocumentAttributes};
use anyhow::Result;
use clap::{ArgAction, Parser, ValueEnum};

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

    /// set safe mode to safe
    #[arg(long, conflicts_with = "safe_mode")]
    safe: bool,

    /// safe mode to use when converting document
    #[arg(short = 'S', long, value_enum, default_value_t = SafeMode::Unsafe)]
    safe_mode: SafeMode,

    /// input from stdin
    #[arg(long, conflicts_with = "files")]
    stdin: bool,

    /// timing information
    #[arg(long)]
    timings: bool,

    /// attributes to pass to the backend
    #[arg(
        short = 'a',
        long = "attribute",
        value_name = "NAME[=VALUE | !]",
        action = ArgAction::Append
    )]
    attributes: Vec<String>,
}

fn setup_logging() {
    use tracing_subscriber::{EnvFilter, prelude::*};

    let env_filter = EnvFilter::try_from_env("ACDC_LOG");

    if let Ok(filter) = env_filter {
        let layer = tracing_subscriber::fmt::layer()
            .with_writer(std::io::stderr)
            .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stderr()))
            .with_timer(tracing_subscriber::fmt::time::Uptime::default())
            .with_filter(filter);

        tracing_subscriber::registry().with(layer).init();
    }
}

fn main() -> Result<()> {
    setup_logging();
    let args = Args::parse();
    let document_attributes = build_attributes_map(&args.attributes);
    let safe_mode = if args.safe {
        SafeMode::Safe
    } else {
        args.safe_mode.clone()
    };

    let mut options = Options {
        generator_metadata: GeneratorMetadata::new(
            env!("CARGO_BIN_NAME"),
            env!("CARGO_PKG_VERSION"),
        ),
        doctype: args.doctype.clone(),
        safe_mode,
        source: Source::Files(args.files.clone()),
        timings: args.timings,
    };

    if args.stdin {
        tracing::debug!("Reading from stdin");
        options.source = Source::Stdin;
    }

    match args.backend {
        Backend::Html => {
            run_processor(
                &args,
                &acdc_html::Processor::new(options, document_attributes),
            )?;
        }

        #[cfg(feature = "tck")]
        Backend::Tck => {
            options.source = Source::Stdin;
            acdc_tck::Processor::new(options, document_attributes).run()?;
        }

        #[cfg(feature = "terminal")]
        Backend::Terminal => {
            run_processor(
                &args,
                &acdc_terminal::Processor::new(options, document_attributes),
            )?;
        }
    }

    Ok(())
}

#[tracing::instrument(skip(processor))]
fn run_processor<P: Processable>(args: &Args, processor: &P) -> Result<(), P::Error> {
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

fn build_attributes_map(values: &[String]) -> DocumentAttributes {
    let mut map = DocumentAttributes::default();
    for raw_attr in values {
        let (name, val) = if let Some(stripped) = raw_attr.strip_suffix('!') {
            (stripped.to_string(), AttributeValue::None)
        } else if let Some((name, val)) = raw_attr.split_once('=') {
            (name.to_string(), AttributeValue::String(val.to_string()))
        } else {
            (raw_attr.to_string(), AttributeValue::Bool(true))
        };
        map.insert(name, val);
    }
    map
}
