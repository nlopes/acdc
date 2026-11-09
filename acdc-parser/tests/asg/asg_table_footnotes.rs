#![cfg(feature = "asg")]

use acdc_parser::{CompatibilityMode, Options, asg, parse_file};
use serde_json::{Value, json};

type Error = Box<dyn std::error::Error>;

#[test]
fn prepared_cells_reject_foreign_bindings_and_duplicate_global_numbers() -> Result<(), Error> {
    use acdc_parser::{
        Block, DelimitedBlock, DelimitedBlockType, Document, Footnote, InlineMacro, InlineNode,
        Location, Paragraph, Table, TableColumn, TableRow, ast::StructuralRef, parse,
    };
    let parsed = parse(
        "[cols=a,options=noheader]\n|===\n|footnote:same[Cell.]\n|===\n",
        &Options::default(),
    )?;
    let cell = parsed
        .document()
        .descendants()
        .find_map(|node| {
            if let StructuralRef::TableCell(cell) = node {
                Some(cell)
            } else {
                None
            }
        })
        .ok_or("cell")?;
    let mut document = Document::from(parsed.document());
    document.push_block(Block::Paragraph(Paragraph::new(
        vec![InlineNode::Macro(InlineMacro::Footnote(
            Footnote::reference(Some("same"), 1, Location::default()),
        ))],
        Location::default(),
    )));
    let before = serde_json::to_value(&document)?;
    assert!(asg::document_with_profile((&document).into(), asg::Profile::Acdc).is_err());
    assert_eq!(serde_json::to_value(&document)?, before);

    let cell = TableColumn::from(cell);
    document.set_blocks([Block::DelimitedBlock(DelimitedBlock::new(
        DelimitedBlockType::DelimitedTable(Table::new(
            vec![TableRow::new(vec![cell.clone(), cell])],
            Location::default(),
        )),
        "|===",
        Location::default(),
    ))]);
    assert!(
        matches!(asg::document_with_profile((&document).into(), asg::Profile::Acdc),
        Err(asg::Error::Unsupported { path, .. }) if path.ends_with("/footnotes/0/number"))
    );
    document.rebuild_footnotes()?;
    let value = asg::document_with_profile((&document).into(), asg::Profile::Acdc)?;
    assert_eq!(
        value.pointer("/blocks/0/rows/0/cells/0/footnotes/0/number"),
        Some(&json!(1))
    );
    assert_eq!(
        value.pointer("/blocks/0/rows/0/cells/1/footnotes/0/number"),
        Some(&json!(2))
    );
    Ok(())
}

#[rstest::rstest]
fn cell_catalogs_export_once_with_local_names_and_global_numbers(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/asg/table_cell_footnotes.adoc");
    let parsed = parse_file(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let value = asg::document_with_profile(parsed.document(), asg::Profile::Acdc)?;
    let expected: Value =
        serde_json::from_str(&std::fs::read_to_string(source.with_extension("json"))?)?;
    assert_eq!(value, expected);
    for (path, number) in [
        ("/footnotes/0", 1),
        ("/blocks/1/rows/0/cells/0/footnotes/0", 2),
        (
            "/blocks/1/rows/0/cells/0/blocks/1/rows/0/cells/0/footnotes/0",
            3,
        ),
        ("/blocks/1/rows/1/cells/0/footnotes/0", 4),
    ] {
        assert_eq!(
            value.pointer(&format!("{path}/number")),
            Some(&json!(number))
        );
        assert_eq!(value.pointer(&format!("{path}/id")), Some(&json!("same")));
    }
    let text = serde_json::to_string(&value)?;
    for body in ["Outernote.", "Firstnote.", "Nestednote.", "Secondnote."] {
        assert_eq!(text.matches(body).count(), 1, "{body}");
    }
    assert!(asg::document(parsed.document()).is_err());
    let schema: Value = serde_json::from_str(include_str!("../../schemas/asg/acdc-v1.json"))?;
    assert!(jsonschema::validator_for(&schema)?.is_valid(&value));
    let mut invalid = value;
    *invalid
        .pointer_mut("/blocks/1/rows/0/cells/0/footnotes")
        .ok_or("cell catalog")? = json!(true);
    assert!(!jsonschema::validator_for(&schema)?.is_valid(&invalid));
    Ok(())
}
