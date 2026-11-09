#![cfg(feature = "asg")]
use acdc_parser::{
    BlockMetadata, CompatibilityMode, ContentAttribute, Document, Header, InlineNode, Location,
    Options, Plain, Subtitle, Title, asg, ast::BlockKind, parse, parse_file,
};
use serde_json::{Value, json};
use std::{fs, path::Path};
type Error = Box<dyn std::error::Error + Send + Sync>;
#[test]
fn header_fields_remain_separate_and_references_are_reconstructible() -> Result<(), Error> {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/asg/header.adoc");
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let parsed = parse_file(
            &source,
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let before = serde_json::to_value(parsed.document())?;
        let value = asg::export_document_with_profile(parsed.document(), asg::Profile::Acdc)?;
        let expected: Value =
            serde_json::from_str(&fs::read_to_string(source.with_extension("json"))?)?;
        assert_eq!(value, expected, "{mode:?}");
        assert_eq!(serde_json::to_value(parsed.document())?, before);
        for (path, expected) in [
            ("/header/id", json!("guide")),
            ("/header/title/1/variant", json!("strong")),
            ("/header/subtitle/1/variant", json!("emphasis")),
            ("/header/reftext/1/variant", json!("strong")),
            ("/header/metadata/roles", json!(["notice"])),
            ("/header/metadata/options", json!(["compact"])),
            ("/header/metadata/attributes/custom", json!("kept")),
            ("/attributes/audience", json!("readers")),
            ("/blocks/1/assignment/value", json!("authors")),
            ("/header/authors/0/firstname", json!("Ada")),
        ] {
            assert_eq!(value.pointer(path), Some(&expected), "{path}");
        }
    }
    Ok(())
}

#[test]
fn subtitle_presence_and_prepared_content_are_not_combined_or_reparsed() -> Result<(), Error> {
    for subtitle in [
        None,
        Some(Subtitle::new(Vec::new())),
        Some(Subtitle::new(vec![InlineNode::PlainText(Plain::new(
            "*literal subtitle*",
            Location::default(),
        ))])),
    ] {
        let mut header = Header::new(Title::new(Vec::new()), Location::default());
        header.set_subtitle(subtitle.clone());
        let mut document = Document::default();
        document.set_header(Some(header));
        let value = asg::export_document_with_profile((&document).into(), asg::Profile::Acdc)?;
        assert_eq!(value.pointer("/header/title"), Some(&json!([])));
        match subtitle {
            None => assert!(value.pointer("/header/subtitle").is_none()),
            Some(subtitle) if subtitle.is_empty() => {
                assert_eq!(value.pointer("/header/subtitle"), Some(&json!([])));
            }
            Some(_) => assert_eq!(
                value.pointer("/header/subtitle/0/value"),
                Some(&json!("*literal subtitle*"))
            ),
        }
    }
    Ok(())
}

#[test]
fn strict_headers_and_unsupported_nested_content_keep_explicit_errors() -> Result<(), Error> {
    for (source, path) in [
        ("= Main: Subtitle", "/header/subtitle"),
        ("[#guide]\n= Main", "/header/metadata/anchors"),
        ("[.notice]\n= Main", "/header/metadata/roles"),
    ] {
        let parsed = parse(source, &Options::default())?;
        assert!(
            matches!(asg::export_document(parsed.document()), Err(asg::Error::Unsupported {path: actual, ..}) if actual == path)
        );
    }
    let parsed = parse("= Main: icon:heart[]", &Options::default())?;
    let value = asg::export_document_with_profile(parsed.document(), asg::Profile::Acdc)?;
    assert_eq!(
        value.pointer("/header/subtitle/0/target"),
        Some(&json!("heart"))
    );
    let parsed = parse("icon:heart[]", &Options::default())?;
    let BlockKind::Paragraph(paragraph) =
        parsed.document().blocks().next().ok_or("paragraph")?.kind()
    else {
        return Err("paragraph".into());
    };
    let mut metadata = BlockMetadata::new();
    metadata.reftext = Some(ContentAttribute::new(
        paragraph
            .content()
            .inlines()
            .map(InlineNode::from)
            .collect(),
    ));
    let mut document = Document::default();
    document.set_header(Some(
        Header::new(Title::new(Vec::new()), Location::default()).with_metadata(metadata)?,
    ));
    let value = asg::export_document_with_profile((&document).into(), asg::Profile::Acdc)?;
    assert_eq!(
        value.pointer("/header/reftext/0/target"),
        Some(&json!("heart"))
    );
    let mut document = Document::default();
    let metadata = BlockMetadata::new().with_roles(vec!["notice"]);
    document.set_header(Some(
        Header::new(Title::default(), Location::default()).with_metadata(metadata)?,
    ));
    assert!(
        matches!(asg::export_document_with_profile((&document).into(), asg::Profile::Acdc), Err(asg::Error::Unsupported {path, ..}) if path == "/header/title")
    );
    Ok(())
}

#[test]
fn header_extension_is_closed_and_keeps_content_arrays() -> Result<(), Error> {
    let schema: Value = serde_json::from_str(include_str!("../../schemas/asg/acdc-v1.json"))?;
    let validator = jsonschema::validator_for(&schema)?;
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/asg/header.adoc");
    let parsed = parse_file(&source, &Options::default())?;
    let valid = asg::export_document_with_profile(parsed.document(), asg::Profile::Acdc)?;
    assert!(validator.is_valid(&valid));
    for (field, value) in [
        ("subtitle", json!("flattened")),
        ("reftext", json!("flattened")),
        ("unknown", json!(true)),
        ("metadata", json!({"unknown":true})),
        ("id", json!(7)),
    ] {
        let mut invalid = valid.clone();
        invalid
            .get_mut("header")
            .and_then(Value::as_object_mut)
            .ok_or("header")?
            .insert(field.into(), value);
        assert!(!validator.is_valid(&invalid), "{field}");
    }
    Ok(())
}

#[test]
fn header_catalogs_keep_stale_duplicate_and_independent_label_checks() -> Result<(), Error> {
    let parsed = parse(
        "[#guide]\n= Main: Subtitle\n\nSee <<guide>>.",
        &Options::default(),
    )?;
    let mut document = Document::from(parsed.document());
    let mut header = Header::from(parsed.document().header().ok_or("header")?);
    header.set_subtitle(Some(Subtitle::new(Vec::new())));
    document.set_header(Some(header));
    assert!(
        matches!(asg::export_document_with_profile((&document).into(), asg::Profile::Acdc), Err(asg::Error::Unsupported {path, ..}) if path == "/references/guide")
    );
    let parsed = parse("[#guide]\n= Main\n\n[#guide]\nBody.", &Options::default())?;
    assert!(
        matches!(asg::export_document_with_profile(parsed.document(), asg::Profile::Acdc), Err(asg::Error::Unsupported {path, ..}) if path == "/blocks/0/id")
    );
    let parsed = parse(
        "= Main: Subtitle\n:reftext: Other\n\nxref:#[].",
        &Options::default(),
    )?;
    let value = asg::export_document_with_profile(parsed.document(), asg::Profile::Acdc)?;
    assert_eq!(value.pointer("/references//owner"), Some(&json!("/header")));
    assert_eq!(
        value.pointer("/references//reftext/0/value"),
        Some(&json!("Other"))
    );
    Ok(())
}
