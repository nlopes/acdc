use acdc_parser::{
    CompatibilityMode, DocumentAttributeAssignment, DocumentAttributeValue, Options,
    ast::{
        BlockKind, BlockRef, ContentRef, DocumentRef, FootnoteDefinitionRef, InlineKind,
        ParagraphRef, TableCellRef, TableRef, TocEntryRef,
    },
    parse,
};

type Error = Box<dyn std::error::Error>;

#[test]
fn nested_attribute_locks_distinguish_values_and_unset_origins() -> Result<(), Error> {
    let cases = [
        ("absent", "", "", Options::default(), "child", "{value}"),
        (
            "header value",
            ":value: parent\n",
            "",
            Options::default(),
            "parent",
            "parent",
        ),
        (
            "body value",
            "",
            ":value: parent\n",
            Options::default(),
            "parent",
            "parent",
        ),
        (
            "header unset",
            ":value!:\n",
            "",
            Options::default(),
            "child",
            "{value}",
        ),
        (
            "body unset",
            "",
            ":value!:\n",
            Options::default(),
            "child",
            "{value}",
        ),
        (
            "caller value",
            "",
            "",
            Options::builder()
                .with_attribute("value", "caller")
                .build()?,
            "caller",
            "caller",
        ),
        (
            "caller unset",
            "",
            "",
            Options::builder().with_attribute("value", ()).build()?,
            "{value}",
            "{value}",
        ),
        (
            "default value",
            "",
            "",
            Options::builder()
                .with_default_attribute("value", "default")
                .build()?,
            "default",
            "default",
        ),
        (
            "default unset",
            "",
            "",
            Options::builder()
                .with_default_attribute("value", false)
                .build()?,
            "child",
            "{value}",
        ),
    ];
    for (name, header, body, options, inside, outside) in cases {
        assert_nested_attribute_lock_case(name, header, body, &options, inside, outside)?;
    }
    Ok(())
}

fn assert_nested_attribute_lock_case(
    name: &str,
    header: &str,
    body: &str,
    options: &Options<'_>,
    inside: &str,
    outside: &str,
) -> Result<(), Error> {
    let input = format!(
        "= T\n{header}\n{body}\n[cols=\"2*a\"]\n|===\n|\n:value: child\n\nInside: {{value}}.\n|\nSibling: {{value}}.\n|===\n\nOutside: {{value}}.\n"
    );
    let parsed = parse(&input, options)?;
    let doc = parsed.document();
    let mut cells = first_table(doc)?
        .rows()
        .next()
        .ok_or("missing table row")?
        .cells();
    assert_eq!(
        cell_paragraphs(cells.next().ok_or("missing first cell")?),
        [format!("Inside: {inside}.")],
        "{name}"
    );
    assert_eq!(
        cell_paragraphs(cells.next().ok_or("missing sibling cell")?),
        [format!("Sibling: {outside}.")],
        "{name}"
    );
    let paragraph = last_paragraph(doc)?;
    assert_eq!(
        paragraph_text(paragraph),
        format!("Outside: {outside}."),
        "{name}"
    );
    Ok(())
}

#[test]
fn nested_cells_lock_inherited_universal_defaults() -> Result<(), Error> {
    for (name, value) in [
        ("caution-caption", "Caution"),
        ("version-label", "Version"),
        ("toc-title", "Table of Contents"),
    ] {
        let input = format!("[cols=a]\n|===\n|\n:{name}: child\n\n{{{name}}}\n|===\n");
        let parsed = parse(&input, &Options::default())?;
        let table = table(
            parsed
                .document()
                .blocks()
                .next()
                .ok_or("missing table block")?,
        )?;
        assert_eq!(
            cell_paragraphs(
                table
                    .rows()
                    .next()
                    .ok_or("missing table row")?
                    .cells()
                    .next()
                    .ok_or("missing table cell")?
            ),
            [value],
            "{name}"
        );
    }
    Ok(())
}

#[test]
fn nested_cells_initialize_and_unlock_document_exceptions() -> Result<(), Error> {
    for (name, parent, child) in [
        ("doctype", "book", "article"),
        ("toc-placement", "preamble", "macro"),
        ("toc-position", "left", "right"),
        ("compat-mode", "", "changed"),
    ] {
        let input = format!(
            "= T\n\n[cols=a]\n|===\n|\n:{name}: {child}\n\n{{{name}}}\n|===\n\n{{{name}}}\n"
        );
        let options = Options::builder().with_attribute(name, parent).build()?;
        let parsed = parse(&input, &options)?;
        let table = table(
            parsed
                .document()
                .blocks()
                .next()
                .ok_or("missing table block")?,
        )?;
        assert_eq!(
            cell_paragraphs(
                table
                    .rows()
                    .next()
                    .ok_or("missing table row")?
                    .cells()
                    .next()
                    .ok_or("missing table cell")?
            ),
            [child],
            "{name}"
        );
        assert_eq!(
            parsed
                .document()
                .attributes()
                .get(name)
                .and_then(DocumentAttributeValue::as_str),
            Some(parent)
        );
    }
    let parsed = parse(
        "= T\n:doctype: book\n:toc:\n:showtitle:\n:my-doctype-note: retained\n:compat-mode: parent\n\n[cols=a]\n|===\n|\n{doctype}; {toc}; {showtitle}; {notitle}; {my-doctype-note}; {compat-mode}\n|===\n",
        &Options::default(),
    )?;
    let table = table(
        parsed
            .document()
            .blocks()
            .next()
            .ok_or("missing table block")?,
    )?;
    let cell = table
        .rows()
        .next()
        .ok_or("missing table row")?
        .cells()
        .next()
        .ok_or("missing table cell")?;
    assert_eq!(
        cell_paragraphs(cell),
        ["article; {toc}; {showtitle}; ; retained; "]
    );
    assert!(
        cell.initial_attributes()
            .any(|(name, value)| name == "doctype"
                && value.value().and_then(DocumentAttributeValue::as_str) == Some("article"))
    );
    Ok(())
}

fn last_paragraph(document: DocumentRef<'_>) -> Result<ParagraphRef<'_>, Error> {
    let Some(BlockKind::Paragraph(paragraph)) = document.blocks().last().map(BlockRef::kind) else {
        return Err("missing outer paragraph".into());
    };
    Ok(paragraph)
}

fn first_table(document: DocumentRef<'_>) -> Result<TableRef<'_>, Error> {
    document
        .blocks()
        .find_map(|block| table(block).ok())
        .ok_or_else(|| "missing table".into())
}

fn table(block: BlockRef<'_>) -> Result<TableRef<'_>, Error> {
    let BlockKind::Table(table) = block.kind() else {
        return Err("expected a table".into());
    };
    Ok(table)
}

fn inline_text(nodes: ContentRef<'_>) -> String {
    nodes
        .inlines()
        .filter_map(|node| {
            let InlineKind::Plain(text) = node.kind() else {
                return None;
            };
            Some(text.text())
        })
        .collect()
}

fn paragraph_text(paragraph: ParagraphRef<'_>) -> String {
    inline_text(paragraph.content())
}

fn cell_paragraphs(cell: TableCellRef<'_>) -> Vec<String> {
    cell.blocks()
        .filter_map(|block| {
            let BlockKind::Paragraph(paragraph) = block.kind() else {
                return None;
            };
            Some(paragraph_text(paragraph))
        })
        .collect()
}

fn outer_table(document: DocumentRef<'_>) -> Result<TableRef<'_>, Error> {
    let BlockKind::Section(section) = document
        .blocks()
        .next()
        .ok_or("expected an outer section")?
        .kind()
    else {
        return Err("expected an outer section".into());
    };
    table(section.blocks().next().ok_or("expected a table")?)
}

#[test]
fn attributes_in_asciidoc_cells_have_nested_document_scope() -> Result<(), Error> {
    let parsed = parse(
        "= T\n:outer: inherited\n\n[cols=\"2*a\"]\n|===\n|\n:cell-local: local\n:sectnums:\n\nInside {cell-local} and {outer}.\n\n:outer: changed\n\nStill {outer}.\n\n:outer!:\n\nStill {outer} after unset.\n\n:cell-local!:\n\nLocal {cell-local} after unset.\n\n|\nSibling {cell-local} and {outer}.\n|===\n\nAfter {cell-local} and {outer}.\n",
        &Options::default(),
    )?;
    let document = parsed.document();
    let table = table(document.blocks().next().ok_or("expected a table")?)?;
    let row = table.rows().next().ok_or("expected a body row")?;
    let first_cell = row.cells().next().ok_or("expected the first cell")?;

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
        .blocks()
        .filter_map(|block| {
            let BlockKind::DocumentAttribute(attribute) = block.kind() else {
                return None;
            };
            Some(attribute)
        })
        .collect();
    assert_eq!(attribute_entries.len(), 3);
    let local_set = attribute_entries.first().ok_or("expected local set")?;
    let local_unset = attribute_entries.last().ok_or("expected local unset")?;
    assert_eq!(local_set.name(), "cell-local");
    assert_eq!(
        *local_set.assignment(),
        DocumentAttributeAssignment::Set(DocumentAttributeValue::from("local"))
    );
    assert_eq!(local_unset.name(), "cell-local");
    assert_eq!(
        *local_unset.assignment(),
        DocumentAttributeAssignment::Unset
    );
    assert_eq!(
        cell_paragraphs(row.cells().nth(1).ok_or("expected the second cell")?),
        ["Sibling {cell-local} and inherited."]
    );
    assert_eq!(
        document
            .attributes()
            .get("outer")
            .and_then(|value| value.text()),
        Some("inherited")
    );
    assert!(!document.attributes().contains("cell-local"));
    assert!(!document.attributes().contains("sectnums"));

    let BlockKind::Paragraph(after) = document
        .blocks()
        .nth(1)
        .map(BlockRef::kind)
        .ok_or("expected trailing content")?
    else {
        return Err("expected a trailing paragraph".into());
    };
    assert_eq!(paragraph_text(after), "After {cell-local} and inherited.");
    Ok(())
}

#[test]
fn attribute_syntax_in_plain_cells_is_literal_text() -> Result<(), Error> {
    let parsed = parse(
        "= T\n\n|===\n|\n:cell-local: value\n\nCell sees {cell-local}.\n|===\n",
        &Options::default(),
    )?;
    let table = table(
        parsed
            .document()
            .blocks()
            .next()
            .ok_or("expected a table")?,
    )?;
    let content = table
        .rows()
        .next()
        .and_then(|row| row.cells().next())
        .ok_or("expected a cell")?
        .blocks();

    let [
        BlockKind::Paragraph(assignment),
        BlockKind::Paragraph(after),
    ] = &*content.map(BlockRef::kind).collect::<Vec<_>>()
    else {
        return Err("expected two literal paragraphs, not an attribute event".into());
    };
    assert_eq!(paragraph_text(*assignment), ":cell-local: value");
    assert_eq!(paragraph_text(*after), "Cell sees {cell-local}.");
    assert!(!parsed.document().attributes().contains("cell-local"));
    Ok(())
}

#[test]
fn nested_section_numbering_uses_cell_source_order() -> Result<(), Error> {
    let parsed = parse(
        "= T\n:outer-value: inherited\n\n[cols=\"2*a\"]\n|===\n|\n:cell-local: local\n:outer-value: changed\n\nCell sees {cell-local} and {outer-value}.\n\n== Before enabling\n\nBefore.\n\n:sectnums:\n:sectnumlevels: 1\n\n== After enabling\n\nAfter.\n\n=== Child\n\nChild.\n\n|\nSibling sees {cell-local} and {outer-value}.\n|===\n",
        &Options::default(),
    )?;
    let document = parsed.document();
    let parsed_table = table(document.blocks().next().ok_or("expected a table")?)?;
    let blocks = parsed_table
        .rows()
        .next()
        .and_then(|row| row.cells().next())
        .ok_or("expected an AsciiDoc cell")?
        .blocks();
    let mut sections = blocks.filter_map(|block| {
        let BlockKind::Section(section) = block.kind() else {
            return None;
        };
        Some(section)
    });
    let before = sections.next().ok_or("expected the first section")?;
    let after = sections.next().ok_or("expected the second section")?;
    let child = after
        .blocks()
        .find_map(|block| {
            let BlockKind::Section(section) = block.kind() else {
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
            .blocks()
            .next()
            .ok_or("expected a table")?,
    )?;
    let section = table
        .rows()
        .next()
        .and_then(|row| row.cells().next())
        .and_then(|cell| {
            cell.blocks().find_map(|block| {
                let BlockKind::Section(section) = block.kind() else {
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
    let ids: Vec<_> = document.toc_entries().map(TocEntryRef::id).collect();

    assert_eq!(ids, ["_outer", "_after"]);
    let nested = document
        .references()
        .get("_nested")
        .ok_or("expected a nested section reference")?;
    assert_eq!(nested.title().map(inline_text).as_deref(), Some("Nested"));

    let table = outer_table(document)?;
    let cell = table
        .rows()
        .next()
        .and_then(|row| row.cells().next())
        .ok_or("expected an AsciiDoc cell")?;
    assert!(matches!(
        cell.blocks().next().map(BlockRef::kind),
        Some(BlockKind::Section(_))
    ));
    Ok(())
}

#[rstest::rstest]
#[case("{who} footnote:[duplicated].", 1)]
#[case("{who} {note}.", 1)]
#[case("{who} {note}.\n\n世界 footnote:[second paragraph].", 2)]
fn duplicated_cells_parse_footnotes_independently(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[case] content: &str,
    #[case] notes_per_cell: u32,
) -> Result<(), Error> {
    let source = format!(
        ":who: Ada\n:note: footnote:[expanded]\n\n[cols=\"2*\"]\n|===\n2*|{content}\n|===\n"
    );
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let row = first_table(parsed.document())?
        .rows()
        .next()
        .ok_or("missing duplicated row")?;
    assert_eq!(row.cells().count(), 2);
    let numbers: Vec<_> = row
        .cells()
        .flat_map(TableCellRef::blocks)
        .filter_map(|block| {
            if let BlockKind::Paragraph(paragraph) = block.kind() {
                Some(paragraph.content().inlines())
            } else {
                None
            }
        })
        .flatten()
        .filter_map(|inline| {
            if let InlineKind::Footnote(footnote) = inline.kind() {
                Some(footnote.number())
            } else {
                None
            }
        })
        .collect();
    let expected: Vec<_> = (1..=notes_per_cell * 2).collect();
    assert_eq!(numbers, expected);
    let registered: Vec<_> = parsed
        .document()
        .footnotes()
        .map(FootnoteDefinitionRef::number)
        .collect();
    assert_eq!(registered, expected);
    let spans: Vec<Vec<_>> = row
        .cells()
        .map(|cell| {
            cell.blocks()
                .map(|block| {
                    let location = block.location();
                    (location.absolute_start(), location.absolute_end())
                })
                .collect()
        })
        .collect();
    let [first, second] = spans.as_slice() else {
        return Err("expected two sets of authored paragraph spans".into());
    };
    assert_eq!(first, second);
    assert_eq!(first.len(), usize::try_from(notes_per_cell)?);
    Ok(())
}

#[rstest::rstest]
fn duplicated_cells_keep_mixed_style_footnote_owners(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        ":who: 世界\n\n[cols=\"1,1,a,1,1,l,1\"]\n|===\n7*|{who} footnote:[note].\n|===\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let cells: Vec<_> = first_table(parsed.document())?
        .rows()
        .next()
        .ok_or("missing duplicated row")?
        .cells()
        .collect();
    assert_eq!(cells.len(), 7);
    for (index, cell) in cells.iter().enumerate() {
        assert_eq!(cell.owns_footnotes(), index == 2);
        let numbers: Vec<_> = cell
            .footnotes()
            .map(FootnoteDefinitionRef::number)
            .collect();
        assert_eq!(numbers, if index == 2 { vec![3] } else { Vec::new() });
        if index == 5 {
            let block = cell.blocks().next().ok_or("missing literal paragraph")?;
            let BlockKind::Paragraph(paragraph) = block.kind() else {
                return Err("expected a literal paragraph".into());
            };
            let inline = paragraph
                .content()
                .inlines()
                .next()
                .ok_or("missing literal text")?;
            let InlineKind::Verbatim(text) = inline.kind() else {
                return Err("expected verbatim cell text".into());
            };
            assert_eq!(text.text(), "{who} footnote:[note].");
        }
    }
    let outer_numbers: Vec<_> = parsed
        .document()
        .footnotes()
        .map(FootnoteDefinitionRef::number)
        .collect();
    assert_eq!(outer_numbers, [1, 2, 4, 5, 6]);
    Ok(())
}
