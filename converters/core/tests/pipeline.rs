//! Shared pipeline activation, source processing, and document isolation contracts.

use std::{
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use acdc_converters_core::{
    BackendProfile, Converter, Diagnostics, Options,
    pipeline::{ExtensionCatalog, ExtensionSelectionError, Extensions, Pipeline},
};
use acdc_parser::{
    Error as ParserError, Options as ParserOptions, OptionsBuilder, SafeMode, WarningKind,
    ast::DocumentRef,
    extensions::{
        BlockBuilder, BlockContext, BlockMacro, BlockMacroOutput, BlockMacroProcessor,
        DisplayIntent, DocumentEditor, MacroExecutionContext, MacroInvocation, Registry,
        TreeProcessor, TreeProcessorRegistration,
    },
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error(transparent)]
    Parser(#[from] ParserError),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

struct JsonConverter {
    options: Options,
    parser_options: ParserOptions<'static>,
}

impl Converter<'static> for JsonConverter {
    type Error = Error;

    fn new(options: Options, parser_options: OptionsBuilder<'static>) -> Result<Self, Error> {
        Ok(Self {
            parser_options: BackendProfile::new("probe", "probe", "json", ".json")
                .apply(parser_options, options.doctype(), options.embedded())
                .build()?,
            options,
        })
    }
    fn options(&self) -> &Options {
        &self.options
    }
    fn parser_options(&self) -> &ParserOptions<'static> {
        &self.parser_options
    }
    fn derive_output_path(&self, _: &Path, _: DocumentRef<'_>) -> Result<Option<PathBuf>, Error> {
        Ok(None)
    }
    fn name(&self) -> &'static str {
        "probe"
    }
    fn write_to<W: Write>(
        &self,
        document: DocumentRef<'_>,
        writer: W,
        _: Option<&Path>,
        _: Option<&Path>,
        diagnostics: &mut Diagnostics<'_>,
    ) -> Result<(), Error> {
        if document.attributes().get("render-warning").is_some() {
            diagnostics.warn("render warning");
        }
        serde_json::to_writer(writer, &document)?;
        Ok(())
    }
}

struct Counter {
    calls: usize,
}

impl BlockMacroProcessor for Counter {
    fn process(
        &mut self,
        invocation: &MacroInvocation<'_>,
        context: &mut MacroExecutionContext,
    ) -> Result<BlockMacroOutput, String> {
        self.calls += 1;
        assert_eq!(
            invocation
                .document_attributes
                .get("backend")
                .and_then(acdc_parser::DocumentAttributeValue::as_str),
            Some("probe")
        );
        context.set_attribute("produced", "yes");
        Ok(BlockMacroOutput::Paragraph(format!(
            "{}:{}",
            invocation.target, self.calls
        )))
    }
}

fn catalog(created: &Arc<AtomicUsize>) -> Result<ExtensionCatalog, Box<dyn std::error::Error>> {
    let mut builder = Registry::builder();
    let created = Arc::clone(created);
    builder.register(BlockMacro::new("count", move || {
        created.fetch_add(1, Ordering::SeqCst);
        Counter { calls: 0 }
    }))?;
    let mut catalog = ExtensionCatalog::default();
    catalog.register("counter", builder.build())?;
    Ok(catalog)
}

fn pipeline(registry: Option<Extensions>) -> Result<Pipeline<JsonConverter>, Error> {
    let pipeline = Pipeline::new(JsonConverter::new(
        Options::default(),
        ParserOptions::builder(),
    )?);
    Ok(match registry {
        Some(registry) => pipeline.with_extensions(registry),
        None => pipeline,
    })
}

fn render(
    pipeline: &Pipeline<JsonConverter>,
    document: DocumentRef<'_>,
) -> Result<(serde_json::Value, Vec<acdc_converters_core::Warning>), Error> {
    let mut bytes = Vec::new();
    let mut warnings = Vec::new();
    let source = pipeline.converter().warning_source();
    pipeline.write_to(
        document,
        &mut bytes,
        None,
        None,
        &mut Diagnostics::new(&source, &mut warnings),
    )?;
    Ok((serde_json::from_slice(&bytes)?, warnings))
}

#[test]
fn pipeline_selection_is_explicit_ordered_and_validated_before_input() -> TestResult {
    let created = Arc::new(AtomicUsize::new(0));
    let catalog = catalog(&created)?;
    assert_eq!(catalog.names().collect::<Vec<_>>(), ["counter"]);
    assert!(catalog.select([] as [&str; 0])?.is_none());
    assert!(
        matches!(catalog.select(["unknown"]), Err(ExtensionSelectionError::Unavailable(name)) if name == "unknown")
    );
    assert_eq!(created.load(Ordering::SeqCst), 0);
    let inactive = pipeline(None)?;
    let ordinary = inactive.parse("count::idle[]")?;
    assert!(!ordinary.document().has_deferred_displays());
    assert!(serde_json::to_string(&ordinary.document())?.contains("count::idle[]"));
    assert_eq!(created.load(Ordering::SeqCst), 0);
    let active = pipeline(catalog.select(["counter", "counter"])?)?;
    let parsed = active.parse("count::first[]\n\ncount::second[]")?;
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    assert_eq!(created.load(Ordering::SeqCst), 1);
    Ok(())
}

#[test]
fn pipeline_files_readers_and_strings_observe_backend_and_source_order() -> TestResult {
    let created = Arc::new(AtomicUsize::new(0));
    let pipeline = pipeline(catalog(&created)?.select(["counter"])?)?;
    let source = "ifdef::backend-probe[]\ncount::one[]\n\nifdef::produced[]\ncount::two[]\nendif::[]\nendif::[]";
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("input.adoc");
    std::fs::write(&path, source)?;
    for parsed in [
        pipeline.parse(source)?,
        pipeline.parse_from_reader(source.as_bytes())?,
        pipeline.parse_file(&path)?,
    ] {
        let (rendered, warnings) = render(&pipeline, parsed.document())?;
        let text = rendered.to_string();
        assert!(text.contains("one:1"));
        assert!(text.contains("two:2"));
        assert!(!text.contains("count::"));
        assert!(warnings.is_empty(), "{warnings:?}");
    }
    assert_eq!(created.load(Ordering::SeqCst), 3);
    Ok(())
}

#[test]
fn pipeline_reader_keeps_include_origin_and_reader_attributes() -> TestResult {
    let directory = tempfile::tempdir()?;
    std::fs::write(directory.path().join("yes.adoc"), "count::included[]")?;
    let created = Arc::new(AtomicUsize::new(0));
    let registry = catalog(&created)?.select(["counter"])?.ok_or("registry")?;
    let pipeline = Pipeline::new(JsonConverter::new(
        Options::default(),
        ParserOptions::builder()
            .with_base_dir(directory.path())
            .with_safe_mode(SafeMode::Safe),
    )?)
    .with_extensions(registry);
    let parsed =
        pipeline.parse_from_reader("count::first[]\n\ninclude::{produced}.adoc[]".as_bytes())?;
    let (rendered, warnings) = render(&pipeline, parsed.document())?;
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    let text = rendered.to_string();
    assert!(text.contains("first:1"));
    assert!(text.contains("included:2"));
    for name in ["docdir", "docfile", "docfilesuffix", "docname"] {
        assert!(parsed.document().attributes().get(name).is_none(), "{name}");
    }
    assert_eq!(created.load(Ordering::SeqCst), 1);
    Ok(())
}

#[test]
fn pipeline_independent_and_parallel_documents_keep_handlers_and_warnings_local() -> TestResult {
    let created = Arc::new(AtomicUsize::new(0));
    let catalog = catalog(&created)?;
    let pipeline = pipeline(catalog.select(["counter"])?)?;
    let first = pipeline.parse(":render-warning:\n\ncount::first[]\n\n<<missing>>")?;
    assert!(first.warnings().iter().any(|warning| matches!(&warning.kind, WarningKind::UnresolvedReference { target } if target == "missing")));
    let before = serde_json::to_value(first.document())?;
    let (one, warnings) = render(&pipeline, first.document())?;
    let (two, repeated) = render(&pipeline, first.document())?;
    assert_eq!(one, two);
    assert_eq!(warnings.len(), 1);
    assert_eq!(repeated.len(), 1);
    assert_eq!(serde_json::to_value(first.document())?, before);
    let clean = pipeline.parse("count::next[]")?;
    assert!(clean.warnings().is_empty(), "{:?}", clean.warnings());
    let (rendered, warnings) = render(&pipeline, clean.document())?;
    assert!(rendered.to_string().contains("next:1"));
    assert!(warnings.is_empty(), "{warnings:?}");
    std::thread::scope(|scope| {
        let threads: Vec<_> = (0..3)
            .map(|_| {
                let catalog = &catalog;
                scope.spawn(move || -> Result<(), String> {
                    let pipeline =
                        self::pipeline(catalog.select(["counter"]).map_err(|e| e.to_string())?)
                            .map_err(|e| e.to_string())?;
                    let parsed = pipeline
                        .parse("count::parallel[]")
                        .map_err(|e| e.to_string())?;
                    let (rendered, _) =
                        render(&pipeline, parsed.document()).map_err(|e| e.to_string())?;
                    assert!(rendered.to_string().contains("parallel:1"));
                    Ok(())
                })
            })
            .collect();
        for thread in threads {
            thread.join().map_err(|_| "thread panicked")??;
        }
        Ok::<_, Box<dyn std::error::Error>>(())
    })?;
    assert_eq!(created.load(Ordering::SeqCst), 5);
    Ok(())
}

struct Capture;
impl TreeProcessor for Capture {
    fn process(&mut self, editor: &mut DocumentEditor<'_>) -> Result<(), ParserError> {
        let target = editor
            .query()
            .select(BlockContext::Listing)
            .next()
            .map(|view| view.target().clone());
        if let Some(target) = target {
            let captured = editor.capture(&target)?;
            editor.insert_before(
                &target,
                BlockBuilder::unordered_list([captured.display(DisplayIntent::CaptionAndTitle)]),
            )?;
        }
        Ok(())
    }
}

#[test]
fn pipeline_finalizes_once_and_prepares_explicit_labels_for_custom_backends() -> TestResult {
    let mut builder = Registry::builder();
    builder.register_tree_processor(TreeProcessorRegistration::new("capture", || Capture))?;
    let registry = builder.build();
    let pipeline = pipeline(Some(registry.clone().into()))?;
    let mut working = registry.parse(
        ":listing-caption: Code\n\n.A *title*\n[#code,reftext=Other]\n----\nbody\n----",
        pipeline.converter().parser_options(),
    )?;
    working.normalize()?;
    working.normalize()?;
    let parsed = working.finish()?;
    assert!(parsed.document().has_deferred_displays());
    let before = serde_json::to_value(parsed.document())?;
    let (first, _) = render(&pipeline, parsed.document())?;
    let (second, _) = render(&pipeline, parsed.document())?;
    assert_eq!(first, second);
    assert!(!first.to_string().contains("generated_display"));
    assert!(first.to_string().contains("Code 1."));
    assert_eq!(serde_json::to_value(parsed.document())?, before);
    Ok(())
}

#[test]
fn pipeline_empty_registries_keep_the_direct_path_and_reader_errors() -> TestResult {
    struct Broken;
    impl Read for Broken {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("reader failed"))
        }
    }
    let pipeline = pipeline(Some(Registry::default().into()))?;
    let parsed = pipeline.parse("Paragraph.")?;
    assert!(serde_json::to_string(&parsed.document())?.contains("Paragraph."));
    assert!(pipeline.parse_from_reader(Broken).is_err());
    Ok(())
}

#[test]
fn pipeline_registry_composition_rejects_conflicts_atomically() -> TestResult {
    let created = Arc::new(AtomicUsize::new(0));
    let mut catalog = catalog(&created)?;
    assert!(matches!(
        catalog.register("counter", Registry::default()),
        Err(ExtensionSelectionError::DuplicateName(_))
    ));
    assert!(matches!(
        catalog.register("invalid name", Registry::default()),
        Err(ExtensionSelectionError::InvalidName(_))
    ));
    let selected = catalog.select(["counter"])?.ok_or("active registry")?;
    let original = selected.parser_registry().ok_or("parser registry")?;
    let mut incoming = Registry::builder();
    incoming.register(BlockMacro::new("fresh", || Counter { calls: 0 }))?;
    incoming.extend(original)?;
    let incoming = incoming.build();
    catalog.register("conflict", incoming.clone())?;
    assert!(matches!(
        catalog.select(["counter", "conflict"]),
        Err(ExtensionSelectionError::Conflict { .. })
    ));
    assert_eq!(created.load(Ordering::SeqCst), 0);
    let mut builder = Registry::builder();
    builder.extend(original)?;
    assert!(builder.extend(&incoming).is_err());
    let pipeline = pipeline(Some(builder.build().into()))?;
    let parsed = pipeline.parse("count::one[]\n\nfresh::idle[]")?;
    let rendered = render(&pipeline, parsed.document())?.0.to_string();
    assert!(rendered.contains("one:1"));
    assert!(rendered.contains("fresh::idle[]"));
    Ok(())
}

struct Replace(&'static str);
impl TreeProcessor for Replace {
    fn process(&mut self, editor: &mut DocumentEditor<'_>) -> Result<(), ParserError> {
        let target = editor
            .query()
            .blocks()
            .next()
            .map(|view| view.target().clone());
        if let Some(target) = target {
            editor.replace_block(&target, BlockBuilder::paragraph(self.0))?;
        }
        Ok(())
    }
}

#[test]
fn pipeline_selection_order_controls_processor_order() -> TestResult {
    let mut catalog = ExtensionCatalog::default();
    for name in ["first", "second"] {
        let mut builder = Registry::builder();
        builder
            .register_tree_processor(TreeProcessorRegistration::new(name, move || Replace(name)))?;
        catalog.register(name, builder.build())?;
    }
    let pipeline = pipeline(catalog.select(["second", "first"])?)?;
    let parsed = pipeline.parse("Original.")?;
    let text = render(&pipeline, parsed.document())?.0.to_string();
    assert!(text.contains("first"));
    assert!(!text.contains("second"));
    Ok(())
}

#[test]
fn pipeline_renderer_factories_wait_for_matching_backend_sessions() -> TestResult {
    use acdc_converters_core::render::{
        BlockRenderProcessor, BlockRenderer, RenderError, RenderHost,
    };

    struct Decline;
    impl BlockRenderProcessor for Decline {
        fn render(
            &mut self,
            _: acdc_parser::ast::BlockRef<'_>,
            _: &mut dyn RenderHost,
        ) -> Result<bool, RenderError> {
            Ok(false)
        }
    }

    let created = Arc::new(AtomicUsize::new(0));
    let factory_created = Arc::clone(&created);
    let registrations =
        Extensions::default().with_renderer(BlockRenderer::new("html", move || {
            factory_created.fetch_add(1, Ordering::SeqCst);
            Decline
        }));
    let mut available = ExtensionCatalog::default();
    available.register("renderer", registrations)?;
    assert_eq!(created.load(Ordering::SeqCst), 0);
    assert!(available.select([] as [&str; 0])?.is_none());
    assert!(available.select(["unknown"]).is_err());
    let selected = available
        .select(["renderer", "renderer"])?
        .ok_or("selected renderer")?;
    assert!(selected.parser_registry().is_none());
    assert!(selected.render_session("markdown").is_none());
    assert_eq!(created.load(Ordering::SeqCst), 0);
    let first = selected.render_session("html").ok_or("first session")?;
    let second = selected.render_session("html").ok_or("second session")?;
    assert_eq!(created.load(Ordering::SeqCst), 2);
    drop((first, second));
    Ok(())
}
