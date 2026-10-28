use acdc_parser::{
    Block, ContentAttribute, ElementId, InlineImage, InlineMacro, InlineMetadata, InlineNode,
    Options, parse,
};

type Error = Box<dyn std::error::Error>;

fn first_image<'a, 'b>(
    document: &'b acdc_parser::Document<'a>,
) -> Result<&'b InlineImage<'a>, Error> {
    let Some(Block::Paragraph(paragraph)) = document.blocks.first() else {
        return Err("expected a paragraph".into());
    };
    let Some(InlineNode::Macro(InlineMacro::Image(image))) = paragraph.content.first() else {
        return Err("expected an inline image".into());
    };
    Ok(image)
}

fn json(image: InlineImage<'_>) -> Result<serde_json::Value, Error> {
    Ok(serde_json::to_value(InlineNode::Macro(
        InlineMacro::Image(Box::new(image)),
    ))?)
}

#[rstest::rstest]
fn reserved_writes_and_removals_leave_the_image_unchanged(
    #[values("alt", "title", "id", "role", "roles", "options", "opts")] name: &str,
) -> Result<(), Error> {
    let parsed = parse(
        "image:photo.png[Alt,title=Tooltip,id=picture,role=wide,opts=inline]",
        &Options::default(),
    )?;
    let mut image = first_image(parsed.document())?.clone();
    let before = json(image.clone())?;
    let error = image
        .set_attribute(name.to_owned().into(), "Shadow".into())
        .err()
        .ok_or("reserved write must fail")?;
    assert_eq!(error.name, name);
    assert_eq!(
        image
            .remove_attribute(name)
            .err()
            .ok_or("reserved removal must fail")?
            .name,
        name
    );
    assert_eq!(json(image)?, before);
    Ok(())
}

#[test]
fn replacing_or_removing_content_keeps_one_serialized_owner() -> Result<(), Error> {
    let parsed = parse(
        "image:photo.png[Old alt,title=Old tooltip]",
        &Options::default(),
    )?;
    let mut image = first_image(parsed.document())?.clone();
    image.alt = Some(ContentAttribute::default());
    image.tooltip = None;
    let value = json(image)?;
    assert_eq!(value.get("alt"), Some(&serde_json::json!([])));
    assert!(value.get("tooltip").is_none());
    assert!(value.get("metadata").is_none());
    Ok(())
}

#[test]
fn unpromoted_attributes_remain_mutable_without_changing_content() -> Result<(), Error> {
    let parsed = parse(
        "image:photo.png[Alt,title=Tooltip,custom=original]",
        &Options::default(),
    )?;
    let mut image = first_image(parsed.document())?.clone();
    let alt = image.alt.clone();
    let tooltip = image.tooltip.clone();
    image.set_attribute("width".into(), "120".into())?;
    image.set_attribute("width".into(), "240".into())?;
    assert_eq!(
        image.attributes().get_string("width").as_deref(),
        Some("240")
    );
    assert_eq!(image.remove_attribute("custom")?, Some("original".into()));
    assert!(!image.attributes().contains_key("custom"));
    assert_eq!(image.alt, alt);
    assert_eq!(image.tooltip, tooltip);
    Ok(())
}

#[test]
fn replacing_metadata_preserves_content_and_macro_attributes() -> Result<(), Error> {
    let parsed = parse(
        "image:photo.png[Alt,title=Tooltip,id=old,role=old,opts=inline,custom=kept]",
        &Options::default(),
    )?;
    let mut image = first_image(parsed.document())?.clone();
    let alt = image.alt.clone();
    let tooltip = image.tooltip.clone();
    let mut metadata = InlineMetadata::default();
    metadata.roles.push("new");
    metadata.id = Some(ElementId::new("new-id", image.location.clone()));
    image.metadata = metadata;
    image.options_mut().push("interactive");

    assert_eq!(image.alt, alt);
    assert_eq!(image.tooltip, tooltip);
    assert_eq!(image.options(), ["inline", "interactive"]);
    assert_eq!(
        image.attributes().get_string("custom").as_deref(),
        Some("kept")
    );
    let value = json(image)?;
    assert_eq!(
        value.pointer("/metadata/id/id"),
        Some(&serde_json::json!("new-id"))
    );
    assert_eq!(
        value.pointer("/metadata/roles"),
        Some(&serde_json::json!(["new"]))
    );
    for name in ["alt", "title", "id", "role", "roles", "opts", "options"] {
        assert!(
            value
                .pointer(&format!("/metadata/attributes/{name}"))
                .is_none()
        );
    }
    Ok(())
}
