#![cfg(feature = "asg")]

use acdc_parser::{
    AttributeValue, Block, CompatibilityMode, ContentAttribute, Document, InlineImage, InlineMacro,
    InlineNode, Location, Options, Source, asg,
    ast::{BlockKind, ContentRef, DocumentRef},
    parse,
};
use serde_json::{Value, json};

type Error = Box<dyn std::error::Error + Send + Sync>;

fn export(document: DocumentRef<'_>) -> Result<Value, asg::Error> {
    asg::document_with_profile(document, asg::Profile::Acdc)
}

#[test]
fn inline_image_metadata_and_content_are_preserved() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let parsed = parse(
            "image:photo.png[alt='*Bird*',title='_Tip_',width=120,height=80,link=https://example.org/view,window=_blank,role=one,opts=interactive,id=img,custom=keep]",
            &options,
        )?;
        let before = serde_json::to_value(parsed.document())?;
        let value = export(parsed.document())?;
        for (path, expected) in [
            ("name", json!("image")),
            ("type", json!("inline")),
            ("target", json!("photo.png")),
            ("alt/0/variant", json!("strong")),
            ("tooltip/0/variant", json!("emphasis")),
            ("id", json!("img")),
            ("metadata/roles", json!(["one"])),
            ("metadata/options", json!(["interactive"])),
            ("metadata/attributes/width", json!("120")),
            ("metadata/attributes/height", json!("80")),
            (
                "metadata/attributes/link",
                json!("https://example.org/view"),
            ),
            ("metadata/attributes/custom", json!("keep")),
        ] {
            assert_eq!(
                value.pointer(&format!("/blocks/0/inlines/0/{path}")),
                Some(&expected),
                "{mode:?}: {path}"
            );
        }
        let BlockKind::Paragraph(paragraph) =
            parsed.document().blocks().next().ok_or("paragraph")?.kind()
        else {
            return Err("paragraph".into());
        };
        let acdc_parser::ast::InlineKind::Image(image) =
            paragraph.content().inlines().next().ok_or("image")?.kind()
        else {
            return Err("image".into());
        };
        assert_eq!(
            value.pointer("/blocks/0/inlines/0/alt"),
            Some(&asg::inlines(image.alt().ok_or("alt")?)?)
        );
        assert_eq!(
            value.pointer("/blocks/0/inlines/0/tooltip"),
            Some(&asg::inlines(image.tooltip().ok_or("tooltip")?)?)
        );
        assert!(value.pointer("/blocks/0/inlines/0/location").is_some());
        assert_eq!(serde_json::to_value(parsed.document())?, before);
        assert!(
            matches!(asg::document(parsed.document()),Err(asg::Error::Unsupported {path,construct:Some("inline image"),..}) if path=="/blocks/0/inlines/0")
        );
    }
    Ok(())
}

#[test]
fn image_text_presence_does_not_create_a_filename_fallback() -> Result<(), Error> {
    for (source, alt, tooltip) in [
        ("image:picture-file.png[]", None, None),
        (
            "image:picture-file.png[alt=\"\",title=\"\"]",
            Some(json!([])),
            Some(json!([])),
        ),
    ] {
        let parsed = parse(source, &Options::default())?;
        let value = export(parsed.document())?;
        assert_eq!(value.pointer("/blocks/0/inlines/0/alt"), alt.as_ref());
        assert_eq!(
            value.pointer("/blocks/0/inlines/0/tooltip"),
            tooltip.as_ref()
        );
    }
    Ok(())
}

#[test]
fn block_image_alternative_content_stays_separate_from_its_caption() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let parsed = parse(
            ".Caption\nimage::photo.png[alt='*Bird*',width=120,height=80]",
            &options,
        )?;
        let value = export(parsed.document())?;
        assert_eq!(value.pointer("/blocks/0/form"), Some(&json!("macro")));
        assert_eq!(
            value.pointer("/blocks/0/title/0/value"),
            Some(&json!("Caption"))
        );
        assert_eq!(
            value.pointer("/blocks/0/caption/counter"),
            Some(&json!("figure"))
        );
        assert_eq!(value.pointer("/blocks/0/caption/number"), Some(&json!(1)));
        assert!(value.pointer("/blocks/0/tooltip").is_none());
        let BlockKind::Image(image) = parsed.document().blocks().next().ok_or("image")?.kind()
        else {
            return Err("image".into());
        };
        assert_eq!(
            value.pointer("/blocks/0/alt"),
            Some(&asg::inlines(image.alt().ok_or("alt")?)?)
        );
        assert!(
            matches!(asg::document(parsed.document()),Err(asg::Error::Unsupported {path,..}) if path=="/blocks/0/alt")
        );
    }
    Ok(())
}

#[test]
fn image_schema_rejects_unknown_fields_and_malformed_nested_values() -> Result<(), Error> {
    let schema: Value = serde_json::from_str(include_str!("../schemas/asg/acdc-v1.json"))?;
    let validator = jsonschema::validator_for(&schema)?;
    let valid = json!({"name":"document","type":"block","profile":asg::ACDC_SCHEMA_ID,"blocks":[{"name":"paragraph","type":"block","inlines":[{"name":"image","type":"inline","target":"photo.png","alt":[],"tooltip":[],"metadata":{"attributes":{"width":"120"}}}]}]});
    assert!(validator.is_valid(&valid));
    for image in [
        json!({"name":"image","type":"inline","target":"photo.png","extra":true}),
        json!({"name":"image","type":"inline","target":2}),
        json!({"name":"image","type":"inline","target":"photo.png","alt":"scalar"}),
        json!({"name":"image","type":"inline","target":"photo.png","tooltip":[{"name":"text","type":"string","value":"Tip","extra":true}]}),
        json!({"name":"image","type":"inline","target":"photo.png","metadata":{"width":"120"}}),
        json!({"name":"image","type":"inline","target":"photo.png","metadata":{"attributes":{"width":120}}}),
    ] {
        let mut invalid = valid.clone();
        *invalid
            .pointer_mut("/blocks/0/inlines/0")
            .ok_or("image pointer")? = image;
        assert!(!validator.is_valid(&invalid), "{invalid}");
    }
    for name in ["audio", "video", "toc"] {
        let value = json!({"name":"document","type":"block","profile":asg::ACDC_SCHEMA_ID,"blocks":[{"name":name,"type":"block","form":"macro","target":"media","alt":[]}]});
        assert!(!validator.is_valid(&value));
    }
    Ok(())
}

#[test]
fn standalone_image_export_checks_the_selected_profile() -> Result<(), Error> {
    let parsed = parse("image:photo.png[alt=\"\",title=\"\"]", &Options::default())?;
    let BlockKind::Paragraph(paragraph) =
        parsed.document().blocks().next().ok_or("paragraph")?.kind()
    else {
        return Err("paragraph".into());
    };
    let value = asg::inlines_with_profile(paragraph.content(), asg::Profile::Acdc)?;
    assert_eq!(
        Some(&value),
        export(parsed.document())?.pointer("/blocks/0/inlines")
    );
    assert!(
        matches!(asg::inlines(paragraph.content()),Err(asg::Error::Unsupported {path,..}) if path=="/0")
    );
    let mut image = InlineImage::new(Source::Name("photo.png"), Location::default());
    image.set_attribute("flag".into(), AttributeValue::Bool(true))?;
    image.set_attribute("width".into(), AttributeValue::String("003".into()))?;
    let value = asg::inlines_with_profile(
        ContentRef::from(&ContentAttribute::new(vec![InlineNode::Macro(
            InlineMacro::Image(Box::new(image)),
        )])),
        asg::Profile::Acdc,
    )?;
    assert!(value.pointer("/0/location").is_none());
    assert!(value.pointer("/0/alt").is_none());
    assert!(value.pointer("/0/tooltip").is_none());
    assert_eq!(
        value.pointer("/0/metadata/attributes/width"),
        Some(&json!("003"))
    );
    assert_eq!(
        value.pointer("/0/metadata/attributes/flag"),
        Some(&json!(""))
    );
    Ok(())
}

#[test]
fn block_image_metadata_cannot_shadow_promoted_content() {
    for name in ["alt", "title"] {
        let mut image = acdc_parser::Image::new(Source::Name("photo.png"), Location::default());
        image
            .metadata
            .attributes
            .set(name.into(), AttributeValue::String("shadow".into()));
        let mut document = Document::default();
        document.push_block(Block::Image(image));
        for profile in [asg::Profile::Upstream, asg::Profile::Acdc] {
            assert!(
                matches!(asg::document_with_profile((&document).into(),profile),Err(asg::Error::Unsupported {path,..}) if path==format!("/blocks/0/metadata/attributes/{name}"))
            );
        }
    }
}
