use acdc_parser::{Block, DelimitedBlockType, InlineNode, Options, Paragraph, TableColumn, parse};

type Error = Box<dyn std::error::Error>;

fn first_cell<'a>(blocks: &'a [Block<'a>]) -> Result<&'a TableColumn<'a>, Error> {
    let table = blocks
        .iter()
        .find_map(|block| {
            let Block::DelimitedBlock(block) = block else {
                return None;
            };
            let DelimitedBlockType::DelimitedTable(table) = &block.inner else {
                return None;
            };
            Some(table)
        })
        .ok_or("missing table")?;
    table
        .rows
        .first()
        .ok_or("missing table row")?
        .columns
        .first()
        .ok_or_else(|| "missing table cell".into())
}

fn plain_text(paragraph: &Paragraph<'_>) -> String {
    paragraph
        .content
        .iter()
        .filter_map(|inline| {
            if let InlineNode::PlainText(text) = inline {
                Some(text.content)
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
        let cell = first_cell(&parsed.document().blocks)?;
        assert_eq!(cell.content.len(), 2, "{style}: {:?}", cell.content);
        for (block, expected) in cell.content.iter().zip([first, "Second paragraph."]) {
            let Block::Paragraph(paragraph) = block else {
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
        let cell = first_cell(&parsed.document().blocks)?;
        let expected = content.split("\n\n").collect::<Vec<_>>();
        assert_eq!(cell.content.len(), expected.len(), "{content}");
        for (block, expected) in cell.content.iter().zip(expected) {
            let Block::Paragraph(paragraph) = block else {
                return Err(format!("unexpected cell block for {content}: {block:?}").into());
            };
            assert!(paragraph.metadata.style.is_none(), "{content}");
            assert!(paragraph.title.is_empty(), "{content}");
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
        let cell = first_cell(&parsed.document().blocks)?;
        assert_eq!(cell.content.len(), 2);
        for (block, (line, column, end_line, end_column)) in
            cell.content.iter().zip([(3, 2, 6, 4), (8, 1, 8, 5)])
        {
            let Block::Paragraph(paragraph) = block else {
                return Err("missing paragraph".into());
            };
            assert_eq!(paragraph.location.start.line, line);
            assert_eq!(paragraph.location.start.column, column);
            assert_eq!(paragraph.location.end.line, end_line);
            assert_eq!(paragraph.location.end.column, end_column);
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
            .attributes
            .get("value")
            .and_then(|value| value.text()),
        Some("outer")
    );
    assert_eq!(document.footnotes.len(), 1);
    assert!(document.references.contains_key("plain-target"));
    assert_eq!(first_cell(&document.blocks)?.content.len(), 2);
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    Ok(())
}
