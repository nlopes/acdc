//! Column controls apply only after the complete specification is recognized.

use acdc_parser::{
    ColumnStyle, ColumnWidth, CompatibilityMode, Options,
    ast::{BlockKind, BlockRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

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
    let Some(BlockKind::Table(table)) = parsed.document_ref().blocks().next().map(BlockRef::kind)
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
    let Some(BlockKind::Table(table)) = parsed.document_ref().blocks().next().map(BlockRef::kind)
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
    let Some(BlockKind::Table(table)) = parsed.document_ref().blocks().next().map(BlockRef::kind)
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
    let Some(BlockKind::Table(table)) = parsed.document_ref().blocks().next().map(BlockRef::kind)
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
