#![cfg(feature = "asg")]

use acdc_parser::{
    Admonition, AdmonitionForm, AdmonitionVariant, Block, CompatibilityMode, Document, Location,
    Options, asg, parse, parse_file,
};
use serde_json::{Value, json};
use std::{fs, path::Path};

type Error = Box<dyn std::error::Error + Send + Sync>;

#[test]
fn block_forms_keep_actual_markers_and_native_body_owners() -> Result<(), Error> {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/asg/block_forms.adoc");
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let parsed = parse_file(&source, &options)?;
        let before = serde_json::to_value(parsed.document())?;
        let value = asg::document_with_profile(parsed.document(), asg::Profile::Acdc)?;
        let expected: Value =
            serde_json::from_str(&fs::read_to_string(source.with_extension("json"))?)?;
        assert_eq!(value, expected, "{mode:?}");
        assert_eq!(serde_json::to_value(parsed.document())?, before);
        for (index, name, form) in [
            (0, "admonition", "paragraph"),
            (1, "admonition", "indented"),
            (2, "admonition", "delimited"),
            (3, "admonition", "delimited"),
            (4, "sidebar", "paragraph"),
            (5, "sidebar", "indented"),
            (6, "example", "paragraph"),
            (7, "quote", "paragraph"),
            (8, "abstract", "paragraph"),
            (9, "partintro", "paragraph"),
            (10, "quote", "marked"),
            (11, "quote", "marked"),
        ] {
            assert_eq!(
                value.pointer(&format!("/blocks/{index}/name")),
                Some(&json!(name))
            );
            assert_eq!(
                value.pointer(&format!("/blocks/{index}/form")),
                Some(&json!(form))
            );
            if form != "delimited" {
                assert!(
                    value
                        .pointer(&format!("/blocks/{index}/delimiter"))
                        .is_none()
                );
            }
        }
        for (path, token) in [
            ("/blocks/2/delimiter", "======"),
            ("/blocks/3/delimiter", "--"),
            ("/blocks/10/marker", "\""),
            ("/blocks/11/marker", ">"),
        ] {
            assert_eq!(value.pointer(path), Some(&json!(token)));
        }
        assert!(value.pointer("/blocks/4/blocks").is_none());
        assert_eq!(
            value.pointer("/blocks/4/inlines/0/value"),
            Some(&json!("A styled sidebar."))
        );
        assert_eq!(
            value.pointer("/blocks/12/blocks/1/name"),
            Some(&json!("sidebar"))
        );
    }
    Ok(())
}

#[test]
fn strict_export_accepts_real_admonition_delimiters_only() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        for delimiter in ["====", "=======", "--"] {
            let source = format!("[NOTE]\n{delimiter}\nBody.\n{delimiter}");
            let parsed = parse(&source, &options)?;
            let value = asg::document(parsed.document())?;
            assert_eq!(
                value.pointer("/blocks/0/delimiter"),
                Some(&json!(delimiter))
            );
            assert_eq!(value.pointer("/blocks/0/variant"), Some(&json!("note")));
        }
        for source in [
            "NOTE: Text.",
            "[NOTE]\n    Text.",
            "[sidebar]\nText.",
            "[abstract]\nText.",
        ] {
            let parsed = parse(source, &options)?;
            assert!(
                matches!(asg::document(parsed.document()), Err(asg::Error::Unsupported {path, ..}) if path == "/blocks/0/form")
            );
        }
        for source in ["> Text.", "\"Text.\"\n-- Author"] {
            let parsed = parse(source, &options)?;
            assert!(
                matches!(asg::document(parsed.document()), Err(asg::Error::Unsupported {path, ..}) if path == "/blocks/0/delimiter")
            );
        }
    }
    Ok(())
}

#[test]
fn unknown_forms_and_nested_unsupported_content_remain_explicit_errors() -> Result<(), Error> {
    for form in [
        AdmonitionForm::Unspecified,
        AdmonitionForm::Delimited(""),
        AdmonitionForm::Delimited(">"),
        AdmonitionForm::Delimited("\""),
    ] {
        let owner = Admonition::new(AdmonitionVariant::Tip, Vec::new(), Location::default())
            .with_form(form);
        let mut document = Document::default();
        document.push_block(Block::Admonition(owner));
        for profile in [asg::Profile::Upstream, asg::Profile::Acdc] {
            assert!(
                matches!(asg::document_with_profile((&document).into(), profile), Err(asg::Error::Unsupported {path, construct:Some("admonition"), ..}) if path == "/blocks/0/delimiter")
            );
        }
    }
    for (source, path) in [
        (
            "[TIP]\n====\nicon:heart[]\n====",
            "/blocks/0/blocks/0/inlines/0",
        ),
        ("[sidebar]\nicon:heart[]", "/blocks/0/inlines/0"),
        ("> icon:heart[]", "/blocks/0/blocks/0/inlines/0"),
    ] {
        let parsed = parse(source, &Options::default())?;
        let value = asg::document_with_profile(parsed.document(), asg::Profile::Acdc)?;
        assert_eq!(
            value.pointer(&format!("{path}/target")),
            Some(&json!("heart"))
        );
    }
    Ok(())
}

#[test]
fn block_form_schema_rejects_invented_delimiters_and_wrong_body_shapes() -> Result<(), Error> {
    let schema: Value = serde_json::from_str(include_str!("../schemas/asg/acdc-v1.json"))?;
    let validator = jsonschema::validator_for(&schema)?;
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/asg/block_forms.adoc");
    let parsed = parse_file(&source, &Options::default())?;
    let valid = asg::document_with_profile(parsed.document(), asg::Profile::Acdc)?;
    assert!(validator.is_valid(&valid));
    for (index, field, value) in [
        (0, "delimiter", json!("====")),
        (0, "variant", json!("unknown")),
        (0, "form", json!("unknown")),
        (0, "inlines", json!([])),
        (4, "blocks", json!([])),
        (4, "inlines", json!("flattened")),
        (4, "extra", json!(true)),
        (10, "delimiter", json!("\"")),
        (11, "marker", json!("|")),
        (2, "marker", json!(">")),
        (
            0,
            "blocks",
            json!([{"name":"section","type":"block","level":1,"title":[]}]),
        ),
    ] {
        let mut invalid = valid.clone();
        invalid
            .pointer_mut(&format!("/blocks/{index}"))
            .and_then(Value::as_object_mut)
            .ok_or("block")?
            .insert(field.into(), value);
        assert!(!validator.is_valid(&invalid), "{index}: {field}");
    }
    for (index, field) in [
        (0, "blocks"),
        (4, "inlines"),
        (10, "marker"),
        (2, "delimiter"),
    ] {
        let mut invalid = valid.clone();
        invalid
            .pointer_mut(&format!("/blocks/{index}"))
            .and_then(Value::as_object_mut)
            .ok_or("block")?
            .remove(field);
        assert!(!validator.is_valid(&invalid), "missing {index}: {field}");
    }
    Ok(())
}
