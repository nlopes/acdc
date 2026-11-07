use crate::{
    Block, CompatibilityMode, Options, Source, Title, Video,
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
    let BlockKind::Video(video) = parsed.document().blocks().next().ok_or("video")?.kind() else {
        return Err("expected video".into());
    };
    let before = serde_json::to_value(parsed.document_storage())?;
    let mut copy = Video::from(video);
    let Block::Video(original) = parsed
        .document_storage()
        .blocks
        .first()
        .ok_or("original video")?
    else {
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
    assert_eq!(serde_json::to_value(parsed.document_storage())?, before);
    Ok(())
}
