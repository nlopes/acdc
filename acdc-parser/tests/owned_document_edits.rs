//! Owned document transfers preserve source lifetimes and registration snapshots.

use acdc_parser::{
    CompatibilityMode, Document, DocumentAttributes, Location, Options, Reference, SafeMode, Title,
    ast::{BlockKind, DocumentRef, StructuralRef},
    parse, parse_file,
};

type Error = Box<dyn std::error::Error>;

const SOURCE: &str = "= Transfers: Subtitle\nÉlodie Durand <elodie@example.org>\n:sectnums:\n\n[#one]\n== First\n\nText footnote:[Note].\n\n[#two]\n== Second\n\nBody.\n";

fn transfer(mut original: Document<'_>) -> Document<'_> {
    let location = Location::from(DocumentRef::from(&original).location());
    let header = original.take_header();
    let attributes = original.take_attributes();
    let blocks = original.take_blocks();
    let footnotes = original.take_footnotes();
    let toc = original.take_toc_entries();
    let references = original.take_references();
    let empty = DocumentRef::from(&original);
    assert!(empty.header().is_none());
    assert_eq!(empty.blocks().count(), 0);
    assert_eq!(empty.footnotes().count(), 0);
    assert_eq!(empty.toc_entries().count(), 0);
    assert!(empty.references().is_empty());
    drop(original);

    let mut rebuilt = Document::default();
    rebuilt.set_header(header);
    rebuilt.set_attributes(attributes);
    rebuilt.set_blocks(blocks);
    rebuilt.set_footnotes(footnotes);
    rebuilt.set_toc_entries(toc);
    rebuilt.set_references(references);
    rebuilt.set_location(location);
    rebuilt
}

fn paragraph_source(document: DocumentRef<'_>) -> Option<&str> {
    document.descendants().find_map(|node| {
        if let StructuralRef::Block(block) = node
            && let BlockKind::Paragraph(paragraph) = block.kind()
        {
            paragraph.source_text()
        } else {
            None
        }
    })
}

#[rstest::rstest]
fn parts_outlive_the_detached_owner_and_keep_all_stored_values(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = String::from(SOURCE);
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let expected = Document::from(parsed.document());
    let rebuilt = transfer(Document::from(parsed.document()));
    assert_eq!(rebuilt, expected);
    assert_eq!(
        serde_json::to_value(DocumentRef::from(&rebuilt))?,
        serde_json::to_value(parsed.document())?
    );
    let original = paragraph_source(parsed.document()).ok_or("source")?;
    let retained = paragraph_source(DocumentRef::from(&rebuilt)).ok_or("retained source")?;
    assert_eq!(original, retained);
    assert!(std::ptr::eq(original.as_ptr(), retained.as_ptr()));
    Ok(())
}

#[rstest::rstest]
fn tree_replacement_leaves_registration_snapshots_unchanged(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        SOURCE,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let mut document = Document::from(parsed.document());
    let blocks = document.take_blocks();
    assert_eq!(blocks.len(), 2);
    document.set_blocks(blocks.rev().take(1));
    let read = DocumentRef::from(&document);
    assert_eq!(read.blocks().count(), 1);
    assert_eq!(
        read.blocks()
            .next()
            .and_then(acdc_parser::ast::BlockRef::id)
            .map(acdc_parser::ast::ElementIdRef::as_str),
        Some("two")
    );
    assert!(read.references().contains("one"));
    assert!(read.references().contains("two"));
    assert_eq!(read.toc_entries().count(), 2);
    assert_eq!(read.footnotes().count(), 1);
    assert_eq!(parsed.document().blocks().count(), 2);
    Ok(())
}

#[rstest::rstest]
fn transfers_preserve_selected_include_provenance(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join("root.adoc");
    let included = directory.path().join("part.adoc");
    std::fs::write(&root, "include::part.adoc[tag=body,indent=0]\n")?;
    std::fs::write(&included, "// tag::body[]\n    λ source.\n// end::body[]\n")?;
    let parsed = parse_file(
        &root,
        &Options::builder()
            .with_compatibility_mode(mode)
            .with_safe_mode(SafeMode::Unsafe)
            .build()?,
    )?;
    let rebuilt = transfer(Document::from(parsed.document()));
    assert_eq!(rebuilt, Document::from(parsed.document()));
    let original = parsed.document().blocks().next().ok_or("original")?;
    let copied = DocumentRef::from(&rebuilt)
        .blocks()
        .next()
        .ok_or("copied")?;
    assert_eq!(
        parsed.resolve_location(copied.location()),
        parsed.resolve_location(original.location())
    );
    assert_eq!(
        parsed.resolve_location(copied.location()).file,
        Some(included)
    );
    assert_eq!(
        paragraph_source(DocumentRef::from(&rebuilt)),
        Some("λ source.")
    );
    Ok(())
}

#[rstest::rstest]
fn reference_transfer_preserves_prepared_values_and_last_duplicate_key(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        SOURCE,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let first = Reference::from(parsed.document().references().get("one").ok_or("first")?);
    let mut second = Reference::from(parsed.document().references().get("two").ok_or("second")?);
    second.title = Some(Title::default());
    let expected = second.clone();
    let mut document = Document::default();
    document.set_references([("same", first), ("same", second)]);
    assert_eq!(DocumentRef::from(&document).references().len(), 1);
    let mut references = document.take_references();
    assert_eq!(references.len(), 1);
    assert_eq!(references.next(), Some(("same", expected)));
    assert_eq!(references.next(), None);
    assert!(DocumentRef::from(&document).references().is_empty());
    Ok(())
}

#[test]
fn default_parts_are_empty_and_replacement_is_explicit() {
    let mut document = Document::default();
    assert!(document.take_header().is_none());
    assert_eq!(document.take_attributes(), DocumentAttributes::default());
    assert_eq!(document.take_blocks().len(), 0);
    assert_eq!(document.take_footnotes().len(), 0);
    assert_eq!(document.take_toc_entries().len(), 0);
    assert_eq!(document.take_references().len(), 0);
    document.set_header(None);
    document.set_attributes(DocumentAttributes::default());
    document.set_blocks([]);
    document.set_footnotes([]);
    document.set_toc_entries([]);
    document.set_references([]);
    document.set_location(Location::default());
    assert_eq!(document, Document::default());
    document.push_block(acdc_parser::Block::Paragraph(acdc_parser::Paragraph::new(
        Vec::new(),
        Location::default(),
    )));
    assert_eq!(DocumentRef::from(&document).blocks().count(), 1);
    document.set_blocks([]);
    assert_eq!(document, Document::default());
}
