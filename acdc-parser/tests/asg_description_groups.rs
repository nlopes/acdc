#![cfg(feature = "asg")]

use acdc_parser::{
    Anchor, Block, CompatibilityMode, DescriptionList, DescriptionListDefinition,
    DescriptionListItem, DescriptionListTerm, Document, Location, Options, asg, parse,
};
use serde_json::{Value, json};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
#[case("first::\nsecond::", false, false)]
#[case("first::\nsecond:: body", true, true)]
#[case(":blank:\n\nfirst::\nsecond:: {blank}", true, true)]
#[case("first::\nsecond::\n+\nAttached.", true, false)]
fn description_exports_keep_term_order_and_definition_presence(
    #[case] source: &str,
    #[case] definition_present: bool,
    #[case] principal_present: bool,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let native = serde_json::to_value(parsed.document())?;
    let strict = asg::document(parsed.document())?;
    let named = asg::document_with_profile(parsed.document(), asg::Profile::Acdc)?;
    let group = named.pointer("/blocks/0/items/0").ok_or("named group")?;
    assert_eq!(
        group.get("definition_present"),
        Some(&json!(definition_present))
    );
    assert_eq!(
        group.get("definition_location").is_some(),
        definition_present
    );
    assert_eq!(group.get("principal").is_some(), principal_present);
    assert_eq!(group.pointer("/terms/0/0/value"), Some(&json!("first")));
    assert_eq!(group.pointer("/terms/1/0/value"), Some(&json!("second")));
    let metadata = group
        .get("term_metadata")
        .and_then(Value::as_array)
        .ok_or("metadata")?;
    assert_eq!(
        metadata.len(),
        group
            .get("terms")
            .and_then(Value::as_array)
            .ok_or("terms")?
            .len()
    );
    assert_eq!(metadata.len(), 2);
    assert!(
        metadata
            .iter()
            .all(|term| term.get("name") == Some(&json!("dlistTerm"))
                && term.get("inlines").is_none())
    );
    assert_eq!(
        native.pointer("/blocks/0/items/0/definition").is_some(),
        definition_present
    );
    for field in ["terms", "principal", "blocks"] {
        assert_eq!(
            strict.pointer(&format!("/blocks/0/items/0/{field}")),
            group.get(field)
        );
    }
    for (value, schema) in [
        (&strict, include_str!("../schemas/asg/schema.json")),
        (&named, include_str!("../schemas/asg/acdc-v1.json")),
    ] {
        let schema: Value = serde_json::from_str(schema)?;
        assert!(
            jsonschema::validator_for(&schema)?.is_valid(value),
            "{value:#}"
        );
    }
    Ok(())
}

#[test]
fn constructed_empty_definitions_and_term_identities_need_named_export() -> Result<(), Error> {
    let terms = ["first", "second"].map(|id| {
        let mut term = DescriptionListTerm::new(Vec::new(), "::", Location::default());
        term.push_anchor(Anchor::new(id, Location::default()));
        term
    });
    let item = DescriptionListItem::new(
        terms.into(),
        Some(DescriptionListDefinition::new(
            None,
            Vec::new(),
            Location::default(),
        )),
        Location::default(),
    );
    let mut document = Document::default();
    document.push_block(Block::DescriptionList(DescriptionList::new(
        vec![item],
        Location::default(),
    )));
    document.rebuild_references_and_toc()?;
    assert!(asg::document((&document).into()).is_err());
    let value = asg::document_with_profile((&document).into(), asg::Profile::Acdc)?;
    for (index, id) in ["first", "second"].into_iter().enumerate() {
        let owner = format!("/blocks/0/items/0/term_metadata/{index}");
        assert_eq!(
            value.pointer(&format!("/references/{id}/owner")),
            Some(&json!(owner))
        );
        assert_eq!(value.pointer(&format!("{owner}/id")), Some(&json!(id)));
    }
    assert_eq!(
        value.pointer("/blocks/0/items/0/definition_present"),
        Some(&json!(true))
    );
    assert!(value.pointer("/blocks/0/items/0/principal").is_none());
    Ok(())
}

#[test]
fn named_description_metadata_is_closed_and_checks_definition_presence() -> Result<(), Error> {
    let parsed = parse("first::\nsecond:: body", &Options::default())?;
    let value = asg::document_with_profile(parsed.document(), asg::Profile::Acdc)?;
    let schema: Value = serde_json::from_str(include_str!("../schemas/asg/acdc-v1.json"))?;
    let validator = jsonschema::validator_for(&schema)?;
    assert!(validator.is_valid(&value));
    for (path, field, replacement) in [
        (
            "/blocks/0/items/0/term_metadata/0",
            "unknown",
            Some(json!(true)),
        ),
        (
            "/blocks/0/items/0/term_metadata/0",
            "marker",
            Some(json!("")),
        ),
        (
            "/blocks/0/items/0",
            "definition_location",
            Some(json!(false)),
        ),
        ("/blocks/0/items/0", "definition_present", None),
        (
            "/blocks/0/items/0",
            "definition_present",
            Some(json!(false)),
        ),
    ] {
        let mut invalid = value.clone();
        let object = invalid
            .pointer_mut(path)
            .and_then(Value::as_object_mut)
            .ok_or("object")?;
        if let Some(replacement) = replacement {
            object.insert(field.into(), replacement);
        } else {
            object.remove(field);
        }
        assert!(!validator.is_valid(&invalid), "{path}/{field}");
    }
    Ok(())
}
