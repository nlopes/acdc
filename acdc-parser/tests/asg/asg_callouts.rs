#![cfg(feature = "asg")]

use acdc_parser::{
    Block, CalloutList, CalloutListItem, CalloutRef, CompatibilityMode, ContentAttribute, Document,
    InlineNode, Location, Options, asg, parse, parse_file,
};
use serde_json::{Value, json};
use std::{fs, path::Path};

type Error = Box<dyn std::error::Error + Send + Sync>;

fn markers(value: &Value) -> Result<Vec<u64>, Error> {
    value
        .as_array()
        .ok_or("inlines")?
        .iter()
        .filter(|node| node.get("name").and_then(Value::as_str) == Some("callout"))
        .map(|node| {
            node.get("number")
                .and_then(Value::as_u64)
                .ok_or_else(|| "number".into())
        })
        .collect()
}

#[test]
fn source_markers_numbers_locations_and_literal_code_are_preserved() -> Result<(), Error> {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/asg/callouts.adoc");
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
        let mut code = before.pointer("/blocks/0/inlines").ok_or("code")?.clone();
        for inline in code.as_array_mut().ok_or("code")? {
            if inline.get("name").and_then(Value::as_str) == Some("callout_reference") {
                let fields = inline.as_object_mut().ok_or("marker")?;
                let form = fields.remove("variant").ok_or("variant")?;
                fields.insert("name".into(), json!("callout"));
                fields.insert("form".into(), form);
            }
        }
        assert_eq!(value.pointer("/blocks/0/inlines"), Some(&code));
        assert_eq!(
            value.pointer("/blocks/0/inlines/1/form"),
            Some(&json!("auto"))
        );
        assert_eq!(
            value.pointer("/blocks/0/inlines/5/form"),
            Some(&json!("explicit"))
        );
        assert!(
            value
                .pointer("/blocks/1/items/0/callout_location")
                .is_some()
        );
        assert_eq!(serde_json::to_value(parsed.document())?, before);
        assert!(
            matches!(asg::document(parsed.document()), Err(asg::Error::Unsupported{path,construct:Some("callout"),..}) if path == "/blocks/0/inlines/1")
        );
    }
    Ok(())
}

#[test]
fn scoped_numbers_keep_repeated_and_unmatched_associations() -> Result<(), Error> {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/asg/callouts.adoc");
    let parsed = parse_file(&source, &Options::default())?;
    let value = asg::document_with_profile(parsed.document(), asg::Profile::Acdc)?;
    let numbers = markers(value.pointer("/blocks/0/inlines").ok_or("code")?)?;
    assert_eq!(numbers, [1, 2, 1, 9]);
    for (index, number, count) in [(0, 1, 2), (1, 2, 1), (2, 3, 0)] {
        assert_eq!(
            value.pointer(&format!("/blocks/1/items/{index}/number")),
            Some(&json!(number))
        );
        assert_eq!(numbers.iter().filter(|&&n| n == number).count(), count);
    }
    assert_eq!(
        markers(
            value
                .pointer("/blocks/2/blocks/0/inlines")
                .ok_or("nested code")?
        )?,
        [1, 1]
    );
    assert_eq!(
        value.pointer("/blocks/2/blocks/1/items/0/number"),
        Some(&json!(1))
    );
    let parsed = parse(
        "----\nfirst <1>\n----\n\nBetween.\n\n----\nsecond <2>\n----\n<1> First\n<2> Second",
        &Options::default(),
    )?;
    let value = asg::document_with_profile(parsed.document(), asg::Profile::Acdc)?;
    assert_eq!(
        markers(value.pointer("/blocks/0/inlines").ok_or("first")?)?,
        [1]
    );
    assert_eq!(
        markers(value.pointer("/blocks/2/inlines").ok_or("second")?)?,
        [2]
    );
    assert_eq!(value.pointer("/blocks/3/items/1/number"), Some(&json!(2)));
    Ok(())
}

#[test]
fn prepared_markers_do_not_renumber_or_invent_locations() -> Result<(), Error> {
    let content = ContentAttribute::new(vec![
        InlineNode::CalloutRef(CalloutRef::auto(42, Location::default())),
        InlineNode::CalloutRef(CalloutRef::explicit(0, Location::default())),
    ]);
    assert_eq!(
        asg::inlines_with_profile((&content).into(), asg::Profile::Acdc)?,
        json!([
            {"name":"callout","type":"inline","form":"auto","number":42},
            {"name":"callout","type":"inline","form":"explicit","number":0},
        ])
    );
    let mut document = Document::default();
    document.push_block(Block::CalloutList(CalloutList::new(
        vec![CalloutListItem::new(
            CalloutRef::auto(42, Location::default()),
            Vec::new(),
            Location::default(),
        )],
        Location::default(),
    )));
    let value = asg::document_with_profile((&document).into(), asg::Profile::Acdc)?;
    assert_eq!(value.pointer("/blocks/0/items/0/number"), Some(&json!(42)));
    assert_eq!(
        value.pointer("/blocks/0/items/0/marker"),
        Some(&json!("<.>"))
    );
    assert!(
        value
            .pointer("/blocks/0/items/0/callout_location")
            .is_none()
    );
    assert!(
        matches!(asg::document((&document).into()), Err(asg::Error::Unsupported{path,..}) if path == "/blocks/0/items/0/callout/number")
    );
    Ok(())
}

#[test]
fn callout_schema_closes_markers_and_scopes_item_numbers() -> Result<(), Error> {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/asg/callouts.adoc");
    let parsed = parse_file(&source, &Options::default())?;
    let value = asg::document_with_profile(parsed.document(), asg::Profile::Acdc)?;
    let schema: Value = serde_json::from_str(include_str!("../../schemas/asg/acdc-v1.json"))?;
    let validator = jsonschema::validator_for(&schema)?;
    assert!(validator.is_valid(&value));
    for (path, field, bad) in [
        ("/blocks/0/inlines/1", "extra", json!(true)),
        ("/blocks/0/inlines/1", "form", json!("unknown")),
        ("/blocks/0/inlines/1", "number", json!(-1)),
        ("/blocks/1/items/0", "number", json!(0)),
        ("/blocks/1", "variant", json!("unordered")),
    ] {
        let mut invalid = value.clone();
        invalid
            .pointer_mut(path)
            .and_then(Value::as_object_mut)
            .ok_or("node")?
            .insert(field.into(), bad);
        assert!(!validator.is_valid(&invalid), "{path}/{field}");
    }
    Ok(())
}
