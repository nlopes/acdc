use acdc_parser::{
    Options,
    ast::{BlockKind, ContentRef, DocumentRef, InlineKind, ParagraphRef, TableCellRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

fn first_cell(document: DocumentRef<'_>) -> Result<TableCellRef<'_>, Error> {
    let table = document
        .blocks()
        .find_map(|block| {
            if let BlockKind::Table(table) = block.kind() {
                Some(table)
            } else {
                None
            }
        })
        .ok_or("missing table")?;
    table
        .rows()
        .next()
        .ok_or("missing table row")?
        .cells()
        .next()
        .ok_or_else(|| "missing table cell".into())
}

fn plain_text(paragraph: ParagraphRef<'_>) -> String {
    paragraph
        .content()
        .inlines()
        .filter_map(|inline| {
            if let InlineKind::Plain(text) = inline.kind() {
                Some(text.text())
            } else {
                None
            }
        })
        .collect()
}

#[test]
fn plain_table_paragraphs_keep_block_syntax_as_text() -> Result<(), Error> {
    let first = "Before.\n////\nInside.\n////\nAfter.\n----\nListing-looking text.\n----\n[source]\n* List-looking text.\n== Heading-looking text.\nimage::example.png[Example]\nEnd.";
    for style in ["1", "1d", "1e", "1s", "1m", "1h"] {
        let source = format!("[cols=\"{style}\"]\n|===\n|{first}\n\nSecond paragraph.\n|===\n");
        let parsed = parse(&source, &Options::default())?;
        let cell = first_cell(parsed.document())?;
        assert_eq!(
            cell.blocks().count(),
            2,
            "{style}: {:?}",
            cell.blocks().collect::<Vec<_>>()
        );
        for (block, expected) in cell.blocks().zip([first, "Second paragraph."]) {
            let BlockKind::Paragraph(paragraph) = block.kind() else {
                return Err(format!("unexpected cell block: {block:?}").into());
            };
            assert_eq!(plain_text(paragraph), expected, "{style}");
        }
    }
    Ok(())
}

#[test]
fn plain_table_paragraphs_do_not_apply_block_metadata() -> Result<(), Error> {
    for content in [
        "NOTE: Paragraph text.",
        "[source]\nText.",
        "[comment]\nText.",
        ".Block-looking title\nText.",
        "----\nText.\n----",
        "// Comment-looking text.\nText.",
        "First.\n  Indented second line.",
        "First.\n\n  Indented second paragraph.",
    ] {
        let source = format!("[cols=\"1\"]\n|===\n|{content}\n|===\n");
        let parsed = parse(&source, &Options::default())?;
        let cell = first_cell(parsed.document())?;
        let expected = content.split("\n\n").collect::<Vec<_>>();
        assert_eq!(cell.blocks().count(), expected.len(), "{content}");
        for (block, expected) in cell.blocks().zip(expected) {
            let BlockKind::Paragraph(paragraph) = block.kind() else {
                return Err(format!("unexpected cell block for {content}: {block:?}").into());
            };
            assert!(paragraph.style().is_none(), "{content}");
            assert!(
                paragraph.title().is_none_or(ContentRef::is_empty),
                "{content}"
            );
            assert_eq!(plain_text(paragraph), expected, "{content}");
        }
    }
    Ok(())
}

#[test]
fn plain_table_paragraphs_retain_source_spans() -> Result<(), Error> {
    for newline in ["\n", "\r\n"] {
        let source = format!(
            "[cols=\"1\"]{newline}|==={newline}|Élodie.{newline}////{newline}José.{newline}////{newline}{newline}Last.{newline}|==={newline}"
        );
        let parsed = parse(&source, &Options::default())?;
        let cell = first_cell(parsed.document())?;
        assert_eq!(cell.blocks().count(), 2);
        for (block, (line, column, end_line, end_column)) in
            cell.blocks().zip([(3, 2, 6, 4), (8, 1, 8, 5)])
        {
            let BlockKind::Paragraph(paragraph) = block.kind() else {
                return Err("missing paragraph".into());
            };
            assert_eq!(paragraph.location().start().line(), line);
            assert_eq!(paragraph.location().start().column(), column);
            assert_eq!(paragraph.location().end().line(), end_line);
            assert_eq!(paragraph.location().end().column(), end_column);
        }
    }
    Ok(())
}

#[test]
fn plain_table_paragraphs_register_inline_effects_once() -> Result<(), Error> {
    let source = ":value: outer\n\n[cols=\"1\"]\n|===\n|[comment]\n:value: inner\nText {value}.\n////\nanchor:plain-target[]footnote:[Visible note.] ((Visible term))\n////\n\nNext.\n|===\n";
    let parsed = parse(source, &Options::default())?;
    let document = parsed.document();
    assert_eq!(
        document
            .attributes()
            .get("value")
            .and_then(|value| value.text()),
        Some("outer")
    );
    assert_eq!(document.footnotes().count(), 1);
    assert!(document.references().contains("plain-target"));
    assert_eq!(first_cell(document)?.blocks().count(), 2);
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    Ok(())
}
