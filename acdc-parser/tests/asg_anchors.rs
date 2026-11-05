#![cfg(feature = "asg")]

use acdc_parser::{
    Anchor, Block, CompatibilityMode, ContentAttribute, Document, InlineNode, Location, Options,
    Paragraph, Plain, asg, parse, parse_file,
};
use serde_json::{Value, json};
use std::{fs, path::Path};

type Error = Box<dyn std::error::Error + Send + Sync>;

#[test]
fn retained_declarations_preserve_primary_selection_labels_and_locations() -> Result<(), Error> {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/asg/anchors.adoc");
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
        for (path, expected) in [
            ("/blocks/0/id", json!("para-first")),
            ("/blocks/0/reftext/0/value", json!("Last label")),
            ("/blocks/0/anchors/0/id", json!("para-last")),
            ("/blocks/0/anchors/0/reftext/0/value", json!("Last label")),
            ("/blocks/0/inlines/1/name", json!("anchor")),
            ("/blocks/0/inlines/1/id", json!("inline")),
            ("/blocks/0/inlines/1/reftext/0/variant", json!("strong")),
            ("/blocks/2/id", json!("section-last")),
            ("/blocks/2/anchors/0/id", json!("section-first")),
        ] {
            assert_eq!(value.pointer(path), Some(&expected), "{path}");
        }
        assert!(value.pointer("/blocks/0/id_location").is_some());
        assert!(value.pointer("/blocks/0/anchors/0/location").is_some());
        assert!(!parsed.document().references().contains("para-last"));
        assert!(!parsed.document().references().contains("section-first"));
        assert_eq!(serde_json::to_value(parsed.document())?, before);
        assert!(
            matches!(asg::document(parsed.document()),Err(asg::Error::Unsupported{path,..}) if path=="/blocks/0/metadata/anchors")
        );
    }
    Ok(())
}

#[test]
fn header_primary_uses_the_heading_anchor_selection() -> Result<(), Error> {
    let parsed = parse("[[first]]\n[[last]]\n= Guide", &Options::default())?;
    let value = asg::document_with_profile(parsed.document(), asg::Profile::Acdc)?;
    assert_eq!(value.pointer("/header/id"), Some(&json!("last")));
    assert_eq!(value.pointer("/header/anchors/0/id"), Some(&json!("first")));
    assert!(parsed.document().references().contains("last"));
    assert!(!parsed.document().references().contains("first"));
    Ok(())
}

#[test]
fn prepared_declarations_keep_duplicate_ids_and_empty_labels_without_inventing_spans()
-> Result<(), Error> {
    let mut paragraph = Paragraph::new(Vec::new(), Location::default());
    let mut metadata = paragraph.take_metadata();
    metadata.id = Some(Anchor::new("id", Location::default()));
    metadata.anchors = vec![
        Anchor::new("id", Location::default()).with_xreflabel(Some(ContentAttribute::default())),
    ];
    metadata.reftext = Some(ContentAttribute::new(vec![InlineNode::PlainText(
        Plain::new("Selected", Location::default()),
    )]));
    paragraph.set_metadata(metadata)?;
    let mut document = Document::default();
    document.push_block(Block::Paragraph(paragraph));
    let value = asg::document_with_profile((&document).into(), asg::Profile::Acdc)?;
    assert_eq!(value.pointer("/blocks/0/id"), Some(&json!("id")));
    assert_eq!(
        value.pointer("/blocks/0/anchors"),
        Some(&json!([{"id":"id","reftext":[]}]))
    );
    assert_eq!(
        value.pointer("/blocks/0/reftext/0/value"),
        Some(&json!("Selected"))
    );
    assert!(value.pointer("/blocks/0/id_location").is_none());
    Ok(())
}

#[test]
fn anchor_children_and_bibliography_keep_explicit_boundaries() -> Result<(), Error> {
    let invalid = ContentAttribute::new(vec![InlineNode::CharacterReference(
        acdc_parser::CharacterReference::new(
            acdc_parser::CharacterReferenceKind::Numeric(0),
            Location::default(),
        )
        .ok_or("numeric reference")?,
    )]);
    let anchor = Anchor::new("inline", Location::default()).with_xreflabel(Some(invalid));
    let mut document = Document::default();
    document.push_block(Block::Paragraph(Paragraph::new(
        vec![InlineNode::InlineAnchor(anchor)],
        Location::default(),
    )));
    let result = asg::document_with_profile((&document).into(), asg::Profile::Acdc);
    assert!(
        matches!(&result,Err(asg::Error::Unsupported{path,..}) if path=="/blocks/0/inlines/0/reftext/0"),
        "{result:?}"
    );
    let mut anchor = Anchor::new("entry", Location::default());
    anchor.set_bibliography(None);
    let mut document = Document::default();
    document.push_block(Block::Paragraph(Paragraph::new(
        vec![InlineNode::InlineAnchor(anchor)],
        Location::default(),
    )));
    let result = asg::document_with_profile((&document).into(), asg::Profile::Acdc);
    assert!(
        matches!(
            &result,
            Err(asg::Error::Unsupported {
                construct: Some("anchor"),
                ..
            })
        ),
        "{result:?}"
    );
    Ok(())
}

#[test]
fn unused_primary_anchor_label_does_not_replace_effective_reference_text() -> Result<(), Error> {
    let invalid = ContentAttribute::new(vec![InlineNode::CharacterReference(
        acdc_parser::CharacterReference::new(
            acdc_parser::CharacterReferenceKind::Numeric(0),
            Location::default(),
        )
        .ok_or("numeric reference")?,
    )]);
    let mut paragraph = Paragraph::new(Vec::new(), Location::default());
    let mut metadata = paragraph.take_metadata();
    metadata.id = Some(Anchor::new("id", Location::default()).with_xreflabel(Some(invalid)));
    metadata.reftext = Some(ContentAttribute::new(vec![InlineNode::PlainText(
        Plain::new("Selected", Location::default()),
    )]));
    paragraph.set_metadata(metadata)?;
    let mut document = Document::default();
    document.push_block(Block::Paragraph(paragraph));
    let value = asg::document_with_profile((&document).into(), asg::Profile::Acdc)?;
    assert_eq!(
        value.pointer("/blocks/0/reftext/0/value"),
        Some(&json!("Selected"))
    );
    Ok(())
}

#[test]
fn anchor_schema_closes_declarations_and_inline_owners() -> Result<(), Error> {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/asg/anchors.adoc");
    let parsed = parse_file(&source, &Options::default())?;
    let value = asg::document_with_profile(parsed.document(), asg::Profile::Acdc)?;
    let schema: Value = serde_json::from_str(include_str!("../schemas/asg/acdc-v1.json"))?;
    let validator = jsonschema::validator_for(&schema)?;
    assert!(validator.is_valid(&value));
    for (path, name, invalid) in [
        ("/blocks/0/anchors/0", "unknown", json!(true)),
        ("/blocks/0/anchors/0", "reftext", json!("flattened")),
        ("/blocks/0/inlines/1", "unknown", json!(true)),
        ("/blocks/0/inlines/1", "id", json!(null)),
        ("/blocks/0/inlines/1", "reftext", json!("flattened")),
        (
            "/blocks/0",
            "id_location",
            json!([{"line":0,"col":1},{"line":0,"col":2}]),
        ),
    ] {
        let mut changed = value.clone();
        changed
            .pointer_mut(path)
            .and_then(Value::as_object_mut)
            .ok_or("node")?
            .insert(name.into(), invalid);
        assert!(!validator.is_valid(&changed), "{path}/{name}");
    }
    Ok(())
}
