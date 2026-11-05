use acdc_parser::{
    AttributeValue, ContentAttribute, ElementId, Image, Link, Location, Source,
    ast::{AttributeValueRef, ContentRef, ImageRef, LinkRef},
};

use super::{link_class_attr, link_control_attributes, link_id_attr, tooltip_attribute};
use crate::image::image_link_control_attributes;

type TestError = Box<dyn std::error::Error>;

#[test]
fn link_attributes_keep_empty_nontext_and_quoted_values_after_owner_edits() -> Result<(), TestError>
{
    for (window, expected) in [
        (AttributeValue::Bool(false), " target=\"'alias'\""),
        (AttributeValue::Bool(true), " target=\"'alias'\""),
        (AttributeValue::None, " target=\"'alias'\""),
        (AttributeValue::from(""), " target=\"'alias'\""),
        (AttributeValue::from("''"), " target=\"''\""),
        (
            AttributeValue::from("\"_blank\""),
            " target=\"&quot;_blank&quot;\"",
        ),
        (
            AttributeValue::from(String::from("'review'")),
            " target=\"'review'\"",
        ),
        (AttributeValue::from(" "), " target=\" \""),
    ] {
        let mut link = Link::new(Source::Name("destination"), Location::default());
        link.set_attribute("window".into(), window)?;
        link.set_attribute("target".into(), "'alias'".into())?;
        assert_eq!(
            link_control_attributes(LinkRef::from(&link).attributes()),
            expected
        );

        let mut metadata = link.take_metadata();
        metadata.id = Some(ElementId::new("'explicit'", Location::default()));
        metadata.roles = vec!["'first", "second'"];
        link.set_metadata(metadata);
        assert_eq!(
            link_id_attr(LinkRef::from(&link).id()),
            " id=\"'explicit'\""
        );
        assert_eq!(
            link_class_attr(LinkRef::from(&link).roles(), true),
            " class=\"bare 'first second'\""
        );
        let mut metadata = link.take_metadata();
        metadata.roles = vec!["duplicate", "duplicate"];
        link.set_metadata(metadata);
        assert_eq!(
            link_class_attr(LinkRef::from(&link).roles(), false),
            " class=\"duplicate duplicate\""
        );
        let mut metadata = link.take_metadata();
        metadata.roles.clear();
        metadata.id = None;
        link.set_metadata(metadata);
        assert_eq!(link_id_attr(LinkRef::from(&link).id()), "");
        assert_eq!(link_class_attr(LinkRef::from(&link).roles(), false), "");
    }
    Ok(())
}

#[test]
fn media_link_controls_keep_their_own_window_and_option_rules()
-> Result<(), Box<dyn std::error::Error>> {
    for (window, expected) in [
        (None, " rel=\"nofollow\""),
        (Some(AttributeValue::Bool(false)), " rel=\"nofollow\""),
        (Some(AttributeValue::None), " rel=\"nofollow\""),
        (Some(AttributeValue::from("")), " rel=\"nofollow\""),
        (
            Some(AttributeValue::from("''")),
            " target=\"''\" rel=\"nofollow noopener\"",
        ),
        (
            Some(AttributeValue::from("'_blank'")),
            " target=\"'_blank'\" rel=\"nofollow noopener\"",
        ),
        (
            Some(AttributeValue::from(String::from("'review'"))),
            " target=\"'review'\" rel=\"nofollow noopener\"",
        ),
    ] {
        let mut image = Image::new(Source::Name("sample"), Location::default());
        let mut metadata = image.take_metadata();
        metadata.attributes.set("target".into(), "alias".into());
        metadata.options = vec!["nofollow", "noopener"];
        image.set_metadata(metadata)?;
        if let Some(window) = window {
            let mut metadata = image.take_metadata();
            metadata.attributes.set("window".into(), window);
            image.set_metadata(metadata)?;
        }
        assert_eq!(
            image_link_control_attributes(
                ImageRef::from(&image).attributes(),
                ImageRef::from(&image).options()
            ),
            expected
        );
        let mut metadata = image.take_metadata();
        metadata.options = vec!["noopener"];
        image.set_metadata(metadata)?;
        if expected == " rel=\"nofollow\"" {
            assert_eq!(
                image_link_control_attributes(
                    ImageRef::from(&image).attributes(),
                    ImageRef::from(&image).options()
                ),
                ""
            );
        }
    }
    Ok(())
}

#[test]
fn tooltip_attributes_keep_absent_and_present_empty_content_distinct() {
    let empty = ContentAttribute::default();
    assert_eq!(tooltip_attribute(None), "");
    assert_eq!(
        tooltip_attribute(Some(ContentRef::from(&empty))),
        " title=\"\""
    );
}

#[test]
fn hyperlink_options_control_rel_without_changing_window_fallbacks() -> Result<(), TestError> {
    for (window, options, expected) in [
        (None, vec!["nofollow"], " rel=\"nofollow\""),
        (None, vec!["noopener"], ""),
        (
            Some("_blank"),
            vec!["nofollow"],
            " target=\"_blank\" rel=\"nofollow noopener\"",
        ),
        (
            Some("reading"),
            vec!["noopener"],
            " target=\"reading\" rel=\"noopener\"",
        ),
        (
            Some("reading"),
            vec!["nofollow"],
            " target=\"reading\" rel=\"nofollow\"",
        ),
        (
            Some("reading"),
            vec!["nofollow", "noopener", "unknown"],
            " target=\"reading\" rel=\"nofollow noopener\"",
        ),
        (Some(""), vec!["nofollow"], " rel=\"nofollow\""),
        (
            Some("review\" & <pane>"),
            vec![],
            " target=\"review&quot; &amp; &lt;pane&gt;\"",
        ),
    ] {
        let mut link = Link::new(Source::Name("destination"), Location::default());
        if let Some(window) = window {
            link.set_attribute("window".into(), window.into())?;
        }
        link.replace_options(options);
        assert_eq!(
            link_control_attributes(LinkRef::from(&link).attributes()),
            expected
        );
    }
    Ok(())
}

#[test]
fn shared_scalar_reads_preserve_decoded_text() -> Result<(), Box<dyn std::error::Error>> {
    for value in [
        "",
        "'",
        "\"",
        "'literal'",
        "\"literal\"",
        "\"α\"tail",
        "é😀 & <pane>",
    ] {
        let mut image = Image::new(Source::Name("sample"), Location::default());
        let mut metadata = image.take_metadata();
        metadata.attributes.set("align".into(), value.into());
        image.set_metadata(metadata)?;
        assert_eq!(
            ImageRef::from(&image)
                .attributes()
                .get("align")
                .and_then(AttributeValueRef::as_str),
            Some(value)
        );
    }
    Ok(())
}
