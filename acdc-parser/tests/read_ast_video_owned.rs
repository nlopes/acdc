//! Video copies retain the parsed values without sharing editable collections.

use acdc_parser::{
    Anchor, AttributeValue, BlockMetadata, Caption, ContentAttribute, ElementAttributes, Location,
    Source, Title, Video, ast::VideoRef,
};

type Error = Box<dyn std::error::Error>;

#[test]
fn video_copies_keep_empty_values_extensions_and_unresolved_metadata() -> Result<(), Error> {
    let mut attributes = ElementAttributes::default();
    attributes.set("custom".into(), AttributeValue::None);
    attributes.set("muted".into(), AttributeValue::Bool(false));
    attributes.set("poster".into(), "".into());
    attributes.set("alt".into(), "video extension".into());
    let mut metadata = BlockMetadata::new().with_attributes(attributes)?;
    metadata.id = Some(Anchor::new("clip", Location::default()));
    metadata
        .anchors
        .push(Anchor::new("alias", Location::default()));
    metadata.caption = Some(Caption::CustomContent(ContentAttribute::default()));
    let video = Video::new(Source::Name("clip"), Location::default())
        .with_title(Title::new(Vec::new()))
        .with_metadata(metadata)?;
    let mut copy = Video::from(VideoRef::from(&video));
    assert_eq!(copy, video);
    assert!(VideoRef::from(&copy).title().is_some());
    assert!(
        VideoRef::from(&copy)
            .title()
            .is_some_and(acdc_parser::ast::ContentRef::is_empty)
    );
    let mut copy_metadata = copy.take_metadata();
    copy_metadata.id = None;
    copy_metadata.anchors.clear();
    copy_metadata.attributes.remove("alt");
    copy_metadata.caption = None;
    copy.set_metadata(copy_metadata)?;
    let metadata = video.clone_metadata();
    assert!(metadata.id.is_some());
    assert_eq!(metadata.anchors.len(), 1);
    assert!(metadata.attributes.contains_key("alt"));
    assert!(metadata.caption.is_some());
    let absent = Video::new(Source::Name("untitled"), Location::default());
    assert!(
        !Video::from(VideoRef::from(&absent))
            .take_title()
            .is_present()
    );
    Ok(())
}
