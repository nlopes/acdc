#![cfg(feature = "asg")]

use acdc_parser::{
    Anchor, Block, CompatibilityMode, ContentAttribute, DescriptionList, DescriptionListItem,
    Document, InlineNode, Location, Options, Plain, Reference, asg, parse, parse_file,
};
use serde_json::{Value, json};
use std::{fs, path::Path};

type Error = Box<dyn std::error::Error + Send + Sync>;

fn text(value: &'static str) -> ContentAttribute<'static> {
    ContentAttribute::new(vec![InlineNode::PlainText(Plain::new(
        value,
        Location::default(),
    ))])
}

#[test]
fn registrations_preserve_membership_and_point_to_exported_owners() -> Result<(), Error> {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/asg/reference_graph.adoc");
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
        let references = value
            .get("references")
            .and_then(Value::as_object)
            .ok_or("references")?;
        assert_eq!(references.len(), parsed.document().references().len());
        for (id, record) in references {
            let owner = record.get("owner").and_then(Value::as_str).ok_or("owner")?;
            assert!(value.pointer(owner).is_some(), "{id}: {owner}");
        }
        assert!(!references.contains_key("earlier"));
        assert_eq!(
            value.pointer("/references/guide/owner"),
            Some(&json!("/header"))
        );
        assert_eq!(
            value.pointer("/references/block/owner"),
            Some(&json!("/blocks/0"))
        );
        assert_eq!(
            value.pointer("/references/inline/owner"),
            Some(&json!("/blocks/0/inlines/1"))
        );
        assert_eq!(
            value.pointer("/references/span/owner"),
            Some(&json!("/blocks/0/inlines/3"))
        );
        assert_eq!(
            value.pointer("/references//reftext/0/variant"),
            Some(&json!("strong"))
        );
        assert!(value.pointer("/references/block/reftext").is_none());
        assert_eq!(
            asg::document_with_profile(parsed.document(), asg::Profile::Acdc)?,
            value
        );
        assert_eq!(serde_json::to_value(parsed.document())?, before);
    }
    Ok(())
}

#[test]
fn prepared_alias_labels_keep_absent_empty_and_formatted_meanings() -> Result<(), Error> {
    let parsed = parse(
        "[#target,reftext='Primary']\n.Titled\nBody.",
        &Options::default(),
    )?;
    for label in [
        None,
        Some(ContentAttribute::default()),
        Some(text("*literal*")),
    ] {
        let primary = Reference::from(
            parsed
                .document()
                .references()
                .get("target")
                .ok_or("primary")?,
        );
        let mut alias = primary.clone();
        alias.set_xreflabel(label.clone());
        alias.set_location(Location::default());
        let mut document = Document::from(parsed.document());
        document.set_references([("target", primary), ("alias/~", alias)]);
        let before = serde_json::to_value(&document)?;
        let value = asg::document_with_profile((&document).into(), asg::Profile::Acdc)?;
        assert_eq!(
            value.pointer("/references/alias~1~0/target"),
            Some(&json!("target"))
        );
        assert_eq!(
            value.pointer("/references/alias~1~0/owner"),
            Some(&json!("/blocks/0"))
        );
        let expected = match label {
            None => Value::Null,
            Some(label) if label.is_empty() => json!([]),
            Some(_) => json!([{"name":"text","type":"string","value":"*literal*"}]),
        };
        assert_eq!(
            value.pointer("/references/alias~1~0/reftext"),
            Some(&expected)
        );
        assert!(value.pointer("/references/alias~1~0/location").is_none());
        assert_eq!(serde_json::to_value(&document)?, before);
        assert!(asg::document((&document).into()).is_err());
    }
    Ok(())
}

#[test]
fn description_item_declarations_keep_separate_registered_labels() -> Result<(), Error> {
    let mut term = acdc_parser::DescriptionListTerm::new(Vec::new(), "::", Location::default());
    term.set_anchors([
        Anchor::new("first", Location::default()).with_xreflabel(Some(text("First"))),
        Anchor::new("second", Location::default()).with_xreflabel(Some(text("Second"))),
    ]);
    let mut document = Document::default();
    document.push_block(Block::DescriptionList(DescriptionList::new(
        vec![DescriptionListItem::new(
            vec![term],
            None,
            Location::default(),
        )],
        Location::default(),
    )));
    document.set_references([
        ("first", {
            let mut reference = Reference::new("first", Location::default());
            reference.set_xreflabel(Some(text("First")));
            reference
        }),
        ("second", {
            let mut reference = Reference::new("second", Location::default());
            reference.set_xreflabel(Some(text("Second")));
            reference
        }),
    ]);
    let value = asg::document_with_profile((&document).into(), asg::Profile::Acdc)?;
    for (id, owner, label) in [
        ("first", "/blocks/0/items/0/term_metadata/0", "First"),
        (
            "second",
            "/blocks/0/items/0/term_metadata/0/anchors/0",
            "Second",
        ),
    ] {
        assert_eq!(
            value.pointer(&format!("/references/{id}/owner")),
            Some(&json!(owner))
        );
        assert_eq!(
            value.pointer(&format!("{owner}/reftext/0/value")),
            Some(&json!(label))
        );
    }
    Ok(())
}

#[test]
fn document_top_uses_resolved_labels_and_first_section_fallback() -> Result<(), Error> {
    for (source, owner, label) in [
        (
            "= Main: Subtitle\n:reftext: *Document*\n\nxref:#[].",
            "/header",
            true,
        ),
        ("== First\n\nxref:#[].", "/blocks/0", false),
        (
            ":reftext: *Document*\n\n== First\n\nxref:#[].",
            "/blocks/0",
            true,
        ),
        ("xref:#[].", "", false),
    ] {
        let parsed = parse(source, &Options::default())?;
        let value = asg::document_with_profile(parsed.document(), asg::Profile::Acdc)?;
        assert_eq!(
            value.pointer("/references//owner"),
            Some(&json!(owner)),
            "{source}"
        );
        assert_eq!(
            value.pointer("/references//reftext/0/variant").is_some(),
            label
        );
        assert!(value.pointer(owner).is_some());
        if !label {
            assert!(value.pointer("/references//reftext").is_none());
        }
    }
    Ok(())
}

#[test]
fn dangling_aliases_and_stale_primary_labels_are_rejected() -> Result<(), Error> {
    let parsed = parse("[#target]\n.Titled\nBody.", &Options::default())?;
    for (key, target, title) in [
        ("alias", "missing", Some(text("Titled"))),
        ("target", "target", Some(text("Stale"))),
    ] {
        let mut document = Document::from(parsed.document());
        let mut reference = Reference::new(target, Location::default());
        reference.set_title(title);
        document.set_references([(key, reference)]);
        assert!(
            matches!(asg::document_with_profile((&document).into(), asg::Profile::Acdc),
            Err(asg::Error::Unsupported{path,..}) if path == format!("/references/{key}"))
        );
    }
    let mut document = Document::from(parsed.document());
    let mut reference = Reference::from(
        parsed
            .document()
            .references()
            .get("target")
            .ok_or("target")?,
    );
    reference.set_xreflabel(Some(text("Stale")));
    document.set_references([("target", reference)]);
    assert!(
        matches!(asg::document_with_profile((&document).into(), asg::Profile::Acdc),
        Err(asg::Error::Unsupported{path,..}) if path == "/references/target")
    );
    Ok(())
}

#[test]
fn registration_schema_is_closed_and_validates_label_and_pointer_types() -> Result<(), Error> {
    let schema: Value = serde_json::from_str(include_str!("../../schemas/asg/acdc-v1.json"))?;
    let validator = jsonschema::validator_for(&schema)?;
    let parsed = parse("[#target]\nBody.", &Options::default())?;
    let value = asg::document_with_profile(parsed.document(), asg::Profile::Acdc)?;
    assert!(validator.is_valid(&value));
    for (field, bad) in [
        ("unknown", json!(true)),
        ("owner", json!("relative")),
        ("owner", json!("/invalid~escape")),
        ("target", json!(7)),
        ("reftext", json!("flattened")),
    ] {
        let mut invalid = value.clone();
        invalid
            .pointer_mut("/references/target")
            .and_then(Value::as_object_mut)
            .ok_or("record")?
            .insert(field.into(), bad);
        assert!(!validator.is_valid(&invalid), "{field}");
    }
    Ok(())
}
