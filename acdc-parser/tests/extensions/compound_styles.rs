use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

use acdc_parser::{
    CompatibilityMode, Error, Options, SafeMode, WarningKind,
    ast::{BlockKind, BlockRef, InlineKind, InlineRef},
    extensions::{
        BlockBuilder, BlockContext, BlockMacro, BlockStyle, BlockStyleOutput, DocumentEditor,
        MAX_STYLE_NESTING, MacroExecutionContext, MacroInvocation, RawBlockContext, RawBlockInput,
        Registry, TreeProcessor, TreeProcessorRegistration,
    },
};

use super::TestResult;

#[test]
fn same_style_reenters_after_return_and_child_macros_keep_their_source() -> TestResult {
    let directory = tempfile::tempdir()?;
    let snippet = directory.path().join("panel.adoc");
    std::fs::write(
        &snippet,
        ".Outer\n[panel#outer.wide]\n-----\n[panel#inner]\n----\nlogo::acme[]\n----\n-----",
    )?;
    for mode in [CompatibilityMode::Draft, CompatibilityMode::Asciidoctor] {
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut registry = Registry::builder();
        let style_events = Arc::clone(&events);
        registry.register_block_style(BlockStyle::compound(
            "panel",
            [RawBlockContext::Listing],
            move || {
                let events = Arc::clone(&style_events);
                let mut count = 0;
                move |_: RawBlockInput<'_>| {
                    count += 1;
                    events
                        .lock()
                        .map_err(|e| e.to_string())?
                        .push(format!("panel {count}"));
                    Ok(BlockStyleOutput::ParseChildren)
                }
            },
        ))?;
        let macro_events = Arc::clone(&events);
        registry.register(BlockMacro::new("logo", move || {
            let events = Arc::clone(&macro_events);
            move |call: &MacroInvocation<'_>, ctx: &mut MacroExecutionContext| {
                events
                    .lock()
                    .map_err(|e| e.to_string())?
                    .push(call.target.to_string());
                assert_eq!(call.location.location.start.line, 6);
                ctx.set_attribute("seen", "yes");
                Ok(BlockBuilder::image("acme.svg").into())
            }
        }))?;
        let options = Options::builder()
            .with_compatibility_mode(mode)
            .with_base_dir(directory.path())
            .with_safe_mode(SafeMode::Safe)
            .build()?;
        let parsed = registry
            .build()
            .parse(
                "include::panel.adoc[]\n\nifdef::seen[]\nAfter.\nendif::[]",
                &options,
            )?
            .finish()?;
        assert_eq!(
            *events.lock().map_err(|e| e.to_string())?,
            ["panel 1", "panel 2", "acme"]
        );
        let mut top = parsed.document().blocks();
        let Some(BlockKind::Open(outer)) = top.next().map(BlockRef::kind) else {
            return Err("missing outer open block".into());
        };
        assert_eq!(outer.id().ok_or("outer ID")?.as_str(), "outer");
        assert!(outer.roles().contains("wide"));
        assert!(outer.title().is_some());
        let Some(BlockKind::Open(inner)) = outer.blocks().next().map(BlockRef::kind) else {
            return Err("missing inner open block".into());
        };
        assert_eq!(inner.id().ok_or("inner ID")?.as_str(), "inner");
        let mut children = inner.blocks().map(BlockRef::kind);
        assert!(matches!(
            children.next(),
            Some(BlockKind::DocumentAttribute(_))
        ));
        let Some(BlockKind::Image(image)) = children.next() else {
            return Err("missing nested logo".into());
        };
        assert_eq!(image.source().to_string(), "acme.svg");
        assert_eq!(
            parsed.resolve_location(image.location()).file.as_deref(),
            Some(snippet.as_path())
        );
        assert!(top.any(|block| matches!(block.kind(), BlockKind::Paragraph(_))));
        assert_eq!(parsed.warnings(), []);
    }
    Ok(())
}

struct RewriteParagraph;

impl TreeProcessor for RewriteParagraph {
    fn process(&mut self, editor: &mut DocumentEditor<'_>) -> Result<(), Error> {
        let targets: Vec<_> = editor
            .query()
            .select(BlockContext::Paragraph)
            .map(|view| view.target().clone())
            .collect();
        assert_eq!(targets.len(), 1);
        for target in targets {
            editor.replace_source(&target, "*{label}*")?;
        }
        Ok(())
    }
}

#[test]
fn tree_edit_of_compound_child_uses_local_attributes_and_clears_stale_warning() -> TestResult {
    let source = "= Document\n:label: Header\n\n:label: Early\n\n[panel]\n----\n<<missing>>\n----\n\n:label: Late";
    let calls = Arc::new(AtomicUsize::new(0));
    let panel_calls = Arc::clone(&calls);
    let mut builder = Registry::builder();
    builder.register_block_style(BlockStyle::compound(
        "panel",
        [RawBlockContext::Listing],
        move || {
            let calls = Arc::clone(&panel_calls);
            move |_: RawBlockInput<'_>| {
                calls.fetch_add(1, Ordering::Relaxed);
                Ok(BlockStyleOutput::ParseChildren)
            }
        },
    ))?;
    let panel = builder.build();
    let mut builder = Registry::builder();
    builder.extend(&panel)?;
    builder.register_tree_processor(TreeProcessorRegistration::new("rewrite", || {
        RewriteParagraph
    }))?;
    let with_edit = builder.build();

    for mode in [CompatibilityMode::Draft, CompatibilityMode::Asciidoctor] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let original = panel.parse(source, &options)?.finish()?;
        assert_eq!(calls.swap(0, Ordering::Relaxed), 1);
        assert!(original.warnings().iter().any(|warning| matches!(
            &warning.kind, WarningKind::UnresolvedReference { target } if target == "missing"
        )));

        let edited = with_edit.parse(source, &options)?.finish()?;
        assert_eq!(calls.swap(0, Ordering::Relaxed), 1);
        assert_eq!(edited.warnings(), []);
        let open = edited
            .document()
            .blocks()
            .find_map(|block| {
                if let BlockKind::Open(open) = block.kind() {
                    Some(open)
                } else {
                    None
                }
            })
            .ok_or("missing panel")?;
        let Some(BlockKind::Paragraph(paragraph)) = open.blocks().next().map(BlockRef::kind) else {
            return Err("missing panel paragraph".into());
        };
        assert_eq!(paragraph.source_text(), Some("*{label}*"));
        let mut inlines = paragraph.content().inlines().map(InlineRef::kind);
        let Some(InlineKind::Bold(bold)) = inlines.next() else {
            return Err("expected bold replacement".into());
        };
        assert_eq!(inlines.len(), 0);
        let mut content = bold.content().inlines().map(InlineRef::kind);
        assert!(matches!(content.next(), Some(InlineKind::Plain(text)) if text.text() == "Early"));
        assert_eq!(content.len(), 0);
    }
    Ok(())
}

fn nested(depth: usize) -> String {
    let mut body = "Text.".to_owned();
    for level in 0..depth {
        let delimiter = ".".repeat(4 + level);
        body = format!("[panel]\n{delimiter}\n{body}\n{delimiter}");
    }
    body
}

#[test]
fn nesting_limit_is_per_active_child_parse_and_errors_at_the_included_invocation() -> TestResult {
    let mut builder = Registry::builder();
    builder.register_block_style(BlockStyle::compound(
        "panel",
        [RawBlockContext::Literal],
        || |_: RawBlockInput<'_>| Ok(BlockStyleOutput::ParseChildren),
    ))?;
    let registry = builder.build();
    let allowed = nested(MAX_STYLE_NESTING);
    let parsed = registry
        .parse(&format!("{allowed}\n\n{allowed}"), &Options::default())?
        .finish()?;
    assert_eq!(parsed.document().blocks().count(), 2);
    let directory = tempfile::tempdir()?;
    let file = directory.path().join("nested.adoc");
    let options = Options::builder()
        .with_base_dir(directory.path())
        .with_safe_mode(SafeMode::Safe)
        .build()?;
    let excessive = nested(MAX_STYLE_NESTING + 1);
    for prefix in ["", "* Item\n+\n"] {
        std::fs::write(&file, format!("{prefix}{excessive}"))?;
        let Err(Error::Extension(origin, msg)) = registry.parse("include::nested.adoc[]", &options)
        else {
            return Err("expected a located depth error".into());
        };
        assert!(msg.contains("nesting limit"));
        assert_eq!(origin.file.as_deref(), Some(file.as_path()));
        assert_eq!(
            origin.location.start.line as usize,
            prefix.lines().count() + MAX_STYLE_NESTING * 2 + 1
        );
    }
    Ok(())
}

#[test]
fn only_requested_bodies_execute_children() -> TestResult {
    let mut builder = Registry::builder();
    builder.register_block_style(BlockStyle::compound(
        "panel",
        [RawBlockContext::Listing],
        || {
            |input: RawBlockInput<'_>| {
                Ok(match input.metadata.positional_attribute(0) {
                    Some("replace") => {
                        BlockStyleOutput::Block(BlockBuilder::paragraph("Replacement"))
                    }
                    Some("drop") => BlockStyleOutput::None,
                    _ => BlockStyleOutput::ParseChildren,
                })
            }
        },
    ))?;
    builder.register(BlockMacro::new("fail", || {
        |_: &MacroInvocation<'_>, _: &mut MacroExecutionContext| Err("child ran".into())
    }))?;
    let registry = builder.build();
    let parsed = registry.parse("[panel,drop]\n----\nfail::[]\n----\n\n[panel,replace]\n----\nfail::[]\n----\n\n[panel]\n----\n----", &Options::default())?.finish()?;
    let kinds: Vec<_> = parsed.document().blocks().map(BlockRef::kind).collect();
    assert!(
        matches!(kinds.as_slice(), [BlockKind::Paragraph(_), BlockKind::Open(empty)] if empty.blocks().count() == 0)
    );
    assert!(
        matches!(registry.parse("[panel]\n----\nfail::[]\n----", &Options::default()), Err(Error::Extension(origin, msg)) if origin.location.start.line == 3 && msg == "child ran")
    );
    Ok(())
}
