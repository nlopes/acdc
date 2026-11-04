use crate::{
    Block, CompatibilityMode, DelimitedBlock, Options, TableFrame, TableGrid, TableStripes,
    ast::{AttributeValueRef, BlockKind, BlockRef, TableRef},
    parse,
};
type Error = Box<dyn std::error::Error>;

fn table<'doc>(block: &'doc DelimitedBlock<'doc>) -> Result<TableRef<'doc>, Error> {
    let BlockKind::Table(table) = BlockRef::from(block).kind() else {
        return Err("expected table".into());
    };
    Ok(table)
}

fn decoration(block: &DelimitedBlock<'_>) -> Result<(TableFrame, TableGrid, TableStripes), Error> {
    let value = table(block)?.presentation().ok_or("missing presentation")?;
    Ok((value.frame(), value.grid(), value.stripes()))
}

#[rstest::rstest]
fn metadata_replacement_updates_resolved_table_controls(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "[frame=none,grid=none,stripes=none]\n|===\n|Cell\n|===\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let Some(Block::DelimitedBlock(original)) = parsed.document().blocks.first() else {
        return Err("expected delimited block".into());
    };
    let mut metadata = original.metadata.clone();
    metadata.attributes.set("frame".into(), "all".into());
    metadata.attributes.set("grid".into(), "rows".into());
    metadata.attributes.set("stripes".into(), "odd".into());
    let edited = original.clone().with_metadata(metadata)?;
    let table = table(&edited)?;
    assert_eq!(table.declared_frame(), Some(AttributeValueRef::Text("all")));
    assert_eq!(table.declared_grid(), Some(AttributeValueRef::Text("rows")));
    assert_eq!(
        table.declared_stripes(),
        Some(AttributeValueRef::Text("odd"))
    );
    let presentation = table.presentation().ok_or("missing presentation")?;
    assert_eq!(presentation.frame(), TableFrame::All);
    assert_eq!(presentation.grid(), TableGrid::Rows);
    assert_eq!(presentation.stripes(), TableStripes::Odd);
    Ok(())
}

#[rstest::rstest]
fn removing_controls_restores_defaults_at_each_table(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "= Tables\n:table-frame: ends\n:table-grid: cols\n:table-stripes: odd\n\n\
         [frame=none,grid=none,stripes=none]\n|===\n|Header defaults\n|===\n\n\
         :table-frame: sides\n:table-grid: rows\n:table-stripes: even\n\n\
         [frame=none,grid=none,stripes=none]\n|===\n|Body defaults\n|===\n\n\
         :table-frame!:\n:table-grid!:\n:table-stripes!:\n\n\
         [frame=none,grid=none,stripes=none]\n|===\n|Unset defaults\n|===\n\n\
         :table-frame: none\n:table-grid: none\n:table-stripes: all\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let blocks = parsed
        .document()
        .blocks
        .iter()
        .filter_map(|block| {
            if let Block::DelimitedBlock(block) = block {
                Some(block)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(blocks.len(), 3);
    for (original, expected) in blocks.into_iter().zip([
        (TableFrame::Ends, TableGrid::Columns, TableStripes::Odd),
        (TableFrame::Sides, TableGrid::Rows, TableStripes::Even),
        (TableFrame::All, TableGrid::All, TableStripes::None),
    ]) {
        let mut edited = original.clone();
        for _ in 0..2 {
            let mut metadata = edited.metadata.clone();
            for name in ["frame", "grid", "stripes"] {
                metadata.attributes.remove(name);
            }
            edited.set_metadata(metadata)?;
            assert_eq!(decoration(&edited)?, expected);
            let mut metadata = edited.metadata.clone();
            for name in ["frame", "grid", "stripes"] {
                metadata.attributes.set(name.into(), "all".into());
            }
            edited.set_metadata(metadata)?;
            assert_eq!(
                decoration(&edited)?,
                (TableFrame::All, TableGrid::All, TableStripes::All)
            );
        }
        assert_eq!(
            decoration(original)?,
            (TableFrame::None, TableGrid::None, TableStripes::None)
        );
    }
    Ok(())
}

#[rstest::rstest]
fn present_controls_mask_document_defaults_after_edits(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    use crate::AttributeValue;

    let parsed = parse(
        "= Tables\n:table-frame: sides\n:table-grid: rows\n:table-stripes: even\n\n|===\n|Cell\n|===\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let Some(Block::DelimitedBlock(original)) = parsed.document().blocks.first() else {
        return Err("expected table".into());
    };
    for (value, expected) in [
        (
            AttributeValue::None,
            (TableFrame::All, TableGrid::All, TableStripes::None),
        ),
        (
            AttributeValue::Bool(false),
            (TableFrame::All, TableGrid::All, TableStripes::None),
        ),
        (
            AttributeValue::Bool(true),
            (TableFrame::None, TableGrid::None, TableStripes::None),
        ),
        (
            AttributeValue::from(""),
            (TableFrame::None, TableGrid::None, TableStripes::None),
        ),
        (
            AttributeValue::from("INVALID"),
            (TableFrame::None, TableGrid::None, TableStripes::None),
        ),
        (
            AttributeValue::from("topbot"),
            (TableFrame::Ends, TableGrid::None, TableStripes::None),
        ),
    ] {
        let mut edited = original.clone();
        let mut metadata = edited.metadata.clone();
        for name in ["frame", "grid", "stripes"] {
            metadata.attributes.set(name.into(), value.clone());
        }
        edited.set_metadata(metadata)?;
        assert_eq!(decoration(&edited)?, expected);
    }
    Ok(())
}

#[rstest::rstest]
#[case("", (TableFrame::All, TableGrid::All, TableStripes::None), (TableFrame::Ends, TableGrid::Columns, TableStripes::Even))]
#[case(":table-frame: sides\n:table-grid: rows\n:table-stripes: odd\n", (TableFrame::Sides, TableGrid::Rows, TableStripes::Odd), (TableFrame::Sides, TableGrid::Rows, TableStripes::Odd))]
fn nested_table_edits_keep_scopes_columns_and_cells_independent(
    #[case] parent_defaults: &str,
    #[case] outer_expected: (TableFrame, TableGrid, TableStripes),
    #[case] nested_expected: (TableFrame, TableGrid, TableStripes),
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    use crate::DelimitedBlockType;

    let source = format!(
        "= Tables\n{parent_defaults}\n\
         [cols=a]\n|===\n|\n:table-frame: ends\n:table-grid: cols\n:table-stripes: even\n\n\
         [cols=\">.>s\",frame=none,grid=none,stripes=none]\n!===\n!Nested\n!===\n|===\n"
    );
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let Some(Block::DelimitedBlock(outer)) = parsed.document().blocks.first() else {
        return Err("expected outer table".into());
    };
    let DelimitedBlockType::DelimitedTable(outer_table) = &outer.inner else {
        return Err("expected outer table body".into());
    };
    let cell = &outer_table
        .rows
        .first()
        .ok_or("missing row")?
        .columns
        .first()
        .ok_or("missing cell")?;
    let nested = cell
        .content
        .iter()
        .find_map(|block| {
            if let Block::DelimitedBlock(block) = block {
                Some(block)
            } else {
                None
            }
        })
        .ok_or("missing nested table")?;
    let mut edited = nested.clone();
    let mut metadata = edited.metadata.clone();
    for name in ["frame", "grid", "stripes"] {
        metadata.attributes.remove(name);
    }
    metadata.attributes.set("cols".into(), "<.<e".into());
    metadata.attributes.set("custom".into(), "".into());
    edited.set_metadata(metadata)?;
    assert_eq!(decoration(&edited)?, nested_expected);
    assert_eq!(decoration(outer)?, outer_expected);
    assert_eq!(
        decoration(nested)?,
        (TableFrame::None, TableGrid::None, TableStripes::None)
    );
    let DelimitedBlockType::DelimitedTable(before) = &nested.inner else {
        return Err("expected table".into());
    };
    let DelimitedBlockType::DelimitedTable(after) = &edited.inner else {
        return Err("expected table".into());
    };
    assert_eq!(after.columns, before.columns);
    assert_eq!(after.rows, before.rows);
    assert_eq!(after.header, before.header);
    assert_eq!(after.footer, before.footer);
    assert_eq!(after.location, before.location);
    assert_eq!(
        table(&edited)?.attributes().get("custom"),
        Some(AttributeValueRef::Text(""))
    );
    let mut edited_outer = outer.clone();
    let mut metadata = edited_outer.metadata.clone();
    metadata.attributes.set("frame".into(), "none".into());
    edited_outer.set_metadata(metadata)?;
    assert_eq!(
        decoration(&edited_outer)?,
        (TableFrame::None, outer_expected.1, outer_expected.2)
    );
    let DelimitedBlockType::DelimitedTable(edited_outer_table) = &edited_outer.inner else {
        return Err("expected table".into());
    };
    assert_eq!(edited_outer_table.rows, outer_table.rows);
    Ok(())
}

#[rstest::rstest]
fn invalid_metadata_replacement_leaves_table_unchanged(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "[frame=none,grid=none,stripes=none]\n|===\n|Cell\n|===\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let Some(Block::DelimitedBlock(original)) = parsed.document().blocks.first() else {
        return Err("expected table".into());
    };
    for name in ["title", "role", "options"] {
        let mut edited = original.clone();
        let before_json = serde_json::to_value(&edited)?;
        let mut metadata = edited.metadata.clone();
        metadata.attributes.set("frame".into(), "all".into());
        metadata.attributes.set(name.into(), "shadow".into());
        let error = edited
            .set_metadata(metadata)
            .err()
            .ok_or("accepted reserved attribute")?;
        assert_eq!(error.name, name);
        assert_eq!(&edited, original);
        assert_eq!(serde_json::to_value(&edited)?, before_json);
    }
    Ok(())
}
