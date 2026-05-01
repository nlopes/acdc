use std::{
    borrow::Cow,
    io::BufReader,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use acdc_converters_core::{
    ConversionResult, Converter, Doctype, GeneratorMetadata, Options, OutputDestination,
};
#[cfg(feature = "html")]
use acdc_converters_html::HtmlVariant;
#[cfg(feature = "markdown")]
use acdc_converters_markdown::MarkdownVariant;
use acdc_parser::{AttributeValue, DocumentAttributes, ParseResult, SafeMode};
use clap::{ArgAction, Args as ClapArgs};
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
    ///
    /// `html` is the default when its feature is compiled in; otherwise
    /// `--backend` must be supplied explicitly.
    #[arg(short = 'b', long, value_enum)]
    #[cfg_attr(feature = "html", arg(default_value = "html"))]
    pub backend: BackendArg,

    /// Backend output variant
    ///
    /// Selects an alternative output style for the chosen backend.
    ///
    /// Accepted values per backend:
    ///   - html:     standard (default), semantic
    ///   - markdown: commonmark, gfm (default)
    ///
    /// `--backend html5s` is preserved as a shortcut for
    /// `--backend html --variant semantic` and rejects `--variant`.
    #[arg(long, value_enum, verbatim_doc_comment)]
    pub variant: Option<VariantArg>,

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

impl Args {
    fn output_destination(&self) -> OutputDestination {
        self.out_file
            .as_ref()
            .map_or(OutputDestination::Derived, |s| {
                if s == "-" {
                    OutputDestination::Stdout
                } else {
                    OutputDestination::File(PathBuf::from(s))
                }
            })
    }
}

pub fn run(args: &Args) -> miette::Result<()> {
    let backend = args.backend.resolve(args.variant)?;

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
            let doctype = if matches!(backend, Backend::Manpage) {
                // Set doctype attribute for the parser (parser checks this to derive manpage attrs)
                document_attributes
                    .insert("doctype".into(), AttributeValue::String("manpage".into()));
                Doctype::Manpage
            } else {
                args.doctype
            };
            (document_attributes, doctype)
        }
        #[cfg(not(feature = "manpage"))]
        {
            let document_attributes = build_attributes_map(&args.attributes);
            (document_attributes, args.doctype)
        }
    };

    let output_destination = args.output_destination();

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
        .build();

    let output_paths = match backend {
        #[cfg(feature = "html")]
        Backend::Html(variant) => run_processor::<acdc_converters_html::Processor, _>(
            args,
            options,
            document_attributes,
            true,
            move |opts, attrs| {
                acdc_converters_html::Processor::new(opts, attrs).with_variant(variant)
            },
        )
        .map_err(|e| error::display(&e)),

        #[cfg(feature = "terminal")]
        Backend::Terminal => {
            // Terminal outputs to stdout with optional pager support
            run_terminal_with_pager(args, options, document_attributes)
                .map_err(|e| error::display(&e))
        }

        #[cfg(feature = "manpage")]
        Backend::Manpage => run_processor::<acdc_converters_manpage::Processor, _>(
            args,
            options,
            document_attributes,
            true,
            acdc_converters_manpage::Processor::new,
        )
        .map_err(|e| error::display(&e)),

        #[cfg(feature = "markdown")]
        Backend::Markdown(variant) => run_processor::<acdc_converters_markdown::Processor, _>(
            args,
            options,
            document_attributes,
            true,
            move |opts, attrs| {
                acdc_converters_markdown::Processor::new(opts, attrs).with_variant(variant)
            },
        )
        .map_err(|e| error::display(&e)),
    };

    let output_paths = output_paths?;

    if args.open {
        open_output_files(&output_paths, &output_destination, |path| open::that(path));
    }

    Ok(())
}

fn open_output_files<E>(
    paths: &[PathBuf],
    output_destination: &OutputDestination,
    mut opener: impl FnMut(&Path) -> Result<(), E>,
) where
    E: std::fmt::Display,
{
    if matches!(output_destination, OutputDestination::Stdout) {
        tracing::warn!("--open ignored when output is stdout");
        eprintln!("Warning: --open ignored when output is stdout");
        return;
    }

    if paths.is_empty() {
        tracing::warn!("--open ignored because conversion produced no output file");
        eprintln!("Warning: --open ignored because conversion produced no output file");
        return;
    }

    for path in paths {
        if let Err(error) = opener(path) {
            tracing::error!(%error, path = %path.display(), "could not open output file");
            eprintln!("Warning: could not open {}: {error}", path.display());
        }
    }
}

/// Run a converter against the input(s). `make_processor` builds each
/// `P` from the per-call options and attributes, letting the caller
/// plumb backend-specific knobs (e.g. `with_variant`) without forcing
/// them into the `Converter` trait or `Options`. Variant-less backends
/// pass `P::new` directly.
#[tracing::instrument(skip(base_options, document_attributes, make_processor))]
fn run_processor<P, F>(
    args: &Args,
    base_options: Options,
    document_attributes: DocumentAttributes<'static>,
    parallelize: bool,
    make_processor: F,
) -> Result<Vec<PathBuf>, P::Error>
where
    P: Converter<'static>,
    P::Error: Send + 'static + From<acdc_parser::Error>,
    F: Fn(Options, DocumentAttributes<'static>) -> P + Send + Sync + Copy,
{
    if !args.stdin && args.files.is_empty() {
        tracing::error!("You must pass at least one file to this processor");
        std::process::exit(1);
    }

    // Handle stdin separately (no parallelization)
    if args.stdin {
        let processor = make_processor(base_options.clone(), document_attributes.clone());
        let parser_options =
            build_parser_options(args, &base_options, processor.document_attributes().clone());
        let stdin = std::io::stdin();
        let mut reader = BufReader::new(stdin.lock());
        let parsed = acdc_parser::parse_from_reader(&mut reader, &parser_options)?;
        // The CLI is a one-shot tool — leak the parsed document so its
        // arena lives for the rest of the process, giving us the `'static`
        // lifetime that `Converter<'static>` expects.
        let parsed = report_warnings(parsed, None);
        return processor
            .convert(parsed.document(), None)
            .map(output_paths_from_result);
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
            let now = Instant::now();
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
        let processor = make_processor(base_options, document_attributes);
        let convert_result = match parse_result {
            Ok(parsed) => {
                // Leak into 'static: CLI is a one-shot process.
                let parsed = report_warnings(parsed, Some(file));
                processor.convert(parsed.document(), Some(file))
            }
            Err(e) => Err(e.into()),
        };
        return Ok(report_errors(std::iter::once((
            file.clone(),
            convert_result,
        ))));
    }

    Ok(run_multi_file::<P, _>(
        args,
        &base_options,
        &document_attributes,
        files_to_process,
        parallelize,
        make_processor,
    ))
}

fn run_multi_file<P, F>(
    args: &Args,
    base_options: &Options,
    document_attributes: &DocumentAttributes<'static>,
    files_to_process: &[PathBuf],
    parallelize: bool,
    make_processor: F,
) -> Vec<PathBuf>
where
    P: Converter<'static>,
    P::Error: Send + 'static + From<acdc_parser::Error>,
    F: Fn(Options, DocumentAttributes<'static>) -> P + Send + Sync + Copy,
{
    let show_timings = base_options.timings();
    let multi_file = files_to_process.len() > 1;
    let wall_clock_start = show_timings.then(Instant::now);

    // Parse all files in parallel, collecting durations when timing.
    let parse_results: Vec<TimedParseResult> = files_to_process
        .par_iter()
        .map(|file| {
            let parser_options =
                build_parser_options(args, base_options, document_attributes.clone());

            if show_timings {
                let now = Instant::now();
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
            .build()
    } else {
        base_options.clone()
    };

    let file_results: Vec<FileResult<P::Error>> = if parallelize {
        parse_results
            .into_par_iter()
            .map(|entry| {
                convert_parse_result::<P, _>(
                    entry,
                    &converter_options,
                    document_attributes,
                    show_timings,
                    make_processor,
                )
            })
            .collect()
    } else {
        parse_results
            .into_iter()
            .map(|entry| {
                convert_parse_result::<P, _>(
                    entry,
                    &converter_options,
                    document_attributes,
                    show_timings,
                    make_processor,
                )
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

    report_errors(file_results.into_iter().map(|fr| (fr.path, fr.result)))
}

fn convert_parse_result<P, F>(
    (file, parse_result, parse_dur): TimedParseResult,
    converter_options: &Options,
    document_attributes: &DocumentAttributes<'static>,
    show_timings: bool,
    make_processor: F,
) -> FileResult<P::Error>
where
    P: Converter<'static>,
    P::Error: From<acdc_parser::Error>,
    F: Fn(Options, DocumentAttributes<'static>) -> P,
{
    let processor = make_processor(converter_options.clone(), document_attributes.clone());
    let now = Instant::now();
    let result = match parse_result {
        Ok(parsed) => {
            let parsed = report_warnings(parsed, Some(&file));
            processor.convert(parsed.document(), Some(&file))
        }
        Err(e) => Err(e.into()),
    };
    let convert_dur = show_timings.then(|| now.elapsed());

    FileResult {
        path: file,
        result,
        parse_dur,
        convert_dur,
    }
}

struct FileResult<E> {
    path: PathBuf,
    result: Result<ConversionResult, E>,
    parse_dur: Option<Duration>,
    convert_dur: Option<Duration>,
}

/// A parsed document paired with its source path and optional parse timing.
/// Used by the timing-aware multi-file path.
type TimedParseResult = (
    PathBuf,
    Result<ParseResult, acdc_parser::Error>,
    Option<Duration>,
);

/// Render the parser warnings to stderr with miette's rich-diagnostic
/// treatment (colored squiggles under the offending span, source snippet,
/// advice line), then leak the `ParseResult` so its arena outlives the process.
/// The CLI is one-shot, so leaking is acceptable and gives converters the
/// `'static` lifetime they expect.
///
/// Terminal pager paths can't use this because they need to leak before the
/// pager opens but print after it closes (otherwise stderr is buried under
/// the pager's screen takeover). Those paths inline `Box::leak` and call
/// [`print_parser_warnings`] after `pager.wait()` instead.
fn report_warnings(parsed: ParseResult, file: Option<&Path>) -> &'static ParseResult {
    let parsed = Box::leak(Box::new(parsed));
    print_parser_warnings(parsed, file);
    parsed
}

/// Render parser warnings to stderr without leaking.
fn print_parser_warnings(parsed: &ParseResult, file: Option<&Path>) {
    for warning in parsed.warnings() {
        eprintln!("{:?}", error::parser_warning_report(warning, file));
    }
}

/// Render converter warnings to stderr with the same miette treatment.
fn render_converter_warnings(
    warnings: impl IntoIterator<Item = acdc_converters_core::Warning>,
    file: Option<&Path>,
) {
    for warning in warnings {
        eprintln!("{:?}", error::converter_warning_report(&warning, file));
    }
}

fn report_errors<E: std::error::Error + 'static>(
    results: impl Iterator<Item = (PathBuf, Result<ConversionResult, E>)>,
) -> Vec<PathBuf> {
    let mut output_paths = Vec::new();
    let mut errors = Vec::new();

    for (file, result) in results {
        match result {
            Ok(result) => {
                let (output_path, warnings) = result.into_parts();
                render_converter_warnings(warnings, Some(&file));
                if let Some(output_path) = output_path {
                    output_paths.push(output_path);
                }
            }
            Err(error) => errors.push((file, error)),
        }
    }

    if !errors.is_empty() {
        eprintln!("\nFailed to process {} file(s):", errors.len());
        for (idx, (file, err)) in errors.iter().enumerate() {
            eprintln!("\n{}. File: {}", idx + 1, file.display());
            let report = error::display(err);
            eprintln!("{report:?}");
        }
        std::process::exit(1);
    }

    output_paths
}

fn output_paths_from_result(result: ConversionResult) -> Vec<PathBuf> {
    let (output_path, warnings) = result.into_parts();
    render_converter_warnings(warnings, None);
    output_path.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;

    use super::*;

    #[test]
    fn opens_reported_paths_for_derived_outputs() {
        let paths = vec![PathBuf::from("doc.html"), PathBuf::from("other.md")];
        let mut opened = Vec::new();

        open_output_files(&paths, &OutputDestination::Derived, |path| {
            opened.push(path.to_path_buf());
            Ok::<(), Infallible>(())
        });

        assert_eq!(opened, paths);
    }

    #[test]
    fn opens_reported_path_for_explicit_output_file() {
        let paths = vec![PathBuf::from("custom.out")];
        let destination = OutputDestination::File(PathBuf::from("custom.out"));
        let mut opened = Vec::new();

        open_output_files(&paths, &destination, |path| {
            opened.push(path.to_path_buf());
            Ok::<(), Infallible>(())
        });

        assert_eq!(opened, paths);
    }

    #[test]
    fn opens_dynamic_manpage_style_output_path() {
        let paths = vec![PathBuf::from("cmd.7")];
        let mut opened = Vec::new();

        open_output_files(&paths, &OutputDestination::Derived, |path| {
            opened.push(path.to_path_buf());
            Ok::<(), Infallible>(())
        });

        assert_eq!(opened, paths);
    }

    #[test]
    fn skips_stdout_output() {
        let paths = vec![PathBuf::from("doc.html")];
        let mut opened = Vec::new();

        open_output_files(&paths, &OutputDestination::Stdout, |path| {
            opened.push(path.to_path_buf());
            Ok::<(), Infallible>(())
        });

        assert!(opened.is_empty());
    }

    #[test]
    fn skips_when_conversion_produced_no_file() {
        let mut opened = Vec::new();

        open_output_files(&[], &OutputDestination::Derived, |path| {
            opened.push(path.to_path_buf());
            Ok::<(), Infallible>(())
        });

        assert!(opened.is_empty());
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

/// A parsed document paired with its source path. Used by the terminal
/// backend's parallel path, which doesn't track per-file timings.
#[cfg(feature = "terminal")]
type ParseResultEntry = (PathBuf, Result<ParseResult, acdc_parser::Error>);

/// Run terminal converter with optional pager support.
/// When stdout is a TTY and pager is not disabled, pipes output through a pager.
#[cfg(feature = "terminal")]
fn run_terminal_stdin(
    args: &Args,
    base_options: &Options,
    document_attributes: DocumentAttributes<'static>,
    output_to_file: bool,
) -> Result<Vec<PathBuf>, acdc_converters_terminal::Error> {
    use std::io::BufWriter;

    use acdc_converters_terminal::Processor;

    let processor = Processor::new(base_options.clone(), document_attributes);
    let parser_options =
        build_parser_options(args, base_options, processor.document_attributes().clone());
    let stdin = std::io::stdin();
    let mut reader = BufReader::new(stdin.lock());
    let parsed = acdc_parser::parse_from_reader(&mut reader, &parser_options)?;

    // If writing to file, use the processor's convert method (respects output_path)
    if output_to_file {
        let parsed = report_warnings(parsed, None);
        return processor
            .convert(parsed.document(), None)
            .map(output_paths_from_result);
    }

    // Try pager. The pager's screen takeover would visually bury anything
    // we eprintln! before it exits, so we leak the parsed doc up front but
    // defer the warning print until after pager.wait().
    if let Some(mut pager) = spawn_pager(args.no_pager) {
        // Leak into 'static: CLI is a one-shot process.
        let parsed: &'static ParseResult = Box::leak(Box::new(parsed));
        let mut converter_warnings = Vec::new();
        if let Some(pager_stdin) = pager.stdin.take() {
            let writer = BufWriter::new(pager_stdin);
            let source = processor.warning_source();
            let mut diagnostics =
                acdc_converters_core::Diagnostics::new(&source, &mut converter_warnings);
            processor.write_to(parsed.document(), writer, None, None, &mut diagnostics)?;
        }
        let _ = pager.wait()?;
        print_parser_warnings(parsed, None);
        render_converter_warnings(converter_warnings, None);
        return Ok(Vec::new());
    }
    let parsed = report_warnings(parsed, None);
    let result = processor.convert(parsed.document(), None)?;
    let (_, warnings) = result.into_parts();
    render_converter_warnings(warnings, None);
    Ok(Vec::new())
}

/// Drive the terminal converter through a spawned pager.
///
/// The pager's screen takeover would visually bury anything we eprintln!
/// before it exits, so we leak each parsed doc up front, write to the pager,
/// then wait for it to close before printing parser/converter warnings.
#[cfg(feature = "terminal")]
fn run_terminal_through_pager(
    processor: &acdc_converters_terminal::Processor<'static>,
    parse_results: Vec<ParseResultEntry>,
    mut pager: std::process::Child,
) -> Result<(), acdc_converters_terminal::Error> {
    use std::io::BufWriter;

    let mut deferred: Vec<(&'static ParseResult, PathBuf)> = Vec::new();
    let mut converter_warnings = Vec::new();
    if let Some(pager_stdin) = pager.stdin.take() {
        let mut writer = BufWriter::new(pager_stdin);
        let source = processor.warning_source();
        let mut diagnostics =
            acdc_converters_core::Diagnostics::new(&source, &mut converter_warnings);
        for (file, parse_result) in parse_results {
            let parsed: &'static ParseResult = Box::leak(Box::new(parse_result?));
            processor.write_to(parsed.document(), &mut writer, None, None, &mut diagnostics)?;
            deferred.push((parsed, file));
        }
        drop(writer); // Flush and close stdin
    }
    // Wait for pager, ignore exit status (user may quit with 'q')
    let _ = pager.wait()?;
    for (parsed, file) in &deferred {
        print_parser_warnings(parsed, Some(file));
    }
    render_converter_warnings(converter_warnings, None);
    Ok(())
}

#[cfg(feature = "terminal")]
fn run_terminal_with_pager(
    args: &Args,
    base_options: Options,
    document_attributes: DocumentAttributes<'static>,
) -> Result<Vec<PathBuf>, acdc_converters_terminal::Error> {
    use acdc_converters_terminal::Processor;

    if !args.stdin && args.files.is_empty() {
        tracing::error!("You must pass at least one file to this processor");
        std::process::exit(1);
    }

    // Check if --out-file specifies a file (not stdout)
    // If so, write directly to file without pager
    let output_to_file = matches!(
        base_options.output_destination(),
        OutputDestination::File(_)
    );

    if args.stdin {
        return run_terminal_stdin(args, &base_options, document_attributes, output_to_file);
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
    let parse_results: Vec<ParseResultEntry> = files_to_process
        .par_iter()
        .map(|file| {
            let parser_options =
                build_parser_options(args, &base_options, document_attributes.clone());

            if base_options.timings() {
                let now = Instant::now();
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
        let mut output_paths = Vec::new();
        for (file, parse_result) in parse_results {
            match parse_result {
                Ok(parsed) => {
                    let parsed = report_warnings(parsed, Some(&file));
                    if let Some(output_path) = processor
                        .convert(parsed.document(), Some(&file))?
                        .into_output_path()
                    {
                        output_paths.push(output_path);
                    }
                }
                Err(e) => return Err(e.into()),
            }
        }
        return Ok(output_paths);
    }

    // Try to spawn pager.
    if let Some(pager) = spawn_pager(args.no_pager) {
        run_terminal_through_pager(&processor, parse_results, pager)?;
    } else {
        // No pager - use convert() which writes to stdout
        for (file, parse_result) in parse_results {
            match parse_result {
                Ok(parsed) => {
                    let parsed = report_warnings(parsed, Some(&file));
                    let result = processor.convert(parsed.document(), Some(&file))?;
                    let (_, warnings) = result.into_parts();
                    render_converter_warnings(warnings, Some(&file));
                }
                Err(e) => return Err(e.into()),
            }
        }
    }

    Ok(Vec::new())
}

/// CLI-local enum mirroring the surface form of `--backend`.
///
/// Each arm is feature-gated so disabling a converter cleanly removes
/// it from the parser, the resolver, and the dispatch match. `Html5s`
/// stays in this enum (rather than being normalised to `Html`) so the
/// resolver can reject the contradictory `--backend html5s --variant
/// <anything>` form before lowering it to the typed [`Backend`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum BackendArg {
    #[cfg(feature = "html")]
    Html,
    #[cfg(feature = "html")]
    Html5s,
    #[cfg(feature = "manpage")]
    Manpage,
    #[cfg(feature = "terminal")]
    Terminal,
    #[cfg(feature = "markdown")]
    #[value(alias = "md")]
    Markdown,
}

impl std::fmt::Display for BackendArg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            #[cfg(feature = "html")]
            Self::Html => f.write_str("html"),
            #[cfg(feature = "html")]
            Self::Html5s => f.write_str("html5s"),
            #[cfg(feature = "manpage")]
            Self::Manpage => f.write_str("manpage"),
            #[cfg(feature = "terminal")]
            Self::Terminal => f.write_str("terminal"),
            #[cfg(feature = "markdown")]
            Self::Markdown => f.write_str("markdown"),
        }
    }
}

/// CLI-local variant choice, parsed from `--variant` before being combined
/// with the backend into the typed [`Backend`]. Each arm is feature-gated
/// so disabling its converter removes its variant names from `--variant`
/// entirely.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum VariantArg {
    #[cfg(feature = "html")]
    Standard,
    #[cfg(feature = "html")]
    Semantic,
    #[cfg(feature = "markdown")]
    #[value(name = "commonmark", alias = "cm")]
    CommonMark,
    #[cfg(feature = "markdown")]
    #[value(alias = "github", alias = "github-flavored")]
    Gfm,
}

impl std::fmt::Display for VariantArg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            #[cfg(feature = "html")]
            Self::Standard => f.write_str("standard"),
            #[cfg(feature = "html")]
            Self::Semantic => f.write_str("semantic"),
            #[cfg(feature = "markdown")]
            Self::CommonMark => f.write_str("commonmark"),
            #[cfg(feature = "markdown")]
            Self::Gfm => f.write_str("gfm"),
        }
    }
}

/// The user's `--backend`/`--variant` pair after validation, lowered into
/// the strongly-typed combination accepted by the converter crates.
///
/// Each arm carries the converter's own variant payload (or no payload
/// for backends that have none). This is the boundary at which CLI
/// surface concerns (the `html5s` alias, cross-backend mismatches) are
/// resolved — downstream code only ever sees a well-typed choice. New
/// variant-bearing backends simply gain a payload here without
/// rearranging anything else.
#[derive(Debug, Clone, Copy)]
enum Backend {
    #[cfg(feature = "html")]
    Html(HtmlVariant),
    #[cfg(feature = "manpage")]
    Manpage,
    #[cfg(feature = "terminal")]
    Terminal,
    #[cfg(feature = "markdown")]
    Markdown(MarkdownVariant),
}

/// Reject a `--variant` for a backend that doesn't define any. Interpolating
/// the backend name keeps the error consistent across no-variant backends
/// and means a future addition only needs one `resolve` arm, not a bespoke
/// error string.
#[cfg(any(feature = "manpage", feature = "terminal"))]
fn require_no_variant(backend: &'static str, variant: Option<VariantArg>) -> miette::Result<()> {
    if variant.is_some() {
        return Err(miette::miette!(
            "backend '{backend}' does not accept a variant"
        ));
    }
    Ok(())
}

impl BackendArg {
    /// Combine the CLI's separate `--backend` / `--variant` flags into a
    /// fully-typed [`Backend`], rejecting any combination the
    /// converter layer can't honour: the `html5s` alias paired with any
    /// variant, a markdown variant on the html backend (or vice versa),
    /// or any variant on a backend that has none.
    fn resolve(self, variant: Option<VariantArg>) -> miette::Result<Backend> {
        match (self, variant) {
            // The `html5s` alias is the surface spelling of "html + semantic".
            // Pairing it with `--variant` would be self-contradicting.
            #[cfg(feature = "html")]
            (Self::Html5s, Some(_)) => Err(miette::miette!(
                "--backend html5s does not accept a variant — it is a backwards-compat \
                 alias for `--backend html --variant semantic`. Drop --variant, or use \
                 `--backend html` with the variant of your choice."
            )),
            #[cfg(feature = "html")]
            (Self::Html5s, None) | (Self::Html, Some(VariantArg::Semantic)) => {
                Ok(Backend::Html(HtmlVariant::Semantic))
            }
            #[cfg(feature = "html")]
            (Self::Html, None | Some(VariantArg::Standard)) => {
                Ok(Backend::Html(HtmlVariant::Standard))
            }
            #[cfg(all(feature = "html", feature = "markdown"))]
            (Self::Html, Some(v @ (VariantArg::CommonMark | VariantArg::Gfm))) => Err(
                miette::miette!("variant '{v}' is not supported by backend 'html'"),
            ),
            #[cfg(feature = "markdown")]
            (Self::Markdown, None) => Ok(Backend::Markdown(MarkdownVariant::default())),
            #[cfg(feature = "markdown")]
            (Self::Markdown, Some(VariantArg::CommonMark)) => {
                Ok(Backend::Markdown(MarkdownVariant::CommonMark))
            }
            #[cfg(feature = "markdown")]
            (Self::Markdown, Some(VariantArg::Gfm)) => {
                Ok(Backend::Markdown(MarkdownVariant::GitHubFlavored))
            }
            #[cfg(all(feature = "markdown", feature = "html"))]
            (Self::Markdown, Some(v @ (VariantArg::Standard | VariantArg::Semantic))) => Err(
                miette::miette!("variant '{v}' is not supported by backend 'markdown'"),
            ),
            #[cfg(feature = "manpage")]
            (Self::Manpage, v) => require_no_variant("manpage", v).map(|()| Backend::Manpage),
            #[cfg(feature = "terminal")]
            (Self::Terminal, v) => require_no_variant("terminal", v).map(|()| Backend::Terminal),
        }
    }
}

fn build_attributes_map(values: &[String]) -> DocumentAttributes<'static> {
    // Start with rendering defaults (from converters/core)
    // CLI-provided attributes will override these defaults
    let mut map = acdc_converters_core::default_rendering_attributes();

    // Add CLI-provided attributes (these take precedence over defaults)
    for raw_attr in values {
        let (name, val): (Cow<'static, str>, AttributeValue<'static>) =
            if let Some(stripped) = raw_attr.strip_suffix('!') {
                (stripped.to_string().into(), AttributeValue::None)
            } else if let Some((name, val)) = raw_attr.split_once('=') {
                (
                    name.to_string().into(),
                    AttributeValue::String(val.to_string().into()),
                )
            } else {
                (raw_attr.clone().into(), AttributeValue::Bool(true))
            };
        map.set(name, val); // use set() to override defaults
    }
    map
}

/// Build parser options from CLI args and base options
fn build_parser_options(
    args: &Args,
    base_options: &Options,
    document_attributes: DocumentAttributes<'static>,
) -> acdc_parser::Options<'static> {
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
