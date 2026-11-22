use std::{
    cell::Cell,
    error::Error,
    ffi::OsString,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use acdc_parser::extensions::{
    BlockMacro, BlockMacroOutput, BlockMacroProcessor, MacroExecutionContext, MacroInvocation,
    Registry,
};
use clap::Parser;

use super::*;

type TestResult = Result<(), Box<dyn Error>>;

#[derive(Parser)]
struct TestArgs {
    #[command(flatten)]
    args: Args,
}

fn args(values: impl IntoIterator<Item = OsString>) -> Result<Args, clap::Error> {
    TestArgs::try_parse_from(std::iter::once(OsString::from("convert")).chain(values))
        .map(|parsed| parsed.args)
}

struct Probe {
    calls: Rc<Cell<usize>>,
}

impl BlockMacroProcessor for Probe {
    fn process(
        &mut self,
        invocation: &MacroInvocation<'_>,
        context: &mut MacroExecutionContext,
    ) -> Result<BlockMacroOutput, String> {
        let backend = invocation
            .document_attributes
            .get("backend")
            .and_then(acdc_parser::DocumentAttributeValue::as_str)
            .ok_or("backend was not selected before processing")?;
        self.calls.set(self.calls.get() + 1);
        context.set_attribute("ready", "yes");
        Ok(BlockMacroOutput::Paragraph(format!(
            "{backend}:{}:{}",
            invocation.target,
            self.calls.get()
        )))
    }
}

fn available(created: &Arc<AtomicUsize>) -> Result<ExtensionCatalog, Box<dyn Error>> {
    let created = Arc::clone(created);
    let mut builder = Registry::builder();
    builder.register(BlockMacro::new("probe", move || {
        created.fetch_add(1, Ordering::SeqCst);
        Probe {
            calls: Rc::new(Cell::new(0)),
        }
    }))?;
    let mut catalog = ExtensionCatalog::default();
    catalog.register("probe", builder.build())?;
    Ok(catalog)
}

#[test]
fn pipeline_cli_rejects_unavailable_selections_before_files_or_stdin() -> TestResult {
    let backend = selection::installed()?.names().next().ok_or("backend")?;
    let created = Arc::new(AtomicUsize::new(0));
    let available = available(&created)?;
    for input in ["--stdin", "missing-input.adoc"] {
        let args = args(
            [
                "--backend",
                backend,
                "--extension",
                "probe",
                "--extension",
                "missing",
                input,
            ]
            .map(OsString::from),
        )?;
        let error = run_with_extensions(&args, &available)
            .err()
            .ok_or("unavailable selection")?;
        assert!(
            error
                .to_string()
                .contains("extension `missing` is not available")
        );
    }
    assert_eq!(created.load(Ordering::SeqCst), 0);
    Ok(())
}

#[cfg(feature = "html")]
#[test]
fn pipeline_cli_files_and_batches_require_explicit_activation() -> TestResult {
    let directory = tempfile::tempdir()?;
    let first = directory.path().join("first.adoc");
    let second = directory.path().join("second.adoc");
    std::fs::write(
        &first,
        "ifdef::backend-html5[]\nprobe::first[]\n\nifdef::ready[]\nprobe::later[]\nendif::[]\nendif::[]",
    )?;
    std::fs::write(&second, "probe::second[]")?;
    let created = Arc::new(AtomicUsize::new(0));
    let available = available(&created)?;
    let inactive = args([
        OsString::from("--backend"),
        OsString::from("html"),
        first.as_os_str().to_owned(),
    ])?;
    run_with_extensions(&inactive, &available).map_err(|error| error.to_string())?;
    assert_eq!(created.load(Ordering::SeqCst), 0);
    assert!(std::fs::read_to_string(first.with_extension("html"))?.contains("probe::first[]"));
    let active = args([
        OsString::from("--backend"),
        OsString::from("html"),
        OsString::from("--extension"),
        OsString::from("probe"),
        first.as_os_str().to_owned(),
        second.as_os_str().to_owned(),
    ])?;
    run_with_extensions(&active, &available).map_err(|error| error.to_string())?;
    assert_eq!(created.load(Ordering::SeqCst), 2);
    let rendered = std::fs::read_to_string(first.with_extension("html"))?;
    assert!(rendered.contains("html5:first:1"));
    assert!(rendered.contains("html5:later:2"));
    assert!(std::fs::read_to_string(second.with_extension("html"))?.contains("html5:second:1"));
    Ok(())
}

#[cfg(feature = "html")]
#[test]
fn pipeline_cli_reader_uses_selected_registrations_and_backend_options() -> TestResult {
    let directory = tempfile::tempdir()?;
    let output = directory.path().join("reader.html");
    let created = Arc::new(AtomicUsize::new(0));
    let available = available(&created)?;
    let selected = available.select(["probe"])?;
    let converter = configure_pipeline(
        acdc_converters_html::Converter::new(
            Options::builder()
                .embedded(true)
                .output_destination(OutputDestination::File(output.clone()))
                .build(),
            ParserOptions::builder(),
        )?,
        selected.as_ref(),
    );
    let summary = convert_reader(
        &converter,
        "probe::reader[]\n\nifdef::ready[]\nprobe::later[]\nendif::[]".as_bytes(),
    )
    .map_err(|error| error.to_string())?;
    assert_eq!(summary.warning_count, 0);
    let rendered = std::fs::read_to_string(output)?;
    assert!(rendered.contains("html5:reader:1"));
    assert!(rendered.contains("html5:later:2"));
    assert!(!rendered.contains("<!DOCTYPE"));
    assert_eq!(created.load(Ordering::SeqCst), 1);
    Ok(())
}

#[cfg(all(feature = "terminal", unix))]
fn pager(path: &std::path::Path) -> std::io::Result<std::process::Child> {
    use std::process::{Command, Stdio};
    Command::new("/bin/sh")
        .args(["-c", "cat > \"$1\"", "pager"])
        .arg(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
}

#[cfg(all(feature = "terminal", unix))]
#[test]
fn pipeline_cli_terminal_reader_files_and_pager_share_processing() -> TestResult {
    let directory = tempfile::tempdir()?;
    let input = directory.path().join("input.adoc");
    let file_output = directory.path().join("file.txt");
    let reader_output = directory.path().join("reader.txt");
    let pager_output = directory.path().join("pager.txt");
    std::fs::write(&input, "probe::file[]")?;
    let created = Arc::new(AtomicUsize::new(0));
    let available = available(&created)?;
    let selection = available.select(["probe"])?;
    let args = args([
        OsString::from("--backend"),
        OsString::from("terminal"),
        OsString::from("--no-pager"),
        OsString::from("--extension"),
        OsString::from("probe"),
        OsString::from("-o"),
        file_output.as_os_str().to_owned(),
        input.as_os_str().to_owned(),
    ])?;
    run_with_extensions(&args, &available).map_err(|error| error.to_string())?;
    assert!(std::fs::read_to_string(file_output)?.contains("file:1"));
    let options = Options::builder()
        .output_destination(OutputDestination::File(reader_output.clone()))
        .build();
    let converter = configure_pipeline(
        acdc_converters_terminal::Converter::new(options.clone(), ParserOptions::builder())?,
        selection.as_ref(),
    );
    let summary = run_terminal_reader(&converter, "probe::reader[]".as_bytes(), || None)?;
    assert_eq!(summary.warning_count, 0);
    assert!(std::fs::read_to_string(reader_output)?.contains("reader:1"));
    let child = pager(&pager_output)?;
    let summary = run_terminal_reader(
        &converter,
        "probe::pager[]\n\n<<missing>>".as_bytes(),
        || Some(child),
    )?;
    assert_eq!(summary.warning_count, 1);
    assert!(std::fs::read_to_string(&pager_output)?.contains("pager:1"));
    let child = pager(&pager_output)?;
    assert_eq!(
        run_terminal_through_pager(&converter, &options, &[input], child)?,
        0
    );
    assert!(std::fs::read_to_string(pager_output)?.contains("file:1"));
    assert_eq!(created.load(Ordering::SeqCst), 4);
    Ok(())
}

/// A downstream converter chooses its own parser profile, serialization, and suffix.
struct NativeJson {
    options: Options,
    parser: acdc_parser::Options<'static>,
}

#[derive(Debug, thiserror::Error)]
enum JsonError {
    #[error(transparent)]
    Parser(#[from] acdc_parser::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

impl acdc_converters_core::Converter<'static> for NativeJson {
    type Error = JsonError;

    fn new(options: Options, parser: OptionsBuilder<'static>) -> Result<Self, Self::Error> {
        let parser =
            acdc_converters_core::BackendProfile::new("native-json", "json", "json", ".ast.json")
                .apply(parser, options.doctype(), options.embedded())
                .build()?;
        Ok(Self { options, parser })
    }
    fn options(&self) -> &Options {
        &self.options
    }
    fn parser_options(&self) -> &acdc_parser::Options<'static> {
        &self.parser
    }
    fn name(&self) -> &'static str {
        "native-json"
    }
    fn supports_text_output(&self) -> bool {
        true
    }
    fn derive_output_path(
        &self,
        input: &Path,
        _: acdc_parser::ast::DocumentRef<'_>,
    ) -> Result<Option<PathBuf>, Self::Error> {
        Ok(Some(input.with_extension("ast.json")))
    }
    fn write_to<W: std::io::Write>(
        &self,
        doc: acdc_parser::ast::DocumentRef<'_>,
        writer: W,
        _: Option<&Path>,
        _: Option<&Path>,
        _: &mut acdc_converters_core::Diagnostics<'_>,
    ) -> Result<(), Self::Error> {
        Ok(serde_json::to_writer(writer, &doc)?)
    }
}

#[test]
fn static_converter_uses_cli_arguments_the_shared_pipeline_and_its_own_output_path() -> TestResult {
    use acdc_converters_core::{postprocess::TextPostprocessor, registry::ConverterSpec};
    use clap::{CommandFactory as _, FromArgMatches as _};

    let mut converters = selection::builtins()?;
    converters.register(
        ConverterSpec::new("native-json", ()).with_aliases(&["nj"]),
        |()| selection::Runner::converter(NativeJson::new),
    )?;
    let temp = tempfile::tempdir()?;
    let file = temp.path().join("doc.adoc");
    std::fs::write(
        &file,
        "ifdef::backend-native-json[]\nSelected before parsing.\nendif::[]",
    )?;
    let mut catalog = ExtensionCatalog::default();
    catalog.register(
        "wrap",
        Extensions::default().with_postprocessor(TextPostprocessor::new(
            "native-json",
            |text, _, _| {
                let doc: serde_json::Value = serde_json::from_str(text)?;
                *text = serde_json::to_string(&serde_json::json!({"wrapped":doc}))?;
                Ok(())
            },
        )),
    )?;
    let command = TestArgs::command().mut_arg("backend", |arg| {
        arg.value_parser(selection::values(converters.backend_spellings()))
    });
    let matches = command.try_get_matches_from([
        OsString::from("convert"),
        "--backend".into(),
        "nj".into(),
        "--extension".into(),
        "wrap".into(),
        file.into_os_string(),
    ])?;
    let args = TestArgs::from_arg_matches(&matches)?.args;
    run_with_registries(&args, &catalog, &converters)?;
    let json = std::fs::read_to_string(temp.path().join("doc.ast.json"))?;
    assert!(
        serde_json::from_str::<serde_json::Value>(&json)?
            .get("wrapped")
            .is_some()
    );
    assert!(json.contains("Selected before parsing."));
    Ok(())
}

#[cfg(all(feature = "html", feature = "markdown"))]
#[test]
fn cli_backend_spellings_and_variant_errors_follow_the_registry() -> TestResult {
    let catalog = selection::installed()?;
    for (backend, variant, canonical, canonical_variant) in [
        ("html5s", None, "html", "semantic"),
        ("md", Some("cm"), "markdown", "commonmark"),
        ("markdown", Some("github-flavored"), "markdown", "gfm"),
        ("md", None, "markdown", "gfm"),
    ] {
        let mut flags = vec!["--backend", backend, "--stdin"];
        if let Some(v) = variant {
            flags.extend(["--variant", v]);
        }
        let args = args(flags.into_iter().map(OsString::from))?;
        assert!(std::ptr::eq(
            catalog.resolve(&args.backend, args.variant.as_deref())?,
            catalog.resolve(canonical, Some(canonical_variant))?
        ));
    }
    for (backend, variant, diagnostic) in [
        ("html5s", "semantic", "does not accept a variant"),
        ("html", "gfm", "not supported by backend 'html'"),
        ("md", "standard", "not supported by backend 'markdown'"),
    ] {
        let args =
            args(["--backend", backend, "--variant", variant, "missing.adoc"].map(OsString::from))?;
        let error = run_with_extensions(&args, &ExtensionCatalog::default())
            .err()
            .ok_or("expected a selection error")?;
        assert!(error.to_string().contains(diagnostic), "{error}");
    }
    Ok(())
}
