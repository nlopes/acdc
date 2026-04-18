use std::{path::PathBuf, time::Duration};

use acdc_converters_core::{
    Backend, Converter, Doctype, GeneratorMetadata, Options, OutputDestination,
};
use acdc_parser::{AttributeValue, DocumentAttributes, SafeMode};
use clap::{ArgAction, Args as ClapArgs};
use miette::Report;
use rayon::prelude::*;

use crate::{
    error,
    timing::{TimingEntry, print_timing_table},
};

/// Convert `AsciiDoc` documents to various output formats
#[derive(ClapArgs, Debug)]
#[allow(clippy::struct_excessive_bools)] // CLI flags are naturally booleans
pub struct Args {
    /// Input from stdin
    #[arg(long, conflicts_with = "files")]
    pub stdin: bool,

    /// Output file (default: based on path of input file); use - to output to STDOUT
    ///
    /// When specified, output is written to this file instead of deriving
    /// the output path from the input file. If multiple input files are
    /// provided with this option, only the first file is processed.
    #[arg(short = 'o', long = "out-file", value_name = "FILE")]
    pub out_file: Option<String>,

    /// List of files to convert
    #[arg(conflicts_with = "stdin")]
    pub files: Vec<PathBuf>,

    /// Backend output format
    #[arg(short = 'b', long, value_parser = clap::value_parser!(Backend), default_value_t = Backend::Html)]
    pub backend: Backend,

    /// Document type to use when converting document
    #[arg(short = 'd', long, value_parser = clap::value_parser!(Doctype), default_value = "article")]
    pub doctype: Doctype,

    /// Set safe mode to safe
    #[arg(long, conflicts_with = "safe_mode")]
    pub safe: bool,

    /// Safe mode to use when converting document
    #[arg(short = 'S', long, value_parser = clap::value_parser!(SafeMode), default_value = "unsafe", conflicts_with = "safe")]
    pub safe_mode: SafeMode,

    /// Show timing information
    #[arg(short = 't', long)]
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

    /// Disable automatic pager for terminal output
    ///
    /// By default, when using the terminal backend and stdout is a TTY,
    /// output is piped through a pager. Respects PAGER env var.
    /// Defaults to `less -FRX` on Unix, `more` on Windows.
    /// Set PAGER="" to disable without this flag.
    #[cfg(feature = "terminal")]
    #[arg(long)]
    pub no_pager: bool,

    /// Open the output file(s) after conversion
    ///
    /// Uses the system's default application to open generated files.
    /// For HTML output, this typically opens a web browser.
    /// Ignored when output is stdout (`-o -`).
    #[arg(long)]
    pub open: bool,
}

pub fn run(args: &Args) -> miette::Result<()> {
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

    // Parse output destination from --out-file argument
    let output_destination = args
        .out_file
        .as_ref()
        .map_or(OutputDestination::Derived, |s| {
            if s == "-" {
                OutputDestination::Stdout
            } else {
                OutputDestination::File(PathBuf::from(s))
            }
        });

    let options = Options::builder()
        .generator_metadata(GeneratorMetadata::new(
            env!("CARGO_BIN_NAME"),
            env!("CARGO_PKG_VERSION"),
        ))
        .doctype(doctype)
        .safe_mode(safe_mode)
        .timings(args.timings)
        .embedded(args.embedded)
        .output_destination(output_destination.clone())
        .backend(args.backend)
        .build();

    let result = match args.backend {
        #[cfg(feature = "html")]
        Backend::Html | Backend::Html5s => run_processor::<acdc_converters_html::Processor>(
            args,
            options,
            document_attributes,
            true,
        )
        .map_err(|e| error::display(&e)),

        #[cfg(feature = "terminal")]
        Backend::Terminal => {
            // Terminal outputs to stdout with optional pager support
            run_terminal_with_pager(args, options, document_attributes)
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

        #[cfg(feature = "markdown")]
        Backend::Markdown => {
            // Markdown outputs to separate files - can process in parallel
            run_processor::<acdc_converters_markdown::Processor>(
                args,
                options,
                document_attributes,
                true,
            )
            .map_err(|e| error::display(&e))
        }

        // Catch-all for backends not compiled in
        #[allow(unreachable_patterns)]
        backend => Err(Report::msg(format!(
            "backend '{backend}' is not available - rebuild with the '{backend}' feature enabled"
        ))),
    };

    if args.open && result.is_ok() {
        open_output_files(args, &output_destination);
    }

    result
}

fn open_output_files(args: &Args, output_destination: &OutputDestination) {
    let paths: Vec<PathBuf> = match output_destination {
        OutputDestination::Stdout => {
            tracing::warn!("--open ignored when output is stdout");
            eprintln!("Warning: --open ignored when output is stdout");
            return;
        }
        OutputDestination::File(path) => vec![path.clone()],
        OutputDestination::Derived => {
            let ext = match args.backend {
                Backend::Html | Backend::Html5s => "html",
                Backend::Markdown => "md",
                Backend::Manpage => return,
                Backend::Terminal => {
                    tracing::warn!("--open ignored for terminal backend");
                    eprintln!("Warning: --open ignored for terminal backend");
                    return;
                }
            };
            args.files.iter().map(|f| f.with_extension(ext)).collect()
        }
    };

    for path in &paths {
        if let Err(error) = open::that(path) {
            tracing::error!(%error, path = %path.display(), "could not open output file");
            eprintln!("Warning: could not open {}: {error}", path.display());
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
    P: Converter,
    P::Error: Send + 'static + From<acdc_parser::Error>,
{
    if !args.stdin && args.files.is_empty() {
        tracing::error!("You must pass at least one file to this processor");
        std::process::exit(1);
    }

    // Handle stdin separately (no parallelization)
    if args.stdin {
        let processor = P::new(base_options.clone(), document_attributes.clone());
        let parser_options =
            build_parser_options(args, &base_options, processor.document_attributes().clone());
        let stdin = std::io::stdin();
        let mut reader = std::io::BufReader::new(stdin.lock());
        let doc = acdc_parser::parse_from_reader(&mut reader, &parser_options)?;
        return processor.convert(&doc, None);
    }

    // When --out-file is specified with multiple files, only process the first file
    // (matches asciidoctor behavior)
    let files_to_process: &[PathBuf] = match (args.out_file.as_ref(), args.files.as_slice()) {
        (Some(_), [first, _, ..]) => {
            eprintln!(
                "Warning: --out-file specified with multiple input files; only processing first file"
            );
            std::slice::from_ref(first)
        }
        _ => &args.files,
    };

    // Single-file fast path: skip rayon thread pool overhead entirely
    if let [file] = files_to_process {
        let parser_options = build_parser_options(args, &base_options, document_attributes.clone());
        let parse_result = if base_options.timings() {
            let now = std::time::Instant::now();
            let result = acdc_parser::parse_file(file, &parser_options);
            let elapsed = now.elapsed();
            if result.is_ok() {
                use acdc_converters_core::PrettyDuration;
                eprintln!("  Parsed {} in {}", file.display(), elapsed.pretty_print());
            }
            result
        } else {
            acdc_parser::parse_file(file, &parser_options)
        };
        let processor = P::new(base_options, document_attributes);
        let convert_result = match parse_result {
            Ok(doc) => processor.convert(&doc, Some(file)),
            Err(e) => Err(e.into()),
        };
        report_errors(std::iter::once((file.clone(), convert_result)));
        return Ok(());
    }

    run_multi_file::<P>(
        args,
        &base_options,
        document_attributes,
        files_to_process,
        parallelize,
    );
    Ok(())
}

fn run_multi_file<P>(
    args: &Args,
    base_options: &Options,
    document_attributes: DocumentAttributes,
    files_to_process: &[PathBuf],
    parallelize: bool,
) where
    P: Converter,
    P::Error: Send + 'static + From<acdc_parser::Error>,
{
    let show_timings = base_options.timings();
    let multi_file = files_to_process.len() > 1;
    let wall_clock_start = show_timings.then(std::time::Instant::now);

    // Parse all files in parallel, collecting durations when timing
    let parse_results: Vec<(
        PathBuf,
        Result<acdc_parser::Document, acdc_parser::Error>,
        Option<Duration>,
    )> = files_to_process
        .par_iter()
        .map(|file| {
            let parser_options =
                build_parser_options(args, base_options, document_attributes.clone());

            if show_timings {
                let now = std::time::Instant::now();
                let result = acdc_parser::parse_file(file, &parser_options);
                let elapsed = now.elapsed();
                (file.clone(), result, Some(elapsed))
            } else {
                let result = acdc_parser::parse_file(file, &parser_options);
                (file.clone(), result, None)
            }
        })
        .collect();

    // Convert documents, timing each conversion from the CLI side.
    //
    // For multi-file + timings: suppress the converter's per-file timing output since
    // we'll print a summary table instead.
    let converter_options = if show_timings && multi_file {
        Options::builder()
            .generator_metadata(base_options.generator_metadata().clone())
            .doctype(base_options.doctype())
            .safe_mode(base_options.safe_mode())
            .timings(false)
            .embedded(base_options.embedded())
            .output_destination(base_options.output_destination().clone())
            .backend(base_options.backend())
            .build()
    } else {
        base_options.clone()
    };

    let file_results: Vec<FileResult<P::Error>> = if parallelize {
        parse_results
            .into_par_iter()
            .map(|(file, parse_result, parse_dur)| {
                let processor = P::new(converter_options.clone(), document_attributes.clone());
                let now = std::time::Instant::now();
                let result = match parse_result {
                    Ok(doc) => processor.convert(&doc, Some(&file)),
                    Err(e) => Err(e.into()),
                };
                let convert_dur = show_timings.then(|| now.elapsed());
                FileResult {
                    path: file,
                    result,
                    parse_dur,
                    convert_dur,
                }
            })
            .collect()
    } else {
        let processor = P::new(converter_options, document_attributes);
        parse_results
            .into_iter()
            .map(|(file, parse_result, parse_dur)| {
                let now = std::time::Instant::now();
                let result = match parse_result {
                    Ok(doc) => processor.convert(&doc, Some(&file)),
                    Err(e) => Err(e.into()),
                };
                let convert_dur = show_timings.then(|| now.elapsed());
                FileResult {
                    path: file,
                    result,
                    parse_dur,
                    convert_dur,
                }
            })
            .collect()
    };

    if show_timings && multi_file {
        let wall_clock = wall_clock_start.map(|s| s.elapsed());
        let timing_entries: Vec<_> = file_results
            .iter()
            .filter_map(|fr| {
                Some(TimingEntry {
                    path: fr.path.clone(),
                    parse: fr.parse_dur?,
                    convert: fr.convert_dur?,
                })
            })
            .collect();
        print_timing_table(&timing_entries, wall_clock);
    }

    report_errors(file_results.into_iter().map(|fr| (fr.path, fr.result)));
}

struct FileResult<E> {
    path: PathBuf,
    result: Result<(), E>,
    parse_dur: Option<Duration>,
    convert_dur: Option<Duration>,
}

fn report_errors<E: std::error::Error + 'static>(
    results: impl Iterator<Item = (PathBuf, Result<(), E>)>,
) {
    let errors: Vec<_> = results
        .filter_map(|(file, result)| result.err().map(|e| (file, e)))
        .collect();

    if !errors.is_empty() {
        eprintln!("\nFailed to process {} file(s):", errors.len());
        for (idx, (file, err)) in errors.iter().enumerate() {
            eprintln!("\n{}. File: {}", idx + 1, file.display());
            let report = error::display(err);
            eprintln!("{report:?}");
        }
        std::process::exit(1);
    }
}

/// Spawn a pager process, returning the child process.
/// Returns None if pager is disabled, unavailable, or stdout is not a TTY.
///
/// Uses shell interpretation for the pager command (like git), allowing:
/// - Paths with spaces: `"/Program Files/Git/usr/bin/less.exe" -FRX`
/// - Complex commands: `less -R | head -100`
///
/// Platform defaults:
/// - Unix: `less -FRX` (quit if fits, raw ANSI, don't clear)
/// - Windows: `more` (built-in, always available)
///
/// On Unix, sets `LESSCHARSET=utf-8` if not already defined to ensure
/// proper UTF-8 display in less.
#[cfg(feature = "terminal")]
fn spawn_pager(no_pager: bool) -> Option<std::process::Child> {
    use std::io::IsTerminal;

    // Platform-specific defaults
    #[cfg(windows)]
    const DEFAULT_PAGER: &str = "more";
    #[cfg(not(windows))]
    const DEFAULT_PAGER: &str = "less -FRX";

    // Skip if disabled or not a TTY
    if no_pager || !std::io::stdout().is_terminal() {
        return None;
    }

    // Check PAGER env var, use platform default if not set
    // Empty PAGER means no pager
    let pager_cmd = std::env::var("PAGER").unwrap_or_else(|_| DEFAULT_PAGER.to_string());
    if pager_cmd.is_empty() {
        return None;
    }

    // Use shell to interpret the command (like git does)
    // This handles paths with spaces, quoted arguments, and complex commands
    #[cfg(windows)]
    {
        std::process::Command::new("cmd")
            .args(["/c", &pager_cmd])
            .stdin(std::process::Stdio::piped())
            .spawn()
            .ok()
    }
    #[cfg(not(windows))]
    {
        let mut cmd = std::process::Command::new("sh");
        cmd.args(["-c", &pager_cmd])
            .stdin(std::process::Stdio::piped());

        // Set LESSCHARSET=utf-8 for proper UTF-8 display in less
        //
        // Only set if not already defined (respect user preferences)
        if std::env::var("LESSCHARSET").is_err() {
            cmd.env("LESSCHARSET", "utf-8");
        }

        cmd.spawn()
            .inspect_err(|error| tracing::error!(%error, %pager_cmd, "Could not spawn the pager"))
            .ok()
    }
}

/// Run terminal converter with optional pager support.
/// When stdout is a TTY and pager is not disabled, pipes output through a pager.
#[cfg(feature = "terminal")]
fn run_terminal_with_pager(
    args: &Args,
    base_options: Options,
    document_attributes: DocumentAttributes,
) -> Result<(), acdc_converters_terminal::Error> {
    use std::io::BufWriter;

    use acdc_converters_terminal::Processor;

    if !args.stdin && args.files.is_empty() {
        tracing::error!("You must pass at least one file to this processor");
        std::process::exit(1);
    }

    // Check if --out-file specifies a file (not stdout)
    // If so, write directly to file without pager
    let output_to_file = args.out_file.as_ref().is_some_and(|s| s != "-");

    // Handle stdin separately
    if args.stdin {
        let processor = Processor::new(base_options.clone(), document_attributes.clone());
        let parser_options =
            build_parser_options(args, &base_options, processor.document_attributes().clone());
        let stdin = std::io::stdin();
        let mut reader = std::io::BufReader::new(stdin.lock());
        let doc = acdc_parser::parse_from_reader(&mut reader, &parser_options)?;

        // If writing to file, use the processor's convert method (respects output_path)
        if output_to_file {
            return processor.convert(&doc, None);
        }

        // Try pager for stdin too
        if let Some(mut pager) = spawn_pager(args.no_pager) {
            if let Some(pager_stdin) = pager.stdin.take() {
                let writer = BufWriter::new(pager_stdin);
                processor.write_to(&doc, writer, None)?;
            }
            let _ = pager.wait()?;
            return Ok(());
        }
        return processor.convert(&doc, None);
    }

    // When --out-file is specified with multiple files, only process the first file
    let files_to_process: &[PathBuf] = match (args.out_file.as_ref(), args.files.as_slice()) {
        (Some(_), [first, _, ..]) => {
            eprintln!(
                "Warning: --out-file specified with multiple input files; only processing first file"
            );
            std::slice::from_ref(first)
        }
        _ => &args.files,
    };

    // Parse all files in parallel
    let parse_results: Vec<(
        std::path::PathBuf,
        Result<acdc_parser::Document, acdc_parser::Error>,
    )> = files_to_process
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

    let processor = Processor::new(base_options, document_attributes);

    // If writing to file, use the processor's convert method (respects output_path)
    if output_to_file {
        for (file, parse_result) in parse_results {
            match parse_result {
                Ok(doc) => processor.convert(&doc, Some(&file))?,
                Err(e) => return Err(e.into()),
            }
        }
        return Ok(());
    }

    // Try to spawn pager
    if let Some(mut pager) = spawn_pager(args.no_pager) {
        if let Some(pager_stdin) = pager.stdin.take() {
            let mut writer = BufWriter::new(pager_stdin);
            for (_file, parse_result) in parse_results {
                match parse_result {
                    Ok(doc) => processor.write_to(&doc, &mut writer, None)?,
                    Err(e) => return Err(e.into()),
                }
            }
            drop(writer); // Flush and close stdin
        }
        // Wait for pager, ignore exit status (user may quit with 'q')
        let _ = pager.wait()?;
    } else {
        // No pager - use convert() which writes to stdout
        for (file, parse_result) in parse_results {
            match parse_result {
                Ok(doc) => processor.convert(&doc, Some(&file))?,
                Err(e) => return Err(e.into()),
            }
        }
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
