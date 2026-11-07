#![cfg(feature = "asg")]

use acdc_parser::{
    Block, CompatibilityMode, ContentAttribute, Document, InlineImage, InlineMacro, InlineNode,
    LineBreak, Location, Options, Paragraph, Plain, Source, Title, asg, parse, parse_file,
};
use serde_json::{Value, json};
use std::{fs, path::Path};

type Error = Box<dyn std::error::Error + Send + Sync>;

fn count(value: &Value) -> usize {
    match value {
        Value::Object(node) => {
            usize::from(
                node.get("name").and_then(Value::as_str) == Some("break")
                    && node.get("type").and_then(Value::as_str) == Some("inline"),
            ) + node.values().map(count).sum::<usize>()
        }
        Value::Array(nodes) => nodes.iter().map(count).sum(),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => 0,
    }
}

#[test]
fn semantic_breaks_keep_wrapping_literal_text_and_nested_locations() -> Result<(), Error> {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/asg/line_breaks.adoc");
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
        assert_eq!(count(&value), 4);
        assert_eq!(
            value.pointer("/blocks/0/inlines"),
            before.pointer("/blocks/0/inlines")
        );
        assert_eq!(
            value.pointer("/blocks/2/inlines"),
            before.pointer("/blocks/2/inlines")
        );
        assert_eq!(
            value.pointer("/blocks/0/inlines/1/name"),
            Some(&json!("break"))
        );
        assert!(value.pointer("/blocks/0/inlines/1/location").is_some());
        assert_eq!(serde_json::to_value(parsed.document())?, before);
        assert!(
            matches!(asg::document(parsed.document()), Err(asg::Error::Unsupported{path,..}) if path == "/blocks/0/inlines/1")
        );
    }
    Ok(())
}

#[test]
fn prepared_content_attributes_keep_breaks_without_markup_or_fake_locations() -> Result<(), Error> {
    let nodes = vec![
        InlineNode::PlainText(Plain::new("Left", Location::default())),
        InlineNode::LineBreak(LineBreak::new(Location::default())),
        InlineNode::PlainText(Plain::new("Right", Location::default())),
    ];
    let mut image = InlineImage::new(Source::Name("bird.svg"), Location::default());
    image.set_alt(Some(ContentAttribute::new(nodes.clone())));
    image.set_tooltip(Some(ContentAttribute::new(nodes.clone())));
    let paragraph = Paragraph::new(
        vec![InlineNode::Macro(InlineMacro::Image(Box::new(image)))],
        Location::default(),
    )
    .with_title(Title::new(nodes));
    let mut document = Document::default();
    document.push_block(Block::Paragraph(paragraph));
    let value = asg::document_with_profile((&document).into(), asg::Profile::Acdc)?;
    for path in [
        "/blocks/0/title/1",
        "/blocks/0/inlines/0/alt/1",
        "/blocks/0/inlines/0/tooltip/1",
    ] {
        assert_eq!(
            value.pointer(path),
            Some(&json!({"name":"break","type":"inline"}))
        );
    }
    Ok(())
}

#[test]
fn soft_wrapping_and_literal_plus_signs_do_not_become_breaks() -> Result<(), Error> {
    for source in [
        "Soft\nwrap.",
        "----\nLiteral +\nnewline\n----",
        "....\nLiteral +\nnewline\n....",
    ] {
        let parsed = parse(source, &Options::default())?;
        let value = asg::document_with_profile(parsed.document(), asg::Profile::Acdc)?;
        assert_eq!(count(&value), 0, "{source}");
        assert!(
            value
                .pointer("/blocks/0/inlines/0/value")
                .and_then(Value::as_str)
                .is_some_and(|text| text.contains('\n'))
        );
    }
    Ok(())
}

#[test]
fn inline_break_schema_is_closed_and_distinct_from_page_breaks() -> Result<(), Error> {
    let parsed = parse("First +\nsecond", &Options::default())?;
    let value = asg::document_with_profile(parsed.document(), asg::Profile::Acdc)?;
    let schema: Value = serde_json::from_str(include_str!("../schemas/asg/acdc-v1.json"))?;
    let validator = jsonschema::validator_for(&schema)?;
    assert!(validator.is_valid(&value));
    for (field, bad) in [
        ("extra", json!(true)),
        ("value", json!("\n")),
        ("type", json!("block")),
        ("variant", json!("page")),
    ] {
        let mut invalid = value.clone();
        invalid
            .pointer_mut("/blocks/0/inlines/1")
            .and_then(Value::as_object_mut)
            .ok_or("break")?
            .insert(field.into(), bad);
        assert!(!validator.is_valid(&invalid), "{field}");
    }
    Ok(())
}
