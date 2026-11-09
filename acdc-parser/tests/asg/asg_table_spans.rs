#![cfg(feature = "asg")]

use acdc_parser::{
    Block, ColumnFormat, CompatibilityMode, DelimitedBlock, DelimitedBlockType, Document, Location,
    Options, Table, TableColumn, TableRow, asg, parse, parse_file,
};
use serde_json::{Value, json};
use std::{fs, path::Path};

type Error = Box<dyn std::error::Error + Send + Sync>;

fn document(rows: Vec<TableRow<'static>>, width: usize) -> Document<'static> {
    let table =
        Table::new(rows, Location::default()).with_columns(vec![ColumnFormat::new(); width]);
    let mut document = Document::default();
    document.push_block(Block::DelimitedBlock(DelimitedBlock::new(
        DelimitedBlockType::DelimitedTable(table),
        "|===",
        Location::default(),
    )));
    document
}

fn cell(colspan: usize, rowspan: usize) -> TableColumn<'static> {
    let mut cell = TableColumn::new(Vec::new());
    cell.set_colspan(colspan);
    cell.set_rowspan(rowspan);
    cell
}

#[test]
fn spans_preserve_order_and_header_body_footer_transitions() -> Result<(), Error> {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/asg/table_spans.adoc");
    let expected: Value =
        serde_json::from_str(&fs::read_to_string(source.with_extension("json"))?)?;
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let parsed = parse_file(
            &source,
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let before = serde_json::to_value(parsed.document())?;
        let value = asg::document_with_profile(parsed.document(), asg::Profile::Acdc)?;
        assert_eq!(value, expected, "{mode:?}");
        assert_eq!(serde_json::to_value(parsed.document())?, before);
        for (path, expected) in [
            ("/blocks/0/header/cells/0/colspan", json!(2)),
            ("/blocks/0/rows/0/cells/0/colspan", json!(2)),
            ("/blocks/0/rows/0/cells/0/rowspan", json!(2)),
            (
                "/blocks/0/rows/1/cells/0/blocks/0/inlines/0/value",
                json!("Three"),
            ),
            ("/blocks/0/footer/cells/0/colspan", json!(4)),
            ("/blocks/1/header/cells/0/rowspan", json!(2)),
        ] {
            assert_eq!(value.pointer(path), Some(&expected), "{path}");
        }
        assert!(value.pointer("/blocks/0/rows/0/cells/1/colspan").is_none());
        assert!(
            matches!(asg::document(parsed.document()), Err(asg::Error::Unsupported{path,..}) if path=="/blocks/0/inner")
        );
    }
    Ok(())
}

#[test]
fn invalid_prepared_spans_reject_at_the_owning_dimension() {
    for (rows, width, expected) in [
        (
            vec![TableRow::new(vec![cell(0, 1)])],
            1,
            "/blocks/0/rows/0/cells/0/colspan",
        ),
        (
            vec![TableRow::new(vec![cell(1, 0)])],
            1,
            "/blocks/0/rows/0/cells/0/rowspan",
        ),
        (
            vec![TableRow::new(vec![cell(1, 2)])],
            1,
            "/blocks/0/rows/0/cells/0/rowspan",
        ),
        (
            vec![TableRow::new(vec![cell(2, 1)])],
            1,
            "/blocks/0/rows/0/cells/0/colspan",
        ),
        (
            vec![TableRow::new(vec![cell(1, 1)])],
            2,
            "/blocks/0/rows/0/cells",
        ),
        (
            vec![TableRow::new(vec![cell(usize::MAX, 1), cell(1, 1)])],
            0,
            "/blocks/0/rows/0/cells/1/colspan",
        ),
        (
            vec![
                TableRow::new(vec![cell(1, 1), cell(1, 2), cell(1, 1)]),
                TableRow::new(vec![cell(2, 1)]),
            ],
            3,
            "/blocks/0/rows/1/cells/0/colspan",
        ),
    ] {
        let document = document(rows, width);
        let value = asg::document_with_profile((&document).into(), asg::Profile::Acdc);
        assert!(
            matches!(&value,Err(asg::Error::Unsupported{path,..}) if path==expected),
            "{value:?}"
        );
    }
}

#[test]
fn unspecified_width_does_not_allocate_by_span_size_or_invent_cells() -> Result<(), Error> {
    let document = document(vec![TableRow::new(vec![cell(usize::MAX, 1)])], 0);
    let value = asg::document_with_profile((&document).into(), asg::Profile::Acdc)?;
    assert_eq!(
        value.pointer("/blocks/0/rows/0/cells/0/colspan"),
        Some(&json!(usize::MAX))
    );
    assert_eq!(value.pointer("/blocks/0/columns"), Some(&json!([])));
    assert_eq!(
        value
            .pointer("/blocks/0/rows/0/cells")
            .and_then(Value::as_array)
            .map(Vec::len),
        Some(1)
    );
    Ok(())
}

#[test]
fn span_schema_is_positive_and_closed_inside_parent_blocks() -> Result<(), Error> {
    let parsed = parse(
        "====\n[cols=2]\n|===\n2+|Both\n|===\n====",
        &Options::default(),
    )?;
    let value = asg::document_with_profile(parsed.document(), asg::Profile::Acdc)?;
    let schema: Value = serde_json::from_str(include_str!("../../schemas/asg/acdc-v1.json"))?;
    let validator = jsonschema::validator_for(&schema)?;
    assert!(validator.is_valid(&value));
    for invalid in [json!(0), json!(-1), json!(1.5), json!("2"), json!(null)] {
        let mut changed = value.clone();
        *changed
            .pointer_mut("/blocks/0/blocks/0/rows/0/cells/0/colspan")
            .ok_or("span")? = invalid;
        assert!(!validator.is_valid(&changed));
    }
    Ok(())
}
