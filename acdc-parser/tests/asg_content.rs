#![cfg(feature = "asg")]

use acdc_parser::{
    Block, Caption, CaptionKind, ContentAttribute, Document, Location, Options, Paragraph, Plain,
    Title, asg, ast::DocumentRef, parse,
};
use serde_json::{Value, json};

type Error = Box<dyn std::error::Error + Send + Sync>;

fn export(document: DocumentRef<'_>) -> Result<Value, asg::Error> {
    asg::document_with_profile(document, asg::Profile::Acdc)
}

#[test]
fn content_presence_and_caption_variants_do_not_collapse() -> Result<(), Error> {
    let mut document = Document::default();
    let mut paragraph = Paragraph::new(Vec::new(), Location::default());
    paragraph.set_title(Title::new(Vec::new()));
    let mut metadata = paragraph.take_metadata();
    metadata.attribution = Some(ContentAttribute::default());
    metadata.citetitle = Some(ContentAttribute::default());
    for (caption, expected) in [
        (None, None),
        (
            Some(Caption::Unnumbered),
            Some(json!({"kind":"unnumbered"})),
        ),
        (
            Some(Caption::Custom("".into())),
            Some(json!({"kind":"custom","prefix":[]})),
        ),
        (
            Some(Caption::Custom("*literal*".into())),
            Some(
                json!({"kind":"custom","prefix":[{"name":"text","type":"string","value":"*literal*"}]}),
            ),
        ),
        (
            Some(Caption::CustomContent(ContentAttribute::default())),
            Some(json!({"kind":"custom","prefix":[]})),
        ),
        (
            Some(Caption::Numbered {
                kind: CaptionKind::Figure,
                label: "Figure".into(),
                number: None,
            }),
            Some(json!({"kind":"numbered","counter":"figure","label":"Figure"})),
        ),
        (
            Some(Caption::Numbered {
                kind: CaptionKind::Example,
                label: "".into(),
                number: std::num::NonZeroU32::new(3),
            }),
            Some(json!({"kind":"numbered","counter":"example","label":"","number":3})),
        ),
    ] {
        metadata.caption = caption;
        paragraph.set_metadata(metadata.clone())?;
        document.set_blocks(vec![Block::Paragraph(paragraph.clone())]);
        let value = export((&document).into())?;
        assert_eq!(value.pointer("/blocks/0/title"), Some(&json!([])));
        assert_eq!(value.pointer("/blocks/0/attribution"), Some(&json!([])));
        assert_eq!(value.pointer("/blocks/0/citetitle"), Some(&json!([])));
        assert_eq!(value.pointer("/blocks/0/caption"), expected.as_ref());
    }
    Ok(())
}

#[test]
fn programmatic_literal_content_is_not_parsed_again() -> Result<(), Error> {
    let mut paragraph = Paragraph::new(Vec::new(), Location::default());
    let mut metadata = paragraph.take_metadata();
    metadata.attribution = Some(ContentAttribute::new(vec![
        acdc_parser::InlineNode::PlainText(Plain::new("*literal* {name}", Location::default())),
    ]));
    paragraph.set_metadata(metadata)?;
    let mut document = Document::default();
    document.push_block(Block::Paragraph(paragraph));
    let value = export((&document).into())?;
    assert_eq!(
        value.pointer("/blocks/0/attribution/0/value"),
        Some(&json!("*literal* {name}"))
    );
    Ok(())
}

#[test]
fn strict_export_is_unchanged_and_profiles_are_explicit() -> Result<(), Error> {
    let parsed = parse("[reftext='*Label*']\n.Title\nText.", &Options::default())?;
    let strict = asg::document(parsed.document())?;
    assert_eq!(
        strict,
        asg::document_with_profile(parsed.document(), asg::Profile::Upstream)?
    );
    let mut extended = export(parsed.document())?;
    assert_eq!(extended.get("profile"), Some(&json!(asg::ACDC_SCHEMA_ID)));
    extended
        .as_object_mut()
        .ok_or("document object")?
        .remove("profile");
    assert_eq!(extended, strict);
    let parsed = parse(
        "[quote,attribution=\"\"]\n____\nText.\n____",
        &Options::default(),
    )?;
    assert!(
        matches!(asg::document(parsed.document()), Err(asg::Error::Unsupported {path, ..}) if path == "/blocks/0/metadata/attribution")
    );
    assert_eq!(
        export(parsed.document())?.pointer("/blocks/0/attribution"),
        Some(&json!([]))
    );
    Ok(())
}

#[test]
fn extension_schema_rejects_unknown_content_properties() -> Result<(), Error> {
    let base: Value = serde_json::from_str(include_str!("../schemas/asg/schema.json"))?;
    let extension: Value = serde_json::from_str(include_str!("../schemas/asg/acdc-v1.json"))?;
    let validator = jsonschema::validator_for(&extension)?;
    let upstream = jsonschema::validator_for(&base)?;
    let valid = json!({"name":"document","type":"block","profile":asg::ACDC_SCHEMA_ID,"blocks":[{"name":"paragraph","type":"block","attribution":[],"citetitle":[],"caption":{"kind":"custom","prefix":[]},"inlines":[]}]});
    assert!(validator.is_valid(&valid));
    assert!(!upstream.is_valid(&valid));
    for (pointer, value) in [
        ("/profile", json!("urn:acdc:asg:2")),
        ("/blocks/0/attribution", json!("scalar")),
        (
            "/blocks/0/attribution",
            json!([{"name":"text","type":"string","value":"Ada","extra":true}]),
        ),
        (
            "/blocks/0/caption",
            json!({"kind":"custom","prefix":[],"extra":true}),
        ),
        (
            "/blocks/0/caption",
            json!({"kind":"numbered","counter":"example","label":"Example","number":0}),
        ),
        (
            "/blocks/0/caption",
            json!({"kind":"numbered","counter":"example"}),
        ),
        (
            "/blocks/0/caption",
            json!({"kind":"unnumbered","prefix":[]}),
        ),
        (
            "/blocks/0/caption",
            json!({"kind":"custom","prefix":[{"name":"image","type":"inline","target":7}]}),
        ),
    ] {
        let mut invalid = valid.clone();
        *invalid.pointer_mut(pointer).ok_or("test pointer")? = value;
        assert!(!validator.is_valid(&invalid), "{invalid}");
    }
    let mut missing = valid.clone();
    missing.as_object_mut().ok_or("object")?.remove("profile");
    assert!(!validator.is_valid(&missing));
    let mut extra = valid;
    extra
        .as_object_mut()
        .ok_or("object")?
        .insert("unknown".into(), json!(true));
    assert!(!validator.is_valid(&extra));
    Ok(())
}

#[test]
fn extension_schema_only_changes_documented_properties() -> Result<(), Error> {
    let base: Value = serde_json::from_str(include_str!("../schemas/asg/schema.json"))?;
    let mut restored: Value = serde_json::from_str(include_str!("../schemas/asg/acdc-v1.json"))?;
    for name in ["$id", "title", "description", "required"] {
        *restored.get_mut(name).ok_or("schema field")? =
            base.get(name).ok_or("base field")?.clone();
    }
    let additions: &[(&str, &[&str])] = &[
        ("/properties", &["profile"]),
        ("/$defs/listItem/properties", &["checked"]),
        (
            "/$defs/abstractBlock/properties",
            &["attribution", "citetitle", "caption"],
        ),
        ("/$defs/inlineSpan/properties", &["id", "metadata"]),
        (
            "/$defs/inlineRef/properties",
            &["id", "metadata", "tooltip"],
        ),
        (
            "/properties/header/properties",
            &[
                "id",
                "reftext",
                "metadata",
                "attribution",
                "citetitle",
                "caption",
                "subtitle",
            ],
        ),
        (
            "/$defs",
            &[
                "acdcCaption",
                "acdcInlineImage",
                "acdcInlineMetadata",
                "acdcSpanMetadata",
                "acdcAdmonition",
                "acdcParagraphParentBlock",
                "acdcMarkedQuote",
                "acdcAttributeEvent",
                "acdcTable",
                "acdcTableRow",
                "acdcTableCell",
                "acdcTableColumnFormat",
                "acdcTableWidth",
                "acdcTablePresentation",
            ],
        ),
    ];
    for &(path, names) in additions {
        let properties = restored
            .pointer_mut(path)
            .and_then(Value::as_object_mut)
            .ok_or("properties")?;
        for name in names {
            assert!(properties.remove(*name).is_some(), "{path}/{name}");
        }
    }
    for (path, count) in [
        ("/$defs/inline/oneOf", 1),
        ("/$defs/list/allOf", 1),
        ("/$defs/blockMacro/allOf", 1),
        ("/$defs/inlineSpan/properties/variant/enum", 1),
        ("/$defs/block/oneOf", 5),
    ] {
        let variants = restored
            .pointer_mut(path)
            .and_then(Value::as_array_mut)
            .ok_or("variants")?;
        for _ in 0..count {
            assert!(variants.pop().is_some(), "{path}");
        }
    }
    assert_eq!(restored, base);
    Ok(())
}

#[test]
fn captioned_leaf_bodies_and_absent_content_retain_their_meaning() -> Result<(), Error> {
    let parsed = parse(
        "[source,rust]\n----\nlet x = \"*literal*\";\n----",
        &Options::default(),
    )?;
    let value = export(parsed.document())?;
    assert_eq!(
        value.pointer("/blocks/0/caption"),
        Some(&json!({"kind":"unnumbered"}))
    );
    assert_eq!(value.pointer("/blocks/0/name"), Some(&json!("listing")));
    assert_eq!(
        value.pointer("/blocks/0/inlines/0/value"),
        Some(&json!("let x = \"*literal*\";"))
    );
    let parsed = parse("Text.", &Options::default())?;
    let value = export(parsed.document())?;
    for name in ["title", "reftext", "caption", "attribution", "citetitle"] {
        assert!(value.pointer(&format!("/blocks/0/{name}")).is_none());
    }
    Ok(())
}
