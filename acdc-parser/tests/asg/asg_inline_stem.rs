#![cfg(feature = "asg")]

use acdc_parser::{
    Block, CompatibilityMode, ContentAttribute, DelimitedBlock, DelimitedBlockType, Document,
    InlineMacro, InlineNode, Location, Options, Stem, StemContent, StemNotation, asg, parse,
    parse_file, parse_inline,
};
use serde_json::{Value, json};
use std::{fs, path::Path};

type Error = Box<dyn std::error::Error + Send + Sync>;

#[test]
fn inline_formulas_keep_placement_notation_and_literal_content() -> Result<(), Error> {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/asg/inline_stem.adoc");
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
        for (path, notation, formula) in [
            ("/blocks/0/inlines/1", "latexmath", "{letter}^2"),
            ("/blocks/0/inlines/3", "asciimath", "a < b & c"),
            ("/blocks/2/inlines/0/tooltip/0", "latexmath", "x"),
            ("/blocks/2/inlines/2/alt/0", "asciimath", "x+y"),
            ("/footnotes/0/inlines/1", "latexmath", "x=1"),
            ("/blocks/5/inlines/1", "asciimath", "x+y"),
        ] {
            assert_eq!(
                value.pointer(&format!("{path}/type")),
                Some(&json!("inline")),
                "{path}"
            );
            assert_eq!(
                value.pointer(&format!("{path}/notation")),
                Some(&json!(notation)),
                "{path}"
            );
            assert_eq!(
                value.pointer(&format!("{path}/inlines/0/value")),
                Some(&json!(formula)),
                "{path}"
            );
            assert!(
                value
                    .pointer(&format!("{path}/inlines/0/location"))
                    .is_none()
            );
        }
        assert_eq!(
            value.pointer("/blocks/1/inlines/2/inlines"),
            Some(&json!([]))
        );
        assert_eq!(serde_json::to_value(parsed.document())?, before);
    }
    Ok(())
}

#[test]
fn prepared_empty_and_literal_formulas_share_the_block_content_policy() -> Result<(), Error> {
    for notation in [StemNotation::Asciimath, StemNotation::Latexmath] {
        for formula in ["", "$x$ *bold* {attribute} &amp; <x> α"] {
            let content = ContentAttribute::new(vec![InlineNode::Macro(InlineMacro::Stem(
                Stem::new(formula, notation.clone(), Location::default()),
            ))]);
            let value = asg::export_inlines_with_profile((&content).into(), asg::Profile::Acdc)?;
            let mut document = Document::default();
            document.push_block(Block::DelimitedBlock(DelimitedBlock::new(
                DelimitedBlockType::DelimitedStem(StemContent::new(formula, notation.clone())),
                "++++",
                Location::default(),
            )));
            let block = asg::export_document_with_profile((&document).into(), asg::Profile::Acdc)?;
            assert_eq!(
                value.pointer("/0/inlines"),
                block.pointer("/blocks/0/inlines")
            );
            assert_eq!(
                value.pointer("/0/notation"),
                block.pointer("/blocks/0/notation")
            );
            assert!(value.pointer("/0/location").is_none());
            if !formula.is_empty() {
                assert_eq!(value.pointer("/0/inlines/0/value"), Some(&json!(formula)));
            }
        }
    }
    let parsed = parse_inline("latexmath:[$x$]", &Options::default())?;
    let value = asg::export_inlines_with_profile(parsed.content(), asg::Profile::Acdc)?;
    assert_eq!(value.pointer("/0/inlines/0/value"), Some(&json!("x")));
    Ok(())
}

#[test]
fn nested_formulas_keep_surrounding_formatting_and_strict_rejection() -> Result<(), Error> {
    let parsed = parse_inline("*Strong latexmath:[x]*", &Options::default())?;
    let value = asg::export_inlines_with_profile(parsed.content(), asg::Profile::Acdc)?;
    assert_eq!(value.pointer("/0/variant"), Some(&json!("strong")));
    assert_eq!(value.pointer("/0/inlines/1/name"), Some(&json!("stem")));
    assert!(
        matches!(asg::export_inlines(parsed.content()), Err(asg::Error::Unsupported{path,construct:Some("STEM"),..}) if path == "/0/inlines/1")
    );
    let parsed = parse(
        "[subs=macros]\nIsolated latexmath:[x < y & z].",
        &Options::default(),
    )?;
    let value = asg::export_document_with_profile(parsed.document(), asg::Profile::Acdc)?;
    assert_eq!(
        value.pointer("/blocks/0/inlines/0/name"),
        Some(&json!("raw"))
    );
    assert_eq!(
        value.pointer("/blocks/0/inlines/1/inlines/0/name"),
        Some(&json!("text"))
    );
    assert_eq!(
        value.pointer("/blocks/0/inlines/1/inlines/0/value"),
        Some(&json!("x < y & z"))
    );
    Ok(())
}

#[test]
fn inline_math_schema_rejects_block_fields_and_prose_content() -> Result<(), Error> {
    let parsed = parse("Math stem:[x].", &Options::default())?;
    let value = asg::export_document_with_profile(parsed.document(), asg::Profile::Acdc)?;
    let schema: Value = serde_json::from_str(include_str!("../../schemas/asg/acdc-v1.json"))?;
    let validator = jsonschema::validator_for(&schema)?;
    assert!(validator.is_valid(&value));
    for (field, bad) in [
        ("extra", json!(true)),
        ("notation", json!("unknown")),
        ("form", json!("paragraph")),
        ("delimiter", json!("++++")),
        (
            "inlines",
            json!([{"name":"span","type":"inline","variant":"strong","form":"constrained","inlines":[]}]),
        ),
    ] {
        let mut invalid = value.clone();
        invalid
            .pointer_mut("/blocks/0/inlines/1")
            .and_then(Value::as_object_mut)
            .ok_or("stem")?
            .insert(field.into(), bad);
        assert!(!validator.is_valid(&invalid), "{field}");
    }
    Ok(())
}
