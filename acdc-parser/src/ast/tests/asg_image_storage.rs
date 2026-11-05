use crate::{
    AttributeValue, Block, Caption, CharacterReference, CharacterReferenceKind, ContentAttribute,
    Document, Header, InlineImage, InlineMacro, InlineNode, Location, Options, Paragraph, Source,
    Title, asg, parse,
};
use serde_json::{Value, json};

type Error = Box<dyn std::error::Error + Send + Sync>;

fn export(document: &Document<'_>) -> Result<Value, asg::Error> {
    asg::document_with_profile(document.into(), asg::Profile::Acdc)
}

#[test]
fn image_support_composes_through_every_supported_content_owner() -> Result<(), Error> {
    let parsed = parse("*image:photo.png[Alt]*", &Options::default())?;
    let Block::Paragraph(source) = parsed
        .document_storage()
        .blocks
        .first()
        .ok_or("paragraph")?
    else {
        return Err("paragraph".into());
    };
    assert!(matches!(
        source.content.first(),
        Some(InlineNode::BoldText(_))
    ));
    let mut paragraph = Paragraph::new(source.content.clone(), Location::default());
    paragraph.title = Title::new(source.content.clone());
    paragraph.metadata.reftext = Some(ContentAttribute::new(source.content.clone()));
    paragraph.metadata.attribution = Some(ContentAttribute::new(source.content.clone()));
    paragraph.metadata.citetitle = Some(ContentAttribute::new(source.content.clone()));
    paragraph.metadata.caption = Some(Caption::CustomContent(ContentAttribute::new(
        source.content.clone(),
    )));
    let mut document = Document::default();
    document.header = Some(Header::new(
        Title::new(source.content.clone()),
        Location::default(),
    ));
    document.blocks.push(Block::Paragraph(paragraph));
    let value = export(&document)?;
    for path in [
        "/header/title",
        "/blocks/0/title",
        "/blocks/0/reftext",
        "/blocks/0/attribution",
        "/blocks/0/citetitle",
        "/blocks/0/caption/prefix",
        "/blocks/0/inlines",
    ] {
        assert_eq!(
            value.pointer(&format!("{path}/0/inlines/0/name")),
            Some(&json!("image")),
            "{path}"
        );
    }
    Ok(())
}

#[test]
fn unsupported_image_content_and_scalar_states_fail_by_property() -> Result<(), Error> {
    let invalid = InlineNode::CharacterReference(
        CharacterReference::new(CharacterReferenceKind::Numeric(0), Location::default())
            .ok_or("numeric reference")?,
    );
    for property in ["alt", "tooltip"] {
        let mut image = InlineImage::new(Source::Name("photo.png"), Location::default());
        if property == "alt" {
            image.alt = Some(ContentAttribute::new(vec![invalid.clone()]));
        } else {
            image.tooltip = Some(ContentAttribute::new(vec![invalid.clone()]));
        }
        let mut document = Document::default();
        document.blocks.push(Block::Paragraph(Paragraph::new(
            vec![InlineNode::Macro(InlineMacro::Image(Box::new(image)))],
            Location::default(),
        )));
        assert!(
            matches!(export(&document),Err(asg::Error::Unsupported {path,..}) if path==format!("/blocks/0/inlines/0/{property}/0"))
        );
    }
    for attribute in [AttributeValue::None, AttributeValue::Bool(false)] {
        let mut image = InlineImage::new(Source::Name("photo.png"), Location::default());
        image.set_attribute("custom".into(), attribute)?;
        let mut document = Document::default();
        document.blocks.push(Block::Paragraph(Paragraph::new(
            vec![InlineNode::Macro(InlineMacro::Image(Box::new(image)))],
            Location::default(),
        )));
        assert!(
            matches!(export(&document),Err(asg::Error::Unsupported {path,..}) if path=="/blocks/0/inlines/0/metadata/attributes/custom")
        );
    }
    Ok(())
}

#[test]
fn block_image_metadata_cannot_shadow_promoted_content() {
    for name in ["alt", "title"] {
        let mut image = crate::Image::new(Source::Name("photo.png"), Location::default());
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
