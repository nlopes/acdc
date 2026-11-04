use acdc_parser::{
    Options,
    ast::{BlockKind, BlockRef, ContentRef, InlineKind, InlineRef},
    parse, parse_inline,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn unexpected(message: &str, actual: impl std::fmt::Debug) -> Box<dyn std::error::Error> {
    std::io::Error::other(format!("{message}, got {actual:?}")).into()
}

fn assert_plain_text(text: ContentRef<'_>, expected: &str) -> TestResult {
    let [InlineKind::Plain(text)] = &*text.inlines().map(InlineRef::kind).collect::<Vec<_>>()
    else {
        return Err(unexpected("expected plain macro text", text));
    };
    assert_eq!(text.text(), expected);
    Ok(())
}

#[test]
fn xref_accepts_non_ascii_path_characters() -> TestResult {
    for (input, target, text) in [
        (
            "xref:die_straße.adoc[my street]",
            "die_straße.adoc",
            "my street",
        ),
        (
            "xref:die_straße.adoc#section[street section]",
            "die_straße.adoc#section",
            "street section",
        ),
    ] {
        let parsed = parse_inline(input, &Options::default())?;
        let [InlineKind::CrossReference(xref)] = &*parsed
            .content()
            .inlines()
            .map(InlineRef::kind)
            .collect::<Vec<_>>()
        else {
            return Err(unexpected(
                "expected one cross-reference macro",
                parsed.content(),
            ));
        };

        assert_eq!(xref.target(), target);
        assert_plain_text(xref.text(), text)?;
    }

    Ok(())
}

#[test]
fn other_inline_macros_accept_non_ascii_path_characters() -> TestResult {
    let parsed = parse_inline("link:die_straße.adoc[street]", &Options::default())?;
    let [InlineKind::Link(link)] = &*parsed
        .content()
        .inlines()
        .map(InlineRef::kind)
        .collect::<Vec<_>>()
    else {
        return Err(unexpected("expected one link macro", parsed.content()));
    };
    assert_eq!(link.target().to_string(), "die_straße.adoc");

    let parsed = parse_inline("image:straße.png[street]", &Options::default())?;
    let [InlineKind::Image(image)] = &*parsed
        .content()
        .inlines()
        .map(InlineRef::kind)
        .collect::<Vec<_>>()
    else {
        return Err(unexpected(
            "expected one inline image macro",
            parsed.content(),
        ));
    };
    assert_eq!(image.source().to_string(), "straße.png");

    let parsed = parse_inline("icon:straße[]", &Options::default())?;
    let [InlineKind::Icon(icon)] = &*parsed
        .content()
        .inlines()
        .map(InlineRef::kind)
        .collect::<Vec<_>>()
    else {
        return Err(unexpected("expected one icon macro", parsed.content()));
    };
    assert_eq!(icon.target().to_string(), "straße");

    Ok(())
}

#[test]
fn block_media_macros_accept_non_ascii_path_characters() -> TestResult {
    let parsed = parse(
        "image::straße.png[]\n\naudio::straße.mp3[]\n\nvideo::straße.mp4[]\n",
        &Options::default(),
    )?;
    let [
        BlockKind::Image(image),
        BlockKind::Audio(audio),
        BlockKind::Video(video),
    ] = &*parsed
        .document_ref()
        .blocks()
        .map(BlockRef::kind)
        .collect::<Vec<_>>()
    else {
        return Err(unexpected(
            "expected image, audio, and video blocks",
            parsed.document_ref().blocks().collect::<Vec<_>>(),
        ));
    };

    assert_eq!(image.source().to_string(), "straße.png");
    assert_eq!(audio.source().to_string(), "straße.mp3");
    assert_eq!(video.source().to_string(), "straße.mp4");

    Ok(())
}
