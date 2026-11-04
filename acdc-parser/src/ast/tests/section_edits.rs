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
    let mut document = Document::default();
    document.attributes = parsed.document().attributes.clone();
    document.blocks = parsed.document().blocks.clone();
    document.toc_entries = parsed.document().toc_entries.clone();
    document.references = parsed.document().references.clone();
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
