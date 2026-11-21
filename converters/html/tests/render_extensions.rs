//! Native renderer activation, dispatch, failures, and conversion-local state.

use std::{
    cell::Cell,
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use acdc_converters_core::{
    Converter as _, Diagnostics, Options,
    pipeline::{ExtensionCatalog, Extensions, Pipeline},
    render::{BlockRenderProcessor, BlockRenderer, RenderError, RenderHost},
};
use acdc_converters_html::Converter;
use acdc_parser::{
    Options as ParserOptions,
    ast::{BlockKind, BlockRef},
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

struct Counter {
    calls: Rc<Cell<usize>>,
}

impl BlockRenderProcessor for Counter {
    fn render(
        &mut self,
        block: BlockRef<'_>,
        host: &mut dyn RenderHost,
    ) -> Result<bool, RenderError> {
        if !matches!(block.kind(), BlockKind::Paragraph(_)) {
            return Ok(false);
        }
        self.calls.set(self.calls.get() + 1);
        host.write(format!("<p>native:{}</p>\n", self.calls.get()).as_bytes())?;
        Ok(true)
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
    fn render(&mut self, _: BlockRef<'_>, _: &mut dyn RenderHost) -> Result<bool, RenderError> {
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
    fn render(
        &mut self,
        block: BlockRef<'_>,
        host: &mut dyn RenderHost,
    ) -> Result<bool, RenderError> {
        if !matches!(block.kind(), BlockKind::Paragraph(_)) {
            return Ok(false);
        }
        self.trace
            .lock()
            .map_err(|_| "trace lock poisoned")?
            .push(self.label);
        if self.handled {
            host.write(self.label.as_bytes())?;
        }
        Ok(self.handled)
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
