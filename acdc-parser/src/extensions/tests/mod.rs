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

#[path = "tree_processors.rs"]
mod tree_processors;

type TestResult = Result<(), Box<dyn StdError>>;

struct Counter {
    calls: Rc<Cell<usize>>,
    total: Arc<AtomicUsize>,
    drops: Arc<AtomicUsize>,
}

impl BlockMacroProcessor for Counter {
    fn process(
        &mut self,
        invocation: &MacroInvocation<'_>,
        _: &mut acdc_parser::extensions::MacroExecutionContext,
    ) -> Result<BlockMacroOutput, String> {
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
fn extensions_committed_callbacks_survive_later_recognition_errors() -> TestResult {
    let total = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let mut builder = Registry::builder();
    builder.register(counter_macro(&total, &drops).with_target(MacroTarget::Required))?;
    let registry = builder.build();
    let error = registry
        .parse("count::one[]\n\ncount::[]", &Options::default())
        .expect_err("missing target");
    assert!(matches!(error, Error::Extension(..)));
    assert_eq!(total.load(Ordering::Relaxed), 1);
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
    fn process(
        &mut self,
        _: &MacroInvocation<'_>,
        _: &mut acdc_parser::extensions::MacroExecutionContext,
    ) -> Result<BlockMacroOutput, String> {
        panic!("unexpected processor invocation")
    }
}

#[test]
fn extensions_empty_or_unused_registry_preserves_document_and_warnings() -> TestResult {
    let options = Options::default();
    for input in [
        include_str!("../../../fixtures/tests/xref_nested_sections.adoc"),
        "= Document\n\n== Parent\n\n==== Skipped\n\n[subs=unknown]\nText",
        "= Document\n\n== Parent\n\n==== Skipped\n\n[subs=unknown]\nText\n\nifdef::missing[]\nrecord::inactive[]\nendif::[]",
        "[format=unknown]\n|===\n|Cell\n\nifdef::missing[]\nrecord::inactive[]\nendif::[]\n|===",
        "[format=unknown,cols=a]\n|===\n|[subs=unknown]\nText\n\nifdef::missing[]\nrecord::inactive[]\nendif::[]\n|===",
    ] {
        let ordinary = acdc_parser::parse(input, &options)?;
        for registry in [Registry::default(), support::registry()] {
            let parsed = registry.parse(input, &options)?;
            assert_eq!(
                parsed.parsed.document_storage(),
                ordinary.document_storage()
            );
            assert_eq!(parsed.parsed.warnings(), ordinary.warnings());
        }
    }
    Ok(())
}

#[test]
fn extensions_nested_sections_preserve_source_position_attribute_snapshots() -> TestResult {
    let input = "== Parent\n\n:mode: outer\n\ndefer::parent[]\n\n=== Child\n\n:mode: inner\n\ndefer::child[]\n\n== Sibling\n\ndefer::sibling[]\n\n:mode: later";
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let parsed = support::registry().parse(input, &options)?;
        let invocations: Vec<_> = parsed
            .deferred_blocks()
            .iter()
            .map(|request| {
                (
                    request.invocation.target.as_ref(),
                    request.invocation.document_attributes.text("mode"),
                    request.invocation.location.location.start.line,
                )
            })
            .collect();
        assert_eq!(
            invocations,
            [
                ("parent", Some("outer"), 5),
                ("child", Some("inner"), 11),
                ("sibling", Some("inner"), 15),
            ]
        );
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

#[test]
fn extensions_callback_attributes_precede_conditionals_and_nested_includes() -> TestResult {
    let directory = tempfile::tempdir()?;
    std::fs::write(
        directory.path().join("outer.adoc"),
        "configure::enabled[yes]\nifdef::enabled[]\ninclude::{enabled}.adoc[]\nendif::[]\nconfigure::enabled[unset=true]\nifndef::enabled[]\nrecord::after[]\nendif::[]",
    )?;
    std::fs::write(
        directory.path().join("yes.adoc"),
        "record::{enabled}[]\n\ndefer::included[]",
    )?;
    let root = directory.path().join("root.adoc");
    let input = "include::outer.adoc[]\n\nifdef::enabled[]\nrecord::inactive[]\nendif::[]\nunknown::ordinary[]";
    std::fs::write(&root, input)?;
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder()
            .with_compatibility_mode(mode)
            .with_safe_mode(SafeMode::Unsafe)
            .with_base_dir(directory.path())
            .build()?;
        for parsed in [
            support::registry().parse(input, &options)?,
            support::registry().parse_file(&root, &options)?,
        ] {
            let document = serde_json::to_string(parsed.parsed.document_storage())?;
            assert!(document.contains("yes"), "{document}");
            assert!(document.contains("after"), "{document}");
            assert!(!document.contains("inactive"), "{document}");
            assert!(document.contains("unknown::ordinary[]"), "{document}");
            let request = &parsed.deferred_blocks()[0];
            assert_eq!(
                request.invocation.document_attributes.text("enabled"),
                Some("yes")
            );
            assert_eq!(request.invocation.location.location.start.line, 3);
            assert!(
                request
                    .invocation
                    .location
                    .file
                    .as_ref()
                    .is_some_and(|path| path.ends_with("yes.adoc"))
            );
            assert_eq!(parsed.deferred_blocks().len(), 1);
            assert_eq!(
                parsed
                    .parsed
                    .conditionals()
                    .map(crate::ConditionalEvaluation::is_active)
                    .collect::<Vec<_>>(),
                [true, true, false]
            );
        }
    }
    Ok(())
}

#[test]
fn extensions_callback_assignments_obey_locks_and_preserve_body_events() -> TestResult {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder()
            .with_compatibility_mode(mode)
            .with_attribute("locked", "caller")
            .with_default_attribute("soft", "caller")
            .build()?;
        let parsed = support::registry().parse("configure::locked[ignored]\nconfigure::soft[accepted]\nconfigure::empty[\"\"]\nrecord::{locked}[{soft},,\"\"]\nconfigure::soft[unset=true]\nifndef::soft[]\nrecord::unset[]\nendif::[]", &options)?;
        let document = parsed.parsed.document_storage();
        let events: Vec<_> = document
            .blocks
            .iter()
            .filter_map(|block| {
                if let Block::DocumentAttribute(event) = block {
                    Some(event.name.as_ref())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(events, ["soft", "empty", "soft"]);
        let paragraphs: Vec<_> = document
            .blocks
            .iter()
            .filter(|block| matches!(block, Block::Paragraph(_)))
            .map(paragraph_text)
            .collect();
        let record: serde_json::Value = serde_json::from_str(&paragraphs[0])?;
        assert_eq!(record["target"], "caller");
        assert_eq!(
            record["positional"],
            serde_json::json!(["accepted", null, ""])
        );
        assert_eq!(record["attributes"]["third"], "");
        assert!(paragraphs[1].contains("unset"));
    }
    Ok(())
}

#[test]
fn extensions_parallel_documents_use_independent_non_send_handlers() -> TestResult {
    let total = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let mut builder = Registry::builder();
    builder.register(counter_macro(&total, &drops))?;
    let registry = builder.build();
    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..4)
            .map(|_| {
                let registry = registry.clone();
                scope.spawn(move || {
                    let result = registry
                        .parse("count::one[]\n\ncount::two[]", &Options::default())
                        .expect("parse");
                    assert_eq!(
                        paragraph_text(&result.parsed.document_storage().blocks[0]),
                        "1"
                    );
                    assert_eq!(
                        paragraph_text(&result.parsed.document_storage().blocks[1]),
                        "2"
                    );
                })
            })
            .collect();
        for handle in handles {
            handle.join().expect("worker");
        }
    });
    assert_eq!(total.load(Ordering::Relaxed), 8);
    assert_eq!(drops.load(Ordering::Relaxed), 4);
    Ok(())
}

#[test]
fn extensions_macro_errors_escape_attachment_recovery_without_later_reads() -> TestResult {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        for (prefix, suffix) in [
            ("", ""),
            ("* Item\n+\n", ""),
            ("Term::\n+\n", ""),
            ("====\n", "====\n"),
            ("[cols=a]\n|===\na|\n", "|===\n"),
        ] {
            let total = Arc::new(AtomicUsize::new(0));
            let drops = Arc::new(AtomicUsize::new(0));
            let mut builder = Registry::builder();
            builder.register(counter_macro(&total, &drops))?;
            let source = format!(
                "{prefix}count::fail[]\n\ninclude::missing.adoc[]\ncount::later[]\n{suffix}"
            );
            let options = Options::builder()
                .with_compatibility_mode(mode)
                .with_safe_mode(SafeMode::Unsafe)
                .build()?;
            let error = builder
                .build()
                .parse(&source, &options)
                .expect_err("callback error");
            assert!(
                matches!(error, Error::Extension(_, ref message) if message == "deliberate failure"),
                "{error:?}"
            );
            assert_eq!(total.load(Ordering::Relaxed), 1, "{prefix}");
        }
    }
    Ok(())
}

#[test]
fn extensions_compound_callbacks_precede_following_conditionals() -> TestResult {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let parsed = support::registry().parse("====\nconfigure::enabled[yes]\n\nifdef::enabled[]\nrecord::selected[]\nendif::[]\n====", &options)?;
        let document = serde_json::to_string(parsed.parsed.document_storage())?;
        assert!(document.contains("selected"), "{document}");
    }
    Ok(())
}

#[test]
fn extensions_table_callbacks_precede_cell_directives_and_keep_local_attributes() -> TestResult {
    let directory = tempfile::tempdir()?;
    std::fs::write(
        directory.path().join("cell.adoc"),
        "record::{local}[]\n\ndefer::inside[]",
    )?;
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder()
            .with_compatibility_mode(mode)
            .with_base_dir(directory.path())
            .with_safe_mode(SafeMode::Unsafe)
            .build()?;
        let source = "[cols=2*a]\n|===\na|configure::local[cell]\n\nifdef::local[]\ninclude::{local}.adoc[]\nendif::[]\na|\nifndef::local[]\nrecord::sibling[]\nendif::[]\n|===\n\nifndef::local[]\nrecord::outer[]\nendif::[]";
        let result = support::registry().parse(source, &options)?;
        let requests = result.deferred_blocks();
        assert_eq!(requests.len(), 1);
        assert_eq!(
            requests[0].invocation.document_attributes.text("local"),
            Some("cell")
        );
        assert!(
            requests[0]
                .invocation
                .location
                .file
                .as_ref()
                .is_some_and(|path| path.ends_with("cell.adoc"))
        );
        assert_eq!(requests[0].invocation.location.location.start.line, 3);
        let document = serde_json::to_string(result.parsed.document_storage())?;
        assert!(document.contains("sibling"), "{document}");
        assert!(document.contains("outer"), "{document}");
        assert_eq!(
            result
                .parsed
                .conditionals()
                .map(crate::ConditionalEvaluation::is_active)
                .collect::<Vec<_>>(),
            [true, true, true]
        );
    }
    Ok(())
}

#[test]
fn extensions_buffered_cells_do_not_change_outer_reader_attributes() -> TestResult {
    let directory = tempfile::tempdir()?;
    let table = "[cols=a]\n|===\n|configure::local[cell]\n|===";
    std::fs::write(directory.path().join("table.adoc"), table)?;
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder()
            .with_compatibility_mode(mode)
            .with_safe_mode(SafeMode::Unsafe)
            .with_base_dir(directory.path())
            .build()?;
        for prefix in [table, "include::table.adoc[]"] {
            let source = format!(
                "{prefix}\n\nifdef::local[]\nrecord::leaked[]\nendif::[]\nifndef::local[]\nrecord::outer[]\nendif::[]"
            );
            let result = support::registry().parse(&source, &options)?;
            let document = serde_json::to_string(result.parsed.document_storage())?;
            assert!(!document.contains("leaked"), "{document}");
            assert!(document.contains("outer"), "{document}");
        }
    }
    Ok(())
}

#[test]
fn extensions_streaming_table_limits_fail_before_callbacks() -> TestResult {
    let total = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let mut builder = Registry::builder();
    builder.register(counter_macro(&total, &drops))?;
    let source = "[cols=a]\n|===\n18446744073709551615*a|count::one[]\nifdef::never[]\nignored\nendif::[]\n|===";
    let error = builder
        .build()
        .parse(source, &Options::default())
        .expect_err("oversized table specifier");
    assert!(matches!(error, Error::Parse(..)), "{error:?}");
    assert_eq!(total.load(Ordering::Relaxed), 0);
    Ok(())
}

#[test]
fn extensions_streamed_incomplete_rows_fail_after_committed_callbacks() -> TestResult {
    let total = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let mut builder = Registry::builder();
    builder.register(counter_macro(&total, &drops))?;
    let error = builder
        .build()
        .parse(
            "[cols=2*a]\n|===\na|count::one[]\nifdef::never[]\nignored\nendif::[]\n|===",
            &Options::default(),
        )
        .expect_err("committed row cannot disappear through recovery");
    assert!(
        matches!(error, Error::Extension(_, ref message) if message.contains("discarded a cell")),
        "{error:?}"
    );
    assert_eq!(total.load(Ordering::Relaxed), 1);
    Ok(())
}

#[test]
fn extensions_duplicated_streamed_cells_keep_distinct_deferred_requests() -> TestResult {
    let result = support::registry().parse(
        "[cols=2*a]\n|===\n2*a|defer::pending[]\nifdef::never[]\nignored\nendif::[]\n|===",
        &Options::default(),
    )?;
    let requests = result.deferred_blocks();
    assert_eq!(requests.len(), 2);
    assert_ne!(requests[0].placeholder, requests[1].placeholder);
    assert_ne!(requests[0].target(), requests[1].target());
    for request in requests {
        assert!(result.query().get(request.target()).is_some());
    }
    Ok(())
}

#[test]
fn extensions_table_formats_preserve_callback_order() -> TestResult {
    for source in [
        "[format=csv,cols=2*a]\n|===\ncount::one[],count::two[]\nifdef::never[]\nignored\nendif::[]\n|===",
        "[cols=a,options=noheader]\n|===\n|count::one[]\n\n|count::two[]\nifdef::never[]\nignored\nendif::[]\n|===",
    ] {
        let total = Arc::new(AtomicUsize::new(0));
        let drops = Arc::new(AtomicUsize::new(0));
        let mut builder = Registry::builder();
        builder.register(counter_macro(&total, &drops))?;
        let result = builder.build().parse(source, &Options::default())?;
        assert_eq!(total.load(Ordering::Relaxed), 2);
        let serialized = serde_json::to_string(result.parsed.document_storage())?;
        assert!(
            serialized
                .find("\"value\":\"1\"")
                .ok_or("first callback output")?
                < serialized
                    .find("\"value\":\"2\"")
                    .ok_or("second callback output")?
        );
    }
    Ok(())
}

#[test]
fn extensions_streamed_table_include_errors_escape_list_recovery() -> TestResult {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).with_safe_mode(SafeMode::Unsafe)
            .with_include_loader(crate::IncludeLoader::custom(|_: &crate::IncludeSourceTarget| -> Result<crate::IncludeSource, crate::IncludeSourceError> {
                Err(crate::IncludeSourceError::new(crate::IncludeSourceErrorKind::Fatal, "deliberate source failure"))
            })).build()?;
        let error = support::registry().parse("* Item\n+\n[cols=a]\n|===\n|configure::enabled[yes]\n\nifdef::enabled[]\ninclude::failure.adoc[]\nendif::[]\n|===\n\nrecord::later[]", &options).expect_err("fatal include error");
        assert!(
            error.to_string().contains("deliberate source failure"),
            "{error:?}"
        );
    }
    Ok(())
}

#[test]
fn extensions_streamed_cell_arguments_use_table_escape_rules() -> TestResult {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let result = support::registry().parse("[cols=a]\n|===\n|record::target[first=\"a\\|b\"]\n\nifdef::never[]\nignored\nendif::[]\n|===", &options)?;
        let document = result.parsed.document_storage();
        let Block::DelimitedBlock(table) = &document.blocks[0] else {
            panic!("table")
        };
        let crate::DelimitedBlockType::DelimitedTable(table) = &table.inner else {
            panic!("table")
        };
        let output: serde_json::Value =
            serde_json::from_str(&paragraph_text(&table.rows[0].columns[0].content[0]))?;
        assert_eq!(output["attributes"]["first"], "a|b", "{mode:?}");
    }
    Ok(())
}
