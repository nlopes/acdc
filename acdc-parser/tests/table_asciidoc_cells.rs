use acdc_parser::{
    AttributeValue, Block, DelimitedBlockType, Document, InlineNode, Options, Paragraph, Table,
    TableColumn, parse,
};

type Error = Box<dyn std::error::Error>;

fn table<'block, 'a>(block: &'block Block<'a>) -> Result<&'block Table<'a>, Error> {
    let Block::DelimitedBlock(block) = block else {
        return Err("expected a delimited block".into());
    };
    let DelimitedBlockType::DelimitedTable(table) = &block.inner else {
        return Err("expected a table".into());
    };
    Ok(table)
}

fn inline_text(nodes: &[InlineNode<'_>]) -> String {
    nodes
        .iter()
        .filter_map(|node| {
            let InlineNode::PlainText(text) = node else {
                return None;
            };
            Some(text.content)
        })
        .collect()
}

fn paragraph_text(paragraph: &Paragraph<'_>) -> String {
    inline_text(&paragraph.content)
}

fn cell_paragraphs(cell: &TableColumn<'_>) -> Vec<String> {
    cell.content
        .iter()
        .filter_map(|block| {
            let Block::Paragraph(paragraph) = block else {
                return None;
            };
            Some(paragraph_text(paragraph))
        })
        .collect()
}

fn outer_table<'document, 'a>(
    document: &'document Document<'a>,
) -> Result<&'document Table<'a>, Error> {
    let Block::Section(section) = document.blocks.first().ok_or("expected an outer section")?
    else {
        return Err("expected an outer section".into());
    };
    table(section.content.first().ok_or("expected a table")?)
}

#[test]
fn attributes_in_asciidoc_cells_have_nested_document_scope() -> Result<(), Error> {
    let parsed = parse(
        "= T\n:outer: inherited\n\n[cols=\"2*a\"]\n|===\n|\n:cell-local: local\n:sectnums:\n\nInside {cell-local} and {outer}.\n\n:outer: changed\n\nStill {outer}.\n\n:outer!:\n\nStill {outer} after unset.\n\n:cell-local!:\n\nLocal {cell-local} after unset.\n\n|\nSibling {cell-local} and {outer}.\n|===\n\nAfter {cell-local} and {outer}.\n",
        &Options::default(),
    )?;
    let document = parsed.document();
    let table = table(document.blocks.first().ok_or("expected a table")?)?;
    let row = table.rows.first().ok_or("expected a body row")?;
    let first_cell = row.columns.first().ok_or("expected the first cell")?;

    assert_eq!(
        cell_paragraphs(first_cell),
        [
            "Inside local and inherited.",
            "Still inherited.",
            "Still inherited after unset.",
            "Local {cell-local} after unset."
        ]
    );
    let attribute_entries: Vec<_> = first_cell
        .content
        .iter()
        .filter_map(|block| {
            let Block::DocumentAttribute(attribute) = block else {
                return None;
            };
            Some(attribute)
        })
        .collect();
    assert_eq!(attribute_entries.len(), 5);
    let changed_outer = attribute_entries
        .get(2)
        .ok_or("expected changed outer entry")?;
    let unset_outer = attribute_entries
        .get(3)
        .ok_or("expected unset outer entry")?;
    assert_eq!(changed_outer.name.as_ref(), "outer");
    assert_eq!(
        changed_outer.value,
        AttributeValue::String("changed".into())
    );
    assert_eq!(unset_outer.name.as_ref(), "outer");
    assert_eq!(unset_outer.value, AttributeValue::Bool(false));
    assert_eq!(
        cell_paragraphs(row.columns.get(1).ok_or("expected the second cell")?),
        ["Sibling {cell-local} and inherited."]
    );
    assert_eq!(
        document.attributes.get("outer"),
        Some(&AttributeValue::String("inherited".into()))
    );
    assert!(!document.attributes.contains_key("cell-local"));
    assert!(!document.attributes.contains_key("sectnums"));

    let Block::Paragraph(after) = document.blocks.get(1).ok_or("expected trailing content")? else {
        return Err("expected a trailing paragraph".into());
    };
    assert_eq!(paragraph_text(after), "After {cell-local} and inherited.");
    Ok(())
}

#[test]
fn nested_section_numbering_uses_cell_source_order() -> Result<(), Error> {
    let parsed = parse(
        "= T\n:outer-value: inherited\n\n[cols=\"2*a\"]\n|===\n|\n:cell-local: local\n:outer-value: changed\n\nCell sees {cell-local} and {outer-value}.\n\n== Before enabling\n\nBefore.\n\n:sectnums:\n:sectnumlevels: 1\n\n== After enabling\n\nAfter.\n\n=== Child\n\nChild.\n\n|\nSibling sees {cell-local} and {outer-value}.\n|===\n",
        &Options::default(),
    )?;
    let document = parsed.document();
    let parsed_table = table(document.blocks.first().ok_or("expected a table")?)?;
    let blocks = &parsed_table
        .rows
        .first()
        .and_then(|row| row.columns.first())
        .ok_or("expected an AsciiDoc cell")?
        .content;
    let mut sections = blocks.iter().filter_map(|block| {
        let Block::Section(section) = block else {
            return None;
        };
        Some(section)
    });
    let before = sections.next().ok_or("expected the first section")?;
    let after = sections.next().ok_or("expected the second section")?;
    let child = after
        .content
        .iter()
        .find_map(|block| {
            let Block::Section(section) = block else {
                return None;
            };
            Some(section)
        })
        .ok_or("expected the child section")?;

    assert!(before.number().is_none());
    assert_eq!(after.number(), Some("1"));
    assert!(child.number().is_none());

    let inherited = parse(
        "= T\n:sectnums:\n\n[cols=\"1a\"]\n|===\n|\n:sectnums!:\n\n== Still numbered\n|===\n",
        &Options::default(),
    )?;
    let table = table(
        inherited
            .document()
            .blocks
            .first()
            .ok_or("expected a table")?,
    )?;
    let section = table
        .rows
        .first()
        .and_then(|row| row.columns.first())
        .and_then(|cell| {
            cell.content.iter().find_map(|block| {
                let Block::Section(section) = block else {
                    return None;
                };
                Some(section)
            })
        })
        .ok_or("expected a nested section")?;
    assert_eq!(section.number(), Some("1"));
    Ok(())
}

#[test]
fn nested_sections_are_not_outer_toc_entries_but_remain_xref_targets() -> Result<(), Error> {
    let parsed = parse(
        "= T\n\n== Outer\n\n[cols=\"1a\"]\n|===\na|\n== Nested\n\nNested text.\n|===\n\n== After\n\nSee <<_nested>>.\n",
        &Options::default(),
    )?;
    let document = parsed.document();
    let ids: Vec<_> = document.toc_entries.iter().map(|entry| entry.id).collect();

    assert_eq!(ids, ["_outer", "_after"]);
    let nested = document
        .references
        .get("_nested")
        .ok_or("expected a nested section reference")?;
    assert_eq!(
        nested
            .title
            .as_ref()
            .map(|title| inline_text(title.as_ref()))
            .as_deref(),
        Some("Nested")
    );

    let table = outer_table(document)?;
    let cell = table
        .rows
        .first()
        .and_then(|row| row.columns.first())
        .ok_or("expected an AsciiDoc cell")?;
    assert!(matches!(cell.content.first(), Some(Block::Section(_))));
    Ok(())
}
