#![allow(clippy::expect_used, clippy::indexing_slicing)]

use std::{
    error::Error,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use acdc_parser::{
    Block, InlineNode, Options, SafeMode, WarningKind,
    extensions::{BlockContext, Registry, TargetError},
};

use super::support;

type TestResult = Result<(), Box<dyn Error>>;

#[test]
fn extensions_query_targets_are_document_local_and_preserve_deferred_identity() -> TestResult {
    let registry = support::registry();
    let mut first = registry.parse("defer::one[]\n\ndefer::two[]", &Options::default())?;
    let second = registry.parse("defer::one[]\n\ndefer::two[]", &Options::default())?;
    let target = first.deferred_blocks()[0].target().clone();
    assert_ne!(&target, second.deferred_blocks()[0].target());
    assert!(second.query().get(&target).is_none());
    assert_eq!(
        first.ensure_id(second.deferred_blocks()[0].target()),
        Err(TargetError::UnknownTarget)
    );
    let id = first.ensure_id(&target)?.to_owned();
    assert_eq!(first.ensure_id(&target)?, id);
    let query = first.query();
    let view = query.get(&target).ok_or("missing deferred target")?;
    assert_eq!(view.context(), BlockContext::Extension);
    assert_eq!(view.output_id(), Some(id.as_str()));
    assert_eq!(first.deferred_blocks()[0].target(), &target);
    assert_eq!(first.deferred_blocks().len(), 2);
    Ok(())
}

#[test]
fn extensions_queries_and_ids_support_discrete_headings() -> TestResult {
    for mode in [
        acdc_parser::CompatibilityMode::Asciidoctor,
        acdc_parser::CompatibilityMode::Draft,
    ] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let mut parsed = support::registry().parse(
            "= Document\n\n[discrete]\n== A *heading*\n\nrecord::target[]",
            &options,
        )?;
        let target = parsed
            .query()
            .select(BlockContext::DiscreteHeading)
            .next()
            .ok_or("missing discrete heading")?
            .target()
            .clone();
        let id = parsed.ensure_id(&target)?.to_owned();
        let query = parsed.query();
        let heading = query.get(&target).ok_or("missing heading target")?;
        assert_eq!(heading.context(), BlockContext::DiscreteHeading);
        assert!(matches!(
            heading.block().kind(),
            acdc_parser::ast::BlockKind::DiscreteHeading(_)
        ));
        assert_eq!(heading.output_id(), Some(id.as_str()));
        assert!(parsed.finish()?.document().references().get(&id).is_some());
    }
    Ok(())
}

#[test]
fn extensions_ids_avoid_authored_inline_and_generated_section_ids() -> TestResult {
    let input = "image::one.svg[]\n\n[[_acdc_1]]\nParagraph.\n\n[#_acdc_1_2]\nSecond.\n\n== acdc 1 3\n\nSection body.";
    let mut parsed = Registry::default().parse(input, &Options::default())?;
    let target = parsed
        .query()
        .select(BlockContext::Image)
        .next()
        .ok_or("missing image")?
        .target()
        .clone();
    let before = parsed.parsed.document_storage().references.len();
    assert_eq!(parsed.ensure_id(&target)?, "_acdc_1_4");
    assert_eq!(parsed.ensure_id(&target)?, "_acdc_1_4");
    assert_eq!(
        parsed.parsed.document_storage().references.len(),
        before + 1
    );
    assert_eq!(parsed.parsed.warnings(), []);
    Ok(())
}

#[test]
fn extensions_ids_preserve_existing_ids_and_query_order() -> TestResult {
    let input =
        "[#authored]\nimage::one.svg[]\n\nimage::two.svg[]\n\n== Heading\n\nimage::three.svg[]";
    let registry = Registry::default();
    let mut first = registry.parse(input, &Options::default())?;
    let mut second = registry.parse(input, &Options::default())?;
    let first_targets: Vec<_> = first
        .query()
        .blocks()
        .map(|view| view.target().clone())
        .collect();
    let second_targets: Vec<_> = second
        .query()
        .blocks()
        .map(|view| view.target().clone())
        .collect();
    for target in &first_targets {
        first.ensure_id(target)?;
    }
    for target in second_targets.iter().rev() {
        second.ensure_id(target)?;
    }
    let ids: Vec<_> = first
        .query()
        .blocks()
        .map(|view| view.output_id().map(str::to_owned))
        .collect();
    let repeated_ids: Vec<_> = second
        .query()
        .blocks()
        .map(|view| view.output_id().map(str::to_owned))
        .collect();
    assert_eq!(ids, repeated_ids);
    assert_eq!(ids[0].as_deref(), Some("authored"));
    assert_eq!(ids[2].as_deref(), Some("_heading"));
    assert_eq!(
        first
            .query()
            .blocks()
            .map(|view| view.target().clone())
            .collect::<Vec<_>>(),
        first_targets
    );
    assert_eq!(
        first.parsed.document_storage(),
        second.parsed.document_storage()
    );
    Ok(())
}

#[test]
fn extensions_assigned_ids_register_titles_captions_and_reference_text() -> TestResult {
    let input = ":figure-caption: Picture\n:refname: Early\n\n.A *formatted* title\n[reftext=\"{refname} *label*\"]\nimage::one.svg[]\n\n:refname: Late\n\nAfter.";
    let mut parsed = Registry::default().parse(input, &Options::default())?;
    let target = parsed
        .query()
        .select(BlockContext::Image)
        .next()
        .ok_or("missing image")?
        .target()
        .clone();
    let id = parsed.ensure_id(&target)?.to_owned();
    let query = parsed.query();
    let image = query.get(&target).ok_or("missing image target")?;
    let reference = parsed
        .parsed
        .document_storage()
        .references
        .get(id.as_str())
        .ok_or("missing reference")?;
    assert_eq!(
        reference
            .title
            .as_ref()
            .map(crate::ContentAttribute::as_inlines),
        image.storage().title().map(crate::Title::as_inlines)
    );
    assert_eq!(
        reference.caption.as_ref(),
        image
            .storage()
            .metadata()
            .and_then(|metadata| metadata.caption.as_ref())
    );
    assert!(reference.xreflabel.as_ref().is_some_and(|label| {
        label
            .as_inlines()
            .iter()
            .any(|inline| matches!(inline, InlineNode::BoldText(_)))
    }));
    assert!(
        matches!(reference.xreflabel.as_ref().and_then(|label| label.as_inlines().first()), Some(InlineNode::PlainText(text)) if text.content == "Early ")
    );
    assert_eq!(parsed.parsed.warnings(), []);
    Ok(())
}

#[test]
fn extensions_generated_ids_do_not_bind_existing_unresolved_references() -> TestResult {
    let mut parsed =
        Registry::default().parse("image::one.svg[]\n\nSee <<_acdc_1>>.", &Options::default())?;
    let target = parsed
        .query()
        .select(BlockContext::Image)
        .next()
        .ok_or("missing image")?
        .target()
        .clone();
    assert_eq!(parsed.ensure_id(&target)?, "_acdc_1_2");
    assert!(
        !parsed
            .parsed
            .document_storage()
            .references
            .contains_key("_acdc_1")
    );
    assert!(
        parsed
            .parsed
            .document_storage()
            .references
            .contains_key("_acdc_1_2")
    );
    assert!(parsed.parsed.warnings().iter().any(|warning| matches!(&warning.kind, WarningKind::UnresolvedReference { target } if target == "_acdc_1")));
    Ok(())
}

#[test]
fn extensions_queries_and_ids_do_not_repeat_processors() -> TestResult {
    let calls = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let mut builder = Registry::builder();
    builder.register(super::counter_macro(&calls, &drops))?;
    let mut parsed = builder
        .build()
        .parse("count::one[]\n\ncount::two[]", &Options::default())?;
    let targets: Vec<_> = parsed
        .query()
        .blocks()
        .map(|view| view.target().clone())
        .collect();
    for target in &targets {
        parsed.ensure_id(target)?;
        parsed.ensure_id(target)?;
    }
    assert_eq!(parsed.query().blocks().count(), 2);
    assert_eq!(calls.load(Ordering::Relaxed), 2);
    assert_eq!(drops.load(Ordering::Relaxed), 0);
    drop(parsed);
    assert_eq!(drops.load(Ordering::Relaxed), 1);
    Ok(())
}

#[test]
fn extensions_query_includes_source_context_and_nested_document_sections() -> TestResult {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/tests/includes/extensions/extensions_queries.adoc");
    let options = Options::builder()
        .with_safe_mode(SafeMode::Unsafe)
        .build()?;
    let parsed = support::registry().parse_file(&path, &options)?;
    let query = parsed.query();
    let images: Vec<_> = query.select(BlockContext::Image).collect();
    assert_eq!(images.len(), 4);
    assert!(images[0].containing_section().is_none());
    assert!(images[1].containing_section().is_some());
    assert!(images[2].containing_section().is_none());
    let source = images[3].source_location();
    assert!(
        source
            .file
            .as_ref()
            .is_some_and(|path| path.ends_with("includes/extensions/extension-query.adoc"))
    );
    assert_eq!(source.location.start.line, 1);
    let sections: Vec<_> = query.select(BlockContext::Section).collect();
    assert_eq!(sections.len(), 3);
    assert!(sections[1].containing_section().is_none());
    assert_eq!(sections[2].containing_section(), Some(sections[0].target()));
    assert_eq!(query.select(BlockContext::Example).count(), 2);
    assert_eq!(query.select(BlockContext::Listing).count(), 1);
    Ok(())
}

#[test]
fn extensions_ids_reject_blocks_without_anchor_storage() -> TestResult {
    let mut parsed =
        Registry::default().parse("Text.\n\n:mode: value\n\nAfter.", &Options::default())?;
    let target = parsed
        .query()
        .select(BlockContext::DocumentAttribute)
        .next()
        .ok_or("missing attribute event")?
        .target()
        .clone();
    let before = serde_json::to_value(parsed.parsed.document_storage())?;
    assert_eq!(
        parsed.ensure_id(&target),
        Err(TargetError::NotReferenceable)
    );
    assert_eq!(
        before,
        serde_json::to_value(parsed.parsed.document_storage())?
    );
    Ok(())
}

#[test]
fn extensions_ids_register_sections_with_empty_automatic_slugs() -> TestResult {
    let mut parsed =
        Registry::default().parse("= D\n:sectnums:\n\n== !!!\n\nBody.", &Options::default())?;
    let target = parsed
        .query()
        .select(BlockContext::Section)
        .next()
        .ok_or("missing section")?
        .target()
        .clone();
    assert!(
        parsed
            .query()
            .get(&target)
            .ok_or("missing target")?
            .output_id()
            .is_none()
    );
    let id = parsed.ensure_id(&target)?.to_owned();
    assert_ne!(id, "");
    let document = parsed.parsed.document_storage();
    let reference = document
        .references
        .get(id.as_str())
        .ok_or("missing section reference")?;
    assert_eq!(reference.section_number(), Some("1"));
    assert_eq!(reference.section_category(), Some("section"));
    assert_eq!(document.toc_entries[0].id, id);
    assert!(!document.references.contains_key(""));
    assert!(matches!(&document.blocks[0], Block::Section(section) if section.id() == id));
    Ok(())
}

#[test]
fn extensions_section_ids_preserve_document_top_references() -> TestResult {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/tests/includes/extensions/extension-query-top.adoc");
    let mut parsed = Registry::default().parse_file(&path, &Options::default())?;
    let top = parsed
        .parsed
        .document_storage()
        .references
        .get("")
        .ok_or("missing document top")?;
    assert_eq!(top.section_category(), None);
    let before =
        serde_json::json!({"title": top.title, "label": top.xreflabel, "location": top.location});
    let target = parsed
        .query()
        .select(BlockContext::Section)
        .next()
        .ok_or("missing section")?
        .target()
        .clone();
    parsed.ensure_id(&target)?;
    let top = parsed
        .parsed
        .document_storage()
        .references
        .get("")
        .ok_or("lost document top")?;
    assert_eq!(top.section_category(), None);
    assert_eq!(
        serde_json::json!({"title": top.title, "label": top.xreflabel, "location": top.location}),
        before
    );
    Ok(())
}

#[test]
fn extensions_ids_reach_nested_containers_and_all_table_row_groups() -> TestResult {
    let input = "====\nimage::example.svg[]\n====\n\n****\nimage::sidebar.svg[]\n****\n\n____\nimage::quote.svg[]\n____\n\n. Ordered\n+\nimage::ordered.svg[]\n\n[source]\n----\ncode <1>\n----\n<1> Explanation\n+\nimage::callout.svg[]\n\n[cols=\"a\",options=\"header,footer\"]\n|===\n|image::head.svg[]\n\n|image::body.svg[]\n\n|image::foot.svg[]\n|===";
    let mut parsed = Registry::default().parse(input, &Options::default())?;
    let images = parsed
        .query()
        .select(BlockContext::Image)
        .map(|view| {
            let Block::Image(image) = view.storage() else {
                return Err("image query returned another context");
            };
            Ok((view.target().clone(), image.source.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    // Header cells contain inline text, not nested document blocks.
    assert_eq!(
        images
            .iter()
            .map(|(_, source)| source.as_str())
            .collect::<Vec<_>>(),
        [
            "example.svg",
            "sidebar.svg",
            "quote.svg",
            "ordered.svg",
            "callout.svg",
            "body.svg",
            "foot.svg"
        ]
    );
    for (target, _) in &images {
        let id = parsed.ensure_id(target)?.to_owned();
        assert!(
            parsed
                .parsed
                .document_storage()
                .references
                .contains_key(id.as_str())
        );
        assert_eq!(
            parsed
                .query()
                .get(target)
                .ok_or("missing image")?
                .output_id(),
            Some(id.as_str())
        );
    }
    Ok(())
}
