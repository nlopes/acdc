#![cfg(feature = "asg")]

use acdc_parser::{
    Block, CompatibilityMode, DelimitedBlock, DelimitedBlockType, Document, Location, Options,
    StemContent, StemNotation, asg, parse, parse_file,
};
use serde_json::{Value, json};
use std::{fs, path::Path};

type Error = Box<dyn std::error::Error + Send + Sync>;

#[test]
fn notation_literal_formula_and_escaping_stay_resolved() -> Result<(), Error> {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/asg/block_stem.adoc");
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
        for (index, text_name) in [(0, "text"), (1, "text"), (2, "raw"), (3, "raw")] {
            let owner = format!("/blocks/{index}");
            assert_eq!(
                value.pointer(&format!("{owner}/inlines/0/name")),
                Some(&json!(text_name))
            );
            assert_eq!(
                value.pointer(&format!("{owner}/inlines/0/value")),
                before.pointer(&format!("{owner}/content"))
            );
            for field in ["notation", "delimiter", "location"] {
                assert_eq!(
                    value.pointer(&format!("{owner}/{field}")),
                    before.pointer(&format!("{owner}/{field}"))
                );
            }
            assert_eq!(
                value.pointer(&format!("{owner}/form")),
                Some(&json!("delimited"))
            );
            assert!(
                value
                    .pointer(&format!("{owner}/inlines/0/location"))
                    .is_none()
            );
        }
        assert_eq!(
            value.pointer("/blocks/0/inlines/0/value"),
            Some(&json!("x^2 + {formula}"))
        );
        assert_eq!(
            value.pointer("/blocks/2/inlines/0/value"),
            Some(&json!("*x*^2"))
        );
        assert_eq!(value.pointer("/blocks/4/inlines"), Some(&json!([])));
        assert_eq!(
            value.pointer("/blocks/6/blocks/0/notation"),
            Some(&json!("asciimath"))
        );
        assert_eq!(serde_json::to_value(parsed.document())?, before);
    }
    Ok(())
}

#[test]
fn prepared_formula_text_and_empty_presence_are_not_reparsed() -> Result<(), Error> {
    for notation in [StemNotation::Latexmath, StemNotation::Asciimath] {
        for formula in ["", "*bold* _italic_ {word} &amp; <x> α"] {
            let mut document = Document::default();
            document.push_block(Block::DelimitedBlock(DelimitedBlock::new(
                DelimitedBlockType::DelimitedStem(StemContent::new(formula, notation.clone())),
                "++++",
                Location::default(),
            )));
            let value = asg::export_document_with_profile((&document).into(), asg::Profile::Acdc)?;
            assert_eq!(
                value.pointer("/blocks/0/notation"),
                Some(&json!(notation.to_string()))
            );
            if formula.is_empty() {
                assert_eq!(value.pointer("/blocks/0/inlines"), Some(&json!([])));
            } else {
                assert_eq!(
                    value.pointer("/blocks/0/inlines/0/value"),
                    Some(&json!(formula))
                );
            }
            assert!(value.pointer("/blocks/0/location").is_none());
            assert!(value.pointer("/blocks/0/inlines/0/location").is_none());
        }
    }
    Ok(())
}

#[test]
fn undelimited_styles_stay_paragraphs_and_strict_notation_rejects() -> Result<(), Error> {
    for style in ["stem", "latexmath", "asciimath"] {
        let parsed = parse(&format!("[{style}]\nx+y"), &Options::default())?;
        let value = asg::export_document_with_profile(parsed.document(), asg::Profile::Acdc)?;
        assert_eq!(value.pointer("/blocks/0/name"), Some(&json!("paragraph")));
        assert_eq!(
            value.pointer("/blocks/0/metadata/attributes/style"),
            Some(&json!(style))
        );
        assert!(value.pointer("/blocks/0/notation").is_none());
        assert!(asg::export_document(parsed.document()).is_ok());
    }
    let parsed = parse("[stem]\n++++\nx+y\n++++", &Options::default())?;
    assert!(
        matches!(asg::export_document(parsed.document()), Err(asg::Error::Unsupported{path,construct:Some("STEM block"),..}) if path == "/blocks/0/notation")
    );
    let mut document = Document::default();
    document.push_block(Block::DelimitedBlock(DelimitedBlock::new(
        DelimitedBlockType::DelimitedStem(StemContent::new("x", StemNotation::Asciimath)),
        "",
        Location::default(),
    )));
    assert!(
        matches!(asg::export_document_with_profile((&document).into(), asg::Profile::Acdc), Err(asg::Error::Unsupported{path,..}) if path == "/blocks/0/delimiter")
    );
    Ok(())
}

#[test]
fn mathematical_schema_is_closed_and_rejects_prose_in_formula_bodies() -> Result<(), Error> {
    let parsed = parse("[stem]\n++++\nx+y\n++++", &Options::default())?;
    let value = asg::export_document_with_profile(parsed.document(), asg::Profile::Acdc)?;
    let schema: Value = serde_json::from_str(include_str!("../../schemas/asg/acdc-v1.json"))?;
    let validator = jsonschema::validator_for(&schema)?;
    assert!(validator.is_valid(&value));
    for (field, bad) in [
        ("extra", json!(true)),
        ("notation", json!("unknown")),
        (
            "inlines",
            json!([{"name":"span","type":"inline","variant":"strong","form":"constrained","inlines":[]}]),
        ),
        (
            "inlines",
            json!([{"name":"text","type":"string","value":"x"},{"name":"text","type":"string","value":"y"}]),
        ),
        ("form", json!("paragraph")),
        ("name", json!("paragraph")),
    ] {
        let mut invalid = value.clone();
        invalid
            .pointer_mut("/blocks/0")
            .and_then(Value::as_object_mut)
            .ok_or("stem")?
            .insert(field.into(), bad);
        assert!(!validator.is_valid(&invalid), "{field}");
    }
    let mut invalid = value;
    invalid
        .pointer_mut("/blocks/0")
        .and_then(Value::as_object_mut)
        .ok_or("stem")?
        .remove("notation");
    assert!(!validator.is_valid(&invalid));
    Ok(())
}
