use crate::{Block, Document, Options, Reference, parse};
type Error = Box<dyn std::error::Error>;

fn section_reference<'d, 'a>(
    references: &'d std::collections::HashMap<&str, Reference<'a>>,
    id: &str,
) -> Result<&'d Reference<'a>, Error> {
    references
        .get(id)
        .filter(|reference| reference.section_name().is_some())
        .ok_or_else(|| format!("no section reference for {id}").into())
}

#[test]
fn renumbering_the_sections_refreshes_the_catalog() -> Result<(), Error> {
    let source = "= A\n:sectnums:\n\n[[first]]\n== First\n\nx\n\n[[second]]\n== Second\n\nx\n";
    let parsed = parse(source, &Options::default())?;
    let mut document = Document {
        attributes: parsed.document_storage().attributes.clone(),
        blocks: parsed.document_storage().blocks.clone(),
        toc_entries: parsed.document_storage().toc_entries.clone(),
        references: parsed.document_storage().references.clone(),
        ..Document::default()
    };
    assert_eq!(
        section_reference(&document.references, "second")?.section_number(),
        Some("2")
    );

    // Drop the first section, with its table of contents entry. `[[first]]`
    // is one of the section's anchors rather than its `id`, so it is found by
    // position.
    if let Some(first) = document
        .blocks
        .iter()
        .position(|block| matches!(block, Block::Section(_)))
    {
        document.blocks.remove(first);
    }
    document.toc_entries.retain(|entry| entry.id != "first");
    assert_eq!(document.toc_entries.len(), 1);
    document.renumber_sections();

    // The catalog follows the tree, so a `<<second>>` no longer quotes the
    // number the section had when it was parsed.
    assert_eq!(
        section_reference(&document.references, "second")?.section_number(),
        Some("1")
    );
    Ok(())
}

#[test]
fn section_kind_reads_preserve_an_inconsistent_private_storage_snapshot() {
    let mut section = crate::Section::new(
        crate::Title::default(),
        1,
        Vec::new(),
        crate::Location::default(),
    );
    section.metadata.style = Some("index");
    assert_eq!(
        crate::ast::SectionRef::from(&section).kind(),
        crate::SectionKind::Normal
    );
    assert_eq!(
        crate::ast::SectionRef::from(&section).style(),
        Some("index")
    );
}
