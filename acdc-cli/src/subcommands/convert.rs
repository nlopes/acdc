use std::path::PathBuf;

use acdc_converters_core::{Doctype, GeneratorMetadata, Options, Processable};
use acdc_parser::{AttributeValue, DocumentAttributes, SafeMode};
use clap::{ArgAction, Args as ClapArgs, ValueEnum};
use rayon::prelude::*;

use crate::error;

#[derive(Debug, ValueEnum, Clone)]
pub enum Backend {
    #[cfg(feature = "html")]
    Html,

    #[cfg(feature = "terminal")]
    Terminal,

    #[cfg(feature = "manpage")]
    Manpage,
}

impl Backend {
    /// Get the backend name as a string
    fn as_str(&self) -> &'static str {
        match self {
            #[cfg(feature = "html")]
            Backend::Html => "html",
            #[cfg(feature = "terminal")]
            Backend::Terminal => "terminal",
            #[cfg(feature = "manpage")]
            Backend::Manpage => "manpage",
        }
    }
}

/// Convert `AsciiDoc` documents to various output formats
#[derive(ClapArgs, Debug)]
#[allow(clippy::struct_excessive_bools)] // CLI flags are naturally booleans
pub struct Args {
    /// List of files to convert
    #[arg(conflicts_with = "stdin")]
    pub files: Vec<PathBuf>,

    /// Backend output format
    #[arg(long, value_enum, default_value_t = Backend::Html)]
    pub backend: Backend,

    /// Document type to use when converting document
    #[arg(long, value_parser = clap::value_parser!(Doctype), default_value = "article")]
    pub doctype: Doctype,

    /// Set safe mode to safe
    #[arg(long, conflicts_with = "safe_mode")]
    pub safe: bool,

    /// Safe mode to use when converting document
    #[arg(short = 'S', long, value_parser = clap::value_parser!(SafeMode), default_value = "unsafe")]
    pub safe_mode: SafeMode,

    /// Input from stdin
    #[arg(long, conflicts_with = "files")]
    pub stdin: bool,

    /// Show timing information
    #[arg(long)]
    pub timings: bool,

    /// Attributes to pass to the backend
    #[arg(
        short = 'a',
        long = "attribute",
        value_name = "NAME[=VALUE | !]",
        action = ArgAction::Append
    )]
    pub attributes: Vec<String>,

    /// Enable Setext-style (underlined) header parsing
    ///
    /// When enabled, headers can use the legacy two-line syntax where
    /// the title is underlined with `=`, `-`, `~`, `^`, or `+` characters.
    #[cfg(feature = "setext")]
    #[arg(long)]
    pub enable_setext_compatibility: bool,

    /// Strict mode
    ///
    /// When enabled, some errors related with non-conformance (but still recoverable)
    /// will not allow conversion. For example: non-conforming manpage titles (not
    /// matching `name(volume)` format) will cause conversion to fail instead of using
    /// fallback values.
    #[arg(long)]
    pub strict: bool,

    /// Suppress enclosing document structure and output an embedded document
    ///
    /// When enabled, the HTML output excludes DOCTYPE, html, head, and body tags.
    /// Only applies to the HTML backend.
    #[arg(short = 'e', long)]
    pub embedded: bool,
}

/// Validate that the requested backend is available (compiled in)
fn validate_backend(backend: &Backend) -> Result<(), String> {
    match backend.as_str() {
        "html" => {
            #[cfg(feature = "html")]
            return Ok(());
            #[cfg(not(feature = "html"))]
            return Err("HTML backend not available. Recompile with --features html".into());
        }
        "terminal" => {
            #[cfg(feature = "terminal")]
            return Ok(());
            #[cfg(not(feature = "terminal"))]
            return Err("Terminal backend not available. Recompile with --features terminal".into());
        }
        "manpage" => {
            #[cfg(feature = "manpage")]
            return Ok(());
            #[cfg(not(feature = "manpage"))]
            return Err("Manpage backend not available. Recompile with --features manpage".into());
        }
        _ => Err(format!("Unknown backend: {}", backend.as_str())),
    }
}

pub fn run(args: &Args) -> miette::Result<()> {
    // Validate backend is available before proceeding
    validate_backend(&args.backend)
        .map_err(|e| miette::miette!("{e}"))?;

    let safe_mode = if args.safe {
        SafeMode::Safe
    } else {
        args.safe_mode
    };

    let (document_attributes, doctype) = {
        #[cfg(feature = "manpage")]
        {
            let mut document_attributes = build_attributes_map(&args.attributes);
            // Auto-set doctype to Manpage when using manpage backend
            // This matches asciidoctor behavior where --backend manpage implies --doctype manpage
            let doctype = if matches!(args.backend, Backend::Manpage) {
                // Set doctype attribute for the parser (parser checks this to derive manpage attrs)
                document_attributes.insert(
                    "doctype".to_string(),
                    AttributeValue::String("manpage".to_string()),
                );
                Doctype::Manpage
            } else {
                args.doctype
            };
            (document_attributes, doctype)
        }
        #[cfg(not(feature = "manpage"))]
        {
            let document_attributes = build_attributes_map(&args.attributes);
            (document_attributes, args.doctype.clone())
        }
    };

    let options = Options::builder()
        .generator_metadata(GeneratorMetadata::new(
            env!("CARGO_BIN_NAME"),
            env!("CARGO_PKG_VERSION"),
        ))
        .doctype(doctype)
        .safe_mode(safe_mode)
        .timings(args.timings)
        .embedded(args.embedded)
        .build();

    match args.backend {
        #[cfg(feature = "html")]
        Backend::Html => {
            // HTML can process files in parallel - each file writes to separate output
            run_processor::<acdc_converters_html::Processor>(
                args,
                options,
                document_attributes,
                true,
            )
            .map_err(|e| error::display(&e))
        }

        #[cfg(feature = "terminal")]
        Backend::Terminal => {
            // Terminal outputs to stdout - must process files sequentially to avoid interleaving
            run_processor::<acdc_converters_terminal::Processor>(
                args,
                options,
                document_attributes,
                false,
            )
            .map_err(|e| error::display(&e))
        }

        #[cfg(feature = "manpage")]
        Backend::Manpage => {
            // Manpage outputs to separate files - can process in parallel
            run_processor::<acdc_converters_manpage::Processor>(
                args,
                options,
                document_attributes,
                true,
            )
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
        let parser_options =
            build_parser_options(args, &base_options, processor.document_attributes());
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
            let parser_options =
                build_parser_options(args, &base_options, document_attributes.clone());

            if base_options.timings() {
                let now = std::time::Instant::now();
                let result = acdc_parser::parse_file(file, &parser_options);
                let elapsed = now.elapsed();
                if result.is_ok() {
                    use acdc_converters_core::PrettyDuration;
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
    let (_successes, errors): (Vec<_>, Vec<_>) = results
        .into_iter()
        .partition(|(_file, result)| result.is_ok());

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
    // Start with rendering defaults (from converters/core)
    // CLI-provided attributes will override these defaults
    let mut map = acdc_converters_core::default_rendering_attributes();

    // Add CLI-provided attributes (these take precedence over defaults)
    for raw_attr in values {
        let (name, val) = if let Some(stripped) = raw_attr.strip_suffix('!') {
            (stripped.to_string(), AttributeValue::None)
        } else if let Some((name, val)) = raw_attr.split_once('=') {
            (name.to_string(), AttributeValue::String(val.to_string()))
        } else {
            (raw_attr.clone(), AttributeValue::Bool(true))
        };
        map.set(name, val); // use set() to override defaults
    }
    map
}

/// Build parser options from CLI args and base options
#[allow(unused_variables)]
fn build_parser_options(
    args: &Args,
    base_options: &Options,
    document_attributes: DocumentAttributes,
) -> acdc_parser::Options {
    let mut builder = acdc_parser::Options::builder()
        .with_safe_mode(base_options.safe_mode())
        .with_attributes(document_attributes);

    if base_options.timings() {
        builder = builder.with_timings();
    }

    if args.strict {
        builder = builder.with_strict();
    }

    #[cfg(feature = "setext")]
    if args.enable_setext_compatibility {
        builder = builder.with_setext();
    }

    builder.build()
}
