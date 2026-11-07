//! Checked builders preserve one owner for each semantic attribute.

use acdc_parser::{
    Admonition, AdmonitionVariant, Anchor, Audio, BlockMetadata, ContentAttribute, DelimitedBlock,
    DelimitedBlockType, ElementAttributes, Header, Image, InlineNode, Location, Paragraph, Plain,
    Section, Source, Title, Video,
};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
fn metadata_builder_rejects_dedicated_names_and_aliases(
    #[values(
        "id",
        "role",
        "roles",
        "opts",
        "options",
        "style",
        "caption",
        "reftext",
        "subs",
        "attribution",
        "citetitle"
    )]
    name: &str,
) -> Result<(), Error> {
    let mut attributes = ElementAttributes::default();
    attributes.set(name.to_owned().into(), "shadow".into());
    let error = BlockMetadata::new()
        .with_attributes(attributes)
        .err()
        .ok_or("reserved key accepted")?;
    assert_eq!(error.name, name);
    Ok(())
}

#[rstest::rstest]
fn owner_builders_check_raw_metadata_even_when_dedicated_fields_are_absent(
    #[values(
        "title",
        "id",
        "role",
        "roles",
        "opts",
        "options",
        "style",
        "caption",
        "reftext",
        "subs",
        "attribution",
        "citetitle"
    )]
    name: &str,
) -> Result<(), Error> {
    let mut metadata = BlockMetadata::new();
    metadata
        .attributes
        .set(name.to_owned().into(), "shadow".into());
    let before = metadata.clone();
    macro_rules! rejected {
        ($owner:expr) => {
            assert_eq!(
                $owner
                    .with_metadata(metadata.clone())
                    .err()
                    .ok_or("reserved key accepted")?
                    .name,
                name
            );
        };
    }
    rejected!(Header::new(Title::default(), Location::default()));
    rejected!(Paragraph::new(Vec::new(), Location::default()));
    rejected!(Section::new(
        Title::default(),
        1,
        Vec::new(),
        Location::default()
    ));
    rejected!(DelimitedBlock::new(
        DelimitedBlockType::DelimitedExample(Vec::new()),
        "====",
        Location::default()
    ));
    rejected!(Admonition::new(
        AdmonitionVariant::Note,
        Vec::new(),
        Location::default()
    ));
    rejected!(Audio::new(Source::Name("audio"), Location::default()));
    rejected!(Video::new(Source::Name("video"), Location::default()));
    rejected!(Image::new(Source::Name("image"), Location::default()));
    assert_eq!(metadata, before);
    Ok(())
}

#[rstest::rstest]
fn image_alt_is_reserved_with_absent_empty_and_present_content(
    #[values(None, Some(ContentAttribute::default()), Some(ContentAttribute::new(vec![InlineNode::PlainText(Plain::new("real", Location::default()))])))]
    alt: Option<ContentAttribute<'static>>,
) -> Result<(), Error> {
    let mut metadata = BlockMetadata::new();
    metadata.attributes.set("alt".into(), "shadow".into());
    let mut image = Image::new(Source::Name("image"), Location::default());
    image.set_alt(alt);
    let error = image
        .with_metadata(metadata)
        .err()
        .ok_or("alt shadow accepted")?;
    assert_eq!(error.name, "alt");
    Ok(())
}

#[test]
fn extensions_stay_contextual_and_typed_values_are_preserved() -> Result<(), Error> {
    let mut values = ElementAttributes::default();
    values.set("alt".into(), "opaque video extension".into());
    values.set("custom".into(), "".into());
    let mut metadata = BlockMetadata::new().with_attributes(values)?;
    metadata.id = Some(Anchor::new("id", Location::default()));
    metadata.roles.push("wide");
    metadata.options.push("loop");
    metadata.style = Some("");
    metadata.reftext = Some(ContentAttribute::default());
    let video =
        Video::new(Source::Name("video"), Location::default()).with_metadata(metadata.clone())?;
    let audio =
        Audio::new(Source::Name("audio"), Location::default()).with_metadata(metadata.clone())?;
    assert_eq!(video.clone_metadata(), metadata);
    assert_eq!(audio.clone_metadata(), metadata);
    let image =
        Image::new(Source::Name("image"), Location::default()).with_metadata(metadata.clone());
    assert_eq!(image.err().ok_or("image alt accepted")?.name, "alt");
    metadata.attributes.remove("alt");
    let mut image = Image::new(Source::Name("image"), Location::default());
    image.set_alt(Some(ContentAttribute::default()));
    image.set_title(Title::new(Vec::new()));
    let image = image.with_metadata(metadata.clone())?;
    assert_eq!(image.clone_metadata(), metadata);
    assert!(acdc_parser::ast::ImageRef::from(&image).alt().is_some());
    assert!(acdc_parser::ast::ImageRef::from(&image).title().is_some());
    Ok(())
}
