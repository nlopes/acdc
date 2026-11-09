#![cfg(feature = "asg")]

use acdc_parser::{
    Block, CompatibilityMode, Document, DoubleQuotedSpan, ElementId, FormattingForm, InlineNode,
    Location, Options, Paragraph, Plain, Subscript, Superscript, Title, asg, parse_file,
    parse_inline,
};
use serde_json::{Value, json};
use std::{fs, path::Path};

type Error = Box<dyn std::error::Error + Send + Sync>;

fn semantic_text(value: &Value) -> String {
    match value {
        Value::Array(nodes) => nodes.iter().map(semantic_text).collect(),
        Value::Object(node) => {
            let content = node.get("value").and_then(Value::as_str).map_or_else(
                || node.get("inlines").map(semantic_text).unwrap_or_default(),
                str::to_owned,
            );
            node.get("quotes")
                .and_then(Value::as_array)
                .map_or(content.clone(), |quotes| {
                    format!(
                        "{}{}{}",
                        quotes.first().and_then(Value::as_str).unwrap_or_default(),
                        content,
                        quotes.get(1).and_then(Value::as_str).unwrap_or_default()
                    )
                })
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => String::new(),
    }
}

#[test]
fn resolved_text_and_nested_span_meaning_are_preserved() -> Result<(), Error> {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/asg/typography.adoc");
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
        for (index, expected) in [
            (0, "The formula E=mc2 and water H2O."),
            (1, "“Double nested” and ‘Single nested’."),
            (2, "Olaf’s keyboard."),
            (3, "Protected ^caret^ ~tilde~ \"`quoted`\"."),
        ] {
            assert_eq!(
                semantic_text(
                    value
                        .pointer(&format!("/blocks/{index}/inlines"))
                        .ok_or("inlines")?
                ),
                expected
            );
        }
        for (path, variant) in [
            ("/blocks/0/inlines/1/variant", "superscript"),
            ("/blocks/0/inlines/3/variant", "subscript"),
            ("/blocks/1/inlines/0/variant", "quotation"),
            ("/blocks/1/inlines/0/inlines/1/variant", "strong"),
        ] {
            assert_eq!(value.pointer(path), Some(&json!(variant)));
        }
        assert_eq!(
            value.pointer("/blocks/1/inlines/0/quotes"),
            Some(&json!(["“", "”"]))
        );
        assert_eq!(
            value.pointer("/blocks/1/inlines/2/quotes"),
            Some(&json!(["‘", "’"]))
        );
        assert_eq!(serde_json::to_value(parsed.document())?, before);
    }
    Ok(())
}

#[test]
fn prepared_spans_keep_metadata_empty_content_and_unknown_locations() -> Result<(), Error> {
    let mut quotation =
        DoubleQuotedSpan::new(Vec::new(), FormattingForm::Constrained, Location::default());
    quotation.replace_role(Some("one two"));
    quotation.set_id(Some(ElementId::new("quoted", Location::default())));
    let subscript = Subscript::new(
        vec![InlineNode::PlainText(Plain::new("x", Location::default()))],
        FormattingForm::Unconstrained,
        Location::default(),
    );
    let superscript = Superscript::new(
        vec![InlineNode::SubscriptText(subscript)],
        FormattingForm::Constrained,
        Location::default(),
    );
    let paragraph = Paragraph::new(
        vec![InlineNode::DoubleQuotedSpan(quotation)],
        Location::default(),
    )
    .with_title(Title::new(vec![InlineNode::SuperscriptText(superscript)]));
    let mut document = Document::default();
    document.push_block(Block::Paragraph(paragraph));
    let value = asg::export_document_with_profile((&document).into(), asg::Profile::Acdc)?;
    assert_eq!(
        value.pointer("/blocks/0/inlines/0"),
        Some(&json!({
            "name":"span","type":"inline","variant":"quotation","form":"constrained",
            "quotes":["“","”"],"id":"quoted","metadata":{"roles":["one","two"]},"inlines":[],
        }))
    );
    assert_eq!(
        value.pointer("/blocks/0/title/0/inlines/0/variant"),
        Some(&json!("subscript"))
    );
    assert_eq!(
        value.pointer("/blocks/0/title/0/inlines/0/form"),
        Some(&json!("unconstrained"))
    );
    assert!(value.pointer("/blocks/0/title/0/location").is_none());
    assert!(value.get("references").is_none());
    Ok(())
}

#[test]
fn literal_protection_and_strict_boundaries_stay_explicit() -> Result<(), Error> {
    for source in ["^2^", "~2~", "\"`Double`\"", "'`Single`'"] {
        let parsed = parse_inline(source, &Options::default())?;
        assert!(
            matches!(asg::export_inlines(parsed.content()), Err(asg::Error::Unsupported{path,..}) if path == "/0"),
            "{source}"
        );
        assert!(asg::export_inlines_with_profile(parsed.content(), asg::Profile::Acdc).is_ok());
    }
    let parsed = parse_inline("Olaf`'s", &Options::default())?;
    assert_eq!(
        semantic_text(&asg::export_inlines(parsed.content())?),
        "Olaf’s"
    );
    let parsed = parse_inline("+^caret^ ~tilde~+", &Options::default())?;
    let value = asg::export_inlines_with_profile(parsed.content(), asg::Profile::Acdc)?;
    assert_eq!(semantic_text(&value), "^caret^ ~tilde~");
    assert!(value.pointer("/0/variant").is_none());
    Ok(())
}

#[test]
fn quotation_schema_requires_bounded_pairs_only_on_quotations() -> Result<(), Error> {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/asg/typography.adoc");
    let parsed = parse_file(&source, &Options::default())?;
    let value = asg::export_document_with_profile(parsed.document(), asg::Profile::Acdc)?;
    let schema: Value = serde_json::from_str(include_str!("../../schemas/asg/acdc-v1.json"))?;
    let validator = jsonschema::validator_for(&schema)?;
    assert!(validator.is_valid(&value));
    for (path, field, bad) in [
        ("/blocks/1/inlines/0", "quotes", json!(["\"", "\""])),
        ("/blocks/1/inlines/0", "quotes", json!(["“"])),
        ("/blocks/1/inlines/0", "extra", json!(true)),
        ("/blocks/0/inlines/1", "quotes", json!(["“", "”"])),
    ] {
        let mut invalid = value.clone();
        invalid
            .pointer_mut(path)
            .and_then(Value::as_object_mut)
            .ok_or("span")?
            .insert(field.into(), bad);
        assert!(!validator.is_valid(&invalid), "{path}/{field}");
    }
    let mut invalid = value;
    invalid
        .pointer_mut("/blocks/1/inlines/0")
        .and_then(Value::as_object_mut)
        .ok_or("quotation")?
        .remove("quotes");
    assert!(!validator.is_valid(&invalid));
    Ok(())
}
