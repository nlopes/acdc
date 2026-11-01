//! Video copies retain the parsed values without sharing editable collections.

use acdc_parser::{
    Anchor, AttributeValue, Block, BlockMetadata, Caption, CompatibilityMode, ContentAttribute,
    ElementAttributes, Location, Options, Source, Title, Video,
    ast::{BlockKind, VideoRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
fn video_copies_preserve_parsed_values_and_allow_independent_edits(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = ".A *café* video\n[#clip,role=wide,opts=\"loop,controls\",custom=\"\"]\nvideo::RvRhUHTV_8k,_SvwdK_HibQ,SGqg_ZzThDU[youtube,width=640,height=360]\n";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let BlockKind::Video(video) = parsed.document_ref().blocks().next().ok_or("video")?.kind()
    else {
        return Err("expected video".into());
    };
    let before = serde_json::to_value(parsed.document())?;
    let mut copy = Video::from(video);
    let Block::Video(original) = parsed.document().blocks.first().ok_or("original video")? else {
        return Err("expected original video".into());
    };
    assert_eq!(&copy, original);
    assert_eq!(
        copy.source.to_string(),
        "RvRhUHTV_8k,_SvwdK_HibQ,SGqg_ZzThDU"
    );
    assert_eq!(copy.metadata.id.as_ref().map(|id| id.id), Some("clip"));
    assert_eq!(VideoRef::from(&copy).title(), video.title());
    assert_eq!(VideoRef::from(&copy).location(), video.location());
    copy.metadata.roles.push("copied");
    copy.metadata.options.clear();
    copy.metadata
        .attributes
        .set("custom".into(), "changed".into());
    copy.title = Title::default();
    copy.source = Source::Name("changed");
    assert!(!video.roles().contains("copied"));
    assert!(video.options().contains("loop"));
    assert_eq!(serde_json::to_value(parsed.document())?, before);
    Ok(())
}

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
    assert!(copy.title.is_present());
    assert!(copy.title.is_empty());
    copy.metadata.id = None;
    copy.metadata.anchors.clear();
    copy.metadata.attributes.remove("alt");
    copy.metadata.caption = None;
    assert!(video.metadata.id.is_some());
    assert_eq!(video.metadata.anchors.len(), 1);
    assert!(video.metadata.attributes.contains_key("alt"));
    assert!(video.metadata.caption.is_some());
    let absent = Video::new(Source::Name("untitled"), Location::default());
    assert!(!Video::from(VideoRef::from(&absent)).title.is_present());
    Ok(())
}
