#![cfg(feature = "asg")]

use acdc_parser::{
    Block, Caption, CaptionKind, CompatibilityMode, ContentAttribute, Document, Location, Options,
    Paragraph, Plain, Title, asg, parse,
};
use serde_json::{Value, json};

type Error = Box<dyn std::error::Error + Send + Sync>;

fn export(document: &Document<'_>) -> Result<Value, asg::Error> {
    asg::document_with_profile(document, asg::Profile::Acdc)
}

#[test]
fn resolved_content_fields_preserve_their_inline_trees() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        for source in [
            "[quote,attribution='*Ada*',citetitle='_Book_']\n____\nBody.\n____",
            "[verse,attribution=\"*literal*\",citetitle=\"\"]\n____\nBody.\n____",
            ".Title\n[caption='*Custom* ']\n====\nBody.\n====",
        ] {
            let parsed = parse(source, &options)?;
            let before = serde_json::to_value(parsed.document())?;
            let value = export(parsed.document())?;
            let metadata = parsed
                .document()
                .blocks
                .first()
                .ok_or("block")?
                .metadata()
                .ok_or("metadata")?;
            if source.contains("attribution=") {
                assert!(metadata.attribution.is_some());
                assert!(metadata.citetitle.is_some());
            } else {
                assert!(matches!(metadata.caption, Some(Caption::CustomContent(_))));
                assert_eq!(
                    value.pointer("/blocks/0/caption/prefix/0/variant"),
                    Some(&json!("strong"))
                );
            }
            for (name, content) in [
                ("attribution", metadata.attribution.as_ref()),
                ("citetitle", metadata.citetitle.as_ref()),
            ] {
                if let Some(content) = content {
                    assert_eq!(
                        value.pointer(&format!("/blocks/0/{name}")),
                        Some(&asg::inlines(content)?)
                    );
                }
            }
            if let Some(Caption::CustomContent(content)) = &metadata.caption {
                assert_eq!(
                    value.pointer("/blocks/0/caption/prefix"),
                    Some(&asg::inlines(content)?)
                );
            }
            assert_eq!(serde_json::to_value(parsed.document())?, before);
        }
    }
    Ok(())
}

#[test]
fn content_presence_and_caption_variants_do_not_collapse() -> Result<(), Error> {
    let mut document = Document::default();
    let mut paragraph = Paragraph::new(Vec::new(), Location::default());
    paragraph.title = Title::new(Vec::new());
    paragraph.metadata.attribution = Some(ContentAttribute::default());
    paragraph.metadata.citetitle = Some(ContentAttribute::default());
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
        paragraph.metadata.caption = caption;
        document.blocks = vec![Block::Paragraph(paragraph.clone())];
        let value = export(&document)?;
        assert_eq!(value.pointer("/blocks/0/title"), Some(&json!([])));
        assert_eq!(value.pointer("/blocks/0/attribution"), Some(&json!([])));
        assert_eq!(value.pointer("/blocks/0/citetitle"), Some(&json!([])));
        assert_eq!(value.pointer("/blocks/0/caption"), expected.as_ref());
    }
    Ok(())
}

#[test]
fn captioned_targets_are_reconstructible_but_stale_catalogs_fail() -> Result<(), Error> {
    let parsed = parse("[#sample]\n.Title\n====\nBody.\n====", &Options::default())?;
    let value = export(parsed.document())?;
    assert_eq!(value.pointer("/blocks/0/caption/number"), Some(&json!(1)));
    let mut document = Document::default();
    document.blocks.clone_from(&parsed.document().blocks);
    document
        .references
        .clone_from(&parsed.document().references);
    document.attributes = parsed.document().attributes.clone();
    document
        .references
        .get_mut("sample")
        .ok_or("reference")?
        .caption = Some(Caption::Unnumbered);
    assert!(
        matches!(export(&document),Err(asg::Error::Unsupported {path,..}) if path=="/references/sample")
    );
    Ok(())
}

#[test]
fn nested_unsupported_content_keeps_its_property_path() -> Result<(), Error> {
    let parsed = parse("image:photo.png[Alt]", &Options::default())?;
    let Block::Paragraph(source) = parsed.document().blocks.first().ok_or("block")? else {
        return Err("paragraph".into());
    };
    for property in ["attribution", "citetitle", "caption"] {
        let mut paragraph = Paragraph::new(Vec::new(), Location::default());
        let content = ContentAttribute::new(source.content.clone());
        match property {
            "attribution" => paragraph.metadata.attribution = Some(content),
            "citetitle" => paragraph.metadata.citetitle = Some(content),
            _ => paragraph.metadata.caption = Some(Caption::CustomContent(content)),
        }
        let mut document = Document::default();
        document.blocks.push(Block::Paragraph(paragraph));
        let field = if property == "caption" {
            "caption/prefix"
        } else {
            property
        };
        assert!(
            matches!(export(&document),Err(asg::Error::Unsupported {path,construct:Some("inline image"),..}) if path==format!("/blocks/0/{field}/0"))
        );
    }
    Ok(())
}

#[test]
fn programmatic_literal_content_is_not_parsed_again() -> Result<(), Error> {
    let mut paragraph = Paragraph::new(Vec::new(), Location::default());
    paragraph.metadata.attribution = Some(ContentAttribute::new(vec![
        acdc_parser::InlineNode::PlainText(Plain {
            content: "*literal* {name}",
            escaped: false,
            location: Location::default(),
        }),
    ]));
    let mut document = Document::default();
    document.blocks.push(Block::Paragraph(paragraph));
    let value = export(&document)?;
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
fn extension_schema_is_closed_and_keeps_the_pinned_structure() -> Result<(), Error> {
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
            json!({"kind":"custom","prefix":[{"name":"image","type":"inline","target":"photo.png"}]}),
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
    let mut restored = extension;
    for name in ["$id", "title", "description", "required"] {
        *restored.get_mut(name).ok_or("schema field")? =
            base.get(name).ok_or("base field")?.clone();
    }
    restored
        .get_mut("properties")
        .and_then(Value::as_object_mut)
        .ok_or("properties")?
        .remove("profile");
    let properties = restored
        .pointer_mut("/$defs/abstractBlock/properties")
        .and_then(Value::as_object_mut)
        .ok_or("block properties")?;
    for name in ["attribution", "citetitle", "caption"] {
        properties.remove(name);
    }
    restored
        .get_mut("$defs")
        .and_then(Value::as_object_mut)
        .ok_or("defs")?
        .remove("acdcCaption");
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
