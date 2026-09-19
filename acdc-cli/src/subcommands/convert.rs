use miette::Result as MietteResult;
use std::{
    borrow::Cow,
    error::Error,
    fmt::Display,
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
#[cfg(feature = "pdf")]
use acdc_converters_pdf::{PageLayout, PageSize, PdfOptions};
#[cfg(feature = "terminal")]
use acdc_converters_terminal::Error as TerminalError;
use acdc_parser::{
    AttributeValue, Options as ParserOptions, OptionsBuilder, ParseResult, SafeMode, Warning,
    parse_file,
};

use clap::{ArgAction, Args as ClapArgs, ValueEnum};
use rayon::prelude::*;

use crate::{
    error::{self, WarningReport, WarningReportContext},
    timing::{TimingEntry, render_summary},
};

type RawAttributes<'a> = std::collections::HashMap<Cow<'a, str>, AttributeValue<'a>>;

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
    #[arg(conflicts_with = "stdin", required_unless_present = "stdin")]
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
    #[cfg(any(feature = "html", feature = "markdown"))]
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

    /// Extra directory of PDF fonts (`ttf`, `otf`, `ttc`, `otc`). Repeatable.
    #[cfg(feature = "pdf")]
    #[arg(long, value_name = "DIR")]
    pub font_dir: Vec<PathBuf>,

    /// Logo image (SVG or raster) shown in the PDF running header.
    #[cfg(feature = "pdf")]
    #[arg(long, value_name = "FILE")]
    pub logo: Option<PathBuf>,

    /// Title shown in the PDF running header.
    #[cfg(feature = "pdf")]
    #[arg(long, value_name = "TEXT")]
    pub title: Option<String>,

    /// Diagonal gray PDF watermark text stamped on every page.
    #[cfg(feature = "pdf")]
    #[arg(long, value_name = "TEXT")]
    pub watermark: Option<String>,

    /// Show the current date and time in the PDF footer watermark metadata.
    #[cfg(feature = "pdf")]
    #[arg(long)]
    pub watermark_timestamp: bool,

    /// PDF page size.
    #[cfg(feature = "pdf")]
    #[arg(long, value_enum, value_name = "SIZE")]
    pub page: Option<PdfPageArg>,

    /// PDF page layout.
    #[cfg(feature = "pdf")]
    #[arg(long, value_enum, value_name = "LAYOUT")]
    pub page_layout: Option<PdfPageLayoutArg>,

    /// PDF theme YAML file. Defaults to the bundled neutral theme.
    #[cfg(feature = "pdf")]
    #[arg(long, value_name = "FILE")]
    pub theme: Option<PathBuf>,

    /// Strip PDF branding chrome (page background, header, footer).
    #[cfg(feature = "pdf")]
    #[arg(long)]
    pub plain: bool,

    /// Prepend a PDF table of contents built from headings.
    #[cfg(feature = "pdf")]
    #[arg(long)]
    pub toc: bool,

    /// Also write the generated Typst markup to this path for debugging.
    #[cfg(feature = "pdf")]
    #[arg(long, value_name = "FILE")]
    pub emit_typst: Option<PathBuf>,

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
    #[arg(long = "setext", alias = "enable-setext-compatibility")]
    pub enable_setext_compatibility: bool,

    /// Strict mode
    ///
    /// When enabled, some errors related with non-conformance (but still recoverable)
    /// will not allow conversion. For example, non-conforming manpage titles and
    /// unresolved PDF images or logos will cause conversion to fail instead of using
    /// fallback values.
    #[arg(long)]
    pub strict: bool,

    /// Suppress enclosing document structure and output an embedded document
    ///
    /// The exact wrapper content omitted depends on the selected backend.
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

    #[cfg(feature = "pdf")]
    fn has_pdf_only_options(&self) -> bool {
        !self.font_dir.is_empty()
            || self.logo.is_some()
            || self.title.is_some()
            || self.watermark.is_some()
            || self.watermark_timestamp
            || self.page.is_some()
            || self.page_layout.is_some()
            || self.theme.is_some()
            || self.plain
            || self.toc
            || self.emit_typst.is_some()
    }
}

pub fn run(args: &Args) -> MietteResult<()> {
    #[cfg(any(feature = "html", feature = "markdown"))]
    let backend = args.backend.resolve(args.variant)?;
    #[cfg(not(any(feature = "html", feature = "markdown")))]
    let backend = args.backend.resolve();

    #[cfg(feature = "pdf")]
    validate_pdf_options(args, backend)?;

    let safe_mode = if args.safe {
        SafeMode::Safe
    } else {
        args.safe_mode
    };

    #[cfg(feature = "manpage")]
    let doctype = if matches!(backend, Backend::Manpage) {
        Doctype::Manpage
    } else {
        args.doctype
    };
    #[cfg(not(feature = "manpage"))]
    let doctype = args.doctype;

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

    let document_attributes = build_parser_options(args, &options);
    let output_paths = match backend {
        #[cfg(feature = "html")]
        Backend::Html(variant) => run_processor::<acdc_converters_html::Processor, _>(
            args,
            &options,
            document_attributes,
            move |opts, attrs| {
                acdc_converters_html::Processor::new_with_variant(opts, attrs, variant)
            },
        ),

        #[cfg(feature = "terminal")]
        Backend::Terminal => {
            // Terminal outputs to stdout with optional pager support
            run_terminal_with_pager(args, &options, document_attributes)
                .map_err(|e| error::display(&e))
        }

        #[cfg(feature = "manpage")]
        Backend::Manpage => run_processor::<acdc_converters_manpage::Processor, _>(
            args,
            &options,
            document_attributes,
            acdc_converters_manpage::Processor::new,
        ),

        #[cfg(feature = "markdown")]
        Backend::Markdown(variant) => run_processor::<acdc_converters_markdown::Processor, _>(
            args,
            &options,
            document_attributes,
            move |opts, attrs| {
                acdc_converters_markdown::Processor::new(opts, attrs)
                    .map(|processor| processor.with_variant(variant))
            },
        ),

        #[cfg(feature = "pdf")]
        Backend::Pdf => {
            let pdf_options = pdf_options_from_args(args);
            run_processor::<acdc_converters_pdf::Processor, _>(
                args,
                &options,
                document_attributes,
                move |opts, attrs| {
                    acdc_converters_pdf::Processor::new(opts, attrs)
                        .map(|processor| processor.with_pdf_options(pdf_options.clone()))
                },
            )
        }
    };

    let output_paths = output_paths?;

    if args.open {
        open_output_files(&output_paths, &output_destination, |path| open::that(path));
    }

    Ok(())
}

#[cfg(feature = "pdf")]
fn validate_pdf_options(args: &Args, backend: Backend) -> MietteResult<()> {
    if matches!(backend, Backend::Pdf) {
        if args.emit_typst.is_some() && !args.stdin && args.files.len() > 1 {
            return Err(miette::miette!(
                "--emit-typst can only be used with a single input file"
            ));
        }
        return Ok(());
    }

    if args.has_pdf_only_options() {
        return Err(miette::miette!(
            "PDF-only options such as --font-dir, --logo, --title, --watermark, \
             --watermark-timestamp, --page, --page-layout, --theme, --plain, --toc, and \
             --emit-typst require `--backend pdf`"
        ));
    }
    Ok(())
}

#[cfg(feature = "pdf")]
fn pdf_options_from_args(args: &Args) -> PdfOptions {
    PdfOptions {
        font_dirs: args.font_dir.clone(),
        logo: args.logo.clone(),
        title: args.title.clone(),
        watermark: args.watermark.clone(),
        watermark_timestamp: args
            .watermark_timestamp
            .then(|| chrono::Local::now().format("%Y-%m-%d %H:%M").to_string()),
        page: args.page.map(PdfPageArg::to_page_size),
        page_layout: args.page_layout.map(PdfPageLayoutArg::to_page_layout),
        page_margin: None,
        theme: args.theme.clone(),
        plain: args.plain,
        toc: args.toc,
        strict_assets: args.strict,
        emit_typst: args.emit_typst.clone(),
    }
}

fn open_output_files<E>(
    paths: &[PathBuf],
    output_destination: &OutputDestination,
    mut opener: impl FnMut(&Path) -> Result<(), E>,
) where
    E: Display,
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

fn selected_input_files(args: &Args) -> &[PathBuf] {
    match (args.out_file.as_ref(), args.files.as_slice()) {
        (Some(_), [first, _, ..]) => {
            eprintln!(
                "Warning: --out-file specified with multiple input files; only processing first file"
            );
            std::slice::from_ref(first)
        }
        _ => &args.files,
    }
}

/// Run a converter against the inputs. The factory supplies backend-specific
/// settings, such as HTML or Markdown variants.
#[tracing::instrument(skip(base_options, document_attributes, make_processor))]
fn run_processor<P, F>(
    args: &Args,
    base_options: &Options,
    document_attributes: OptionsBuilder<'static>,
    make_processor: F,
) -> MietteResult<Vec<PathBuf>>
where
    P: Converter<'static>,
    P::Error: Send + 'static,
    F: Fn(Options, OptionsBuilder<'static>) -> Result<P, P::Error> + Send + Sync,
{
    // Handle stdin separately (no parallelization)
    if args.stdin {
        let processor = make_processor(base_options.clone(), document_attributes)
            .map_err(|e| error::display(&e))?;
        let parser_options = processor.parser_options();
        let stdin = std::io::stdin();
        let mut reader = BufReader::new(stdin.lock());
        let mut parsed = acdc_parser::parse_from_reader(&mut reader, parser_options)
            .map_err(|error| error::display(&error))?;
        apply_diagrams(&mut parsed, base_options, processor.name(), None)
            .map_err(|error| error::display(&error))?;
        let parsed = parsed.report_warnings(WarningRenderContext::new());
        return processor
            .convert(parsed.document(), None)
            .map(|result| result.report(WarningRenderContext::new()))
            .map_err(|error| error::display(&error));
    }

    // When --out-file is specified with multiple files, only process the first file
    // (matches asciidoctor behavior)
    let files_to_process = selected_input_files(args);

    // Single-file fast path: skip rayon thread pool overhead entirely
    if let [file] = files_to_process {
        let processor = make_processor(base_options.clone(), document_attributes)
            .map_err(|e| error::display(&e))?;
        let parser_options = processor.parser_options();
        let parse_result = if base_options.timings() {
            let now = Instant::now();
            let result = parse_file(file, parser_options);
            let elapsed = now.elapsed();
            if result.is_ok() {
                use acdc_converters_core::PrettyDuration;
                eprintln!("  Parsed {} in {}", file.display(), elapsed.pretty_print());
            }
            result
        } else {
            parse_file(file, parser_options)
        };
        let convert_result = match parse_result {
            Ok(mut parsed) => {
                apply_diagrams(&mut parsed, base_options, processor.name(), Some(file))
                    .map_err(|error| error::display(&error))?;
                let parsed = parsed.report_warnings(WarningRenderContext::new().with_file(file));
                processor.convert(parsed.document(), Some(file))
            }
            Err(e) => Err(e.into()),
        };
        return vec![FileResult {
            path: file.clone(),
            result: convert_result,
            parser_warnings: Vec::new(),
            diagram_failure: None,
            parse_dur: None,
            convert_dur: None,
        }]
        .report();
    }

    run_multi_file::<P, _>(
        args,
        base_options,
        &document_attributes,
        files_to_process,
        make_processor,
    )
}

fn run_multi_file<P, F>(
    _args: &Args,
    base_options: &Options,
    document_attributes: &OptionsBuilder<'static>,
    files_to_process: &[PathBuf],
    make_processor: F,
) -> MietteResult<Vec<PathBuf>>
where
    P: Converter<'static>,
    P::Error: Send + 'static,
    F: Fn(Options, OptionsBuilder<'static>) -> Result<P, P::Error> + Send + Sync,
{
    let show_timings = base_options.timings();
    let multi_file = files_to_process.len() > 1;
    let wall_clock_start = show_timings.then(Instant::now);

    // Parse and convert each document in one worker so parsed arenas are bounded
    // by the active Rayon worker set rather than retained for the whole batch.
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

    let file_results: Vec<FileResult<P::Error>> = files_to_process
        .par_iter()
        .map(|file| -> Result<_, P::Error> {
            let processor = make_processor(converter_options.clone(), document_attributes.clone())?;
            let parser_options = processor.parser_options();
            let entry = if show_timings {
                let now = Instant::now();
                let result = parse_file(file, parser_options);
                (file.clone(), result, Some(now.elapsed()))
            } else {
                let result = parse_file(file, parser_options);
                (file.clone(), result, None)
            };
            Ok(convert_parse_result(entry, &processor, base_options, show_timings))
        })
        .collect::<Result<_, _>>()
        .map_err(|e| error::display(&e))?;

    if show_timings && multi_file {
        let wall_clock = wall_clock_start.map(|s| s.elapsed());
        let timing_entries: Vec<_> = file_results
            .iter()
            .filter_map(FileResult::timing_entry)
            .collect();
        render_summary(&timing_entries, wall_clock);
    }

    file_results.report()
}

fn convert_parse_result<P>(
    (file, parse_result, parse_dur): TimedParseResult,
    processor: &P,
    base_options: &Options,
    show_timings: bool,
) -> FileResult<P::Error>
where
    P: Converter<'static>,
{
    let now = Instant::now();
    let mut diagram_failure = None;
    let (result, parser_warnings) = match parse_result {
        Ok(mut parsed) => {
            let parser_warnings = parsed.take_warnings();
            // A diagram failure only reaches here when the document asked to
            // abort on one; it is carried to the reporter so this worker does
            // not print out of turn.
            diagram_failure =
                apply_diagrams(&mut parsed, base_options, processor.name(), Some(&file)).err();
            let result = processor.convert(parsed.document(), Some(&file));
            (result, parser_warnings)
        }
        Err(error) => (Err(error.into()), Vec::new()),
    };
    let convert_dur = show_timings.then(|| now.elapsed());

    FileResult {
        path: file,
        result,
        parser_warnings,
        diagram_failure,
        parse_dur,
        convert_dur,
    }
}

struct FileResult<E> {
    path: PathBuf,
    result: Result<ConversionResult, E>,
    parser_warnings: Vec<Warning>,
    /// Set only when the document asked to abort on a diagram failure.
    diagram_failure: Option<DiagramError>,
    parse_dur: Option<Duration>,
    convert_dur: Option<Duration>,
}

impl<E> FileResult<E> {
    fn timing_entry(&self) -> Option<TimingEntry> {
        Some(TimingEntry {
            path: self.path.clone(),
            parse: self.parse_dur?,
            convert: self.convert_dur?,
        })
    }
}

trait FileResultsReporter {
    fn report(self) -> MietteResult<Vec<PathBuf>>;
}

impl<E> FileResultsReporter for Vec<FileResult<E>>
where
    E: Error + 'static,
{
    fn report(self) -> MietteResult<Vec<PathBuf>> {
        let mut output_paths = Vec::new();
        let mut errors: Vec<(PathBuf, miette::Report)> = Vec::new();

        for file_result in self {
            file_result
                .parser_warnings
                .render(WarningRenderContext::new().with_file(&file_result.path));
            if let Some(failure) = &file_result.diagram_failure {
                errors.push((file_result.path.clone(), error::display(failure)));
            }
            match file_result.result {
                Ok(result) => {
                    let (output_path, warnings) = result.into_parts();
                    warnings.render(WarningRenderContext::new().with_file(&file_result.path));
                    if let Some(output_path) = output_path {
                        output_paths.push(output_path);
                    }
                }
                Err(error) => errors.push((file_result.path, error::display(&error))),
            }
        }

        if !errors.is_empty() {
            eprintln!("\nFailed to process {} file(s):", errors.len());
            for (idx, (file, report)) in errors.iter().enumerate() {
                eprintln!("\n{}. File: {}", idx + 1, file.display());
                eprintln!("{report:?}");
            }
            return Err(miette::miette!(
                "failed to process {} file(s)",
                errors.len()
            ));
        }

        Ok(output_paths)
    }
}

/// The failure type of the diagram pass, or an uninhabited stand-in when the
/// `diagram` feature is off, so the call sites need no `cfg` of their own.
#[cfg(feature = "diagram")]
type DiagramError = acdc_diagram::Error;
#[cfg(not(feature = "diagram"))]
type DiagramError = std::convert::Infallible;

/// Generate the document's diagrams, rewriting each diagram block into the
/// image it produced.
///
/// This runs between parsing and conversion, so every backend sees ordinary
/// image blocks and none of them needs to know about diagrams. Warnings are
/// rendered as they are produced; a document that sets
/// `:diagram-on-error: abort` returns its first failure instead, and the
/// conversion stops.
#[cfg(feature = "diagram")]
fn apply_diagrams(
    parsed: &mut ParseResult,
    base_options: &Options,
    backend: &'static str,
    file: Option<&Path>,
) -> Result<(), DiagramError> {
    let processor = acdc_diagram::Processor::new(diagram_options(base_options, backend, file));
    let mut warnings = Vec::new();
    let outcome = parsed
        .with_document_mut(|document, arena| processor.process(document, arena, &mut warnings));
    warnings.render(WarningRenderContext::new().with_optional_file(file));
    outcome
}

/// Tell the diagram pass where the document lives and where its output goes.
///
/// `imagesdir` and the diagram cache are resolved against the output
/// directory, which is the directory `--out-file` names, or the document's own
/// directory when the output path is derived or goes to stdout.
#[cfg(feature = "diagram")]
fn diagram_options(
    base_options: &Options,
    backend: &'static str,
    file: Option<&Path>,
) -> acdc_diagram::Options {
    fn directory_of(path: &Path) -> Option<PathBuf> {
        path.parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .map(Path::to_path_buf)
    }

    let base_dir = file
        .and_then(directory_of)
        .unwrap_or_else(|| PathBuf::from("."));
    let output_dir = match base_options.output_destination() {
        OutputDestination::File(path) => directory_of(path).unwrap_or_else(|| base_dir.clone()),
        OutputDestination::Stdout | OutputDestination::Derived => base_dir.clone(),
    };

    acdc_diagram::Options::builder()
        .base_dir(base_dir)
        .output_dir(output_dir)
        .backend(backend)
        .unsafe_mode(matches!(base_options.safe_mode(), SafeMode::Unsafe))
        .build()
}

/// No-op stand-in so the conversion paths read the same either way.
#[cfg(not(feature = "diagram"))]
#[allow(clippy::unnecessary_wraps)]
fn apply_diagrams(
    _parsed: &mut ParseResult,
    _base_options: &Options,
    _backend: &'static str,
    _file: Option<&Path>,
) -> Result<(), DiagramError> {
    Ok(())
}

/// The diagram pass for the terminal paths, whose only error channel is the
/// terminal converter's own error type.
///
/// An abort is carried through as an I/O error holding the diagram failure's
/// message, which is what the caller ends up printing.
#[cfg(feature = "terminal")]
fn apply_diagrams_as(
    parsed: &mut ParseResult,
    base_options: &Options,
    backend: &'static str,
    file: Option<&Path>,
) -> Result<(), acdc_converters_terminal::Error> {
    apply_diagrams(parsed, base_options, backend, file)
        .map_err(|error| std::io::Error::other(error.to_string()).into())
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
/// advice line). Returns the `ParseResult` by value so the caller can drive
/// the converter and drop it when the conversion finishes.
///
/// Terminal pager paths drain warnings via `parsed.take_warnings()` instead
/// — the pager's screen takeover would visually bury anything we
/// `eprintln!` before it exits, so they stash the warnings and print them
/// after `pager.wait()`.
#[derive(Debug, Clone, Copy)]
struct WarningRenderContext<'a> {
    file: Option<&'a Path>,
}

impl<'a> WarningRenderContext<'a> {
    const fn new() -> Self {
        Self { file: None }
    }

    const fn with_file(mut self, file: &'a Path) -> Self {
        self.file = Some(file);
        self
    }

    #[cfg(feature = "diagram")]
    const fn with_optional_file(mut self, file: Option<&'a Path>) -> Self {
        self.file = file;
        self
    }
}

trait ParseResultWarningReporter {
    fn report_warnings(self, context: WarningRenderContext<'_>) -> Self;
}

impl ParseResultWarningReporter for ParseResult {
    fn report_warnings(self, context: WarningRenderContext<'_>) -> Self {
        self.warnings().render(context);
        self
    }
}

trait WarningRenderer {
    fn render(&self, context: WarningRenderContext<'_>);
}

impl WarningRenderer for [Warning] {
    fn render(&self, context: WarningRenderContext<'_>) {
        let context = WarningReportContext::new().with_optional_file(context.file);
        for warning in self {
            eprintln!("{:?}", warning.to_report(context));
        }
    }
}

impl WarningRenderer for [acdc_converters_core::Warning] {
    fn render(&self, context: WarningRenderContext<'_>) {
        let context = WarningReportContext::new().with_optional_file(context.file);
        for warning in self {
            eprintln!("{:?}", warning.to_report(context));
        }
    }
}

trait ConversionResultReporter {
    fn report(self, context: WarningRenderContext<'_>) -> Vec<PathBuf>;
}

impl ConversionResultReporter for ConversionResult {
    fn report(self, context: WarningRenderContext<'_>) -> Vec<PathBuf> {
        let (output_path, warnings) = self.into_parts();
        warnings.render(context);
        output_path.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "pdf")]
    use crate::Commands;
    use std::convert::Infallible;

    #[cfg(feature = "pdf")]
    use clap::{CommandFactory, Parser};

    use super::*;

    #[cfg(feature = "pdf")]
    fn parse_pdf_args<const N: usize>(raw: [&str; N]) -> MietteResult<Args> {
        let cli = crate::Cli::try_parse_from(raw).map_err(|error| miette::miette!(error))?;
        match cli.command {
            Commands::Convert(args) => Ok(args),
            #[cfg(feature = "inspect")]
            Commands::Inspect(_) => Err(miette::miette!("test command selected inspect")),
            #[cfg(feature = "lint")]
            Commands::Lint(_) => Err(miette::miette!("test command selected lint")),
            #[cfg(feature = "tck")]
            Commands::Tck(_) => Err(miette::miette!("test command selected tck")),
        }
    }

    #[cfg(feature = "pdf")]
    #[test]
    fn pdf_feature_exposes_convert_command() {
        assert!(
            crate::Cli::command()
                .get_subcommands()
                .any(|command| command.get_name() == "convert")
        );
    }

    #[cfg(feature = "pdf")]
    #[test]
    fn maps_pdf_command_line_options() -> MietteResult<()> {
        let args = parse_pdf_args([
            "acdc",
            "convert",
            "--backend",
            "pdf",
            "--font-dir",
            "fonts/primary",
            "--font-dir",
            "fonts/secondary",
            "--logo",
            "assets/logo.svg",
            "--title",
            "Architecture",
            "--watermark",
            "Draft",
            "--watermark-timestamp",
            "--page",
            "a3",
            "--page-layout",
            "landscape",
            "--theme",
            "theme.yml",
            "--plain",
            "--toc",
            "--strict",
            "--emit-typst",
            "debug.typ",
            "document.adoc",
        ])?;
        let options = pdf_options_from_args(&args);

        assert_eq!(
            options.font_dirs,
            [
                PathBuf::from("fonts/primary"),
                PathBuf::from("fonts/secondary")
            ]
        );
        assert_eq!(options.logo, Some(PathBuf::from("assets/logo.svg")));
        assert_eq!(options.title.as_deref(), Some("Architecture"));
        assert_eq!(options.watermark.as_deref(), Some("Draft"));
        assert!(options.watermark_timestamp.is_some());
        assert_eq!(options.page, Some(PageSize::A3));
        assert_eq!(options.page_layout, Some(PageLayout::Landscape));
        assert_eq!(options.theme, Some(PathBuf::from("theme.yml")));
        assert!(options.plain);
        assert!(options.toc);
        assert!(options.strict_assets);
        assert_eq!(options.emit_typst, Some(PathBuf::from("debug.typ")));
        Ok(())
    }

    #[cfg(feature = "pdf")]
    #[test]
    fn rejects_emit_typst_for_multiple_inputs() -> MietteResult<()> {
        let args = parse_pdf_args([
            "acdc",
            "convert",
            "--backend",
            "pdf",
            "--emit-typst",
            "debug.typ",
            "first.adoc",
            "second.adoc",
        ])?;
        let backend = {
            #[cfg(any(feature = "html", feature = "markdown"))]
            {
                args.backend.resolve(args.variant)?
            }
            #[cfg(not(any(feature = "html", feature = "markdown")))]
            {
                args.backend.resolve()
            }
        };
        let Err(error) = validate_pdf_options(&args, backend) else {
            return Err(miette::miette!(
                "--emit-typst unexpectedly accepted multiple inputs"
            ));
        };

        assert!(error.to_string().contains("single input file"));
        Ok(())
    }

    #[cfg(all(feature = "pdf", feature = "html"))]
    #[test]
    fn rejects_pdf_options_for_other_backends() -> MietteResult<()> {
        let args = parse_pdf_args([
            "acdc",
            "convert",
            "--backend",
            "html",
            "--title",
            "Architecture",
            "document.adoc",
        ])?;
        let backend = args.backend.resolve(args.variant)?;
        let Err(error) = validate_pdf_options(&args, backend) else {
            return Err(miette::miette!(
                "PDF-only option unexpectedly accepted by HTML"
            ));
        };
        let message = error.to_string();

        assert!(message.contains("--title"));
        assert!(message.contains("--theme"));
        assert!(message.contains("--backend pdf"));
        Ok(())
    }

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
    fn later_cli_defaults_cancel_earlier_overrides() -> Result<(), Box<dyn Error>> {
        for values in [
            ["probe=first", "probe=last@"],
            ["probe!", "!probe=@"],
            ["probe=first", "probe@=last"],
        ] {
            let values = values.map(str::to_string);
            let options = ParserOptions::builder()
                .with_attributes(build_attribute_overrides(&values))
                .with_defaults(build_attributes_map(&values))
                .build()?;
            let parsed = acdc_parser::parse(":probe: document\n\n{probe}\n", &options)?;
            assert_eq!(
                parsed
                    .document()
                    .attributes
                    .get("probe")
                    .and_then(|value| value.text()),
                Some("document")
            );
        }
        Ok(())
    }

    #[test]
    fn cli_attribute_input_keeps_text_and_typed_numeric_values() -> Result<(), Box<dyn Error>> {
        let parsed = parse_attribute("max-include-depth=064");
        assert_eq!(parsed.value, AttributeValue::String(Cow::Borrowed("064")));

        let attributes = ParserOptions::with_attributes(build_attributes_map(&[
            "max-include-depth=064".to_string(),
        ]))?
        .into_document_attributes();
        assert_eq!(
            attributes
                .get("max-include-depth")
                .and_then(acdc_parser::DocumentAttributeValue::as_integer),
            Some(64)
        );
        assert_eq!(
            attributes
                .get("max-include-depth")
                .and_then(|value| value.text()),
            Some("064")
        );
        Ok(())
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

#[cfg(feature = "terminal")]
fn parse_terminal_file(
    base_options: &Options,
    parser_options: &ParserOptions<'static>,
    file: &Path,
) -> Result<ParseResult, acdc_parser::Error> {
    if base_options.timings() {
        let now = Instant::now();
        let result = parse_file(file, parser_options);
        if result.is_ok() {
            use acdc_converters_core::PrettyDuration;
            eprintln!(
                "  Parsed {} in {}",
                file.display(),
                now.elapsed().pretty_print()
            );
        }
        result
    } else {
        parse_file(file, parser_options)
    }
}

/// Run terminal converter with optional pager support.
/// When stdout is a TTY and pager is not disabled, pipes output through a pager.
#[cfg(feature = "terminal")]
fn run_terminal_stdin(
    args: &Args,
    base_options: &Options,
    document_attributes: OptionsBuilder<'static>,
    output_to_file: bool,
) -> Result<Vec<PathBuf>, TerminalError> {
    use std::io::BufWriter;

    use acdc_converters_terminal::Processor;

    let processor = Processor::new(base_options.clone(), document_attributes)?;
    let parser_options = processor.parser_options();
    let stdin = std::io::stdin();
    let mut reader = BufReader::new(stdin.lock());
    let mut parsed = acdc_parser::parse_from_reader(&mut reader, parser_options)?;
    apply_diagrams_as(&mut parsed, base_options, processor.name(), None)?;

    // If writing to file, use the processor's convert method (respects output_path)
    if output_to_file {
        let parsed = parsed.report_warnings(WarningRenderContext::new());
        return processor
            .convert(parsed.document(), None)
            .map(|result| result.report(WarningRenderContext::new()));
    }

    // Try pager. The pager's screen takeover would visually bury anything we
    // eprintln! before it exits, so we drain warnings up front and print them
    // after pager.wait().
    if let Some(mut pager) = spawn_pager(args.no_pager) {
        let mut parsed = parsed;
        let parser_warnings = parsed.take_warnings();
        let mut converter_warnings = Vec::new();
        if let Some(pager_stdin) = pager.stdin.take() {
            let writer = BufWriter::new(pager_stdin);
            let source = processor.warning_source();
            let mut diagnostics =
                acdc_converters_core::Diagnostics::new(&source, &mut converter_warnings);
            processor.write_to(parsed.document(), writer, None, None, &mut diagnostics)?;
        }
        // `parsed` and its arena drop here; the pager output is already in flight.
        drop(parsed);
        let _ = pager.wait()?;
        parser_warnings.render(WarningRenderContext::new());
        converter_warnings.render(WarningRenderContext::new());
        return Ok(Vec::new());
    }
    let parsed = parsed.report_warnings(WarningRenderContext::new());
    let result = processor.convert(parsed.document(), None)?;
    result.report(WarningRenderContext::new());
    Ok(Vec::new())
}

/// Drive the terminal converter through a spawned pager.
///
/// The pager's screen takeover would visually bury anything we eprintln!
/// before it exits, so for each input we drain its parser warnings up front,
/// write to the pager, drop the `ParseResult`, then print warnings after
/// `pager.wait()`.
#[cfg(feature = "terminal")]
fn run_terminal_through_pager(
    processor: &acdc_converters_terminal::Processor<'static>,
    _args: &Args,
    base_options: &Options,
    files: &[PathBuf],
    mut pager: std::process::Child,
) -> Result<(), TerminalError> {
    use std::io::BufWriter;

    let mut deferred: Vec<(Vec<Warning>, PathBuf)> = Vec::new();
    let mut converter_warnings = Vec::new();
    if let Some(pager_stdin) = pager.stdin.take() {
        let mut writer = BufWriter::new(pager_stdin);
        let source = processor.warning_source();
        let mut diagnostics =
            acdc_converters_core::Diagnostics::new(&source, &mut converter_warnings);
        for file in files {
            let mut parsed = parse_terminal_file(base_options, processor.parser_options(), file)?;
            let parser_warnings = parsed.take_warnings();
            apply_diagrams_as(&mut parsed, base_options, processor.name(), Some(file))?;
            processor.write_to(parsed.document(), &mut writer, None, None, &mut diagnostics)?;
            // `parsed` drops here — output is already buffered into `writer`.
            deferred.push((parser_warnings, file.clone()));
        }
        drop(writer); // Flush and close stdin
    }
    // Wait for pager, ignore exit status (user may quit with 'q')
    let _ = pager.wait()?;
    for (warnings, file) in &deferred {
        warnings.render(WarningRenderContext::new().with_file(file));
    }
    converter_warnings.render(WarningRenderContext::new());
    Ok(())
}

#[cfg(feature = "terminal")]
fn run_terminal_with_pager(
    args: &Args,
    base_options: &Options,
    document_attributes: OptionsBuilder<'static>,
) -> Result<Vec<PathBuf>, TerminalError> {
    use acdc_converters_terminal::Processor;

    // Check if --out-file specifies a file (not stdout)
    // If so, write directly to file without pager
    let output_to_file = matches!(
        base_options.output_destination(),
        OutputDestination::File(_)
    );

    if args.stdin {
        return run_terminal_stdin(args, base_options, document_attributes, output_to_file);
    }

    let files_to_process = selected_input_files(args);
    let processor = Processor::new(base_options.clone(), document_attributes)?;

    // If writing to file, use the processor's convert method (respects output_path)
    if output_to_file {
        let mut output_paths = Vec::new();
        for file in files_to_process {
            let mut parsed =
                parse_terminal_file(base_options, processor.parser_options(), file)?;
            apply_diagrams_as(&mut parsed, base_options, processor.name(), Some(file))?;
            let parsed = parsed.report_warnings(WarningRenderContext::new().with_file(file));
            let result = processor.convert(parsed.document(), Some(file))?;
            let (output_path, warnings) = result.into_parts();
            warnings.render(WarningRenderContext::new().with_file(file));
            if let Some(output_path) = output_path {
                output_paths.push(output_path);
            }
        }
        return Ok(output_paths);
    }

    // Try to spawn pager.
    if let Some(pager) = spawn_pager(args.no_pager) {
        run_terminal_through_pager(&processor, args, base_options, files_to_process, pager)?;
    } else {
        // No pager - use convert() which writes to stdout
        for file in files_to_process {
            let mut parsed =
                parse_terminal_file(base_options, processor.parser_options(), file)?;
            apply_diagrams_as(&mut parsed, base_options, processor.name(), Some(file))?;
            let parsed = parsed.report_warnings(WarningRenderContext::new().with_file(file));
            let result = processor.convert(parsed.document(), Some(file))?;
            let (_, warnings) = result.into_parts();
            warnings.render(WarningRenderContext::new().with_file(file));
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
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
    #[cfg(feature = "pdf")]
    Pdf,
}

/// PDF page size parsed from `--page`.
#[cfg(feature = "pdf")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum PdfPageArg {
    A3,
    A4,
    A5,
    Executive,
    Legal,
    Letter,
    Tabloid,
}

#[cfg(feature = "pdf")]
impl PdfPageArg {
    const fn to_page_size(self) -> PageSize {
        match self {
            Self::A3 => PageSize::A3,
            Self::A4 => PageSize::A4,
            Self::A5 => PageSize::A5,
            Self::Executive => PageSize::Executive,
            Self::Legal => PageSize::Legal,
            Self::Letter => PageSize::Letter,
            Self::Tabloid => PageSize::Tabloid,
        }
    }
}

/// PDF page layout parsed from `--page-layout`.
#[cfg(feature = "pdf")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum PdfPageLayoutArg {
    Portrait,
    Landscape,
}

#[cfg(feature = "pdf")]
impl PdfPageLayoutArg {
    const fn to_page_layout(self) -> PageLayout {
        match self {
            Self::Portrait => PageLayout::Portrait,
            Self::Landscape => PageLayout::Landscape,
        }
    }
}

impl Display for BackendArg {
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
            #[cfg(feature = "pdf")]
            Self::Pdf => f.write_str("pdf"),
        }
    }
}

/// CLI-local variant choice, parsed from `--variant` before being combined
/// with the backend into the typed [`Backend`]. Each arm is feature-gated
/// so disabling its converter removes its variant names from `--variant`
/// entirely.
#[cfg(any(feature = "html", feature = "markdown"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
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

#[cfg(any(feature = "html", feature = "markdown"))]
impl Display for VariantArg {
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
    #[cfg(feature = "pdf")]
    Pdf,
}

/// Reject a `--variant` for a backend that doesn't define any. Interpolating
/// the backend name keeps the error consistent across no-variant backends
/// and means a future addition only needs one `resolve` arm, not a bespoke
/// error string.
#[cfg(all(
    any(feature = "manpage", feature = "pdf", feature = "terminal"),
    any(feature = "html", feature = "markdown")
))]
fn require_no_variant(backend: &'static str, variant: Option<VariantArg>) -> MietteResult<()> {
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
    #[cfg(any(feature = "html", feature = "markdown"))]
    fn resolve(self, variant: Option<VariantArg>) -> MietteResult<Backend> {
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
            #[cfg(feature = "pdf")]
            (Self::Pdf, v) => require_no_variant("pdf", v).map(|()| Backend::Pdf),
            #[cfg(feature = "terminal")]
            (Self::Terminal, v) => require_no_variant("terminal", v).map(|()| Backend::Terminal),
        }
    }

    /// Resolve the backend when no compiled converter accepts `--variant`.
    #[cfg(not(any(feature = "html", feature = "markdown")))]
    fn resolve(self) -> Backend {
        match self {
            #[cfg(feature = "manpage")]
            Self::Manpage => Backend::Manpage,
            #[cfg(feature = "pdf")]
            Self::Pdf => Backend::Pdf,
            #[cfg(feature = "terminal")]
            Self::Terminal => Backend::Terminal,
        }
    }
}

fn build_attributes_map(values: &[String]) -> RawAttributes<'static> {
    let mut map = RawAttributes::with_capacity(values.len());

    for raw_attr in values {
        let attribute = parse_attribute(raw_attr);
        map.insert(attribute.name, attribute.value);
    }
    map
}

fn build_attribute_overrides(values: &[String]) -> RawAttributes<'static> {
    let mut overrides = RawAttributes::with_capacity(values.len());
    for raw_attr in values {
        let attribute = parse_attribute(raw_attr);
        if attribute.locked {
            overrides.insert(attribute.name, attribute.value);
        } else {
            // A later default cancels an earlier override for the same name.
            overrides.remove(&attribute.name);
        }
    }
    overrides
}

struct ParsedAttribute {
    name: Cow<'static, str>,
    value: AttributeValue<'static>,
    locked: bool,
}

fn strip_soft_modifier(value: &str) -> (&str, bool) {
    value
        .strip_suffix('@')
        .map_or((value, false), |value| (value, true))
}

fn parse_attribute(raw_attr: &str) -> ParsedAttribute {
    let (name, value, locked) = if let Some((raw_name, raw_value)) = raw_attr.split_once('=') {
        let (name, name_is_soft) = strip_soft_modifier(raw_name);
        let (value, value_is_soft) = strip_soft_modifier(raw_value);

        if value_is_soft
            && value.is_empty()
            && let Some(name) = name.strip_prefix('!')
        {
            (name, AttributeValue::Bool(false), false)
        } else {
            (
                name,
                AttributeValue::String(value.to_string().into()),
                !(name_is_soft || value_is_soft),
            )
        }
    } else {
        let (raw_attr, is_soft) = strip_soft_modifier(raw_attr);

        if let Some(name) = raw_attr
            .strip_prefix('!')
            .or_else(|| raw_attr.strip_suffix('!'))
        {
            let value = if is_soft {
                AttributeValue::Bool(false)
            } else {
                AttributeValue::None
            };
            (name, value, !is_soft)
        } else {
            (raw_attr, AttributeValue::Bool(true), !is_soft)
        }
    };

    ParsedAttribute {
        name: name.to_string().into(),
        value,
        locked,
    }
}

/// Build parser options from CLI args and base options
fn build_parser_options(args: &Args, base_options: &Options) -> OptionsBuilder<'static> {
    let mut builder = ParserOptions::builder()
        .with_safe_mode(base_options.safe_mode())
        .with_attributes(build_attribute_overrides(&args.attributes))
        .with_defaults(build_attributes_map(&args.attributes));

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

    builder
}
