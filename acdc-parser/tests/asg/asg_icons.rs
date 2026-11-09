#![cfg(feature = "asg")]

use acdc_parser::{
    AttributeValue, Block, CharacterReference, CharacterReferenceKind, CompatibilityMode,
    ContentAttribute, Document, Footnote, FootnoteDefinition, Icon, InlineMacro, InlineNode,
    Location, Options, Paragraph, Plain, Source, asg, parse, parse_file,
};
use serde_json::{Value, json};
use std::{fs, path::Path};

type Error = Box<dyn std::error::Error + Send + Sync>;

fn document(icon: Icon<'static>) -> Document<'static> {
    let mut document = Document::default();
    document.push_block(Block::Paragraph(Paragraph::new(
        vec![InlineNode::Macro(InlineMacro::Icon(Box::new(icon)))],
        Location::default(),
    )));
    document
}

fn export(document: &Document<'_>) -> Result<Value, asg::Error> {
    asg::document_with_profile(document.into(), asg::Profile::Acdc)
}

#[test]
fn source_icon_modes_controls_and_content_are_preserved() -> Result<(), Error> {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/asg/icons.adoc");
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
            ("/attributes/icons", json!("font")),
            ("/blocks/0/inlines/0/target", json!("star")),
            ("/blocks/0/inlines/0/id", json!("first")),
            ("/blocks/0/inlines/0/alt/0/variant", json!("strong")),
            ("/blocks/0/inlines/0/tooltip/0/variant", json!("emphasis")),
            ("/blocks/0/inlines/0/metadata/roles", json!(["red", "blue"])),
            (
                "/blocks/0/inlines/0/metadata/options",
                json!(["nofollow", "custom"]),
            ),
            ("/blocks/1/inlines/2/alt", json!([])),
            ("/blocks/1/inlines/2/tooltip", json!([])),
        ] {
            assert_eq!(value.pointer(path), Some(&expected), "{path}");
        }
        let controls = value
            .pointer("/blocks/0/inlines/0/metadata/attributes")
            .ok_or("controls")?;
        for (name, expected) in [
            ("size", "2x"),
            ("rotate", "90"),
            ("flip", "horizontal"),
            ("link", "https://example.org"),
            ("window", "preview"),
            ("custom", "kept"),
        ] {
            assert_eq!(controls.get(name), Some(&json!(expected)), "{name}");
        }
        assert!(value.pointer("/blocks/1/inlines/0/alt").is_none());
        assert!(value.pointer("/blocks/0/inlines/0/location").is_some());
        for (path, expected) in [
            ("/blocks/3/assignment/value", json!("image")),
            ("/blocks/4/attribute", json!("iconsdir")),
            ("/blocks/4/assignment/value", json!("assets/icons")),
            ("/blocks/5/attribute", json!("icontype")),
            ("/blocks/5/assignment/value", json!("svg")),
            (
                "/blocks/7/rows/0/cells/0/blocks/0/inlines/1/target",
                json!("star"),
            ),
            ("/blocks/8/assignment/kind", json!("unset")),
            ("/blocks/9/inlines/0/alt/0/value", json!("Information")),
        ] {
            assert_eq!(value.pointer(path), Some(&expected), "{path}");
        }
        assert_eq!(serde_json::to_value(parsed.document())?, before);
    }
    Ok(())
}

#[test]
fn prepared_icons_keep_absent_empty_and_literal_values() -> Result<(), Error> {
    let mut icon = Icon::new(Source::Name(""), Location::default());
    icon.set_tooltip(Some(ContentAttribute::default()));
    icon.set_attribute("size".into(), AttributeValue::String("".into()))?;
    let value = export(&document(icon))?;
    assert_eq!(
        value.pointer("/blocks/0/inlines/0/target"),
        Some(&json!(""))
    );
    assert!(value.pointer("/blocks/0/inlines/0/alt").is_none());
    assert_eq!(
        value.pointer("/blocks/0/inlines/0/tooltip"),
        Some(&json!([]))
    );
    assert_eq!(
        value.pointer("/blocks/0/inlines/0/metadata/attributes/size"),
        Some(&json!(""))
    );
    assert!(value.pointer("/blocks/0/inlines/0/location").is_none());
    let mut icon = Icon::new(Source::Name("étoile"), Location::default());
    icon.set_alt(Some(ContentAttribute::new(vec![InlineNode::PlainText(
        Plain::new("*literal*", Location::default()),
    )])));
    let value = export(&document(icon))?;
    assert_eq!(
        value.pointer("/blocks/0/inlines/0/alt/0/value"),
        Some(&json!("*literal*"))
    );
    assert!(value.pointer("/blocks/0/inlines/0/tooltip").is_none());
    Ok(())
}

#[test]
fn invalid_icon_children_and_controls_keep_exact_paths() -> Result<(), Error> {
    for property in ["alt", "tooltip"] {
        let mut icon = Icon::new(Source::Name("star"), Location::default());
        let content = Some(ContentAttribute::new(vec![InlineNode::CharacterReference(
            CharacterReference::new(CharacterReferenceKind::Numeric(0), Location::default())
                .ok_or("numeric reference")?,
        )]));
        if property == "alt" {
            icon.set_alt(content);
        } else {
            icon.set_tooltip(content);
        }
        assert!(
            matches!(export(&document(icon)), Err(asg::Error::Unsupported{path,..}) if path == format!("/blocks/0/inlines/0/{property}/0"))
        );
    }
    for value in [AttributeValue::None, AttributeValue::Bool(false)] {
        let mut icon = Icon::new(Source::Name("star"), Location::default());
        icon.set_attribute("custom".into(), value)?;
        assert!(
            matches!(export(&document(icon)), Err(asg::Error::Unsupported{path,..}) if path == "/blocks/0/inlines/0/metadata/attributes/custom")
        );
    }
    Ok(())
}

#[test]
fn canonical_note_icon_errors_and_strict_rejection_remain_explicit() -> Result<(), Error> {
    let mut icon = Icon::new(Source::Name("star"), Location::default());
    icon.set_attribute("custom".into(), AttributeValue::Bool(false))?;
    let mut document = Document::default();
    document.set_footnotes([FootnoteDefinition::new(
        vec![InlineNode::Macro(InlineMacro::Icon(Box::new(icon)))],
        1,
        Location::default(),
    )]);
    document.push_block(Block::Paragraph(Paragraph::new(
        vec![InlineNode::Macro(InlineMacro::Footnote(
            Footnote::reference(None, 1, Location::default()),
        ))],
        Location::default(),
    )));
    assert!(
        matches!(export(&document), Err(asg::Error::Unsupported{path,..}) if path == "/footnotes/0/inlines/0/metadata/attributes/custom")
    );
    let parsed = parse("icon:star[]", &Options::default())?;
    assert!(
        matches!(asg::document(parsed.document()), Err(asg::Error::Unsupported{path,construct:Some("icon"),..}) if path == "/blocks/0/inlines/0")
    );
    Ok(())
}

#[test]
fn icon_schema_is_closed_and_uses_content_and_metadata_contracts() -> Result<(), Error> {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/asg/icons.adoc");
    let parsed = parse_file(&source, &Options::default())?;
    let value = asg::document_with_profile(parsed.document(), asg::Profile::Acdc)?;
    let schema: Value = serde_json::from_str(include_str!("../../schemas/asg/acdc-v1.json"))?;
    let validator = jsonschema::validator_for(&schema)?;
    assert!(validator.is_valid(&value));
    for (field, bad) in [
        ("unknown", json!(true)),
        ("alt", json!("Star")),
        ("tooltip", json!("Tip")),
        ("target", json!({"name":"star"})),
        ("metadata", json!({"attributes":{"custom":false}})),
        ("mode", json!("font")),
    ] {
        let mut invalid = value.clone();
        invalid
            .pointer_mut("/blocks/0/inlines/0")
            .and_then(Value::as_object_mut)
            .ok_or("icon")?
            .insert(field.into(), bad);
        assert!(!validator.is_valid(&invalid), "{field}");
    }
    Ok(())
}
