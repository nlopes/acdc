use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use acdc_parser::{
    CompatibilityMode, Error, Options, WarningKind,
    extensions::{
        BlockBuilder, BlockMacro, BlockMacroOutput, DocumentEditor, MacroExecutionContext,
        MacroInvocation, Registry, TreeProcessor, TreeProcessorRegistration,
    },
};

use super::TestResult;

/// Native parsing recovers each source by dropping the content that holds
/// `greet::[]` at the given line.
const DISCARDED: [(&str, u32); 7] = [
    // A `[quote]` body inherits the section context and stops at `== B`.
    (
        "= Doc\n\n== A\n\n[quote]\n____\ngreet::[]\n\n== B\n\ntext\n____\n\nAfter.",
        7,
    ),
    ("= Doc\n\n== A\n\n> greet::[]\n> \n> == B\n\nAfter.", 5),
    (
        "= Doc\n\n== A\n\n* item\n+\n[quote]\n____\ngreet::[]\n\n== B\n____\n\nAfter.",
        9,
    ),
    (
        "= Doc\n\n== A\n\nterm:: def\n+\n[quote]\n____\ngreet::[]\n\n== B\n____\n\nAfter.",
        9,
    ),
    // The conditional makes the attached example stream. List recovery then
    // drops the attachment after its child macro has executed.
    (
        "* item\n+\n====\nifdef::x[]\nX.\nendif::[]\ngreet::[]\n|===\n|===\n====\n\nAfter.",
        7,
    ),
    (
        "term:: def\n+\n====\nifdef::x[]\nX.\nendif::[]\ngreet::[]\n|===\n|===\n====\n\nAfter.",
        7,
    ),
    // The processor runs before a conditional splits the quote into reader windows.
    (
        "== A\n\n[quote]\n____\ngreet::[]\n\nifdef::x[]\nX.\nendif::[]\n\n== B\n\ntext\n____",
        5,
    ),
];

fn greet_registry(calls: &Arc<AtomicUsize>) -> Result<Registry, Box<dyn std::error::Error>> {
    struct Resolve;
    impl TreeProcessor for Resolve {
        fn process(&mut self, editor: &mut DocumentEditor<'_>) -> Result<(), Error> {
            let targets: Vec<_> = editor
                .deferred_blocks()
                .map(|request| request.target().clone())
                .collect();
            for target in targets {
                editor.replace_placeholder(&target, BlockBuilder::paragraph("Hello"))?;
            }
            Ok(())
        }
    }

    let counter = Arc::clone(calls);
    let mut builder = Registry::builder();
    builder.register(BlockMacro::new("greet", move || {
        let counter = Arc::clone(&counter);
        move |_: &MacroInvocation<'_>, ctx: &mut MacroExecutionContext| {
            counter.fetch_add(1, Ordering::Relaxed);
            ctx.set_attribute("seen", "yes");
            Ok(BlockMacroOutput::Deferred)
        }
    }))?;
    builder.register_tree_processor(TreeProcessorRegistration::for_macro("greet", || Resolve))?;
    Ok(builder.build())
}

#[test]
fn recovery_reports_content_discarded_after_a_block_macro_ran() -> TestResult {
    let calls = Arc::new(AtomicUsize::new(0));
    let registry = greet_registry(&calls)?;
    let mut failures = Vec::new();
    for mode in [CompatibilityMode::Draft, CompatibilityMode::Asciidoctor] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        for (source, line) in DISCARDED {
            // Native parsing recovers and keeps the document.
            let _recovered = acdc_parser::parse(source, &options)?;
            calls.store(0, Ordering::Relaxed);
            let outcome = registry.parse(source, &options).and_then(|parsed| {
                let pending = parsed.deferred_blocks().len();
                parsed.finish().map(|_| pending)
            });
            let ran = calls.load(Ordering::Relaxed);
            match outcome {
                Err(Error::Extension(location, message))
                    if location.location.start.line == line && ran == 1 =>
                {
                    assert!(message.contains("block processor executed"), "{message}");
                }
                other => failures.push(format!("{mode:?} {source:?}: ran={ran} {other:?}")),
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
    Ok(())
}

#[test]
fn recovery_without_executed_macros_keeps_native_content_recovery() -> TestResult {
    let calls = Arc::new(AtomicUsize::new(0));
    let registry = greet_registry(&calls)?;
    for mode in [CompatibilityMode::Draft, CompatibilityMode::Asciidoctor] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        for (source, _) in DISCARDED.into_iter().take(4) {
            // An escaped macro selects the registry reader without a callback.
            let source = source.replace("greet::[]", "\\greet::[]");
            let native = acdc_parser::parse(&source, &options)?;
            let parsed = registry.parse(&source, &options)?.finish()?;
            assert_eq!(calls.load(Ordering::Relaxed), 0);
            for warnings in [native.warnings(), parsed.warnings()] {
                assert!(
                    warnings
                        .iter()
                        .any(|warning| matches!(warning.kind, WarningKind::ContentRecovery { .. })),
                    "{mode:?} {source:?}: {warnings:?}"
                );
            }
            assert_eq!(
                serde_json::to_value(parsed.document())?,
                serde_json::to_value(native.document())?,
                "{mode:?}: {source}"
            );
        }
    }
    Ok(())
}
