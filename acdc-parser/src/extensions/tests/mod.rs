#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use std::{
    cell::Cell,
    error::Error as StdError,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use acdc_parser::{
    Block, CompatibilityMode, Error, InlineNode, Options, SafeMode, WarningKind,
    extensions::{
        BlockMacro, BlockMacroOutput, BlockMacroProcessor, MacroContext, MacroInvocation,
        MacroTarget, RegistrationError, Registry,
    },
};

#[path = "support.rs"]
mod support;

#[path = "queries.rs"]
mod queries;

type TestResult = Result<(), Box<dyn StdError>>;

struct Counter {
    calls: Rc<Cell<usize>>,
    total: Arc<AtomicUsize>,
    drops: Arc<AtomicUsize>,
}

impl BlockMacroProcessor for Counter {
    fn process(&mut self, invocation: &MacroInvocation<'_>) -> Result<BlockMacroOutput, String> {
        self.total.fetch_add(1, Ordering::Relaxed);
        self.calls.set(self.calls.get() + 1);
        match invocation.target.as_ref() {
            "fail" => Err("deliberate failure".into()),
            "skip" => Ok(BlockMacroOutput::None),
            _ => Ok(BlockMacroOutput::Paragraph(self.calls.get().to_string())),
        }
    }
}

impl Drop for Counter {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::Relaxed);
    }
}

fn counter_macro(total: &Arc<AtomicUsize>, drops: &Arc<AtomicUsize>) -> BlockMacro {
    let total = Arc::clone(total);
    let drops = Arc::clone(drops);
    BlockMacro::new("count", move || Counter {
        calls: Rc::new(Cell::new(0)),
        total: Arc::clone(&total),
        drops: Arc::clone(&drops),
    })
}

fn paragraph_text(block: &Block<'_>) -> String {
    let Block::Paragraph(paragraph) = block else {
        panic!("expected paragraph, got {block:?}");
    };
    paragraph
        .content
        .iter()
        .filter_map(|node| {
            if let InlineNode::PlainText(text) = node {
                Some(text.content)
            } else {
                None
            }
        })
        .collect()
}

#[test]
fn extensions_fresh_local_processors_live_with_their_result() -> TestResult {
    let total = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let mut builder = Registry::builder();
    builder.register(counter_macro(&total, &drops))?;
    let registry = builder.build();
    let first = registry.parse("count::one[]\n\ncount::two[]", &Options::default())?;
    let second = registry
        .clone()
        .parse("count::one[]", &Options::default())?;
    assert_eq!(
        paragraph_text(&first.parsed.document_storage().blocks[1]),
        "2"
    );
    assert_eq!(
        paragraph_text(&second.parsed.document_storage().blocks[0]),
        "1"
    );
    assert_eq!(total.load(Ordering::Relaxed), 3);
    assert_eq!(drops.load(Ordering::Relaxed), 0);
    drop(first);
    assert_eq!(drops.load(Ordering::Relaxed), 1);
    drop(second);
    assert_eq!(drops.load(Ordering::Relaxed), 2);
    Ok(())
}

#[test]
fn extensions_no_callbacks_for_rejected_document() -> TestResult {
    let total = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let mut builder = Registry::builder();
    builder.register(counter_macro(&total, &drops).with_target(MacroTarget::Required))?;
    let registry = builder.build();
    let error = registry
        .parse("count::one[]\n\ncount::[]", &Options::default())
        .expect_err("missing target");
    assert!(matches!(error, Error::Extension(..)));
    assert_eq!(total.load(Ordering::Relaxed), 0);
    assert_eq!(drops.load(Ordering::Relaxed), 1);
    Ok(())
}

#[test]
fn extensions_processor_error_stops_later_callbacks() -> TestResult {
    let total = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let mut builder = Registry::builder();
    builder.register(counter_macro(&total, &drops))?;
    let error = builder
        .build()
        .parse(
            "count::one[]\n\ncount::fail[]\n\ncount::three[]",
            &Options::default(),
        )
        .expect_err("processor error");
    assert_eq!(
        error
            .source_location()
            .ok_or("missing location")?
            .location
            .start
            .line,
        3
    );
    assert_eq!(total.load(Ordering::Relaxed), 2);
    assert_eq!(drops.load(Ordering::Relaxed), 1);
    Ok(())
}

#[test]
fn extensions_skipped_output_is_absent_from_reference_catalog() -> TestResult {
    let total = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let mut builder = Registry::builder();
    builder.register(counter_macro(&total, &drops))?;
    let parsed = builder.build().parse(
        ".Removed footnote:[removed]\n[#gone]\ncount::skip[]\n\nSee <<gone>> and footnote:[retained].",
        &Options::default(),
    )?;
    assert!(
        !parsed
            .parsed
            .document_storage()
            .references
            .contains_key("gone")
    );
    assert_eq!(parsed.parsed.document_storage().blocks.len(), 1);
    assert_eq!(parsed.parsed.document_storage().footnotes.len(), 1);
    assert_eq!(parsed.parsed.document_storage().footnotes[0].number, 1);
    assert!(parsed.parsed.warnings().iter().any(|warning| matches!(&warning.kind, WarningKind::UnresolvedReference {target} if target == "gone")));
    Ok(())
}

#[test]
fn extensions_context_limits_and_inactive_content() -> TestResult {
    let total = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let mut builder = Registry::builder();
    builder.register(counter_macro(&total, &drops).with_contexts([MacroContext::Compound]))?;
    let parsed = builder.build().parse("count::document[]\n\n====\ncount::compound[]\n====\n\nifdef::missing[]\ncount::inactive[]\nendif::[]", &Options::default())?;
    assert_eq!(
        paragraph_text(&parsed.parsed.document_storage().blocks[0]),
        "count::document[]"
    );
    assert_eq!(total.load(Ordering::Relaxed), 1);
    Ok(())
}

#[test]
fn extensions_registry_rejects_ambiguous_names_and_replaces_explicitly() -> TestResult {
    let total = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let mut builder = Registry::builder();
    builder.register(counter_macro(&total, &drops))?;
    assert!(matches!(
        builder.register(counter_macro(&total, &drops)),
        Err(RegistrationError::DuplicateName(_))
    ));
    builder.replace(counter_macro(&total, &drops).with_target(MacroTarget::Empty))?;
    assert!(
        builder
            .build()
            .parse("count::target[]", &Options::default())
            .is_err()
    );
    for (name, reserved) in [
        ("image", true),
        ("include", true),
        ("bad:name", false),
        ("", false),
    ] {
        let error = Registry::builder()
            .register(BlockMacro::new(name, || Never))
            .expect_err("bad registration");
        assert_eq!(
            matches!(error, RegistrationError::ReservedName(_)),
            reserved
        );
    }
    Ok(())
}

struct Never;
impl BlockMacroProcessor for Never {
    fn process(&mut self, _: &MacroInvocation<'_>) -> Result<BlockMacroOutput, String> {
        panic!("unexpected processor invocation")
    }
}

#[test]
fn extensions_empty_or_unused_registry_preserves_document_and_warnings() -> TestResult {
    let input = include_str!("../../../fixtures/tests/xref_nested_sections.adoc");
    let options = Options::default();
    let ordinary = acdc_parser::parse(input, &options)?;
    for registry in [Registry::default(), support::registry()] {
        let parsed = registry.parse(input, &options)?;
        assert_eq!(
            parsed.parsed.document_storage(),
            ordinary.document_storage()
        );
        assert_eq!(parsed.parsed.warnings(), ordinary.warnings());
    }
    Ok(())
}

#[test]
fn extensions_macro_arguments_preserve_precedence_and_expand_targets() -> TestResult {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let registry = support::registry();
        for (arguments, expected) in [
            ("", serde_json::json!({"first": "fallback"})),
            ("positional", serde_json::json!({"first": "positional"})),
            (
                "positional,first=named",
                serde_json::json!({"first": "named"}),
            ),
            ("first=\"\"", serde_json::json!({"first": ""})),
            (
                ",second=override",
                serde_json::json!({"first": "fallback", "second": "override"}),
            ),
            (
                "first=\"expanded {mode}\"",
                serde_json::json!({"first": "expanded chosen"}),
            ),
        ] {
            let source = format!(":mode: chosen\n\nrecord::{{mode}}[{arguments}]");
            let parsed = registry.parse(&source, &options)?;
            let output: serde_json::Value =
                serde_json::from_str(&paragraph_text(&parsed.parsed.document_storage().blocks[0]))?;
            assert_eq!(output["target"], "chosen");
            assert_eq!(output["attributes"], expected, "{mode:?}: {arguments}");
        }
    }
    Ok(())
}

#[cfg(feature = "asg")]
#[test]
fn extensions_pending_blocks_are_rejected_by_checked_asg_exports() -> TestResult {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let mut builder = Registry::builder();
        builder.register(BlockMacro::new("defer", || support::Defer))?;
        let pending = builder.build().parse("defer::[]", &options)?;
        for profile in [crate::asg::Profile::Upstream, crate::asg::Profile::Acdc] {
            let error =
                crate::asg::export_document_with_profile(pending.parsed.document(), profile)
                    .expect_err("pending requests must not be exported");
            assert!(matches!(error, crate::asg::Error::Unsupported {
                path,
                construct: Some("pending extension block"),
                location: Some(_),
                ..
            } if path == "/blocks/0"));
        }
        assert_eq!(pending.deferred_blocks().len(), 1);
    }
    Ok(())
}

#[test]
fn extensions_deferred_requests_keep_include_context_and_placeholder_identity() -> TestResult {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/tests/includes/extensions/extensions_registered.adoc");
    let options = Options::builder()
        .with_safe_mode(SafeMode::Unsafe)
        .build()?;
    let parsed = support::registry().parse_file(&path, &options)?;
    let requests = parsed.deferred_blocks();
    assert_eq!(requests.len(), 2);
    assert_ne!(requests[0].placeholder, requests[1].placeholder);
    assert!(
        requests[0]
            .invocation
            .location
            .file
            .as_ref()
            .is_some_and(|file| file.ends_with("includes/extensions/extensions.adoc"))
    );
    assert_eq!(requests[0].invocation.location.location.start.line, 5);
    let Block::Section(section) = parsed
        .parsed
        .document_storage()
        .blocks
        .last()
        .ok_or("missing section")?
    else {
        panic!("expected section");
    };
    let Block::Extension(last) = section.content.last().ok_or("missing deferred block")? else {
        panic!("expected placeholder");
    };
    assert_eq!(last.placeholder(), Some(requests[1].placeholder));
    assert_eq!(
        requests[1]
            .invocation
            .positional
            .iter()
            .map(|value| value.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("one"), None, Some("three")]
    );
    Ok(())
}

#[test]
fn extensions_output_captions_use_source_position_attributes() -> TestResult {
    let total = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let mut builder = Registry::builder();
    builder.register(counter_macro(&total, &drops))?;
    let parsed = builder.build().parse(
        ":example-caption: Before\n\n.Title\n[example]\ncount::one[]\n\n:example-caption: After\n\n.Title\n[example]\ncount::two[]",
        &Options::default(),
    )?;
    // Captions are semantic data omitted from the JSON fixture format.
    let captions: Vec<_> = parsed
        .parsed
        .document_storage()
        .blocks
        .iter()
        .filter_map(|block| block.metadata()?.caption.as_ref())
        .collect();
    assert!(
        matches!(captions[0], acdc_parser::Caption::Numbered { label, number, .. } if label == "Before" && number.is_some_and(|number| number.get() == 1))
    );
    assert!(
        matches!(captions[1], acdc_parser::Caption::Numbered { label, number, .. } if label == "After" && number.is_some_and(|number| number.get() == 2))
    );
    Ok(())
}
