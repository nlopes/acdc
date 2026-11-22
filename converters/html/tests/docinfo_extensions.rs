//! Frame insertion uses the library pipeline shared with CLI output routing.
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use acdc_converters_core::{
    Converter as _, Diagnostics, Options, Warning,
    docinfo::{DocinfoContext, DocinfoHook, DocinfoPosition},
    pipeline::{ExtensionCatalog, Extensions, Pipeline},
};
use acdc_converters_html::{Converter, HtmlVariant};
use acdc_parser::{Options as ParserOptions, ast::DocumentRef};

type TestResult = Result<(), Box<dyn std::error::Error>>;
fn pipeline(
    embedded: bool,
    variant: HtmlVariant,
    selection: Extensions,
) -> Result<Pipeline<Converter<'static>>, acdc_converters_html::Error> {
    Ok(Pipeline::new(Converter::new_with_variant(
        Options::builder().embedded(embedded).build(),
        ParserOptions::builder(),
        variant,
    )?)
    .with_extensions(selection))
}
fn render(
    p: &Pipeline<Converter<'_>>,
    doc: DocumentRef<'_>,
) -> Result<(String, Vec<Warning>), Box<dyn std::error::Error>> {
    let mut output = Vec::new();
    let mut warnings = Vec::new();
    p.write_to(
        doc,
        &mut output,
        None,
        None,
        &mut Diagnostics::new(&p.converter().warning_source(), &mut warnings),
    )?;
    Ok((String::from_utf8(output)?, warnings))
}
#[test]
fn ordered_docinfo_tracks_traversal_and_is_fresh_for_each_full_document() -> TestResult {
    let count = Arc::new(AtomicUsize::new(0));
    let mut catalog = ExtensionCatalog::default();
    for tag in ["one", "two"] {
        let count = Arc::clone(&count);
        catalog.register(
            tag,
            Extensions::default().with_docinfo(DocinfoHook::new("html", move || {
                count.fetch_add(1, Ordering::SeqCst);
                let mut calls = 0;
                move |ctx: DocinfoContext<'_>, diagnostics: &mut Diagnostics<'_>| {
                    calls += 1;
                    if ctx.position == DocinfoPosition::Footer {
                        diagnostics.warn(tag);
                    }
                    let value = ctx
                        .attributes
                        .get("phase")
                        .and_then(|a| a.text())
                        .unwrap_or("missing");
                    Ok(Some(format!("<!--{tag}-{calls}-{value}-->")))
                }
            })),
        )?;
    }
    let selection = catalog.select(["one", "two", "one"])?.ok_or("selection")?;
    let source = "= Frame\n:phase: early\n:!stylesheet:\n\nText.\n\n:phase: late\n\nEnd.";
    for variant in [HtmlVariant::Standard, HtmlVariant::Semantic] {
        let p = pipeline(false, variant, selection.clone())?;
        let parsed = p.parse(source)?;
        let first = render(&p, parsed.document())?;
        assert_eq!(first, render(&p, parsed.document())?);
        for (text, closing) in [
            ("<!--one-1-early-->\n<!--two-1-early-->", "</head>"),
            ("<!--one-2-early-->\n<!--two-2-early-->", "Text."),
            ("<!--one-3-late-->\n<!--two-3-late-->", "</body>"),
        ] {
            let (_, tail) = first
                .0
                .split_once(text)
                .ok_or_else(|| format!("missing {text}"))?;
            assert!(tail.contains(closing));
        }
        assert_eq!(first.1.len(), 2);
        let other = pipeline(true, variant, selection.clone())?;
        let (embedded, warnings) = render(&other, parsed.document())?;
        assert!(!embedded.contains("<!--one-"));
        assert_eq!(warnings.len(), 0);
    }
    assert_eq!(count.load(Ordering::SeqCst), 8);
    std::thread::scope(|scope| {
        let workers = ["left", "right"].map(|phase| {
            let selected = selection.clone();
            scope.spawn(move || -> Result<(), String> {
                let p =
                    pipeline(false, HtmlVariant::Standard, selected).map_err(|e| e.to_string())?;
                let parsed = p
                    .parse(&format!("= Frame\n:phase: {phase}\n\nBody"))
                    .map_err(|e| e.to_string())?;
                let (html, warnings) = render(&p, parsed.document()).map_err(|e| e.to_string())?;
                assert_eq!(html.matches(&format!("<!--one-3-{phase}-->")).count(), 1);
                assert_eq!(warnings.len(), 2);
                Ok(())
            })
        });
        for worker in workers {
            worker.join().map_err(|_| "docinfo worker panicked")??;
        }
        Ok::<(), String>(())
    })?;
    Ok(())
}

#[test]
fn docinfo_failure_stops_subsequent_hooks_and_keeps_prior_output() -> TestResult {
    let extensions = Extensions::default()
        .with_docinfo(DocinfoHook::new("html", || {
            |ctx: DocinfoContext<'_>, _: &mut Diagnostics<'_>| {
                if ctx.position == DocinfoPosition::Footer {
                    return Err("footer failure".into());
                }
                Ok(None)
            }
        }))
        .with_docinfo(DocinfoHook::new("html", || {
            |ctx: DocinfoContext<'_>, _: &mut Diagnostics<'_>| {
                Ok(Some(format!("<!--late {:?}-->", ctx.position)))
            }
        }));
    let p = pipeline(false, HtmlVariant::Standard, extensions)?;
    let parsed = p.parse("Text")?;
    let mut output = Vec::new();
    let error = p
        .write_to(
            parsed.document(),
            &mut output,
            None,
            None,
            &mut Diagnostics::new(&p.converter().warning_source(), &mut Vec::new()),
        )
        .err()
        .ok_or("expected failure")?;
    assert!(error.to_string().contains("footer failure"));
    let partial = String::from_utf8(output)?;
    assert!(partial.contains("<!--late Header-->"));
    assert!(!partial.contains("<!--late Footer-->"));
    Ok(())
}
