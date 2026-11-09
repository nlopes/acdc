#![cfg(feature = "asg")]

use acdc_parser::{
    Block, Bold, Button, CharacterReference, CharacterReferenceKind, CompatibilityMode,
    ContentAttribute, Document, FormattingForm, InlineMacro, InlineNode, KeyboardShortcut,
    Location, MenuPath, Options, Paragraph, Plain, asg, parse, parse_file,
};
use serde_json::{Value, json};
use std::{fs, path::Path};

type Error = Box<dyn std::error::Error + Send + Sync>;

fn plain(text: &'static str) -> ContentAttribute<'static> {
    ContentAttribute::new(vec![InlineNode::PlainText(Plain::new(
        text,
        Location::default(),
    ))])
}

fn invalid() -> Result<ContentAttribute<'static>, Error> {
    Ok(ContentAttribute::new(vec![InlineNode::CharacterReference(
        CharacterReference::new(CharacterReferenceKind::Numeric(0), Location::default())
            .ok_or("numeric reference")?,
    )]))
}

#[test]
fn source_interface_labels_keep_component_order_and_nested_content() -> Result<(), Error> {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/asg/ui_macros.adoc");
    let expected: Value =
        serde_json::from_str(&fs::read_to_string(source.with_extension("json"))?)?;
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let parsed = parse_file(
            &source,
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let before = serde_json::to_value(parsed.document())?;
        let value = asg::export_document_with_profile(parsed.document(), asg::Profile::Acdc)?;
        assert_eq!(value, expected, "{mode:?}");
        for (path, expected) in [
            ("/blocks/0/inlines/1/keys/0/0/value", json!("Ctrl")),
            ("/blocks/0/inlines/1/keys/1/0/value", json!("C")),
            ("/blocks/0/inlines/3/keys/1/0/value", json!("+")),
            ("/blocks/1/inlines/3/inlines/0/variant", json!("strong")),
            ("/blocks/2/inlines/1/target/0/value", json!("File")),
            ("/blocks/2/inlines/1/items/0/0/value", json!("New")),
            ("/blocks/2/inlines/1/items/1/0/value", json!("Project")),
            ("/blocks/3/inlines/1/inlines", json!([])),
            ("/blocks/4/inlines/1/keys", json!([])),
            ("/blocks/5/inlines/1/items", json!([])),
        ] {
            assert_eq!(value.pointer(path), Some(&expected), "{path}");
        }
        assert_eq!(serde_json::to_value(parsed.document())?, before);
    }
    Ok(())
}

#[test]
fn prepared_labels_preserve_empty_components_and_inline_structure() -> Result<(), Error> {
    let formatted = ContentAttribute::new(vec![InlineNode::BoldText(Bold::new(
        vec![InlineNode::PlainText(Plain::new(
            "Formatted",
            Location::default(),
        ))],
        FormattingForm::Constrained,
        Location::default(),
    ))]);
    let mut document = Document::default();
    document.push_block(Block::Paragraph(Paragraph::new(
        vec![
            InlineNode::Macro(InlineMacro::Button(Button::new(
                plain("*literal*"),
                Location::default(),
            ))),
            InlineNode::Macro(InlineMacro::KeyboardShortcut(KeyboardShortcut::new(
                vec![
                    ContentAttribute::default(),
                    formatted.clone(),
                    ContentAttribute::default(),
                ],
                Location::default(),
            ))),
            InlineNode::Macro(InlineMacro::MenuPath(MenuPath::new(
                ContentAttribute::default(),
                vec![ContentAttribute::default(), formatted],
                Location::default(),
            ))),
        ],
        Location::default(),
    )));
    let value = asg::export_document_with_profile((&document).into(), asg::Profile::Acdc)?;
    for (path, expected) in [
        ("/blocks/0/inlines/0/inlines/0/value", json!("*literal*")),
        ("/blocks/0/inlines/1/keys/0", json!([])),
        ("/blocks/0/inlines/1/keys/1/0/variant", json!("strong")),
        ("/blocks/0/inlines/1/keys/2", json!([])),
        ("/blocks/0/inlines/2/target", json!([])),
        ("/blocks/0/inlines/2/items/0", json!([])),
        ("/blocks/0/inlines/2/items/1/0/variant", json!("strong")),
    ] {
        assert_eq!(value.pointer(path), Some(&expected), "{path}");
    }
    assert!(value.pointer("/blocks/0/inlines/0/location").is_none());
    Ok(())
}

#[test]
fn nested_label_errors_and_footnotes_keep_their_owners() -> Result<(), Error> {
    for (value, path) in [
        (
            InlineMacro::Button(Button::new(invalid()?, Location::default())),
            "/0/inlines/0",
        ),
        (
            InlineMacro::KeyboardShortcut(KeyboardShortcut::new(
                vec![plain("Ctrl"), invalid()?],
                Location::default(),
            )),
            "/0/keys/1/0",
        ),
        (
            InlineMacro::MenuPath(MenuPath::new(invalid()?, Vec::new(), Location::default())),
            "/0/target/0",
        ),
        (
            InlineMacro::MenuPath(MenuPath::new(
                plain("File"),
                vec![invalid()?],
                Location::default(),
            )),
            "/0/items/0/0",
        ),
    ] {
        let content = ContentAttribute::new(vec![InlineNode::Macro(value)]);
        assert!(
            matches!(asg::export_inlines_with_profile((&content).into(), asg::Profile::Acdc), Err(asg::Error::Unsupported{path:actual,..}) if actual == path)
        );
    }
    let parsed = parse(
        "= Guide\n:experimental:\n\nbtn:[Save footnote:n[Note.]]",
        &Options::default(),
    )?;
    let value = asg::export_document_with_profile(parsed.document(), asg::Profile::Acdc)?;
    assert_eq!(
        value.pointer("/blocks/0/inlines/0/inlines/1/name"),
        Some(&json!("footnote"))
    );
    assert_eq!(
        value.pointer("/footnotes/0/inlines/0/value"),
        Some(&json!("Note."))
    );
    assert_eq!(
        value
            .get("footnotes")
            .and_then(Value::as_array)
            .map(Vec::len),
        Some(1)
    );
    Ok(())
}

#[test]
fn disabled_source_macros_and_strict_export_stay_literal_or_rejected() -> Result<(), Error> {
    let source = "kbd:[Ctrl+C] btn:[Save] menu:File[Save]";
    let parsed = parse(source, &Options::default())?;
    let value = asg::export_document(parsed.document())?;
    assert_eq!(
        value.pointer("/blocks/0/inlines/0/value"),
        Some(&json!(source))
    );
    for (source, owner) in [
        ("kbd:[Ctrl+C]", "keyboard"),
        ("btn:[Save]", "button"),
        ("menu:File[Save]", "menu"),
    ] {
        let parsed = parse(
            &format!("= Guide\n:experimental:\n\n{source}"),
            &Options::default(),
        )?;
        assert!(
            matches!(asg::export_document(parsed.document()), Err(asg::Error::Unsupported{path,construct:Some(actual),..}) if path == "/blocks/0/inlines/0" && actual == owner)
        );
        assert!(asg::export_document_with_profile(parsed.document(), asg::Profile::Acdc).is_ok());
    }
    Ok(())
}

#[test]
fn interface_schema_is_closed_and_rejects_flattened_labels() -> Result<(), Error> {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/asg/ui_macros.adoc");
    let parsed = parse_file(&source, &Options::default())?;
    let value = asg::export_document_with_profile(parsed.document(), asg::Profile::Acdc)?;
    let schema: Value = serde_json::from_str(include_str!("../../schemas/asg/acdc-v1.json"))?;
    let validator = jsonschema::validator_for(&schema)?;
    assert!(validator.is_valid(&value));
    for (path, field, bad) in [
        ("/blocks/0/inlines/1", "extra", json!(true)),
        ("/blocks/0/inlines/1", "keys", json!(["Ctrl", "C"])),
        ("/blocks/1/inlines/1", "inlines", json!("Save")),
        ("/blocks/2/inlines/1", "target", json!("File")),
        ("/blocks/2/inlines/1", "items", json!(["New", "Project"])),
        ("/blocks/2/inlines/1", "metadata", json!({"roles":["menu"]})),
    ] {
        let mut invalid = value.clone();
        invalid
            .pointer_mut(path)
            .and_then(Value::as_object_mut)
            .ok_or("interface")?
            .insert(field.into(), bad);
        assert!(!validator.is_valid(&invalid), "{path}/{field}");
    }
    Ok(())
}
