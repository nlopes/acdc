use std::path::PathBuf;

use acdc_converters_common::{GeneratorMetadata, Options, Processable};
use acdc_core::{Doctype, SafeMode};
use acdc_parser::{AttributeValue, DocumentAttributes};
use clap::{ArgAction, Parser, ValueEnum};
use rayon::prelude::*;

mod error;

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

fn main() -> miette::Result<()> {
    setup_logging();
    let args = Args::parse();
    let document_attributes = build_attributes_map(&args.attributes);
    let safe_mode = if args.safe {
        SafeMode::Safe
    } else {
        args.safe_mode.clone()
    };

    let options = Options {
        generator_metadata: GeneratorMetadata::new(
            env!("CARGO_BIN_NAME"),
            env!("CARGO_PKG_VERSION"),
        ),
        doctype: args.doctype.clone(),
        safe_mode,
        timings: args.timings,
    };

    match args.backend {
        #[cfg(feature = "html")]
        Backend::Html => {
            // HTML can process files in parallel - each file writes to separate output
            run_processor::<acdc_html::Processor>(&args, options, document_attributes, true)
                .map_err(|e| error::display(&e))
        }

        #[cfg(feature = "terminal")]
        Backend::Terminal => {
            // Terminal outputs to stdout - must process files sequentially to avoid interleaving
            run_processor::<acdc_terminal::Processor>(&args, options, document_attributes, false)
                .map_err(|e| error::display(&e))
        }
    }
}

#[tracing::instrument(skip(base_options, document_attributes))]
fn run_processor<P>(
    args: &Args,
    base_options: Options,
    document_attributes: DocumentAttributes,
    parallelize: bool,
) -> Result<(), P::Error>
where
    P: Processable<Options = Options>,
    P::Error: Send + std::error::Error + 'static + From<acdc_parser::Error>,
{
    if !args.stdin && args.files.is_empty() {
        tracing::error!("You must pass at least one file to this processor");
        std::process::exit(1);
    }

    // Handle stdin separately (no parallelization)
    if args.stdin {
        let processor = P::new(base_options.clone(), document_attributes.clone());
        let parser_options = acdc_parser::Options {
            safe_mode: base_options.safe_mode.clone(),
            timings: base_options.timings,
            document_attributes: document_attributes.clone(),
        };
        let stdin = std::io::stdin();
        let mut reader = std::io::BufReader::new(stdin.lock());
        let doc = acdc_parser::parse_from_reader(&mut reader, &parser_options)?;
        return processor.convert(&doc, None);
    }

    // PHASE 1: Parse all files in parallel (always - parsing is the expensive part)
    let parse_results: Vec<(PathBuf, Result<acdc_parser::Document, acdc_parser::Error>)> = args
        .files
        .par_iter()
        .map(|file| {
            let parser_options = acdc_parser::Options {
                safe_mode: base_options.safe_mode.clone(),
                timings: base_options.timings,
                document_attributes: document_attributes.clone(),
            };

            if base_options.timings {
                let now = std::time::Instant::now();
                let result = acdc_parser::parse_file(file, &parser_options);
                let elapsed = now.elapsed();
                if result.is_ok() {
                    use acdc_converters_common::PrettyDuration;
                    eprintln!("  Parsed {} in {}", file.display(), elapsed.pretty_print());
                }
                (file.clone(), result)
            } else {
                let result = acdc_parser::parse_file(file, &parser_options);
                (file.clone(), result)
            }
        })
        .collect();

    // PHASE 2: Convert documents - either in parallel or sequentially
    let results: Vec<(PathBuf, Result<(), P::Error>)> = if parallelize {
        // Parallel conversion for converters with separate output files (e.g., HTML)
        parse_results
            .into_par_iter()
            .map(|(file, parse_result)| {
                let processor = P::new(base_options.clone(), document_attributes.clone());
                let convert_result = match parse_result {
                    Ok(doc) => processor.convert(&doc, Some(&file)),
                    Err(e) => Err(e.into()),
                };
                (file, convert_result)
            })
            .collect()
    } else {
        // Sequential conversion for converters that output to stdout (e.g., Terminal)
        let processor = P::new(base_options, document_attributes);
        parse_results
            .into_iter()
            .map(|(file, parse_result)| {
                let convert_result = match parse_result {
                    Ok(doc) => processor.convert(&doc, Some(&file)),
                    Err(e) => Err(e.into()),
                };
                (file, convert_result)
            })
            .collect()
    };

    // Separate successes from errors
    let (successes, errors): (Vec<_>, Vec<_>) =
        results.into_iter().partition(|(_, result)| result.is_ok());

    // Count successful conversions (we don't need to do anything with them)
    let _success_count = successes.len();

    // If there are errors, collect and display them
    if !errors.is_empty() {
        eprintln!("\nFailed to process {} file(s):", errors.len());
        for (idx, (file, result)) in errors.iter().enumerate() {
            if let Err(error) = result {
                eprintln!("\n{}. File: {}", idx + 1, file.display());
                let report = error::display(error);
                eprintln!("{report:?}");
            }
        }
        std::process::exit(1);
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
            (raw_attr.clone(), AttributeValue::Bool(true))
        };
        map.insert(name, val);
    }
    map
}
