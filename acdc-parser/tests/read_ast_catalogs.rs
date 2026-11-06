use std::{alloc::System, hint::black_box};

use acdc_parser::{
    ContentAttribute, Document, Options, Reference, SafeMode,
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

#[test]
fn catalog_labels_keep_absent_and_empty_values_separate() -> Result<(), Error> {
    let parsed = parse(
        "[#plain]\n== Title\n\n[#label,reftext='*Chosen*']\n== Other\n\n[#empty,reftext='']\n== Empty\n\n[#untitled]\nBody.\n",
        &Options::default(),
    )?;
    let references = parsed.document().references();
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
    let mut owned = Reference::from(
        parsed
            .document()
            .references()
            .get("plain")
            .ok_or("plain owner")?,
    );
    owned.set_title(Some(ContentAttribute::new(Vec::new())));
    owned.set_xreflabel(Some(ContentAttribute::new(Vec::new())));
    let empty_title = ReferenceRef::from(&owned);
    assert!(empty_title.title().is_some_and(ContentRef::is_empty));
    assert!(empty_title.xreflabel().is_some_and(ContentRef::is_empty));
    Ok(())
}

#[test]
fn catalog_handles_are_shared_and_do_not_allocate_on_reads() -> Result<(), Error> {
    fn shared<T: Copy + Send + Sync>() {}
    shared::<ReferencesRef<'_>>();
    shared::<ReferenceRef<'_>>();
    shared::<TocEntryRef<'_>>();
    let parsed = parse("[#target]\n== Title\n", &Options::default())?;
    let doc = parsed.document();
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
        fixtures.join("xref_included_sources.adoc"),
        &Options::builder()
            .with_safe_mode(SafeMode::Unsafe)
            .build()?,
    )?;
    let doc = parsed.document();
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

#[test]
fn caller_constructed_catalog_reads_do_not_allocate_or_copy_entries() -> Result<(), Error> {
    let parsed = parse(
        "[#target,reftext='*Chosen*']\n== Title\n",
        &Options::default(),
    )?;
    let catalog: std::collections::HashMap<_, _> = parsed.document().references().into();
    let before = serde_json::to_value(parsed.document())?;
    let region = Region::new(ALLOCATOR);
    let references = ReferencesRef::from(&catalog);
    let reference = references.get("target").ok_or("target")?;
    black_box(reference.title());
    black_box(reference.xreflabel());
    black_box(reference.location());
    black_box(references.iter().count());
    let stats = region.change();
    assert_eq!(stats.allocations, 0);
    assert_eq!(stats.reallocations, 0);
    assert_eq!(references.len(), catalog.len());
    assert_eq!(serde_json::to_value(parsed.document())?, before);
    Ok(())
}
