#![cfg(feature = "asg")]

use acdc_parser::{CompatibilityMode, Options, asg, parse};
use serde::Deserialize;
use serde_json::{Map, Value, json};

type Error = Box<dyn std::error::Error + Send + Sync>;

const CORE: &str = include_str!("../schemas/asg/proposals/content-images.jsonpatch");
const CAPTIONS: &str = include_str!("../schemas/asg/proposals/caption-state.jsonpatch");

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Change {
    op: String,
    path: String,
    value: Value,
}

fn apply(schema: &mut Value, patch: &str) -> Result<(), Error> {
    for change in serde_json::from_str::<Vec<Change>>(patch)? {
        let (parent, key) = change.path.rsplit_once('/').ok_or("property path")?;
        let target = schema.pointer_mut(parent).ok_or("patch parent")?;
        match (change.op.as_str(), target) {
            ("add", Value::Array(values)) if key == "-" => values.push(change.value),
            ("add", Value::Object(properties)) => {
                if properties.insert(key.into(), change.value).is_some() {
                    return Err("proposal unexpectedly replaces an upstream property".into());
                }
            }
            ("replace", Value::Object(properties)) => {
                *properties.get_mut(key).ok_or("replacement property")? = change.value;
            }
            _ => return Err("unsupported proposal patch operation".into()),
        }
    }
    Ok(())
}

fn schemas() -> Result<(Value, Value, Value), Error> {
    let base: Value = serde_json::from_str(include_str!("../schemas/asg/schema.json"))?;
    let mut core = base.clone();
    apply(&mut core, CORE)?;
    let mut captions = core.clone();
    apply(&mut captions, CAPTIONS)?;
    Ok((base, core, captions))
}

fn example(content: Value) -> Value {
    Value::Object(Map::from_iter([
        ("name".into(), "document".into()),
        ("type".into(), "block".into()),
        ("blocks".into(), Value::Array(vec![content])),
    ]))
}

#[test]
fn proposed_content_and_images_are_closed_and_independent_of_the_pin() -> Result<(), Error> {
    let (base, core, _) = schemas()?;
    let original = jsonschema::validator_for(&base)?;
    let proposed = jsonschema::validator_for(&core)?;
    let examples = [
        example(
            json!({"name":"quote","type":"block","form":"delimited","delimiter":"____","attribution":[],"citetitle":[{"name":"span","type":"inline","variant":"emphasis","form":"constrained","inlines":[{"name":"text","type":"string","value":"Book"}]}],"blocks":[]}),
        ),
        example(
            json!({"name":"paragraph","type":"block","caption":[{"name":"image","type":"inline","target":"badge.png","alt":[],"tooltip":[]}],"inlines":[]}),
        ),
        example(
            json!({"name":"image","type":"block","form":"macro","target":"photo.png","alt":[]}),
        ),
    ];
    for value in examples {
        assert!(!original.is_valid(&value));
        assert!(proposed.is_valid(&value), "{value}");
    }
    for value in [
        example(json!({"name":"paragraph","type":"block","attribution":"scalar"})),
        example(
            json!({"name":"paragraph","type":"block","caption":[{"name":"image","type":"inline","target":"badge.png","unknown":true}]}),
        ),
        example(json!({"name":"paragraph","type":"block","caption":[],"unknown":true})),
        example(json!({"name":"video","type":"block","form":"macro","target":"movie","alt":[]})),
    ] {
        assert!(!proposed.is_valid(&value), "{value}");
    }
    assert_eq!(base.get("$id"), Some(&json!(asg::SCHEMA_ID)));
    Ok(())
}

#[test]
fn caption_state_is_an_explicit_optional_supplement() -> Result<(), Error> {
    let (_, core, captions) = schemas()?;
    let core = jsonschema::validator_for(&core)?;
    let captions = jsonschema::validator_for(&captions)?;
    for state in [
        json!({"kind":"unnumbered"}),
        json!({"kind":"numbered","counter":"figure","label":"Figure"}),
        json!({"kind":"numbered","counter":"example","label":"Example","number":2}),
    ] {
        let value = example(json!({"name":"paragraph","type":"block","captioning":state}));
        assert!(!core.is_valid(&value));
        assert!(captions.is_valid(&value), "{value}");
    }
    assert!(captions.is_valid(&example(
        json!({"name":"paragraph","type":"block","caption":[],"captioning":{"kind":"custom"}})
    )));
    for value in [
        example(json!({"name":"paragraph","type":"block","captioning":{"kind":"custom"}})),
        example(
            json!({"name":"paragraph","type":"block","caption":[],"captioning":{"kind":"unnumbered"}}),
        ),
        example(
            json!({"name":"paragraph","type":"block","captioning":{"kind":"numbered","counter":"figure","label":"Figure","number":0}}),
        ),
        example(
            json!({"name":"paragraph","type":"block","captioning":{"kind":"unnumbered","unknown":true}}),
        ),
    ] {
        assert!(!captions.is_valid(&value), "{value}");
    }
    Ok(())
}

fn proposed_value(value: &mut Value) -> Result<(), Error> {
    match value {
        Value::Object(properties) => {
            if properties.get("type").and_then(Value::as_str) == Some("block")
                && let Some(value) = properties.remove("caption")
            {
                let Value::Object(mut caption) = value else {
                    return Err("expected acdc caption object".into());
                };
                let kind = caption
                    .get("kind")
                    .and_then(Value::as_str)
                    .ok_or("caption kind")?;
                if kind == "custom" {
                    properties.insert(
                        "caption".into(),
                        caption.remove("prefix").ok_or("caption prefix")?,
                    );
                }
                properties.insert("captioning".into(), Value::Object(caption));
            }
            for child in properties.values_mut() {
                proposed_value(child)?;
            }
        }
        Value::Array(values) => {
            for child in values {
                proposed_value(child)?;
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
    Ok(())
}

#[test]
fn migration_preserves_real_semantic_content_without_changing_native_values() -> Result<(), Error> {
    let (_, _, schema) = schemas()?;
    let proposed = jsonschema::validator_for(&schema)?;
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let parsed = parse(
            include_str!("../fixtures/tests/asg_content_images.adoc"),
            &options,
        )?;
        let before = serde_json::to_value(parsed.document())?;
        let mut value = asg::document_with_profile(parsed.document(), asg::Profile::Acdc)?;
        let prefix = value
            .pointer("/blocks/1/caption/prefix")
            .ok_or("caption prefix")?
            .clone();
        let alt = value.pointer("/blocks/2/alt").ok_or("image alt")?.clone();
        value.as_object_mut().ok_or("document")?.remove("profile");
        proposed_value(&mut value)?;
        assert!(proposed.is_valid(&value), "{value}");
        assert_eq!(value.pointer("/blocks/1/caption"), Some(&prefix));
        assert_eq!(
            value.pointer("/blocks/1/captioning/kind"),
            Some(&json!("custom"))
        );
        assert_eq!(value.pointer("/blocks/2/alt"), Some(&alt));
        assert_eq!(
            value.pointer("/blocks/2/captioning/counter"),
            Some(&json!("figure"))
        );
        assert_eq!(serde_json::to_value(parsed.document())?, before);
    }
    Ok(())
}
