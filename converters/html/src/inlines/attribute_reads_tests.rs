use acdc_parser::{
    AttributeValue, ContentAttribute, ElementId, Image, Link, Location, Source,
    ast::{ContentRef, ImageRef, LinkRef},
};

use super::{link_class_attr, link_id_attr, tooltip_attribute, window_attrs};
use crate::image::image_link_control_attributes;

type TestError = Box<dyn std::error::Error>;

#[test]
fn link_attributes_keep_empty_nontext_and_quoted_values_after_owner_edits() -> Result<(), TestError>
{
    for (window, expected) in [
        (AttributeValue::Bool(false), " target=\"alias\""),
        (AttributeValue::Bool(true), " target=\"alias\""),
        (AttributeValue::None, " target=\"alias\""),
        (AttributeValue::from("''"), " target=\"alias\""),
        (
            AttributeValue::from("\"_blank\""),
            " target=\"_blank\" rel=\"noopener\"",
        ),
        (
            AttributeValue::from(String::from("'review'")),
            " target=\"review\"",
        ),
        (AttributeValue::from(" "), " target=\" \""),
    ] {
        let mut link = Link::new(Source::Name("destination"), Location::default());
        link.set_attribute("window".into(), window)?;
        link.set_attribute("target".into(), "'alias'".into())?;
        assert_eq!(window_attrs(LinkRef::from(&link).attributes()), expected);

        link.metadata.id = Some(ElementId::new("'explicit'", Location::default()));
        link.metadata.roles = vec!["'first", "second'"];
        assert_eq!(link_id_attr(LinkRef::from(&link).id()), " id=\"explicit\"");
        assert_eq!(
            link_class_attr(LinkRef::from(&link).roles(), true),
            " class=\"bare first second\""
        );
        link.metadata.roles = vec!["duplicate", "duplicate"];
        assert_eq!(
            link_class_attr(LinkRef::from(&link).roles(), false),
            " class=\"duplicate duplicate\""
        );
        link.metadata.roles.clear();
        link.metadata.id = None;
        assert_eq!(link_id_attr(LinkRef::from(&link).id()), "");
        assert_eq!(link_class_attr(LinkRef::from(&link).roles(), false), "");
    }
    Ok(())
}

#[test]
fn media_link_controls_keep_their_own_window_and_option_rules() {
    for (window, expected) in [
        (None, " rel=\"nofollow\""),
        (Some(AttributeValue::Bool(false)), " rel=\"nofollow\""),
        (Some(AttributeValue::None), " rel=\"nofollow\""),
        (Some(AttributeValue::from("''")), " rel=\"nofollow\""),
        (
            Some(AttributeValue::from("'_blank'")),
            " target=\"_blank\" rel=\"nofollow noopener\"",
        ),
        (
            Some(AttributeValue::from(String::from("'review'"))),
            " target=\"review\" rel=\"nofollow noopener\"",
        ),
    ] {
        let mut image = Image::new(Source::Name("sample"), Location::default());
        image
            .metadata
            .attributes
            .set("target".into(), "alias".into());
        image.metadata.options = vec!["nofollow", "noopener"];
        if let Some(window) = window {
            image.metadata.attributes.set("window".into(), window);
        }
        assert_eq!(
            image_link_control_attributes(
                ImageRef::from(&image).attributes(),
                ImageRef::from(&image).options()
            ),
            expected
        );
        image.metadata.options = vec!["noopener"];
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
