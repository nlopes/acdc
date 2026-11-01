use std::{alloc::System, hint::black_box};

use acdc_parser::{
    CompatibilityMode, ContentAttribute, Document, Location, Options, Reference, SafeMode, Title,
    TocEntry,
    ast::{ContentRef, DocumentRef, InlineKind, ReferenceRef, ReferencesRef, TocEntryRef},
    parse, parse_file,
};
use stats_alloc::{INSTRUMENTED_SYSTEM, Region, StatsAlloc};

#[global_allocator]
static ALLOCATOR: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

type Error = Box<dyn std::error::Error>;

fn text(content: ContentRef<'_>) -> String {
    content
        .inlines()
        .map(|inline| {
            if let InlineKind::Plain(node) = inline.kind() {
                node.text().to_string()
            } else if let InlineKind::Bold(node) = inline.kind() {
                text(node.content())
            } else {
                String::new()
            }
        })
        .collect()
}

#[rstest::rstest]
fn catalog_reads_match_registered_values_and_preserve_json(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let options = Options::builder().with_compatibility_mode(mode).build()?;
    for source in [
        include_str!("../fixtures/tests/section_reference_metadata.adoc"),
        include_str!("../fixtures/tests/named_section_reftext_cross_references.adoc"),
        "[[alias]]\n[#primary,reftext='*Label*']\n== Title\n\n[#untitled]\nBody.\n\n[#empty,reftext='']\n.Empty title\nBody.\n",
        "[bibliography]\n== References\n\n* [[[book,Book]]] Bibliography.\n\nSee <<book>>.\n",
    ] {
        let parsed = parse(source, &options)?;
        let owner = parsed.document();
        let before = serde_json::to_value(owner)?;
        let doc = parsed.document_ref();
        let references = doc.references();
        assert_eq!(references.len(), owner.references.len());
        assert_eq!(references.is_empty(), owner.references.is_empty());
        assert_eq!(references.iter().len(), references.len());
        assert!(references.get("not-registered").is_none());
        for (id, reference) in references.iter() {
            let original = owner.references.get(id).ok_or("registered reference")?;
            assert!(references.contains(id));
            assert_eq!(reference.title().is_some(), original.title.is_some());
            assert_eq!(
                reference.xreflabel().is_some(),
                original.xreflabel.is_some()
            );
            assert_eq!(
                reference.location().absolute_start(),
                original.location.absolute_start
            );
            assert_eq!(Location::from(reference.location()), original.location);
            assert_eq!(reference.section_name(), original.section_name());
            assert_eq!(reference.section_number(), original.section_number());
            if let (Some(number), Some(original)) =
                (reference.section_number(), original.section_number())
            {
                assert!(std::ptr::eq(number, original));
            }
            assert_eq!(reference.is_bibliography(), original.is_bibliography());
            assert_eq!(
                reference.has_automatic_citation(),
                original.has_automatic_citation()
            );
            assert_eq!(
                reference.title(),
                references.get(id).ok_or("lookup")?.title()
            );
        }
        let mut entries = doc.toc_entries();
        assert_eq!(entries.len(), owner.toc_entries.len());
        assert_eq!(
            entries.next_back().map(TocEntryRef::id),
            owner.toc_entries.last().map(|entry| entry.id)
        );
        for (entry, original) in doc.toc_entries().zip(&owner.toc_entries) {
            assert_eq!(entry.id(), original.id);
            assert_eq!(entry.level(), original.level);
            assert_eq!(entry.kind(), original.kind);
            assert_eq!(entry.number(), original.number());
            assert_eq!(entry.title().is_some(), original.title.is_present());
            assert_eq!(entry.xreflabel().is_some(), original.xreflabel.is_some());
            assert_eq!(Location::from(entry.location()), original.location);
        }
        assert_eq!(serde_json::to_value(owner)?, before);
    }
    Ok(())
}

#[test]
fn catalog_labels_keep_absent_and_empty_values_separate() -> Result<(), Error> {
    let parsed = parse(
        "[#plain]\n== Title\n\n[#label,reftext='*Chosen*']\n== Other\n\n[#empty,reftext='']\n== Empty\n\n[#untitled]\nBody.\n",
        &Options::default(),
    )?;
    let references = parsed.document_ref().references();
    let plain = references.get("plain").ok_or("plain")?;
    assert_eq!(plain.title().map(text).as_deref(), Some("Title"));
    assert!(plain.xreflabel().is_none());
    let label = references.get("label").ok_or("label")?;
    assert_eq!(label.xreflabel().map(text).as_deref(), Some("Chosen"));
    assert_eq!(label.title().map(text).as_deref(), Some("Other"));
    let empty = references.get("empty").ok_or("empty")?;
    assert!(empty.xreflabel().is_none());
    let untitled = references.get("untitled").ok_or("untitled")?;
    assert!(untitled.title().is_none());
    assert!(untitled.xreflabel().is_none());
    assert!(references.get("missing").is_none());
    let mut owned = parsed
        .document()
        .references
        .get("plain")
        .ok_or("plain owner")?
        .clone();
    owned.title = Some(Title::new(Vec::new()));
    owned.xreflabel = Some(ContentAttribute::new(Vec::new()));
    let empty_title = ReferenceRef::from(&owned);
    assert!(empty_title.title().is_some_and(ContentRef::is_empty));
    assert!(empty_title.xreflabel().is_some_and(ContentRef::is_empty));
    Ok(())
}

#[test]
fn catalog_reads_do_not_repair_caller_edited_trees() -> Result<(), Error> {
    let parsed = parse("[#target]\n== Original\n", &Options::default())?;
    let mut document = Document::default();
    document.references = parsed.document().references.clone();
    document.toc_entries = parsed.document().toc_entries.clone();
    document.blocks.clear();
    let doc = DocumentRef::from(&document);
    assert_eq!(
        doc.references()
            .get("target")
            .ok_or("target")?
            .title()
            .map(text)
            .as_deref(),
        Some("Original")
    );
    assert_eq!(
        doc.toc_entries()
            .next()
            .ok_or("TOC entry")?
            .title()
            .map(text)
            .as_deref(),
        Some("Original")
    );
    document.blocks = parsed.document().blocks.clone();
    document.references.clear();
    document.toc_entries.clear();
    let doc = DocumentRef::from(&document);
    assert!(doc.blocks().next().is_some());
    assert!(doc.references().is_empty());
    assert_eq!(doc.toc_entries().len(), 0);
    Ok(())
}

#[test]
fn catalog_handles_are_shared_and_do_not_allocate_on_reads() -> Result<(), Error> {
    fn shared<T: Copy + Send + Sync>() {}
    shared::<ReferencesRef<'_>>();
    shared::<ReferenceRef<'_>>();
    shared::<TocEntryRef<'_>>();
    let parsed = parse("[#target]\n== Title\n", &Options::default())?;
    let doc = parsed.document_ref();
    let region = Region::new(ALLOCATOR);
    let references = doc.references();
    black_box(references.get("target"));
    black_box(references.contains("target"));
    black_box(references.len());
    black_box(references.iter().count());
    for entry in doc.toc_entries() {
        black_box((
            entry.id(),
            entry.title(),
            entry.xreflabel(),
            entry.level(),
            entry.kind(),
            entry.number(),
            entry.location(),
        ));
    }
    for (_, reference) in references.iter() {
        black_box((
            reference.title(),
            reference.xreflabel(),
            reference.section_name(),
            reference.section_number(),
            reference.is_bibliography(),
            reference.has_automatic_citation(),
            reference.location(),
        ));
    }
    let stats = region.change();
    assert_eq!(stats.allocations, 0);
    assert_eq!(stats.reallocations, 0);
    Ok(())
}

#[test]
fn empty_document_has_no_fabricated_catalog_entries() {
    let document = Document::default();
    let doc = DocumentRef::from(&document);
    assert!(doc.references().is_empty());
    assert_eq!(doc.references().iter().len(), 0);
    assert_eq!(doc.toc_entries().len(), 0);
}

#[test]
fn catalog_locations_resolve_to_the_included_source() -> Result<(), Error> {
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/tests");
    let parsed = parse_file(
        &fixtures.join("xref_included_sources.adoc"),
        &Options::builder()
            .with_safe_mode(SafeMode::Unsafe)
            .build()?,
    )?;
    let doc = parsed.document_ref();
    let reference = doc
        .references()
        .get("included-target")
        .ok_or("included target")?;
    let entry = doc
        .toc_entries()
        .find(|entry| entry.id() == "included-target")
        .ok_or("included entry")?;
    let expected = fixtures.join("xref_include_target.adoc").canonicalize()?;
    for (location, line) in [(reference.location(), 1), (entry.location(), 2)] {
        let resolved = parsed.resolve_location(location);
        assert_eq!(
            resolved.file.ok_or("source file")?.canonicalize()?,
            expected
        );
        assert_eq!(resolved.location.start.line, line);
        assert_eq!(
            location
                .start()
                .include_chain()
                .ok_or("include chain")?
                .collect::<Vec<_>>(),
            ["xref_include_target.adoc"]
        );
    }
    Ok(())
}

#[rstest::rstest]
fn catalog_snapshots_preserve_registered_state_without_refreshing(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let options = Options::builder().with_compatibility_mode(mode).build()?;
    for source in [
        include_str!("../fixtures/tests/section_reference_metadata.adoc"),
        include_str!("../fixtures/tests/xref_reftext_order.adoc"),
        "= Catalog\n\n[#figure]\n.A *bird*\nimage::bird.svg[]\n\n[bibliography]\n== References\n\n* [[[book,Book]]] Entry.\n\nSee <<book>>.\n",
    ] {
        let parsed = parse(source, &options)?;
        let original = parsed.document();
        let before = serde_json::to_value(original)?;
        let doc = parsed.document_ref();
        let mut references = std::collections::HashMap::from(doc.references());
        for (id, reference) in doc.references().iter() {
            assert_eq!(
                &Reference::from(reference),
                references.get(id).ok_or("copied target")?
            );
        }
        let mut entries: Vec<_> = doc.toc_entries().map(TocEntry::from).collect();
        assert_eq!(references, original.references);
        assert_eq!(entries, original.toc_entries);
        let original_references = original.references.clone();
        let original_entries = original.toc_entries.clone();
        assert_eq!(
            serde_json::to_value(&entries)?,
            serde_json::to_value(&original.toc_entries)?
        );
        for reference in references.values_mut() {
            reference.title = Some(Title::new(Vec::new()));
            reference.xreflabel = Some(ContentAttribute::new(Vec::new()));
            reference.caption = None;
            reference.location = Location::default();
        }
        for entry in &mut entries {
            entry.title = Title::new(Vec::new());
            entry.xreflabel = Some(ContentAttribute::new(Vec::new()));
            entry.location = Location::default();
        }
        assert_eq!(serde_json::to_value(original)?, before);
        assert_eq!(original.references, original_references);
        assert_eq!(original.toc_entries, original_entries);

        let mut stale = Document::default();
        stale.references = original.references.clone();
        stale.toc_entries = original.toc_entries.clone();
        stale.blocks.clear();
        let stale_doc = DocumentRef::from(&stale);
        for (id, reference) in stale_doc.references().iter() {
            assert_eq!(
                &Reference::from(reference),
                original.references.get(id).ok_or("original target")?
            );
        }
        assert_eq!(
            stale_doc
                .toc_entries()
                .map(TocEntry::from)
                .collect::<Vec<_>>(),
            original.toc_entries
        );
    }
    Ok(())
}

#[test]
fn catalog_snapshots_keep_absent_and_explicitly_empty_content() -> Result<(), Error> {
    let parsed = parse("[#target]\n== Original\n", &Options::default())?;
    let original = parsed.document().references.get("target").ok_or("target")?;
    for title in [None, Some(Title::new(Vec::new()))] {
        for label in [None, Some(ContentAttribute::new(Vec::new()))] {
            let mut owner = original.clone();
            owner.title = title.clone();
            owner.xreflabel = label;
            let copied = Reference::from(ReferenceRef::from(&owner));
            assert_eq!(copied, owner);
        }
    }
    let mut entry = parsed
        .document()
        .toc_entries
        .first()
        .ok_or("entry")?
        .clone();
    entry.title = Title::new(Vec::new());
    entry.xreflabel = Some(ContentAttribute::new(Vec::new()));
    assert_eq!(TocEntry::from(TocEntryRef::from(&entry)), entry);
    Ok(())
}
