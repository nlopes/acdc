//! Ordering and document-local state with native lists and other processors.

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use acdc_parser::{
    CompatibilityMode, Error, Options,
    extensions::{
        BlockBuilder, BlockContext, BlockMacro, BlockMacroOutput, DocumentEditor,
        MacroExecutionContext, MacroInvocation, Registry, TreeProcessor, TreeProcessorRegistration,
    },
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

struct Produce;

impl TreeProcessor for Produce {
    fn process(&mut self, editor: &mut DocumentEditor<'_>) -> Result<(), Error> {
        let targets: Vec<_> = editor
            .deferred_blocks()
            .map(|request| request.target().clone())
            .collect();
        for target in targets {
            editor.replace_placeholder(
                &target,
                BlockBuilder::listing("Generated code.")
                    .with_attributes("title=Generated")
                    .with_id("produced"),
            )?;
        }
        Ok(())
    }
}

#[test]
fn producers_before_and_after_lists_keep_registration_order_and_run_once() -> TestResult {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        for producer_first in [true, false] {
            let calls = Arc::new(AtomicUsize::new(0));
            let counter = Arc::clone(&calls);
            let mut builder = Registry::builder();
            builder.register(BlockMacro::new("produce", move || {
                let calls = Arc::clone(&counter);
                move |_: &MacroInvocation<'_>, _: &mut MacroExecutionContext| {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Ok(BlockMacroOutput::Deferred)
                }
            }))?;
            let registration = TreeProcessorRegistration::for_macro("produce", || Produce);
            if producer_first {
                builder.register_tree_processor(registration.clone())?;
            }
            builder.extend(&acdc_extensions_lists::registry()?)?;
            if !producer_first {
                builder.register_tree_processor(registration)?;
            }
            let options = Options::builder().with_compatibility_mode(mode).build()?;
            let mut working = builder
                .build()
                .parse("list-of::listing[]\n\nproduce::[]", &options)?;
            assert_eq!(working.query().select(BlockContext::Listing).count(), 1);
            assert_eq!(
                working.query().select(BlockContext::Paragraph).count(),
                usize::from(producer_first)
            );
            working.normalize()?;
            working.normalize()?;
            assert_eq!(working.finish()?.warnings(), []);
            assert_eq!(calls.load(Ordering::SeqCst), 1);
        }
    }
    Ok(())
}

#[test]
fn parallel_documents_share_registrations_without_sharing_titles_or_ids() -> TestResult {
    let registry = acdc_extensions_lists::registry()?;
    let mut workers = Vec::new();
    for index in 0..8 {
        let registry = registry.clone();
        workers.push(std::thread::spawn(move || -> Result<_, String> {
            let source = format!("list-of::image[]\n\n.Worker {index}\nimage::target.svg[]");
            let mut modes = Vec::new();
            for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
                let options = Options::builder()
                    .with_compatibility_mode(mode)
                    .build()
                    .map_err(|error| error.to_string())?;
                let working = registry
                    .parse(&source, &options)
                    .map_err(|error| error.to_string())?;
                let ids: Vec<_> = working
                    .query()
                    .select(BlockContext::Image)
                    .filter_map(|view| view.output_id().map(str::to_owned))
                    .collect();
                let parsed = working.finish().map_err(|error| error.to_string())?;
                assert_eq!(parsed.warnings(), []);
                let tree =
                    serde_json::to_string(&parsed.document()).map_err(|error| error.to_string())?;
                modes.push((ids, tree));
            }
            Ok((index, modes))
        }));
    }
    let mut expected_ids = None;
    for worker in workers {
        let (index, modes) = worker.join().map_err(|_| "worker panicked")??;
        for (ids, tree) in modes {
            assert_eq!(ids.len(), 1);
            if let Some(expected) = &expected_ids {
                assert_eq!(&ids, expected);
            } else {
                expected_ids = Some(ids);
            }
            assert!(tree.contains(&format!("Worker {index}")), "{tree}");
            for other in (0..8).filter(|other| *other != index) {
                assert!(!tree.contains(&format!("Worker {other}")), "{tree}");
            }
        }
    }
    Ok(())
}
