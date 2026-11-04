//! Column controls apply only after the complete specification is recognized.

use acdc_parser::{
    ColumnStyle, ColumnWidth, CompatibilityMode, Options,
    ast::{BlockKind, BlockRef, ColumnFormatRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

fn table_shape(input: &str, options: &Options) -> Result<(Vec<ColumnWidth>, Vec<usize>), Error> {
    let parsed = parse(input, options)?;
    let Some(BlockKind::Table(table)) = parsed.document().blocks().next().map(BlockRef::kind)
    else {
        return Err("table".into());
    };
    let widths = table.columns().map(ColumnFormatRef::width).collect();
    let cells = table
        .header()
        .into_iter()
        .chain(table.rows())
        .chain(table.footer())
        .map(|row| row.cells().count())
        .collect();
    Ok((widths, cells))
}

#[rstest::rstest]
#[case("3")]
#[case("\"3\"")]
#[case("\" 3 \"")]
fn integer_cols_gives_equal_columns(
    #[case] cols: &str,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let options = Options::builder().with_compatibility_mode(mode).build()?;
    let source = format!("[cols={cols}]\n|===\n|A |B |C\n\n|1 |2 |3\n|===\n");
    let shape = table_shape(&source, &options)?;
    let multiplied = table_shape(
        "[cols=\"3*\"]\n|===\n|A |B |C\n\n|1 |2 |3\n|===\n",
        &options,
    )?;
    assert_eq!(shape, multiplied);
    assert_eq!(shape, (vec![ColumnWidth::Proportional(1); 3], vec![3, 3]));
    Ok(())
}

#[rstest::rstest]
fn zero_cols_takes_columns_from_first_row(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let options = Options::builder().with_compatibility_mode(mode).build()?;
    let shape = table_shape("[cols=0]\n|===\n|A |B |C\n\n|1 |2 |3\n|===\n", &options)?;
    let implicit = table_shape("|===\n|A |B |C\n\n|1 |2 |3\n|===\n", &options)?;
    assert_eq!(shape, implicit);
    assert_eq!(shape, (Vec::new(), vec![3, 3]));
    Ok(())
}

#[rstest::rstest]
fn cols_with_leading_zero_is_one_column_spec(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let options = Options::builder().with_compatibility_mode(mode).build()?;
    let shape = table_shape("[cols=03]\n|===\n|A |B |C\n\n|1 |2 |3\n|===\n", &options)?;
    assert_eq!(shape, (vec![ColumnWidth::Proportional(3)], vec![1; 6]));
    Ok(())
}

#[rstest::rstest]
#[case("'1a'")]
#[case("junk^a")]
#[case("1a-junk")]
#[case("2**a")]
#[case("<>")]
#[case("1..>a")]
#[case(".")]
fn malformed_column_records_do_not_contribute_controls(
    #[case] invalid: &str,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = format!("[cols=\"{invalid},2a\"]\n|===\n|First |Second\n|===\n");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let Some(BlockKind::Table(table)) = parsed.document().blocks().next().map(BlockRef::kind)
    else {
        return Err("table".into());
    };
    let formats = table.columns().collect::<Vec<_>>();
    let [format] = formats.as_slice() else {
        return Err("one valid column".into());
    };
    assert_eq!(format.style(), ColumnStyle::AsciiDoc);
    assert_eq!(format.width(), ColumnWidth::Proportional(2));
    assert_eq!(table.rows().count(), 2);
    Ok(())
}

#[rstest::rstest]
fn all_invalid_column_records_leave_column_inference_enabled(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "[cols=\"'1a','2a'\"]\n|===\n|*First* |*Second*\n|===\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let Some(BlockKind::Table(table)) = parsed.document().blocks().next().map(BlockRef::kind)
    else {
        return Err("table".into());
    };
    assert_eq!(table.columns().count(), 0);
    assert_eq!(table.rows().count(), 1);
    assert_eq!(table.rows().next().ok_or("row")?.cells().count(), 2);
    Ok(())
}

#[rstest::rstest]
#[case("2", 2)]
#[case("1;2", 2)]
#[case("0*a,1", 1)]
#[case("1 a, 2 a", 2)]
fn column_count_and_record_boundaries_follow_the_complete_value(
    #[case] cols: &str,
    #[case] expected: usize,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = format!("[cols=\"{cols}\"]\n|===\n|First |Second\n|===\n");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let Some(BlockKind::Table(table)) = parsed.document().blocks().next().map(BlockRef::kind)
    else {
        return Err("table".into());
    };
    assert_eq!(table.columns().count(), expected);
    Ok(())
}

#[rstest::rstest]
#[case("101")]
#[case("9999999999999999999999999999999999999999999")]
#[case("101*a")]
#[case("9999999999999999999999999999999999999999999*a")]
fn complete_counts_still_obey_the_resource_limit(
    #[case] cols: &str,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = format!("[cols=\"{cols}\"]\n|===\n|First\n|===\n");
    let error = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )
    .err()
    .ok_or("column limit error")?;
    assert!(
        error.to_string().contains("exceeds the maximum of 100"),
        "{error}"
    );
    Ok(())
}

#[rstest::rstest]
fn complete_alignment_width_and_style_survive_repetition(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "[cols=\"2*<.>3a,~m\"]\n|===\n|One |Two |Three\n|===\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let Some(BlockKind::Table(table)) = parsed.document().blocks().next().map(BlockRef::kind)
    else {
        return Err("table".into());
    };
    let formats = table.columns().collect::<Vec<_>>();
    let [first, second, last] = formats.as_slice() else {
        return Err("three columns".into());
    };
    for format in [first, second] {
        assert_eq!(format.halign(), acdc_parser::HorizontalAlignment::Left);
        assert_eq!(format.valign(), acdc_parser::VerticalAlignment::Bottom);
        assert_eq!(format.width(), ColumnWidth::Proportional(3));
        assert_eq!(format.style(), ColumnStyle::AsciiDoc);
    }
    assert_eq!(last.width(), ColumnWidth::Auto);
    assert_eq!(last.style(), ColumnStyle::Monospace);
    Ok(())
}
