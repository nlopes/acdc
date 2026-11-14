use std::{
    cell::Cell,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use acdc_parser::{
    Block, Error, InlineMacro, InlineNode, Options, SafeMode, WarningKind,
    extensions::{
        BlockBuilder, BlockContext, BlockMacro, DocumentEditor, MacroTarget, Registry, TargetId,
        TreeProcessor, TreeProcessorRegistration,
    },
};

use super::{TestResult, paragraph_text, support};

struct Fresh {
    local: Rc<Cell<usize>>,
    calls: Arc<AtomicUsize>,
    drops: Arc<AtomicUsize>,
    fail: bool,
}

impl TreeProcessor for Fresh {
    fn process(&mut self, editor: &mut DocumentEditor<'_>) -> Result<(), Error> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.local.set(self.local.get() + 1);
        let request = editor.deferred_blocks().first().expect("pending request");
        if self.fail {
            return Err(Error::Extension(
                Box::new(request.invocation.location.clone()),
                "deliberate tree failure".into(),
            ));
        }
        let target = request.target().clone();
        editor.replace_placeholder(
            &target,
            BlockBuilder::paragraph(self.local.get().to_string()),
        )
    }
}

impl Drop for Fresh {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::Relaxed);
    }
}

#[test]
fn extensions_tree_processors_are_fresh_local_and_retained_until_result_drop() -> TestResult {
    let calls = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let owned_calls = Arc::clone(&calls);
    let owned_drops = Arc::clone(&drops);
    let mut builder = Registry::builder();
    builder.register(BlockMacro::new("defer", || support::Defer))?;
    builder.register_tree_processor(TreeProcessorRegistration::new("fresh", move || Fresh {
        local: Rc::new(Cell::new(0)),
        calls: Arc::clone(&owned_calls),
        drops: Arc::clone(&owned_drops),
        fail: false,
    }))?;
    let registry = builder.build();
    assert!(!registry.is_empty());
    let first = registry.parse("defer::[]", &Options::default())?;
    let second = registry.clone().parse("defer::[]", &Options::default())?;
    assert_eq!(
        paragraph_text(&first.parsed.document_storage().blocks[0]),
        "1"
    );
    assert_eq!(
        paragraph_text(&second.parsed.document_storage().blocks[0]),
        "1"
    );
    assert_eq!(calls.load(Ordering::Relaxed), 2);
    assert_eq!(drops.load(Ordering::Relaxed), 0);
    drop(first);
    drop(second);
    assert_eq!(drops.load(Ordering::Relaxed), 2);
    Ok(())
}

#[test]
fn extensions_tree_errors_stop_later_processors_at_included_request() -> TestResult {
    let calls = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let mut builder = Registry::builder();
    builder
        .register(BlockMacro::new("defer", || support::Defer).with_target(MacroTarget::Required))?;
    for name in ["failure", "later"] {
        let calls = Arc::clone(&calls);
        let drops = Arc::clone(&drops);
        builder.register_tree_processor(TreeProcessorRegistration::new(name, move || Fresh {
            local: Rc::new(Cell::new(0)),
            calls: Arc::clone(&calls),
            drops: Arc::clone(&drops),
            fail: true,
        }))?;
    }
    let registry = builder.build();
    let options = Options::builder()
        .with_safe_mode(SafeMode::Unsafe)
        .build()?;
    let error = registry
        .parse_file(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("fixtures/tests/includes/extensions/extension-tree-error.adoc"),
            &options,
        )
        .expect_err("tree processor error");
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert_eq!(drops.load(Ordering::Relaxed), 2);
    assert_eq!(
        error
            .source_location()
            .expect("request location")
            .location
            .start
            .line,
        3
    );
    assert!(
        error
            .source_location()
            .and_then(|location| location.file.as_ref())
            .is_some_and(|file| file.ends_with("extension-tree-error.adoc"))
    );
    assert!(registry.parse("defer::[]", &Options::default()).is_err());
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert_eq!(drops.load(Ordering::Relaxed), 4);
    Ok(())
}

struct CheckedEdits;

impl TreeProcessor for CheckedEdits {
    fn process(&mut self, editor: &mut DocumentEditor<'_>) -> Result<(), Error> {
        let target = editor
            .query()
            .select(BlockContext::Paragraph)
            .next()
            .expect("paragraph")
            .target()
            .clone();
        let before = serde_json::to_string(editor.document_storage()).expect("document JSON");
        let foreign = Registry::default().parse("Foreign", &Options::default())?;
        let foreign_target = foreign
            .query()
            .blocks()
            .next()
            .expect("foreign")
            .target()
            .clone();
        assert!(editor.remove_section(&target).is_err());
        assert!(
            editor
                .replace_placeholder(&target, BlockBuilder::paragraph("bad"))
                .is_err()
        );
        assert!(editor.discard_placeholder(&target).is_err());
        assert!(
            editor
                .insert_before(&foreign_target, BlockBuilder::paragraph("bad"))
                .is_err()
        );
        assert!(
            editor
                .insert_before(&target, BlockBuilder::paragraph("bad").with_id(""))
                .is_err()
        );
        assert!(
            editor
                .insert_before(&target, BlockBuilder::paragraph("bad").with_id("existing"))
                .is_err()
        );
        assert_eq!(
            serde_json::to_string(editor.document_storage()).expect("document JSON"),
            before
        );
        let new = editor.insert_before(
            &target,
            BlockBuilder::paragraph("*literal* <&> {counter:x}").with_id("new"),
        )?;
        assert_eq!(
            paragraph_text(editor.query().get(&new).expect("inserted").storage()),
            "*literal* <&> {counter:x}"
        );
        assert!(editor.document_storage().references.contains_key("new"));
        Ok(())
    }
}

#[test]
fn extensions_tree_only_registry_preserves_parsing_and_checks_edits_before_mutation() -> TestResult
{
    let mut builder = Registry::builder();
    builder.register_tree_processor(TreeProcessorRegistration::new("checked", || CheckedEdits))?;
    assert!(
        builder
            .register_tree_processor(TreeProcessorRegistration::new("checked", || CheckedEdits))
            .is_err()
    );
    assert!(
        builder
            .register_tree_processor(TreeProcessorRegistration::new("bad:name", || CheckedEdits))
            .is_err()
    );
    let result = builder
        .build()
        .parse("[#existing]\nOriginal", &Options::default())?;
    assert_eq!(result.parsed.document_storage().blocks.len(), 2);
    Ok(())
}

struct FillNested;

impl TreeProcessor for FillNested {
    fn process(&mut self, editor: &mut DocumentEditor<'_>) -> Result<(), Error> {
        let targets: Vec<_> = editor
            .deferred_blocks()
            .iter()
            .map(|request| request.target().clone())
            .collect();
        for target in &targets {
            let created = editor.insert_before(target, BlockBuilder::paragraph("Inserted"))?;
            let id = editor.ensure_id(&created)?.to_owned();
            assert_eq!(
                editor.query().get(&created).expect("inserted").output_id(),
                Some(id.as_str())
            );
            editor.replace_placeholder(target, BlockBuilder::paragraph("Filled"))?;
        }
        let sections: Vec<_> = editor
            .query()
            .select(BlockContext::Section)
            .filter(|view| view.output_id() == Some("cell-remove"))
            .map(|view| view.target().clone())
            .collect();
        for section in sections {
            editor.remove_section(&section)?;
        }
        assert_eq!(
            targets
                .iter()
                .filter(|target| editor.query().get(target).is_some())
                .count(),
            6
        );
        Ok(())
    }
}

#[test]
fn extensions_tree_edits_follow_list_compound_and_table_cell_containers() -> TestResult {
    let mut builder = Registry::builder();
    builder.register(BlockMacro::new("defer", || support::Defer))?;
    builder.register_tree_processor(TreeProcessorRegistration::new("nested", || FillNested))?;
    let result = builder.build().parse(
        include_str!("../../../fixtures/tests/includes/extensions/extension-tree-nested.adoc"),
        &Options::default(),
    )?;
    assert!(result.deferred_blocks().is_empty());
    assert_eq!(
        result
            .query()
            .select(BlockContext::Paragraph)
            .filter(|view| paragraph_text(view.storage()) == "Filled")
            .count(),
        6
    );
    assert!(
        !result
            .parsed
            .document_storage()
            .references
            .contains_key("cell-remove")
    );
    let query = result.query();
    let cell = query
        .select(BlockContext::Paragraph)
        .find(|view| view.output_id() == Some("cell-keep-content"))
        .expect("kept cell");
    assert_eq!(
        cell.containing_section()
            .and_then(|target| query.get(target))
            .and_then(|view| view.output_id()),
        Some("cell-keep")
    );
    Ok(())
}

struct Producer(Arc<AtomicUsize>);

impl TreeProcessor for Producer {
    fn process(&mut self, editor: &mut DocumentEditor<'_>) -> Result<(), Error> {
        self.0.fetch_add(1, Ordering::Relaxed);
        let requests: Vec<_> = editor
            .deferred_blocks()
            .iter()
            .map(|request| {
                (
                    request.target().clone(),
                    request.invocation.target == "discard",
                )
            })
            .collect();
        for (target, discard) in requests {
            let target = &target;
            if discard {
                editor.discard_placeholder(target)?;
            } else {
                let inserted = editor.insert_before(
                    target,
                    BlockBuilder::paragraph("Produced").with_id("produced"),
                )?;
                assert_eq!(editor.ensure_id(&inserted)?, "produced");
                assert!(editor.query().get(target).is_some());
            }
        }
        Ok(())
    }
}

struct Consumer(Arc<AtomicUsize>);

impl TreeProcessor for Consumer {
    fn process(&mut self, editor: &mut DocumentEditor<'_>) -> Result<(), Error> {
        self.0.fetch_add(1, Ordering::Relaxed);
        let selection: Vec<_> = editor
            .query()
            .select(BlockContext::Paragraph)
            .map(|view| paragraph_text(view.storage()))
            .collect();
        let targets: Vec<_> = editor
            .deferred_blocks()
            .iter()
            .map(|request| request.target().clone())
            .collect();
        for target in targets {
            editor.replace_placeholder(&target, BlockBuilder::unordered_list(selection.clone()))?;
        }
        assert!(editor.deferred_blocks().is_empty());
        Ok(())
    }
}

struct Remove(Arc<AtomicUsize>);

struct RemoveFirstAndAssign;

impl TreeProcessor for RemoveFirstAndAssign {
    fn process(&mut self, editor: &mut DocumentEditor<'_>) -> Result<(), Error> {
        let first = editor
            .query()
            .select(BlockContext::Section)
            .next()
            .expect("first")
            .target()
            .clone();
        editor.remove_section(&first)?;
        let unnamed = editor
            .query()
            .select(BlockContext::Section)
            .find(|view| view.output_id().is_none())
            .expect("unnamed child")
            .target()
            .clone();
        let id = editor.ensure_id(&unnamed)?.to_owned();
        assert_eq!(
            editor.document_storage().references[id.as_str()].section_number(),
            Some("2.1")
        );
        Ok(())
    }
}

#[test]
fn extensions_tree_removal_preserves_duplicate_and_hidden_section_numbers() -> TestResult {
    let mut builder = Registry::builder();
    builder.register_tree_processor(TreeProcessorRegistration::new("remove", || {
        RemoveFirstAndAssign
    }))?;
    let result = builder.build().parse("= Guide\n:sectnums:\n:sectnumlevels: 1\n:idprefix:\n\n[#duplicate]\n== First\n\n[#duplicate]\n== Second\n\n=== !!!\n\nSee <<duplicate>>.", &Options::default())?;
    let document = result.parsed.document_storage();
    assert_eq!(document.references["duplicate"].section_number(), Some("2"));
    assert_eq!(document.toc_entries.len(), 2);
    assert_eq!(document.toc_entries[0].number(), Some("2"));
    assert_eq!(document.toc_entries[1].number(), None);
    assert!(
        !result
            .parsed
            .warnings()
            .iter()
            .any(|warning| matches!(warning.kind, WarningKind::DuplicateId { .. }))
    );
    Ok(())
}

struct RemoveAndFill;

impl TreeProcessor for RemoveAndFill {
    fn process(&mut self, editor: &mut DocumentEditor<'_>) -> Result<(), Error> {
        Remove(Arc::new(AtomicUsize::new(0))).process(editor)?;
        let targets: Vec<_> = editor
            .deferred_blocks()
            .iter()
            .map(|request| request.target().clone())
            .collect();
        for target in targets {
            editor.replace_placeholder(&target, BlockBuilder::paragraph("Generated"))?;
        }
        Ok(())
    }
}

#[test]
fn extensions_generated_captions_keep_source_labels_and_advance_past_removed_ordinals() -> TestResult
{
    let mut builder = Registry::builder();
    builder.register(BlockMacro::new("defer", || support::Defer))?;
    builder.register_tree_processor(TreeProcessorRegistration::new("fill", || RemoveAndFill))?;
    let result = builder.build().parse(
        include_str!("../../../fixtures/tests/includes/extensions/extension-tree-captions.adoc"),
        &Options::default(),
    )?;
    let references = &result.parsed.document_storage().references;
    assert!(matches!(&references["automatic"].caption,
        Some(acdc_parser::Caption::Numbered { label, number: Some(number), .. })
        if label == "Early" && number.get() == 3));
    assert!(
        matches!(&references["custom"].caption,
        Some(acdc_parser::Caption::CustomContent(label)) if matches!(label.as_inlines(), [InlineNode::RawText(text)] if text.content == "Custom ")),
        "{:?}",
        references["custom"].caption
    );
    assert!(matches!(&references["survivor"].caption,
        Some(acdc_parser::Caption::Numbered { number: Some(number), .. }) if number.get() == 2));
    Ok(())
}

impl TreeProcessor for Remove {
    fn process(&mut self, editor: &mut DocumentEditor<'_>) -> Result<(), Error> {
        self.0.fetch_add(1, Ordering::Relaxed);
        let targets: Vec<_> = editor
            .query()
            .select(BlockContext::Section)
            .filter(|view| view.output_id() == Some("gone"))
            .map(|view| view.target().clone())
            .collect();
        for target in targets {
            editor.remove_section(&target)?;
            assert!(editor.query().get(&target).is_none());
        }
        Ok(())
    }
}

#[test]
fn extensions_tree_order_and_snapshot_selection() -> TestResult {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut builder = Registry::builder();
    builder.register(BlockMacro::new("defer", || support::Defer))?;
    for name in ["producer", "consumer", "remover"] {
        let calls = Arc::clone(&calls);
        match name {
            "producer" => {
                builder
                    .register_tree_processor(TreeProcessorRegistration::new(name, move || {
                        Producer(Arc::clone(&calls))
                    }))?;
            }
            "consumer" => {
                builder
                    .register_tree_processor(TreeProcessorRegistration::new(name, move || {
                        Consumer(Arc::clone(&calls))
                    }))?;
            }
            _ => {
                builder
                    .register_tree_processor(TreeProcessorRegistration::new(name, move || {
                        Remove(Arc::clone(&calls))
                    }))?;
            }
        }
    }
    let result = builder.build().parse("[#gone]\n== Remove\n\nRemoved\n\n[#stay]\n== Keep\n\n.Title\n[#snapshot]\n[role=kept]\ndefer::content[]\n\ndefer::discard[]", &Options::default())?;
    assert_eq!(calls.load(Ordering::Relaxed), 3);
    assert!(result.deferred_blocks().is_empty());
    let query = result.query();
    let view = query
        .select(BlockContext::UnorderedList)
        .next()
        .ok_or("missing replacement")?;
    assert_eq!(view.output_id(), Some("snapshot"));
    assert!(
        view.storage()
            .metadata()
            .ok_or("missing metadata")?
            .roles
            .contains(&"kept")
    );
    let Block::UnorderedList(list) = view.storage() else {
        panic!("expected list")
    };
    let items: Vec<_> = list
        .items
        .iter()
        .flat_map(|item| item.principal.iter())
        .filter_map(|inline| {
            if let InlineNode::PlainText(text) = inline {
                Some(text.content)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(items, ["Removed", "Produced"]);
    assert!(
        !result
            .parsed
            .document_storage()
            .references
            .contains_key("gone")
    );
    assert_eq!(result.parsed.document_storage().toc_entries.len(), 1);
    Ok(())
}

struct CaptureAndRemove;

impl TreeProcessor for CaptureAndRemove {
    fn process(&mut self, editor: &mut DocumentEditor<'_>) -> Result<(), Error> {
        let gone = editor
            .query()
            .select(BlockContext::Section)
            .find(|view| view.output_id() == Some("gone"))
            .expect("gone")
            .target()
            .clone();
        let targets: Vec<TargetId> = editor
            .query()
            .blocks()
            .map(|view| view.target().clone())
            .collect();
        editor.remove_section(&gone)?;
        assert!(editor.deferred_blocks().is_empty());
        let stay = editor
            .query()
            .select(BlockContext::Section)
            .next()
            .expect("stay")
            .target()
            .clone();
        editor.insert_before(&stay, BlockBuilder::paragraph("New owner").with_id("gone"))?;
        assert!(editor.query().get(&targets[0]).is_none());
        assert!(editor.query().get(&targets[1]).is_none());
        assert!(editor.ensure_id(&targets[0]).is_err());
        assert_eq!(
            editor.query().get(&targets[2]).expect("stay").output_id(),
            Some("stay")
        );
        assert_eq!(
            paragraph_text(
                editor
                    .query()
                    .get(&targets[3])
                    .expect("surviving paragraph")
                    .storage()
            ),
            "Survives"
        );
        Ok(())
    }
}

#[test]
fn extensions_removed_handles_do_not_rebind_and_surviving_paths_shift() -> TestResult {
    let mut builder = Registry::builder();
    builder.register(BlockMacro::new("defer", || support::Defer))?;
    builder.register_tree_processor(TreeProcessorRegistration::new("capture", || {
        CaptureAndRemove
    }))?;
    let result = builder.build().parse(
        "[#gone]\n== Remove\n\ndefer::pending[]\n\n[#stay]\n== Keep\n\nSurvives",
        &Options::default(),
    )?;
    assert_eq!(
        result
            .query()
            .select(BlockContext::Paragraph)
            .next()
            .expect("new owner")
            .output_id(),
        Some("gone")
    );
    Ok(())
}

#[test]
fn extensions_removal_repairs_catalogs_without_renumbering_survivors() -> TestResult {
    let mut builder = Registry::builder();
    builder.register_tree_processor(TreeProcessorRegistration::new("remove", || {
        Remove(Arc::new(AtomicUsize::new(0)))
    }))?;
    let result = builder.build().parse(
        include_str!("../../../fixtures/tests/includes/extensions/extension-tree-numbering.adoc"),
        &Options::default(),
    )?;
    let document = result.parsed.document_storage();
    assert_eq!(document.toc_entries.len(), 1);
    assert_eq!(document.toc_entries[0].number(), Some("2"));
    let section = result
        .query()
        .select(BlockContext::Section)
        .next()
        .ok_or("missing section")?;
    let Block::Section(section) = section.storage() else {
        panic!("expected section")
    };
    assert_eq!(section.number(), Some("2"));
    assert_eq!(document.references["stay"].section_number(), Some("2"));
    assert!(!document.references.contains_key("gone"));
    assert!(!document.references.contains_key("inline-gone"));
    assert!(!document.references.contains_key("figure-gone"));
    assert!(!document.references.contains_key("_removed_child"));
    assert_eq!(
        document.references["figure-stay"].caption,
        result
            .query()
            .select(BlockContext::Image)
            .next()
            .and_then(|view| view
                .storage()
                .metadata()
                .and_then(|metadata| metadata.caption.as_ref()))
            .cloned()
    );
    assert!(matches!(document.references["figure-stay"].caption,
        Some(acdc_parser::Caption::Numbered { number: Some(number), .. }) if number.get() == 2));
    assert_eq!(document.footnotes.len(), 1);
    assert_eq!(document.footnotes[0].number, 1);
    assert!(result.parsed.warnings().iter().any(|warning| matches!(&warning.kind, WarningKind::UnresolvedReference { target } if target == "gone")));
    assert!(!result.parsed.warnings().iter().any(|warning| matches!(&warning.kind, WarningKind::UnresolvedReference { target } if target == "only-in-removed")));
    let mut footnotes = Vec::new();
    for block in &section.content {
        if let Block::Paragraph(paragraph) = block {
            for inline in &paragraph.content {
                if let InlineNode::Macro(InlineMacro::Footnote(note)) = inline {
                    footnotes.push(note.number);
                }
            }
        }
    }
    assert_eq!(footnotes, [1]);
    Ok(())
}
