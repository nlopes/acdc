#![cfg(feature = "asg")]

use acdc_parser::{
    CompatibilityMode, ContentAttribute, InlineNode, Location, Options, Plain, asg,
    ast::{BlockKind, ContentRef, InlineKind},
    parse, parse_inline,
};
use serde_json::json;

type Error = Box<dyn std::error::Error + Send + Sync>;

#[test]
fn standalone_typed_content_preserves_formatting_and_unicode_locations() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let parsed = parse_inline("*α*", &options)?;
        let before = serde_json::to_value(parsed.content())?;
        for profile in [asg::Profile::Upstream, asg::Profile::Acdc] {
            let value = asg::export_inlines_with_profile(parsed.content(), profile)?;
            assert_eq!(value.pointer("/0/variant"), Some(&json!("strong")));
            assert_eq!(value.pointer("/0/inlines/0/value"), Some(&json!("α")));
            assert_eq!(
                value.pointer("/0/location"),
                Some(&json!([
                    {"line":1,"col":1}, {"line":1,"col":3}
                ]))
            );
            assert_eq!(serde_json::to_value(parsed.content())?, before);
        }
    }
    Ok(())
}

#[test]
fn typed_titles_and_image_content_export_without_owner_access() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        let parsed = parse(
            ".*Caption*\nimage:photo.png[alt='*Bird*',title='_Tip_']",
            &options,
        )?;
        let before = serde_json::to_value(parsed.document())?;
        let BlockKind::Paragraph(paragraph) =
            parsed.document().blocks().next().ok_or("paragraph")?.kind()
        else {
            return Err("paragraph".into());
        };
        let title = asg::export_inlines(paragraph.title().ok_or("title")?)?;
        assert_eq!(title.pointer("/0/variant"), Some(&json!("strong")));
        assert_eq!(title.pointer("/0/inlines/0/value"), Some(&json!("Caption")));
        let InlineKind::Image(image) = paragraph.content().inlines().next().ok_or("image")?.kind()
        else {
            return Err("image".into());
        };
        for (content, variant, text) in [
            (image.alt().ok_or("alt")?, "strong", "Bird"),
            (image.tooltip().ok_or("tooltip")?, "emphasis", "Tip"),
        ] {
            let value = asg::export_inlines(content)?;
            assert_eq!(value.pointer("/0/variant"), Some(&json!(variant)));
            assert_eq!(value.pointer("/0/inlines/0/value"), Some(&json!(text)));
        }
        let error = asg::export_inlines(paragraph.content())
            .err()
            .ok_or("strict rejection")?;
        let asg::Error::Unsupported {
            path,
            construct,
            location,
            ..
        } = error
        else {
            return Err("unsupported image".into());
        };
        assert_eq!(path, "/0");
        assert_eq!(construct, Some("inline image"));
        let location = location.ok_or("image location")?;
        assert_eq!(location.start.line, 2);
        assert_eq!(location.start.column, 1);
        let value = asg::export_inlines_with_profile(paragraph.content(), asg::Profile::Acdc)?;
        assert_eq!(value.pointer("/0/target"), Some(&json!("photo.png")));
        assert_eq!(
            value.pointer("/0/alt/0/inlines/0/value"),
            Some(&json!("Bird"))
        );
        assert_eq!(
            value.pointer("/0/tooltip/0/inlines/0/value"),
            Some(&json!("Tip"))
        );
        assert_eq!(serde_json::to_value(parsed.document())?, before);
    }
    Ok(())
}

#[test]
fn absent_and_present_empty_image_content_remain_distinct() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let options = Options::builder().with_compatibility_mode(mode).build()?;
        for (source, present) in [
            ("image:photo.png[]", false),
            ("image:photo.png[alt=\"\",title=\"\"]", true),
        ] {
            let parsed = parse(source, &options)?;
            let BlockKind::Paragraph(paragraph) =
                parsed.document().blocks().next().ok_or("paragraph")?.kind()
            else {
                return Err("paragraph".into());
            };
            let InlineKind::Image(image) =
                paragraph.content().inlines().next().ok_or("image")?.kind()
            else {
                return Err("image".into());
            };
            assert_eq!(image.alt().is_some(), present);
            assert_eq!(image.tooltip().is_some(), present);
            for content in [image.alt(), image.tooltip()].into_iter().flatten() {
                assert!(content.is_empty());
                assert_eq!(asg::export_inlines(content)?, json!([]));
            }
            let value = asg::export_inlines_with_profile(paragraph.content(), asg::Profile::Acdc)?;
            for field in ["alt", "tooltip"] {
                assert_eq!(
                    value.pointer(&format!("/0/{field}")),
                    present.then_some(&json!([]))
                );
            }
        }
    }
    Ok(())
}

#[test]
fn caller_built_content_exports_literal_text_without_reparsing() -> Result<(), Error> {
    for text in [None, Some("*literal* {name} α")] {
        let content = ContentAttribute::new(
            text.into_iter()
                .map(|content| InlineNode::PlainText(Plain::new(content, Location::default())))
                .collect(),
        );
        let before = serde_json::to_value(&content)?;
        for profile in [asg::Profile::Upstream, asg::Profile::Acdc] {
            let value = asg::export_inlines_with_profile(ContentRef::from(&content), profile)?;
            if let Some(text) = text {
                assert_eq!(value.pointer("/0/name"), Some(&json!("text")));
                assert_eq!(value.pointer("/0/value"), Some(&json!(text)));
                assert!(value.pointer("/0/inlines").is_none());
            } else {
                assert_eq!(value, json!([]));
            }
            assert_eq!(serde_json::to_value(&content)?, before);
        }
    }
    Ok(())
}
