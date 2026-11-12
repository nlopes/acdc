use acdc_parser::{
    ColumnStyle, CompatibilityMode, HorizontalAlignment, Options, VerticalAlignment, WarningKind,
    ast::{BlockKind, BlockRef, TableRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

fn table(block: BlockRef<'_>) -> Result<TableRef<'_>, Error> {
    let BlockKind::Table(table) = block.kind() else {
        return Err("expected a table".into());
    };
    Ok(table)
}

#[rstest::rstest]
#[case::two_columns(
    include_str!("../../fixtures/tests/table_final_row_rowspan.adoc"),
    &[2, 1],
)]
#[case::inferred_columns("|===\n|a .2+|span\n|b\n|===\n", &[2, 1])]
#[case::three_columns("[cols=\"3*\"]\n|===\n|a |b .2+|span\n|c |d\n|===\n", &[3, 2])]
#[case::combined_spans("[cols=\"3*\"]\n|===\n|a 2.2+|span\n|b\n|===\n", &[2, 1])]
#[case::multiple_spans("[cols=\"3*\"]\n|===\n|a .2+|span1 .2+|span2\n|b\n|===\n", &[3, 1])]
#[case::three_rows("[cols=\"2*\"]\n|===\n|a .3+|span\n|b\n|c\n|===\n", &[2, 1, 1])]
#[case::leading_span("[cols=\"2*\"]\n|===\n.2+|span |a\n|b\n|===\n", &[2, 1])]
#[case::middle_span("[cols=\"3*\"]\n|===\n|a .2+|span |b\n|c |d\n|===\n", &[3, 2])]
#[case::following_row("[cols=\"2*\"]\n|===\n|a .2+|span\n|b\n|c |d\n|===\n", &[2, 1, 2])]
fn final_row_completed_by_rowspans_is_retained(
    #[case] source: &str,
    #[case] expected_cells: &[usize],
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let table = table(
        parsed
            .document()
            .blocks()
            .next()
            .ok_or("expected a table")?,
    )?;
    let cells = table
        .rows()
        .map(|row| row.cells().count())
        .collect::<Vec<_>>();
    assert_eq!(cells, expected_cells);
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    Ok(())
}

#[rstest::rstest]
#[case::without_span("[cols=\"3*\"]\n|===\n|a |b |c\n|d\n|===\n")]
#[case::with_span("[cols=\"3*\"]\n|===\n|a |b .2+|span\n|c\n|===\n")]
fn final_row_with_unfilled_columns_is_dropped_and_warns(
    #[case] source: &str,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let table = table(
        parsed
            .document()
            .blocks()
            .next()
            .ok_or("expected a table")?,
    )?;
    assert_eq!(table.rows().count(), 1);
    let [warning] = parsed.warnings() else {
        return Err("expected one incomplete-row warning".into());
    };
    assert!(matches!(warning.kind, WarningKind::TableIncompleteRow));
    assert_eq!(
        warning
            .source_location()
            .ok_or("expected a warning location")?
            .location
            .start
            .line,
        4,
    );
    Ok(())
}

#[test]
fn combined_spans_preserve_source_column_styles() -> Result<(), Error> {
    let parsed = parse(
        "[cols=\">.>s,<.<e,^.^h\"]\n|===\n2.2+^.^e|Combined\n|First companion\n|Second companion\n|First\n|Second\n|Third\n|===\n",
        &Options::default(),
    )?;
    let table = table(
        parsed
            .document()
            .blocks()
            .next()
            .ok_or("expected a table")?,
    )?;

    let first_row = table.rows().next().ok_or("expected the first row")?;
    let [combined, first_companion] = &*first_row.cells().collect::<Vec<_>>() else {
        return Err("expected two cells in the first row".into());
    };
    assert_eq!(combined.colspan(), 2);
    assert_eq!(combined.rowspan(), 2);
    assert_eq!(combined.halign(), Some(HorizontalAlignment::Center));
    assert_eq!(combined.valign(), Some(VerticalAlignment::Middle));
    assert_eq!(combined.style(), Some(ColumnStyle::Emphasis));
    assert_eq!(first_companion.style(), Some(ColumnStyle::Emphasis));

    let second_row = table.rows().nth(1).ok_or("expected the second row")?;
    let [second_companion] = &*second_row.cells().collect::<Vec<_>>() else {
        return Err("expected one cell in the second row".into());
    };
    assert_eq!(second_companion.style(), Some(ColumnStyle::Strong));

    let third_row = table.rows().nth(2).ok_or("expected the third row")?;
    let [first, second, third] = &*third_row.cells().collect::<Vec<_>>() else {
        return Err("expected three cells in the third row".into());
    };
    assert_eq!(first.style(), Some(ColumnStyle::Strong));
    assert_eq!(second.style(), Some(ColumnStyle::Emphasis));
    assert_eq!(third.style(), Some(ColumnStyle::Header));
    Ok(())
}

#[test]
fn duplicated_cells_resolve_each_generated_column_style() -> Result<(), Error> {
    let parsed = parse(
        "[cols=\">.>s,<.<e,^.^h\"]\n|===\n3*|Value\n|===\n",
        &Options::default(),
    )?;
    let table = table(
        parsed
            .document()
            .blocks()
            .next()
            .ok_or("expected a table")?,
    )?;
    let row = table.rows().next().ok_or("expected a row")?;
    let [first, second, third] = &*row.cells().collect::<Vec<_>>() else {
        return Err("expected three duplicated cells".into());
    };
    assert_eq!(first.style(), Some(ColumnStyle::Strong));
    assert_eq!(second.style(), Some(ColumnStyle::Emphasis));
    assert_eq!(third.style(), Some(ColumnStyle::Header));
    Ok(())
}
