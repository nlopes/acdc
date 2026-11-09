#![cfg(feature = "asg")]

use std::{collections::BTreeMap, fs, path::Path};

use acdc_parser::{CompatibilityMode, Options, asg, parse, parse_file};
use serde_json::{Value, json};

type Error = Box<dyn std::error::Error + Send + Sync>;

fn inspect_blocks(
    values: &[Value],
    attributes: &mut BTreeMap<String, String>,
    images: &mut Vec<String>,
    events: &mut Vec<(String, Value)>,
) -> Result<(), Error> {
    for value in values {
        match value.get("name").and_then(Value::as_str) {
            Some("attribute") => {
                let name = value
                    .get("attribute")
                    .and_then(Value::as_str)
                    .ok_or("name")?;
                let assignment = value.get("assignment").ok_or("assignment")?;
                events.push((name.to_owned(), assignment.clone()));
                match assignment.get("kind").and_then(Value::as_str) {
                    Some("set") => {
                        let value = assignment
                            .get("value")
                            .and_then(Value::as_str)
                            .ok_or("value")?;
                        attributes.insert(name.to_owned(), value.to_owned());
                    }
                    Some("unset") => {
                        attributes.remove(name);
                    }
                    _ => return Err("unknown assignment".into()),
                }
            }
            Some("image") => {
                let target = value
                    .get("target")
                    .and_then(Value::as_str)
                    .ok_or("target")?;
                images.push(
                    attributes
                        .get("imagesdir")
                        .map_or_else(|| target.to_owned(), |dir| format!("{dir}/{target}")),
                );
            }
            _ => {}
        }
        if let Some(blocks) = value.get("blocks").and_then(Value::as_array) {
            inspect_blocks(blocks, attributes, images, events)?;
        }
    }
    Ok(())
}

#[test]
fn body_events_keep_header_snapshot_values_and_media_order() -> Result<(), Error> {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/asg/body_attributes.adoc");
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let parsed = parse_file(&source, &options)?;
        let native = serde_json::to_value(parsed.document())?;
        let value = asg::export_document_with_profile(parsed.document(), asg::Profile::Acdc)?;
        let expected: Value =
            serde_json::from_str(&fs::read_to_string(source.with_extension("json"))?)?;
        assert_eq!(value, expected, "{mode:?}");
        assert_eq!(serde_json::to_value(parsed.document())?, native);
        assert_eq!(
            value.pointer("/attributes/imagesdir"),
            Some(&json!("header"))
        );
        assert_eq!(value.pointer("/attributes/flag"), Some(&json!("header")));
        let mut attributes = BTreeMap::from([("imagesdir".into(), "header".into())]);
        let mut images = Vec::new();
        let mut events = Vec::new();
        inspect_blocks(
            value
                .get("blocks")
                .and_then(Value::as_array)
                .ok_or("blocks")?,
            &mut attributes,
            &mut images,
            &mut events,
        )?;
        assert_eq!(
            images,
            [
                "header/before.png",
                "body/middle.png",
                "after.png",
                "nested/nested.png",
                "last.png"
            ]
        );
        for (name, expected) in [
            ("flag", json!({"kind":"set","value":"false"})),
            ("flag", json!({"kind":"unset"})),
            ("presence", json!({"kind":"set","value":""})),
            ("presence", json!({"kind":"unset"})),
            ("toclevels", json!({"kind":"set","value":"03"})),
            ("name", json!({"kind":"set","value":"attribute"})),
        ] {
            assert!(events.contains(&(name.into(), expected)), "{events:?}");
        }
    }
    Ok(())
}

#[test]
fn strict_export_and_unresolved_value_errors_remain_explicit() -> Result<(), Error> {
    let parsed = parse("Before.\n\n:feature: false\n\nAfter.", &Options::default())?;
    assert!(
        matches!(asg::export_document(parsed.document()), Err(asg::Error::Unsupported {path, ..}) if path == "/blocks/1")
    );
    let parsed = parse(
        "Before.\n\n:feature: pass:[*protected*]\n\nAfter.",
        &Options::default(),
    )?;
    assert!(
        matches!(asg::export_document_with_profile(parsed.document(), asg::Profile::Acdc), Err(asg::Error::Unsupported {path, ..}) if path == "/blocks/1/assignment/value")
    );
    Ok(())
}

#[test]
fn body_event_schema_requires_separate_discriminators_and_exact_assignment_shapes()
-> Result<(), Error> {
    let schema: Value = serde_json::from_str(include_str!("../../schemas/asg/acdc-v1.json"))?;
    let validator = jsonschema::validator_for(&schema)?;
    let document = |event: Value| json!({"name":"document","type":"block","profile":asg::ACDC_SCHEMA_ID,"blocks":[event]});
    let event = |assignment: Value| json!({"name":"attribute","type":"block","attribute":"name","assignment":assignment});
    for assignment in [
        json!({"kind":"set","value":"false"}),
        json!({"kind":"set","value":""}),
        json!({"kind":"unset"}),
    ] {
        assert!(validator.is_valid(&document(event(assignment))));
    }
    for assignment in [
        json!({"kind":"set"}),
        json!({"kind":"unset","value":""}),
        json!({"kind":"set","value":false}),
        json!({"kind":"set","value":null}),
        json!({"kind":"other"}),
        json!({"kind":"unset","extra":true}),
    ] {
        assert!(!validator.is_valid(&document(event(assignment))));
    }
    let mut extra = event(json!({"kind":"unset"}));
    extra
        .as_object_mut()
        .ok_or("object")?
        .insert("unknown".into(), json!(true));
    assert!(!validator.is_valid(&document(extra)));
    Ok(())
}

#[rstest::rstest]
#[case(":imagesdir!:", None, "unset")]
#[case(":imagesdir: tail", Some("tail"), "set")]
#[case(":imagesdir:", Some(""), "set")]
fn trailing_open_block_attributes_keep_source_order(
    #[case] entry: &str,
    #[case] expected: Option<&str>,
    #[case] kind: &str,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = format!(
        "= Boundary\n:imagesdir: header\n\n--\n:imagesdir: inner\nimage::inside.png[]\n\n{entry}\n--\n\nimage::after.png[]\n"
    );
    let options = Options::builder().with_compatibility_mode(mode).build()?;
    let parsed = parse(&source, &options)?;
    let native = serde_json::to_value(parsed.document())?;
    let value = asg::export_document_with_profile(parsed.document(), asg::Profile::Acdc)?;
    let blocks = value
        .get("blocks")
        .and_then(Value::as_array)
        .ok_or("blocks")?;
    let trailing = blocks
        .first()
        .and_then(|block| block.get("blocks"))
        .and_then(Value::as_array)
        .and_then(|blocks| blocks.last())
        .ok_or("trailing event")?;
    assert_eq!(trailing.get("attribute"), Some(&json!("imagesdir")));
    assert_eq!(
        trailing.pointer("/assignment/kind").and_then(Value::as_str),
        Some(kind)
    );
    assert_eq!(trailing.pointer("/location/0/line"), Some(&json!(8)));
    let mut attributes = BTreeMap::from([("imagesdir".into(), "header".into())]);
    inspect_blocks(blocks, &mut attributes, &mut Vec::new(), &mut Vec::new())?;
    assert_eq!(attributes.get("imagesdir").map(String::as_str), expected);
    assert_eq!(serde_json::to_value(parsed.document())?, native);
    Ok(())
}
