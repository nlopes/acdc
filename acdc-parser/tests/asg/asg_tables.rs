#![cfg(feature = "asg")]
use acdc_parser::{
    Block, ColumnFormat, ColumnStyle, ColumnWidth, CompatibilityMode, DelimitedBlock,
    DelimitedBlockType, Document, InlineNode, Location, Options, Paragraph, Table, TableCell,
    TableRow, Verbatim, asg, parse, parse_file,
};
use serde_json::{Value, json};
use std::{fs, path::Path};
type Error = Box<dyn std::error::Error + Send + Sync>;
#[test]
fn basic_tables_preserve_sections_cells_and_formatting() -> Result<(), Error> {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/asg/tables.adoc");
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let parsed = parse_file(
            &source,
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let before = serde_json::to_value(parsed.document())?;
        let value = asg::export_document_with_profile(parsed.document(), asg::Profile::Acdc)?;
        let expected: Value =
            serde_json::from_str(&fs::read_to_string(source.with_extension("json"))?)?;
        assert_eq!(value, expected, "{mode:?}");
        assert_eq!(serde_json::to_value(parsed.document())?, before);
        for (path, expected) in [
            ("/blocks/0/name", json!("table")),
            ("/blocks/0/delimiter", json!("|====")),
            ("/blocks/0/id", json!("record")),
            ("/blocks/0/caption/counter", json!("table")),
            (
                "/blocks/0/header/cells/0/blocks/0/inlines/0/value",
                json!("Name"),
            ),
            (
                "/blocks/0/footer/cells/1/blocks/0/inlines/0/value",
                json!("2"),
            ),
            ("/blocks/0/rows/1/cells/0/halign", json!("center")),
            ("/blocks/0/rows/1/cells/0/valign", json!("middle")),
            (
                "/blocks/0/presentation",
                json!({"frame":"none","grid":"none","stripes":"even"}),
            ),
            (
                "/blocks/2/presentation",
                json!({"frame":"ends","grid":"rows","stripes":"odd"}),
            ),
            ("/blocks/2/columns/0/style", json!("strong")),
            (
                "/blocks/2/rows/0/cells/3/blocks/0/inlines/0/name",
                json!("text"),
            ),
            (
                "/blocks/2/rows/0/cells/3/blocks/0/inlines/0/value",
                json!("<raw>"),
            ),
            ("/blocks/3/columns/0/width/kind", json!("auto")),
            ("/blocks/3/rows/0/cells/1/blocks", json!([])),
        ] {
            assert_eq!(value.pointer(path), Some(&expected), "{path}");
        }
        assert_eq!(
            value.pointer("/blocks/0/location"),
            before.pointer("/blocks/0/location")
        );
        assert_eq!(
            value.pointer("/blocks/0/delimited_location"),
            before.pointer("/blocks/0/content/location")
        );
    }
    Ok(())
}

fn with_table(table: Table<'_>) -> Document<'_> {
    let mut document = Document::default();
    document.push_block(Block::DelimitedBlock(DelimitedBlock::new(
        DelimitedBlockType::DelimitedTable(table),
        "|===",
        Location::default(),
    )));
    document
}

#[test]
fn empty_table_parts_keep_presence_and_prepared_column_values() -> Result<(), Error> {
    for header in [false, true] {
        for footer in [false, true] {
            let table = Table::new(Vec::new(), Location::default())
                .with_header(header.then(|| TableRow::new(Vec::new())))
                .with_footer(footer.then(|| TableRow::new(Vec::new())))
                .with_columns(vec![
                    ColumnFormat::new().with_width(ColumnWidth::Percentage(42)),
                    ColumnFormat::new().with_width(ColumnWidth::Proportional(0)),
                    ColumnFormat::new().with_width(ColumnWidth::Auto),
                ]);
            let document = with_table(table);
            let value = asg::export_document_with_profile((&document).into(), asg::Profile::Acdc)?;
            assert_eq!(value.pointer("/blocks/0/header").is_some(), header);
            assert_eq!(value.pointer("/blocks/0/footer").is_some(), footer);
            assert_eq!(value.pointer("/blocks/0/rows"), Some(&json!([])));
            assert_eq!(
                value.pointer("/blocks/0/columns/0/width"),
                Some(&json!({"kind":"percentage","value":42}))
            );
            assert_eq!(
                value.pointer("/blocks/0/columns/1/width/value"),
                Some(&json!(0))
            );
            assert!(value.pointer("/blocks/0/presentation").is_none());
            assert!(value.pointer("/blocks/0/delimited_location").is_none());
        }
    }
    Ok(())
}

#[test]
fn literal_column_context_preserves_prepared_verbatim_paragraphs() -> Result<(), Error> {
    let paragraph = Paragraph::new(
        vec![InlineNode::VerbatimText(Verbatim::new(
            "*literal* <raw>",
            Location::default(),
        ))],
        Location::default(),
    );
    let table = Table::new(
        vec![TableRow::new(vec![TableCell::new(vec![Block::Paragraph(
            paragraph,
        )])])],
        Location::default(),
    )
    .with_columns(vec![ColumnFormat::new().with_style(ColumnStyle::Literal)]);
    let document = with_table(table);
    let value = asg::export_document_with_profile((&document).into(), asg::Profile::Acdc)?;
    assert_eq!(
        value.pointer("/blocks/0/rows/0/cells/0/blocks/0/name"),
        Some(&json!("paragraph"))
    );
    assert_eq!(
        value.pointer("/blocks/0/rows/0/cells/0/blocks/0/inlines/0/value"),
        Some(&json!("*literal* <raw>"))
    );
    assert!(value.pointer("/blocks/0/rows/0/cells/0/style").is_none());
    Ok(())
}

#[test]
fn table_decoration_keeps_source_defaults_after_prepared_metadata_edits() -> Result<(), Error> {
    let parsed = parse(
        ":table-frame: sides\n\n|===\n|Cell\n|===",
        &Options::default(),
    )?;
    let Block::DelimitedBlock(mut block) =
        Block::from(parsed.document().blocks().next().ok_or("table")?)
    else {
        return Err("table".into());
    };
    for expected in ["none", "sides"] {
        let mut metadata = block.take_metadata();
        if expected == "none" {
            metadata.attributes.set("frame".into(), "none".into());
        } else {
            metadata.attributes.remove("frame");
        }
        block.set_metadata(metadata)?;
        let mut document = Document::default();
        document.push_block(Block::DelimitedBlock(block.clone()));
        let value = asg::export_document_with_profile((&document).into(), asg::Profile::Acdc)?;
        assert_eq!(
            value.pointer("/blocks/0/presentation/frame"),
            Some(&json!(expected))
        );
    }
    Ok(())
}

#[test]
fn strict_tables_and_invalid_ordinary_cell_blocks_reject() -> Result<(), Error> {
    let parsed = parse("|===\n|Cell\n|===", &Options::default())?;
    assert!(
        matches!(asg::export_document(parsed.document()),Err(asg::Error::Unsupported {path,..}) if path=="/blocks/0/inner")
    );
    let parsed = parse("|===\n|icon:heart[]\n|===", &Options::default())?;
    let value = asg::export_document_with_profile(parsed.document(), asg::Profile::Acdc)?;
    assert_eq!(
        value.pointer("/blocks/0/rows/0/cells/0/blocks/0/inlines/0/target"),
        Some(&json!("heart"))
    );
    let parsed = parse("* Nested item", &Options::default())?;
    let child = Block::from(parsed.document().blocks().next().ok_or("list")?);
    let document = with_table(Table::new(
        vec![TableRow::new(vec![TableCell::new(vec![child])])],
        Location::default(),
    ));
    assert!(
        matches!(asg::export_document_with_profile((&document).into(),asg::Profile::Acdc),Err(asg::Error::Unsupported {path,construct:Some("unordered list"),..}) if path=="/blocks/0/rows/0/cells/0/blocks/0")
    );
    let paragraph = Paragraph::new(
        vec![InlineNode::VerbatimText(Verbatim::new(
            "unclassified",
            Location::default(),
        ))],
        Location::default(),
    );
    let document = with_table(Table::new(
        vec![TableRow::new(vec![TableCell::new(vec![Block::Paragraph(
            paragraph,
        )])])],
        Location::default(),
    ));
    assert!(
        matches!(asg::export_document_with_profile((&document).into(),asg::Profile::Acdc),Err(asg::Error::Unsupported {path,construct:Some("verbatim text"),..}) if path=="/blocks/0/rows/0/cells/0/blocks/0/inlines/0")
    );
    Ok(())
}

#[test]
fn table_schema_rejects_unknown_fields_wrong_types_and_zero_spans() -> Result<(), Error> {
    let schema: Value = serde_json::from_str(include_str!("../../schemas/asg/acdc-v1.json"))?;
    let validator = jsonschema::validator_for(&schema)?;
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/asg/tables.adoc");
    let parsed = parse_file(&source, &Options::default())?;
    let valid = asg::export_document_with_profile(parsed.document(), asg::Profile::Acdc)?;
    assert!(validator.is_valid(&valid));
    for (path, field, value) in [
        ("/blocks/0", "unknown", json!(true)),
        ("/blocks/0/header", "unknown", json!(true)),
        ("/blocks/0/rows/0/cells/0", "unknown", json!(true)),
        ("/blocks/0/rows/0/cells/0", "blocks", json!("flattened")),
        ("/blocks/0/rows/0/cells/0", "colspan", json!(0)),
        ("/blocks/0/rows/0/cells/0", "rowspan", json!(0)),
        ("/blocks/0/rows/0/cells/0", "initial_attributes", json!({})),
        (
            "/blocks/0/columns/0",
            "width",
            json!({"kind":"auto","value":1}),
        ),
        (
            "/blocks/0/columns/0",
            "width",
            json!({"kind":"proportional","value":-1}),
        ),
        ("/blocks/0/columns/0", "style", json!("unknown")),
        ("/blocks/0/presentation", "frame", json!("unknown")),
        ("/blocks/0", "header", json!(null)),
    ] {
        let mut invalid = valid.clone();
        invalid
            .pointer_mut(path)
            .and_then(Value::as_object_mut)
            .ok_or("object")?
            .insert(field.into(), value);
        assert!(!validator.is_valid(&invalid), "{path}/{field}");
    }
    Ok(())
}
