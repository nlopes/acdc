use acdc_parser::{Block, InlineMacro, InlineNode, Options, parse, parse_inline};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn unexpected(message: &str, actual: impl std::fmt::Debug) -> Box<dyn std::error::Error> {
    std::io::Error::other(format!("{message}, got {actual:?}")).into()
}

fn assert_plain_text(text: &[InlineNode<'_>], expected: &str) -> TestResult {
    let [InlineNode::PlainText(text)] = text else {
        return Err(unexpected("expected plain macro text", text));
    };
    assert_eq!(text.content, expected);
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
        let [InlineNode::Macro(InlineMacro::CrossReference(xref))] = parsed.inlines() else {
            return Err(unexpected(
                "expected one cross-reference macro",
                parsed.inlines(),
            ));
        };

        assert_eq!(xref.target, target);
        assert_plain_text(&xref.text, text)?;
    }

    Ok(())
}

#[test]
fn other_inline_macros_accept_non_ascii_path_characters() -> TestResult {
    let parsed = parse_inline("link:die_straße.adoc[street]", &Options::default())?;
    let [InlineNode::Macro(InlineMacro::Link(link))] = parsed.inlines() else {
        return Err(unexpected("expected one link macro", parsed.inlines()));
    };
    assert_eq!(link.target.to_string(), "die_straße.adoc");

    let parsed = parse_inline("image:straße.png[street]", &Options::default())?;
    let [InlineNode::Macro(InlineMacro::Image(image))] = parsed.inlines() else {
        return Err(unexpected(
            "expected one inline image macro",
            parsed.inlines(),
        ));
    };
    assert_eq!(image.source.to_string(), "straße.png");

    let parsed = parse_inline("icon:straße[]", &Options::default())?;
    let [InlineNode::Macro(InlineMacro::Icon(icon))] = parsed.inlines() else {
        return Err(unexpected("expected one icon macro", parsed.inlines()));
    };
    assert_eq!(icon.target.to_string(), "straße");

    Ok(())
}

#[test]
fn block_media_macros_accept_non_ascii_path_characters() -> TestResult {
    let parsed = parse(
        "image::straße.png[]\n\naudio::straße.mp3[]\n\nvideo::straße.mp4[]\n",
        &Options::default(),
    )?;
    let [
        Block::Image(image),
        Block::Audio(audio),
        Block::Video(video),
    ] = parsed.document().blocks.as_slice()
    else {
        return Err(unexpected(
            "expected image, audio, and video blocks",
            &parsed.document().blocks,
        ));
    };

    assert_eq!(image.source.to_string(), "straße.png");
    assert_eq!(audio.source.to_string(), "straße.mp3");
    let [video_source] = video.sources.as_slice() else {
        return Err(unexpected("expected one video source", &video.sources));
    };
    assert_eq!(video_source.to_string(), "straße.mp4");

    Ok(())
}
