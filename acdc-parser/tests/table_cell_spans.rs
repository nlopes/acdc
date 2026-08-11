use acdc_parser::{
    Block, ColumnStyle, DelimitedBlockType, HorizontalAlignment, Options, Table, VerticalAlignment,
    parse,
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

#[test]
fn combined_spans_preserve_source_column_styles() -> Result<(), Error> {
    let parsed = parse(
        "[cols=\">.>s,<.<e,^.^h\"]\n|===\n2.2+^.^e|Combined\n|First companion\n|Second companion\n|First\n|Second\n|Third\n|===\n",
        &Options::default(),
    )?;
    let table = table(parsed.document().blocks.first().ok_or("expected a table")?)?;

    let first_row = table.rows.first().ok_or("expected the first row")?;
    let [combined, first_companion] = first_row.columns.as_slice() else {
        return Err("expected two cells in the first row".into());
    };
    assert_eq!(combined.colspan, 2);
    assert_eq!(combined.rowspan, 2);
    assert_eq!(combined.halign, Some(HorizontalAlignment::Center));
    assert_eq!(combined.valign, Some(VerticalAlignment::Middle));
    assert_eq!(combined.style, Some(ColumnStyle::Emphasis));
    assert_eq!(first_companion.style, Some(ColumnStyle::Emphasis));

    let second_row = table.rows.get(1).ok_or("expected the second row")?;
    let [second_companion] = second_row.columns.as_slice() else {
        return Err("expected one cell in the second row".into());
    };
    assert_eq!(second_companion.style, Some(ColumnStyle::Strong));

    let third_row = table.rows.get(2).ok_or("expected the third row")?;
    let [first, second, third] = third_row.columns.as_slice() else {
        return Err("expected three cells in the third row".into());
    };
    assert_eq!(first.style, Some(ColumnStyle::Strong));
    assert_eq!(second.style, Some(ColumnStyle::Emphasis));
    assert_eq!(third.style, Some(ColumnStyle::Header));
    Ok(())
}

#[test]
fn duplicated_cells_resolve_each_generated_column_style() -> Result<(), Error> {
    let parsed = parse(
        "[cols=\">.>s,<.<e,^.^h\"]\n|===\n3*|Value\n|===\n",
        &Options::default(),
    )?;
    let table = table(parsed.document().blocks.first().ok_or("expected a table")?)?;
    let row = table.rows.first().ok_or("expected a row")?;
    let [first, second, third] = row.columns.as_slice() else {
        return Err("expected three duplicated cells".into());
    };
    assert_eq!(first.style, Some(ColumnStyle::Strong));
    assert_eq!(second.style, Some(ColumnStyle::Emphasis));
    assert_eq!(third.style, Some(ColumnStyle::Header));
    Ok(())
}
