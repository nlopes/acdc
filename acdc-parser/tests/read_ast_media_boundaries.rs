use std::path::Path;

use acdc_parser::{
    Anchor, Audio, Caption, CommentKind, CompatibilityMode, ContentAttribute, Location, Options,
    SafeMode, Source, SourceUrl, ThematicBreak, Title, Video,
    ast::{
        AnchorRef, AttributeValueRef, AudioRef, BlockKind, ContentRef, InlineKind, SourceKind,
        ThematicBreakRef, VideoRef,
    },
    parse, parse_file,
};

type Error = Box<dyn std::error::Error>;

fn text(content: ContentRef<'_>) -> Result<String, Error> {
    let mut result = String::new();
    for inline in content.inlines() {
        if let InlineKind::Plain(node) = inline.kind() {
            result.push_str(node.text());
        } else if let InlineKind::Bold(node) = inline.kind() {
            result.push_str(&text(node.content())?);
        } else if let InlineKind::Raw(node) = inline.kind() {
            result.push_str(node.text());
        } else if let InlineKind::Verbatim(node) = inline.kind() {
            result.push_str(node.text());
        } else {
            return Err("unexpected test content".into());
        }
    }
    Ok(result)
}

#[rstest::rstest]
fn media_reads_keep_complete_operands_controls_titles_and_captions(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        ".Music\n[#song.audio,caption='*Prefix*',options=autoplay]\naudio::music.ogg[options=loop]\n\n\
         .Playlist\n[#playlist.video]\nvideo::RvRhUHTV_8k,,SGqg_ZzThDU[youtube,width=640,options=controls]\n\n\
         video::one.mp4,two.mp4[poster=thumb.png]\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let before = serde_json::to_value(parsed.document())?;
    let mut blocks = parsed.document_ref().blocks();
    let BlockKind::Audio(audio) = blocks.next().ok_or("audio")?.kind() else {
        return Err("audio".into());
    };
    assert_eq!(
        audio.source().kind(),
        SourceKind::Path(Path::new("music.ogg"))
    );
    assert_eq!(text(audio.title().ok_or("title")?)?, "Music");
    assert_eq!(text(audio.caption_content().ok_or("caption")?)?, "Prefix");
    assert_eq!(audio.id().ok_or("id")?.as_str(), "song");
    assert!(audio.roles().contains("audio"));
    assert!(audio.options().contains("loop"));
    assert_eq!(
        audio.attributes().get("title"),
        audio.title().map(AttributeValueRef::Content)
    );
    let BlockKind::Video(video) = blocks.next().ok_or("video")?.kind() else {
        return Err("video".into());
    };
    assert_eq!(
        video.source().kind(),
        SourceKind::Name("RvRhUHTV_8k,,SGqg_ZzThDU")
    );
    assert_eq!(
        video.attributes().get("poster"),
        Some(AttributeValueRef::Text("youtube"))
    );
    assert_eq!(
        video.attributes().get("width"),
        Some(AttributeValueRef::Text("640"))
    );
    assert!(video.options().contains("controls"));
    assert!(video.roles().contains("video"));
    assert_eq!(video.id().ok_or("video id")?.as_str(), "playlist");
    assert_eq!(text(video.title().ok_or("video title")?)?, "Playlist");
    assert!(video.caption_content().is_none());
    let BlockKind::Video(local) = blocks.next().ok_or("local video")?.kind() else {
        return Err("local video".into());
    };
    assert_eq!(
        local.source().kind(),
        SourceKind::Path(Path::new("one.mp4,two.mp4"))
    );
    assert_eq!(
        local.attributes().get("poster"),
        Some(AttributeValueRef::Text("thumb.png"))
    );
    assert_eq!(serde_json::to_value(parsed.document())?, before);
    Ok(())
}

#[rstest::rstest]
fn toc_discrete_and_page_break_reads_keep_ids_and_aliases(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "[[toc-alias]]\n[#contents.nav,options=custom]\ntoc::[levels=2]\n\n\
         [[heading-alias]]\n[discrete#loose.note]\n== Loose *heading*\n\n\
         .New page\n[[page-alias]]\n[#page.print,options=custom]\n<<<\n\nAfter.\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let mut blocks = parsed.document_ref().blocks();
    let BlockKind::TableOfContents(toc) = blocks.next().ok_or("toc")?.kind() else {
        return Err("toc".into());
    };
    assert_eq!(toc.id().ok_or("toc id")?.as_str(), "contents");
    assert_eq!(
        toc.anchors().map(AnchorRef::id).collect::<Vec<_>>(),
        ["contents", "toc-alias"]
    );
    assert!(toc.roles().contains("nav"));
    assert!(toc.options().contains("custom"));
    assert_eq!(
        toc.attributes().get("levels"),
        Some(AttributeValueRef::Text("2"))
    );
    assert!(!toc.attributes().contains("title"));
    let BlockKind::DiscreteHeader(heading) = blocks.next().ok_or("heading")?.kind() else {
        return Err("discrete heading".into());
    };
    assert_eq!(heading.level(), 1);
    assert_eq!(heading.style(), Some("discrete"));
    assert_eq!(
        text(heading.title().ok_or("heading title")?)?,
        "Loose heading"
    );
    assert_eq!(heading.id().ok_or("heading id")?.as_str(), "loose");
    assert_eq!(
        heading.anchors().map(AnchorRef::id).collect::<Vec<_>>(),
        ["loose", "heading-alias"]
    );
    let BlockKind::PageBreak(page) = blocks.next().ok_or("page")?.kind() else {
        return Err("page break".into());
    };
    assert_eq!(text(page.title().ok_or("page title")?)?, "New page");
    assert_eq!(page.id().ok_or("page id")?.as_str(), "page");
    assert_eq!(
        page.anchors().map(AnchorRef::id).collect::<Vec<_>>(),
        ["page", "page-alias"]
    );
    assert!(page.roles().contains("print"));
    assert!(page.options().contains("custom"));
    assert!(matches!(
        blocks.next().ok_or("after")?.kind(),
        BlockKind::Paragraph(_)
    ));
    assert!(blocks.next().is_none());
    Ok(())
}

#[rstest::rstest]
fn thematic_break_inventory_contains_only_retained_title_and_identity(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        ".Pause\n[[first]]\n[[alias,Label]]\n[role=not-retained,width=50]\n'''\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let BlockKind::ThematicBreak(rule) =
        parsed.document_ref().blocks().next().ok_or("rule")?.kind()
    else {
        return Err("thematic break".into());
    };
    assert_eq!(text(rule.title().ok_or("title")?)?, "Pause");
    assert_eq!(rule.id().ok_or("id")?.as_str(), "first");
    assert_eq!(
        rule.anchors().map(AnchorRef::id).collect::<Vec<_>>(),
        ["first", "alias"]
    );
    assert_eq!(
        text(
            rule.anchors()
                .nth(1)
                .ok_or("anchor")?
                .reftext()
                .ok_or("reftext")?
        )?,
        "Label"
    );
    assert_eq!(
        rule.attributes().get("title"),
        rule.title().map(AttributeValueRef::Content)
    );
    assert_eq!(
        rule.attributes().get("id"),
        Some(AttributeValueRef::Text("first"))
    );
    assert!(!rule.attributes().contains("role"));
    assert!(!rule.attributes().contains("width"));
    assert_eq!(rule.attributes().iter().count(), 2);
    for (name, value) in rule.attributes().iter() {
        assert_eq!(rule.attributes().get(name), Some(value));
    }
    Ok(())
}

#[rstest::rstest]
fn comment_reads_preserve_the_retained_form_and_literal_text(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let options = Options::builder().with_compatibility_mode(mode).build()?;
    let line = parse("// retained", &options)?;
    let BlockKind::Comment(comment) = line
        .document_ref()
        .blocks()
        .next()
        .ok_or("line comment")?
        .kind()
    else {
        return Err("line comment".into());
    };
    assert_eq!(comment.kind(), CommentKind::Line);
    assert_eq!(comment.text(), " retained");
    let parsed = parse(
        "[comment]\n*literal* {name}\n\n[comment]\n--\nMore *literal*.\n--\n",
        &options,
    )?;
    let mut blocks = parsed.document_ref().blocks();
    let BlockKind::Comment(comment) = blocks.next().ok_or("paragraph comment")?.kind() else {
        return Err("paragraph comment".into());
    };
    assert_eq!(comment.kind(), CommentKind::Paragraph);
    assert_eq!(comment.text(), "*literal* {name}");
    let BlockKind::DelimitedComment(comment) = blocks.next().ok_or("delimited comment")?.kind()
    else {
        return Err("delimited comment".into());
    };
    assert_eq!(comment.delimiter(), "--");
    assert_eq!(text(comment.content())?, "More *literal*.");
    Ok(())
}

#[rstest::rstest]
fn included_media_and_cell_blocks_keep_source_scope(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    let main = directory.path().join("main.adoc");
    let include = directory.path().join("media.adoc");
    std::fs::write(
        &main,
        "= T\n:enabled:\n\ninclude::media.adoc[]\n\n[cols=a]\n|===\n|\nvideo::one,,three[youtube]\n|===\n",
    )?;
    std::fs::write(
        &include,
        "ifdef::enabled[]\naudio::música.ogg[]\nendif::[]\n",
    )?;
    let parsed = parse_file(
        &main,
        &Options::builder()
            .with_compatibility_mode(mode)
            .with_safe_mode(SafeMode::Unsafe)
            .build()?,
    )?;
    let mut blocks = parsed.document_ref().blocks();
    let BlockKind::Audio(audio) = blocks.next().ok_or("audio")?.kind() else {
        return Err("audio".into());
    };
    assert_eq!(
        audio.source().kind(),
        SourceKind::Path(Path::new("música.ogg"))
    );
    let location = parsed.resolve_location(audio.location());
    assert_eq!(location.file, Some(include));
    assert_eq!(
        (location.location.start.line, location.location.start.column),
        (2, 1)
    );
    let BlockKind::Table(table) = blocks.next().ok_or("table")?.kind() else {
        return Err("table".into());
    };
    let cell = table
        .rows()
        .next()
        .ok_or("row")?
        .cells()
        .next()
        .ok_or("cell")?;
    let BlockKind::Video(video) = cell.blocks().next().ok_or("video")?.kind() else {
        return Err("video".into());
    };
    assert_eq!(video.source().kind(), SourceKind::Name("one,,three"));
    assert!(blocks.next().is_none());
    Ok(())
}

#[test]
fn constructed_owners_preserve_empty_content_and_scalar_controls() -> Result<(), Error> {
    fn shared<T: Copy + Send + Sync>(_: T) {}
    let mut audio = Audio::new(
        Source::Url(SourceUrl::new("https://EXAMPLE.org:443/music")?),
        Location::default(),
    );
    audio.title = Title::new(Vec::new());
    audio.metadata.caption = Some(Caption::CustomContent(ContentAttribute::default()));
    audio.metadata.attributes.set("custom".into(), false.into());
    audio
        .metadata
        .attributes
        .set("title".into(), "stale title".into());
    audio.metadata.id = Some(Anchor::new("primary", Location::default()));
    audio
        .metadata
        .anchors
        .push(Anchor::new("alias", Location::default()));
    let read = AudioRef::from(&audio);
    assert_eq!(
        read.source().kind(),
        SourceKind::Url("https://EXAMPLE.org:443/music")
    );
    assert!(read.title().ok_or("empty title")?.is_empty());
    assert!(read.caption_content().ok_or("empty caption")?.is_empty());
    assert_eq!(
        read.attributes().get("custom"),
        Some(AttributeValueRef::Bool(false))
    );
    assert_eq!(
        read.attributes().get("title"),
        read.title().map(AttributeValueRef::Content)
    );
    shared(read);
    std::thread::scope(|scope| {
        let anchors = read.anchors();
        scope.spawn(move || assert_eq!(anchors.count(), 2));
    });
    let video = Video::new(Source::Name(""), Location::default());
    let video = VideoRef::from(&video);
    assert_eq!(video.source().kind(), SourceKind::Name(""));
    assert!(video.title().is_none());
    shared(video);
    let rule = ThematicBreak::default();
    let rule = ThematicBreakRef::from(&rule);
    assert!(rule.attributes().is_empty());
    assert!(rule.id().is_none());
    shared(rule);
    Ok(())
}
