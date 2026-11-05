#![cfg(feature = "asg")]

use acdc_parser::{
    CompatibilityMode, DocumentAttributeAssignment, Options, asg,
    ast::{BlockKind, BlockRef},
    parse, parse_file,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path};

type Error = Box<dyn std::error::Error + Send + Sync>;

fn apply(value: &Value, attributes: &mut BTreeMap<String, String>) -> Result<(), Error> {
    let name = value
        .get("attribute")
        .and_then(Value::as_str)
        .ok_or("attribute")?;
    let assignment = value.get("assignment").ok_or("assignment")?;
    match assignment.get("kind").and_then(Value::as_str) {
        Some("set") => {
            attributes.insert(
                name.into(),
                assignment
                    .get("value")
                    .and_then(Value::as_str)
                    .ok_or("value")?
                    .into(),
            );
        }
        Some("unset") => {
            attributes.remove(name);
        }
        _ => return Err("assignment kind".into()),
    }
    Ok(())
}

fn images(
    blocks: &[Value],
    attributes: &mut BTreeMap<String, String>,
    output: &mut Vec<String>,
) -> Result<(), Error> {
    for block in blocks {
        match block.get("name").and_then(Value::as_str) {
            Some("attribute") => apply(block, attributes)?,
            Some("image") => {
                let target = block
                    .get("target")
                    .and_then(Value::as_str)
                    .ok_or("target")?;
                output.push(
                    attributes
                        .get("imagesdir")
                        .map_or_else(|| target.into(), |dir| format!("{dir}/{target}")),
                );
            }
            Some("table") => {
                for row in block
                    .get("header")
                    .into_iter()
                    .chain(
                        block
                            .get("rows")
                            .and_then(Value::as_array)
                            .into_iter()
                            .flatten(),
                    )
                    .chain(block.get("footer"))
                {
                    for cell in row.get("cells").and_then(Value::as_array).ok_or("cells")? {
                        let mut local = attributes.clone();
                        for entry in cell
                            .get("initial_attributes")
                            .and_then(Value::as_array)
                            .into_iter()
                            .flatten()
                        {
                            apply(entry, &mut local)?;
                        }
                        images(
                            cell.get("blocks")
                                .and_then(Value::as_array)
                                .ok_or("blocks")?,
                            &mut local,
                            output,
                        )?;
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

#[test]
fn nested_cell_blocks_and_media_events_stay_in_their_scope() -> Result<(), Error> {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/asg/table_cells.adoc");
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
        assert_eq!(
            value,
            asg::document_with_profile(parsed.document(), asg::Profile::Acdc)?
        );
        assert_eq!(serde_json::to_value(parsed.document())?, before);
        let mut attributes = BTreeMap::new();
        let mut output = Vec::new();
        images(
            value
                .get("blocks")
                .and_then(Value::as_array)
                .ok_or("blocks")?,
            &mut attributes,
            &mut output,
        )?;
        assert_eq!(
            output,
            [
                "first/one.svg",
                "changed/changed.svg",
                "plain.svg",
                "second/two.svg",
                "after.svg"
            ]
        );
        assert!(attributes.is_empty());
        assert_eq!(
            value.pointer("/blocks/0/rows/0/cells/0/blocks/3/name"),
            Some(&json!("list"))
        );
        assert_eq!(
            value.pointer("/blocks/0/rows/0/cells/0/blocks/4/name"),
            Some(&json!("listing"))
        );
        assert!(
            matches!(asg::document(parsed.document()),Err(asg::Error::Unsupported{path,..}) if path=="/blocks/0/inner")
        );
    }
    Ok(())
}

#[test]
fn initial_assignments_keep_native_order_and_distinct_normalized_snapshots() -> Result<(), Error> {
    let parsed = parse(
        "= T\n\n[cols=\"a,a\"]\n|===\n|\n:toc: macro\n\nFirst.\n|\n:toc: preamble\n\nSecond.\n|===",
        &Options::default(),
    )?;
    let Some(BlockKind::Table(table)) = parsed.document().blocks().next().map(BlockRef::kind)
    else {
        return Err("table".into());
    };
    let value = asg::document_with_profile(parsed.document(), asg::Profile::Acdc)?;
    let mut snapshots = Vec::new();
    for (index, cell) in table.rows().next().ok_or("row")?.cells().enumerate() {
        let native = cell
            .initial_attributes()
            .map(|(name, assignment)| {
                let assignment = match assignment {
                    DocumentAttributeAssignment::Unset => json!({"kind":"unset"}),
                    DocumentAttributeAssignment::Set(value) => {
                        let mut text = String::new();
                        value.write_text(&mut text)?;
                        json!({"kind":"set","value":text})
                    }
                };
                Ok(json!({"attribute":name,"assignment":assignment}))
            })
            .collect::<Result<Vec<_>, Error>>()?;
        assert_eq!(
            value.pointer(&format!(
                "/blocks/0/rows/0/cells/{index}/initial_attributes"
            )),
            Some(&json!(native))
        );
        snapshots.push(native);
    }
    assert_ne!(snapshots.first(), snapshots.last());
    assert!(value.pointer("/attributes/toc").is_none());
    Ok(())
}

#[test]
fn nested_sections_and_tables_keep_native_block_owners() -> Result<(), Error> {
    let parsed = parse(
        "[cols=a]\n|===\n|\n== Inner\n\n[cols=2]\n!===\n!One !Two\n!===\n|===",
        &Options::default(),
    )?;
    let value = asg::document_with_profile(parsed.document(), asg::Profile::Acdc)?;
    assert_eq!(
        value.pointer("/blocks/0/rows/0/cells/0/blocks/0/name"),
        Some(&json!("section"))
    );
    assert_eq!(
        value.pointer("/blocks/0/rows/0/cells/0/blocks/0/blocks/0/name"),
        Some(&json!("table"))
    );
    assert_eq!(
        value.pointer("/blocks/0/rows/0/cells/0/blocks/0/blocks/0/delimiter"),
        Some(&json!("!==="))
    );
    Ok(())
}

#[test]
fn nested_errors_and_initial_assignment_schema_remain_precise() -> Result<(), Error> {
    let parsed = parse("[cols=a]\n|===\n|icon:heart[]\n|===", &Options::default())?;
    let value = asg::document_with_profile(parsed.document(), asg::Profile::Acdc)?;
    assert_eq!(
        value.pointer("/blocks/0/rows/0/cells/0/blocks/0/inlines/0/target"),
        Some(&json!("heart"))
    );
    let parsed = parse(
        "[cols=a]\n|===\n|\n:toc: macro\n\nText.\n|===",
        &Options::default(),
    )?;
    let value = asg::document_with_profile(parsed.document(), asg::Profile::Acdc)?;
    let schema: Value = serde_json::from_str(include_str!("../schemas/asg/acdc-v1.json"))?;
    let validator = jsonschema::validator_for(&schema)?;
    assert!(validator.is_valid(&value));
    for invalid in [
        json!({}),
        json!({"attribute":"bad name","assignment":{"kind":"unset"}}),
        json!({"attribute":"x","assignment":{"kind":"set"}}),
        json!({"attribute":"x","assignment":{"kind":"unset","value":""}}),
        json!({"attribute":"x","assignment":{"kind":"unset"},"unknown":true}),
    ] {
        let mut changed = value.clone();
        *changed
            .pointer_mut("/blocks/0/rows/0/cells/0/initial_attributes/0")
            .ok_or("initial")? = invalid;
        assert!(!validator.is_valid(&changed));
    }
    Ok(())
}
