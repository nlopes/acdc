//! Native renderer activation, dispatch, failures, and conversion-local state.

use std::{
    cell::Cell,
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
};

use acdc_converters_core::{
    BackendTarget, Converter as _, Diagnostics, Options,
    docinfo::{DocinfoContext, DocinfoHook, DocinfoPosition},
    pipeline::{ExtensionCatalog, Extensions, Pipeline},
    postprocess::TextPostprocessor,
    render::{BlockRenderProcessor, BlockRenderer, Next, RenderContext, RenderError},
};
use acdc_converters_html::{Converter, HtmlVariant};
use acdc_parser::{
    Options as ParserOptions,
    ast::{BlockKind, BlockRef},
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

struct Counter {
    calls: Rc<Cell<usize>>,
}

impl BlockRenderProcessor for Counter {
    fn accepts(
        &mut self,
        block: BlockRef<'_>,
        _: &mut dyn RenderContext,
    ) -> Result<bool, RenderError> {
        Ok(matches!(block.kind(), BlockKind::Paragraph(_)))
    }
    fn render(&mut self, _: BlockRef<'_>, mut next: Next<'_, '_>) -> Result<(), RenderError> {
        let host = next.host();
        self.calls.set(self.calls.get() + 1);
        host.write(format!("<p>native:{}</p>\n", self.calls.get()).as_bytes())?;
        Ok(())
    }
}

fn available(created: &Arc<AtomicUsize>) -> Result<ExtensionCatalog, Box<dyn std::error::Error>> {
    let created = Arc::clone(created);
    let mut catalog = ExtensionCatalog::default();
    catalog.register(
        "counter",
        Extensions::default().with_renderer(BlockRenderer::new("html", move || {
            created.fetch_add(1, Ordering::SeqCst);
            Counter {
                calls: Rc::new(Cell::new(0)),
            }
        })),
    )?;
    Ok(catalog)
}

fn pipeline(
    selection: Option<Extensions>,
) -> Result<Pipeline<Converter<'static>>, acdc_converters_html::Error> {
    let pipeline = Pipeline::new(Converter::new(
        Options::builder().embedded(true).build(),
        ParserOptions::builder(),
    )?);
    Ok(match selection {
        Some(selection) => pipeline.with_extensions(selection),
        None => pipeline,
    })
}

fn render(
    pipeline: &Pipeline<Converter<'_>>,
    source: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let parsed = pipeline.parse(source)?;
    let mut output = Vec::new();
    let mut warnings = Vec::new();
    let warning_source = pipeline.converter().warning_source();
    pipeline.write_to(
        parsed.document(),
        &mut output,
        None,
        None,
        &mut Diagnostics::new(&warning_source, &mut warnings),
    )?;
    assert!(warnings.is_empty(), "{warnings:?}");
    Ok(String::from_utf8(output)?)
}

#[test]
fn native_renderers_are_explicit_and_fresh_across_nested_and_parallel_conversions() -> TestResult {
    let created = Arc::new(AtomicUsize::new(0));
    let available = available(&created)?;
    assert_eq!(created.load(Ordering::SeqCst), 0);
    let inactive = pipeline(None)?;
    assert!(render(&inactive, "ordinary")?.contains("ordinary"));
    assert_eq!(created.load(Ordering::SeqCst), 0);
    let selected = available
        .select(["counter", "counter"])?
        .ok_or("selection")?;
    assert!(selected.parser_registry().is_none());
    let active = pipeline(Some(selected))?;
    assert_eq!(created.load(Ordering::SeqCst), 0);
    let source = "one\n\n====\ntwo\n====\n\n[cols=a]\n|===\n|three\n|===";
    let first = render(&active, source)?;
    assert!(first.contains("native:1"));
    assert!(first.contains("native:2"));
    assert!(first.contains("native:3"));
    assert!(!first.contains("native:4"));
    assert_eq!(first, render(&active, source)?);
    assert_eq!(created.load(Ordering::SeqCst), 2);
    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..3)
            .map(|_| {
                let available = &available;
                scope.spawn(move || -> Result<(), String> {
                    let pipeline = pipeline(
                        available
                            .select(["counter"])
                            .map_err(|error| error.to_string())?,
                    )
                    .map_err(|error| error.to_string())?;
                    let output =
                        render(&pipeline, "parallel").map_err(|error| error.to_string())?;
                    assert!(output.contains("native:1"));
                    Ok(())
                })
            })
            .collect();
        for handle in handles {
            handle.join().map_err(|_| "thread failure")??;
        }
        Ok::<_, Box<dyn std::error::Error>>(())
    })?;
    assert_eq!(created.load(Ordering::SeqCst), 5);
    Ok(())
}

struct Fail(Arc<AtomicUsize>);
impl BlockRenderProcessor for Fail {
    fn accepts(&mut self, _: BlockRef<'_>, _: &mut dyn RenderContext) -> Result<bool, RenderError> {
        Ok(true)
    }
    fn render(&mut self, _: BlockRef<'_>, _: Next<'_, '_>) -> Result<(), RenderError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Err(std::io::Error::other("native failure").into())
    }
}

#[test]
fn native_renderer_errors_stop_once_and_other_backend_factories_stay_inactive() -> TestResult {
    let created = Arc::new(AtomicUsize::new(0));
    let other = Arc::clone(&created);
    let registrations =
        Extensions::default().with_renderer(BlockRenderer::new("markdown", move || {
            other.fetch_add(1, Ordering::SeqCst);
            Counter {
                calls: Rc::new(Cell::new(0)),
            }
        }));
    assert!(render(&pipeline(Some(registrations))?, "ordinary")?.contains("ordinary"));
    assert_eq!(created.load(Ordering::SeqCst), 0);
    let calls = Arc::new(AtomicUsize::new(0));
    let factory_calls = Arc::clone(&calls);
    let registrations = Extensions::default()
        .with_renderer(BlockRenderer::new("html", move || {
            Fail(Arc::clone(&factory_calls))
        }));
    let pipeline = pipeline(Some(registrations))?;
    let error = render(&pipeline, "one\n\ntwo")
        .err()
        .ok_or("expected failure")?;
    assert!(error.to_string().contains("native failure"));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    Ok(())
}

struct Decision {
    label: &'static str,
    handled: bool,
    trace: Arc<Mutex<Vec<&'static str>>>,
}

impl BlockRenderProcessor for Decision {
    fn accepts(
        &mut self,
        block: BlockRef<'_>,
        _: &mut dyn RenderContext,
    ) -> Result<bool, RenderError> {
        if !matches!(block.kind(), BlockKind::Paragraph(_)) {
            return Ok(false);
        }
        self.trace
            .lock()
            .map_err(|_| "trace lock poisoned")?
            .push(self.label);
        Ok(self.handled)
    }
    fn render(&mut self, _: BlockRef<'_>, mut next: Next<'_, '_>) -> Result<(), RenderError> {
        next.host().write(self.label.as_bytes())
    }
}

#[test]
fn native_renderer_dispatch_follows_selection_order_and_stops_when_handled() -> TestResult {
    let trace = Arc::new(Mutex::new(Vec::new()));
    let mut available = ExtensionCatalog::default();
    for (label, handled) in [("decline", false), ("second", true), ("third", true)] {
        let trace = Arc::clone(&trace);
        available.register(
            label,
            Extensions::default().with_renderer(BlockRenderer::new("html", move || Decision {
                label,
                handled,
                trace: Arc::clone(&trace),
            })),
        )?;
    }
    let active = pipeline(available.select(["decline", "second", "third"])?)?;
    assert_eq!(render(&active, "source")?, "second");
    assert_eq!(
        *trace.lock().map_err(|_| "trace lock poisoned")?,
        ["decline", "second"]
    );
    trace.lock().map_err(|_| "trace lock poisoned")?.clear();
    let active = pipeline(available.select(["third", "second", "decline"])?)?;
    assert_eq!(render(&active, "source")?, "third");
    assert_eq!(*trace.lock().map_err(|_| "trace lock poisoned")?, ["third"]);
    Ok(())
}

#[test]
fn decorators_wrap_the_next_handler_and_declines_only_emit_diagnostics() -> TestResult {
    struct Decorator(&'static str);
    impl BlockRenderProcessor for Decorator {
        fn accepts(
            &mut self,
            _: BlockRef<'_>,
            context: &mut dyn RenderContext,
        ) -> Result<bool, RenderError> {
            if self.0 == "decline" {
                context.diagnostics().warn("declined");
                return Ok(false);
            }
            Ok(true)
        }
        fn render(&mut self, _: BlockRef<'_>, mut next: Next<'_, '_>) -> Result<(), RenderError> {
            next.host().write(format!("<{}>", self.0).as_bytes())?;
            next.render()?.write(format!("</{}>", self.0).as_bytes())
        }
    }
    let mut extensions = Extensions::default();
    for tag in ["outer", "decline", "inner"] {
        extensions = extensions.with_renderer(BlockRenderer::new("html", move || Decorator(tag)));
    }
    let pipeline = pipeline(Some(extensions))?;
    let parsed = pipeline.parse("ordinary")?;
    let mut warnings = Vec::new();
    let mut output = Vec::new();
    pipeline.write_to(
        parsed.document(),
        &mut output,
        None,
        None,
        &mut Diagnostics::new(&pipeline.converter().warning_source(), &mut warnings),
    )?;
    let html = String::from_utf8(output)?;
    assert!(html.starts_with("<outer><inner>"), "{html}");
    assert!(html.ends_with("</inner></outer>"), "{html}");
    assert_eq!(html.matches("<p>ordinary</p>").count(), 1);
    assert_eq!(warnings.len(), 1);
    Ok(())
}

#[test]
fn variant_target_filters_renderers_docinfo_factories_and_csp() -> TestResult {
    const HASH: &str = "sha256-0123456789abcdef";
    let semantic = BackendTarget::variant("html", "semantic");
    let renders = Arc::new(AtomicUsize::new(0));
    let frames = Arc::new(AtomicUsize::new(0));
    let renderer_count = Arc::clone(&renders);
    let docinfo_count = Arc::clone(&frames);
    let renderer = BlockRenderer::for_target(semantic, move || {
        renderer_count.fetch_add(1, Ordering::SeqCst);
        Counter {
            calls: Rc::new(Cell::new(0)),
        }
    })
    .with_html_script_hashes(&[HASH]);
    let docinfo = DocinfoHook::for_target(semantic, move || {
        docinfo_count.fetch_add(1, Ordering::SeqCst);
        |ctx: DocinfoContext<'_>, _: &mut Diagnostics<'_>| {
            Ok((ctx.position == DocinfoPosition::Footer)
                .then(|| "<!--semantic-footer-->".to_owned()))
        }
    });
    let renderer_only = Extensions::default().with_renderer(renderer);
    let docinfo_only = Extensions::default().with_docinfo(docinfo.clone());
    let selection = renderer_only.clone().with_docinfo(docinfo);
    for variant in [HtmlVariant::Standard, HtmlVariant::Semantic] {
        let selected = variant == HtmlVariant::Semantic;
        for embedded in [false, true] {
            let converter = Converter::new_with_variant(
                Options::builder().embedded(embedded).build(),
                ParserOptions::builder(),
                variant,
            )?;
            assert_eq!(converter.uses_render_extensions(&renderer_only), selected);
            assert_eq!(
                converter.uses_render_extensions(&docinfo_only),
                selected && !embedded
            );
            let p = Pipeline::new(converter).with_extensions(selection.clone());
            let text = render(&p, "= Title\n:csp:\n\nParagraph.")?;
            assert_eq!(text.contains("native:1"), selected);
            assert_eq!(
                text.contains("<!--semantic-footer-->"),
                selected && !embedded
            );
            assert_eq!(text.contains(HASH), selected && !embedded);
        }
    }
    assert_eq!(renders.load(Ordering::SeqCst), 2);
    assert_eq!(frames.load(Ordering::SeqCst), 1);
    Ok(())
}

#[test]
fn html_postprocessors_match_family_variant_and_existing_display_names_in_order() -> TestResult {
    let (calls, recorded_calls) = mpsc::channel();
    let mut selection = Extensions::default();
    for (target, hook_name) in [
        (BackendTarget::family("html"), "family"),
        (BackendTarget::variant("html", "semantic"), "semantic"),
        (BackendTarget::variant("html", "standard"), "standard"),
        (BackendTarget::family("html5s"), "wrong-family"),
    ] {
        let calls = calls.clone();
        selection =
            selection.with_postprocessor(TextPostprocessor::for_target(target, move |_, _, _| {
                calls.send(hook_name)?;
                Ok(())
            }));
    }
    for name in ["html", "html5s"] {
        let calls = calls.clone();
        selection = selection.with_postprocessor(TextPostprocessor::new(name, move |_, ctx, _| {
            assert_eq!(ctx.backend, name);
            calls.send(name)?;
            Ok(())
        }));
    }
    for (variant, expected) in [
        (HtmlVariant::Standard, ["family", "standard", "html"]),
        (HtmlVariant::Semantic, ["family", "semantic", "html5s"]),
    ] {
        let converter = Converter::new_with_variant(
            Options::builder().embedded(true).build(),
            ParserOptions::builder(),
            variant,
        )?;
        let p = Pipeline::new(converter).with_extensions(selection.clone());
        assert_eq!(
            p.hook_target(),
            BackendTarget::variant("html", variant.as_str())
        );
        render(&p, "Paragraph.")?;
        assert_eq!(recorded_calls.try_iter().collect::<Vec<_>>(), expected);
    }
    Ok(())
}
