use acdc_parser::{
    Block, ColumnStyle, CompatibilityMode, DelimitedBlockType, Document, Options, parse,
};

use super::HeadResources;

type Error = Box<dyn std::error::Error>;
type Flags = (bool, bool, bool);

fn flags(source: &str, options: &Options<'_>) -> Result<Flags, Error> {
    let parsed = parse(source, options)?;
    let resources = HeadResources::collect(parsed.document_ref());
    assert_eq!(
        super::uses_stem(parsed.document()),
        resources.stem || resources.nested_stem
    );
    Ok((resources.stem, resources.font_icons, resources.nested_stem))
}

#[test]
fn head_resources_keep_header_values_and_any_enabled_body_resources() -> Result<(), Error> {
    let cases = [
        ("= Doc\n\nBody.\n", (false, false, false)),
        ("= Doc\n:stem:\n\nBody.\n", (true, false, false)),
        ("= Doc\n:icons: font\n\nBody.\n", (false, true, false)),
        (
            "= Doc\n:stem:\n:icons: font\n\nBody.\n\n:stem!:\n:icons!:\n\nAfter.\n",
            (true, true, false),
        ),
        (
            "= Doc\n\nBody.\n\n:stem: latexmath\n:icons: font\n\nAfter.\n\n:stem!:\n:icons: image\n",
            (true, true, false),
        ),
        (
            "= Doc\n\n[source]\n----\n:stem:\n:icons: font\n----\n",
            (false, false, false),
        ),
        (
            "= Doc\n\n////\n:stem:\n:icons: font\n////\n",
            (false, false, false),
        ),
    ];
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        for (source, expected) in cases {
            assert_eq!(flags(source, &options)?, expected, "{mode:?}: {source}");
        }
    }
    Ok(())
}

#[test]
fn head_resources_follow_compound_blocks_and_list_attachments() -> Result<(), Error> {
    let sources = [
        "== Section\n\nInside.\n\n:stem:\n:icons: font\n\nAfter.\n",
        "--\nInside.\n\n:stem:\n:icons: font\n\nAfter.\n--\n",
        "****\nInside.\n\n:stem:\n:icons: font\n\nAfter.\n****\n",
        "____\nInside.\n\n:stem:\n:icons: font\n\nAfter.\n____\n",
        "[NOTE]\n====\nInside.\n\n:stem:\n:icons: font\n\nAfter.\n====\n",
        "* Item\n+\n--\nInside.\n\n:stem:\n:icons: font\n\nAfter.\n--\n",
        ". Item\n+\n--\nInside.\n\n:stem:\n:icons: font\n\nAfter.\n--\n",
        "Term:: Definition\n+\n--\nInside.\n\n:stem:\n:icons: font\n\nAfter.\n--\n",
        "[source]\n----\nputs 'x' <1>\n----\n<1> Explanation\n+\n--\nInside.\n\n:stem:\n:icons: font\n\nAfter.\n--\n",
    ];
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        for source in sources {
            assert_eq!(
                flags(source, &options)?,
                (true, true, false),
                "{mode:?}: {source}"
            );
        }
    }
    Ok(())
}

#[test]
fn head_resources_keep_nested_stem_separate_and_restore_parent_attributes() -> Result<(), Error> {
    let source = "= Doc\n\nOutside.\n\n[cols=a]\n|===\n|\nInside.\n\n:stem:\n:icons: font\n\nAfter.\n|===\n\n:probe: outside\n\nLast.\n";
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        assert_eq!(flags(source, &options)?, (false, false, true));
    }
    Ok(())
}

#[test]
fn head_resources_find_attributes_at_the_start_of_cells_and_after_prose() -> Result<(), Error> {
    let cell_start =
        "= Doc\n\nOutside.\n\n[cols=a]\n|===\n|:stem:\n:icons: font\n\nInside.\n|===\n";
    let body_event = "= Doc\n\nOutside.\n\n[cols=a]\n|===\n|:stem:\n:icons: font\n\nInside.\n\n:probe: body\n\nAfter.\n|===\n";
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        assert_eq!(flags(cell_start, &options)?, (false, false, true));
        assert_eq!(flags(body_event, &options)?, (false, false, true));
    }
    Ok(())
}

#[test]
fn head_resources_keep_cell_overrides_and_column_defaults_after_owner_edits() -> Result<(), Error> {
    let source = "[cols=a]\n|===\n|\nInside.\n\n:stem:\n:icons: font\n\nAfter.\n|===\n";
    let parsed = parse(source, &Options::default())?;
    for (declared, local, expected) in [
        (
            ColumnStyle::Default,
            Some(ColumnStyle::AsciiDoc),
            (false, false, true),
        ),
        (ColumnStyle::AsciiDoc, None, (false, false, true)),
        (ColumnStyle::Default, None, (true, true, false)),
    ] {
        let mut document = Document::default();
        document.attributes = parsed.document().attributes.clone();
        document.blocks = parsed.document().blocks.clone();
        for block in &mut document.blocks {
            if let Block::DelimitedBlock(block) = block
                && let DelimitedBlockType::DelimitedTable(table) = &mut block.inner
            {
                for format in &mut table.columns {
                    format.style = declared;
                }
                for row in table
                    .header
                    .iter_mut()
                    .chain(&mut table.rows)
                    .chain(&mut table.footer)
                {
                    for cell in &mut row.columns {
                        cell.style = local;
                    }
                }
            }
        }
        let resources = HeadResources::collect((&document).into());
        assert_eq!(
            (resources.stem, resources.font_icons, resources.nested_stem),
            expected
        );
    }
    Ok(())
}

#[test]
fn head_resources_use_stored_cell_order_for_spanning_column_defaults() -> Result<(), Error> {
    let parsed = parse(
        "[cols=a]\n|===\n|Inside.\n\n:stem:\n:icons: font\n\nAfter.\n|===\n",
        &Options::default(),
    )?;
    let mut document = Document::default();
    document.blocks = parsed.document().blocks.clone();
    let Some(Block::DelimitedBlock(block)) = document.blocks.first_mut() else {
        return Err("expected a table".into());
    };
    let DelimitedBlockType::DelimitedTable(table) = &mut block.inner else {
        return Err("expected table content".into());
    };
    let row = table.rows.first_mut().ok_or("expected a table row")?;
    let mut second = row.columns.first().ok_or("expected a table cell")?.clone();
    let mut first = second.clone();
    first.content.clear();
    first.style = Some(ColumnStyle::Default);
    first.colspan = 2;
    second.style = None;
    row.columns = vec![first, second];
    let format = table
        .columns
        .first()
        .ok_or("expected a column format")?
        .clone();
    table.columns = [
        ColumnStyle::Default,
        ColumnStyle::AsciiDoc,
        ColumnStyle::Default,
    ]
    .into_iter()
    .map(|style| {
        let mut format = format.clone();
        format.style = style;
        format
    })
    .collect();
    let resources = HeadResources::collect((&document).into());
    assert_eq!(
        (resources.stem, resources.font_icons, resources.nested_stem),
        (false, false, true)
    );
    Ok(())
}
