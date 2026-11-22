//! An active registry must not change how ordinary content parses.

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use acdc_parser::{
    CompatibilityMode, Options, WarningKind,
    ast::{BlockKind, BlockRef},
    extensions::{BlockBuilder, BlockMacro, MacroExecutionContext, MacroInvocation, Registry},
};

use super::TestResult;

fn greet_registry(calls: &Arc<AtomicUsize>) -> Result<Registry, Box<dyn std::error::Error>> {
    let calls = Arc::clone(calls);
    let mut builder = Registry::builder();
    builder.register(BlockMacro::new("greet", move || {
        let calls = Arc::clone(&calls);
        move |invocation: &MacroInvocation<'_>, _: &mut MacroExecutionContext| {
            calls.fetch_add(1, Ordering::SeqCst);
            // Echo the source so parity tests can compare the entire document.
            Ok(BlockBuilder::paragraph(invocation.source.to_string()).into())
        }
    }))?;
    Ok(builder.build())
}

fn section_levels<'doc>(blocks: impl Iterator<Item = BlockRef<'doc>>, levels: &mut Vec<u8>) {
    for block in blocks {
        if let BlockKind::Section(section) = block.kind() {
            levels.push(section.level());
            section_levels(section.blocks(), levels);
        }
    }
}

fn out_of_sequence(warnings: &[acdc_parser::Warning]) -> usize {
    warnings
        .iter()
        .filter(|warning| matches!(warning.kind, WarningKind::SectionLevelOutOfSequence { .. }))
        .count()
}

#[test]
fn out_of_sequence_warnings_survive_preprocessor_offset_shifts() -> TestResult {
    let source = "greet::x[]\n\nifdef::nothing[]\nhidden\nendif::[]\n\n== A\n\n==== B\n";
    for mode in [CompatibilityMode::Draft, CompatibilityMode::Asciidoctor] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let ordinary = acdc_parser::parse(source, &options)?;
        assert_eq!(out_of_sequence(ordinary.warnings()), 1);
        let calls = Arc::new(AtomicUsize::new(0));
        let extended = greet_registry(&calls)?.parse(source, &options)?.finish()?;
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(out_of_sequence(extended.warnings()), 1, "{mode:?}");
    }
    Ok(())
}

#[test]
fn caller_leveloffset_applies_in_extension_sessions() -> TestResult {
    for source in [
        "greet::x[]\n\n== Section\n",
        "ifdef::nothing[]\nhidden\nendif::[]\ngreet::x[]\n\n== Section\n",
    ] {
        let options = Options::builder()
            .with_attribute("leveloffset", "1")
            .build()?;
        let mut ordinary = Vec::new();
        section_levels(
            acdc_parser::parse(source, &options)?.document().blocks(),
            &mut ordinary,
        );
        assert_eq!(ordinary, [2]);
        let calls = Arc::new(AtomicUsize::new(0));
        let extended = greet_registry(&calls)?.parse(source, &options)?.finish()?;
        let mut levels = Vec::new();
        section_levels(extended.document().blocks(), &mut levels);
        assert_eq!(levels, ordinary, "{source:?}");
    }
    Ok(())
}

#[test]
fn macro_lines_with_trailing_text_or_spaced_targets_stay_ordinary_content() -> TestResult {
    for mode in [CompatibilityMode::Draft, CompatibilityMode::Asciidoctor] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        for source in ["greet::World[] says hello", "greet:: some term[x]"] {
            let calls = Arc::new(AtomicUsize::new(0));
            let extended = greet_registry(&calls)?.parse(source, &options)?.finish()?;
            assert_eq!(calls.load(Ordering::SeqCst), 0, "{source:?}");
            let ordinary = acdc_parser::parse(source, &options)?;
            let kinds = |blocks: Vec<BlockRef<'_>>| {
                blocks
                    .into_iter()
                    .map(|block| format!("{:?}", std::mem::discriminant(&block.kind())))
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                kinds(extended.document().blocks().collect()),
                kinds(ordinary.document().blocks().collect()),
                "{source:?}"
            );
            assert_eq!(extended.warnings(), ordinary.warnings(), "{source:?}");
        }
    }
    Ok(())
}

#[test]
fn unquoted_none_arguments_follow_compatibility_mode() -> TestResult {
    for (mode, expected) in [
        (CompatibilityMode::Draft, Some("None".to_owned())),
        (CompatibilityMode::Asciidoctor, None),
    ] {
        let seen = Arc::new(std::sync::Mutex::new(None));
        let mut builder = Registry::builder();
        let record = Arc::clone(&seen);
        builder.register(BlockMacro::new("greet", move || {
            let record = Arc::clone(&record);
            move |call: &MacroInvocation<'_>, _: &mut MacroExecutionContext| {
                *record.lock().map_err(|e| e.to_string())? =
                    Some(call.attributes.get("mode").map(ToString::to_string));
                Ok(BlockBuilder::paragraph("hello").into())
            }
        }))?;
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let _parsed = builder
            .build()
            .parse("greet::x[mode=None]", &options)?
            .finish()?;
        assert_eq!(
            seen.lock().map_err(|e| e.to_string())?.clone(),
            Some(expected),
            "{mode:?}"
        );
    }
    Ok(())
}

#[test]
fn streamed_styled_quote_recovers_at_its_parent_section_boundary() -> TestResult {
    let calls = Arc::new(AtomicUsize::new(0));
    let registry = greet_registry(&calls)?;
    for mode in [CompatibilityMode::Draft, CompatibilityMode::Asciidoctor] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        for (intro, ending) in [
            ("\\greet::[]", "____\n\nAfter."),
            ("=== Child\n\n\\greet::[]", "____\n\nAfter."),
            ("\\greet::[]", ""),
        ] {
            let source = format!(
                "= Doc\n\n== A\n\n[quote]\n____\n{intro}\n\nifdef::x[]\nX.\nendif::[]\n\n== B\n\ngreet::discarded[]\n{ending}"
            );
            let ordinary = acdc_parser::parse(&source, &options)?;
            let extended = registry.parse(&source, &options)?.finish()?;
            assert!(
                ordinary
                    .warnings()
                    .iter()
                    .any(|w| matches!(w.kind, WarningKind::ContentRecovery { .. }))
            );
            assert_eq!(extended.warnings(), ordinary.warnings(), "{mode:?}");
            assert_eq!(
                serde_json::to_value(extended.document())?,
                serde_json::to_value(ordinary.document())?,
                "{mode:?}"
            );
            assert_eq!(extended.document().toc_entries().len(), 1);
            assert_eq!(ordinary.document().toc_entries().len(), 1);
        }
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    Ok(())
}

#[test]
fn streamed_styled_quote_retains_a_deeper_section() -> TestResult {
    let source = "== A\n\n[quote]\n____\n\\greet::[]\n\nifdef::x[]\nX.\nendif::[]\n\n=== Child\n\ntext\n____\n\nAfter.";
    let calls = Arc::new(AtomicUsize::new(0));
    let registry = greet_registry(&calls)?;
    for mode in [CompatibilityMode::Draft, CompatibilityMode::Asciidoctor] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let ordinary = acdc_parser::parse(source, &options)?;
        let extended = registry.parse(source, &options)?.finish()?;
        assert_eq!(ordinary.document().toc_entries().len(), 2);
        assert_eq!(extended.document().toc_entries().len(), 2);
        assert_eq!(extended.warnings(), ordinary.warnings(), "{mode:?}");
        assert_eq!(
            serde_json::to_value(extended.document())?,
            serde_json::to_value(ordinary.document())?,
            "{mode:?}"
        );
    }
    Ok(())
}

#[rstest::rstest]
#[case::unordered("* item")]
#[case::description("term:: def")]
fn table_in_streamed_list_attachment(
    #[case] item: &str,
    #[values(
        "|===\n|===",
        "|===",
        "|===\n|Cell",
        "[cols=a]\n|===\n|Cell",
        "[separator=;]\n|===\n;Cell",
        ",===\nCell",
        "|===\n|===="
    )]
    table: &str,
    #[values(CompatibilityMode::Draft, CompatibilityMode::Asciidoctor)] mode: CompatibilityMode,
) -> TestResult {
    let calls = Arc::new(AtomicUsize::new(0));
    let registry = greet_registry(&calls)?;
    let options = Options::builder().with_compatibility_mode(mode).build()?;
    let source =
        format!("{item}\n+\n====\nifdef::x[]\nX.\nendif::[]\ngreet::[]\n{table}\n====\n\nAfter.");
    let ordinary = acdc_parser::parse(&source, &options)?;
    let extended = registry.parse(&source, &options)?.finish()?;
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    if table.ends_with("\n|===") {
        assert_eq!(ordinary.warnings(), &[]);
    } else {
        assert!(
            matches!(ordinary.warnings(), [warning] if matches!(warning.kind, WarningKind::UnterminatedTable { .. }))
        );
    }
    assert_eq!(extended.warnings(), ordinary.warnings());
    assert_eq!(
        serde_json::to_value(extended.document())?,
        serde_json::to_value(ordinary.document())?
    );
    Ok(())
}
