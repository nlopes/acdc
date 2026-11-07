use acdc_parser::{
    Options,
    ast::{BlockKind, BlockRef, InlineKind},
    parse, parse_inline,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn unexpected(message: &str, actual: impl std::fmt::Debug) -> Box<dyn std::error::Error> {
    std::io::Error::other(format!("{message}, got {actual:?}")).into()
}

#[test]
fn block_media_macros_accept_spaced_local_targets() -> TestResult {
    let parsed = parse(
        ":asset: attribute file.png\n\nimage::block image.png[]\n\nimage::{asset}[]\n\naudio::audio clip.mp3[]\n\nvideo::video clip.mp4[]\n",
        &Options::default(),
    )?;
    let [
        BlockKind::Image(image),
        BlockKind::Image(attribute_image),
        BlockKind::Audio(audio),
        BlockKind::Video(video),
    ] = &*parsed
        .document()
        .blocks()
        .map(BlockRef::kind)
        .collect::<Vec<_>>()
    else {
        return Err(unexpected(
            "expected two images, one audio block, and one video block",
            parsed.document().blocks().collect::<Vec<_>>(),
        ));
    };

    assert_eq!(image.source().to_string(), "block image.png");
    assert_eq!(attribute_image.source().to_string(), "attribute file.png");
    assert_eq!(audio.source().to_string(), "audio clip.mp3");
    assert_eq!(video.source().to_string(), "video clip.mp4");

    Ok(())
}

#[test]
fn inline_images_accept_spaced_encoded_and_attribute_expanded_targets() -> TestResult {
    let parsed = parse(
        ":asset: attribute file.png\n\nInline image:inline image.png[], image:already%20encoded.png[], image:https://example.com/remote image.png[], and image:{asset}[].\n",
        &Options::default(),
    )?;
    let [BlockKind::Paragraph(paragraph)] = &*parsed
        .document()
        .blocks()
        .map(BlockRef::kind)
        .collect::<Vec<_>>()
    else {
        return Err(unexpected(
            "expected one paragraph",
            parsed.document().blocks().collect::<Vec<_>>(),
        ));
    };
    let sources = paragraph
        .content()
        .inlines()
        .filter_map(|inline| {
            let InlineKind::Image(image) = inline.kind() else {
                return None;
            };
            Some(image.source().to_string())
        })
        .collect::<Vec<_>>();

    assert_eq!(
        sources,
        [
            "inline image.png",
            "already%20encoded.png",
            "https://example.com/remote image.png",
            "attribute file.png"
        ]
    );
    Ok(())
}

#[test]
fn media_macros_accept_encoded_paths_and_spaced_urls() -> TestResult {
    let parsed = parse(
        "image::already%20encoded.png[]\n\nimage::https://example.com/remote image.png[]\n",
        &Options::default(),
    )?;
    let [BlockKind::Image(encoded), BlockKind::Image(remote)] = &*parsed
        .document()
        .blocks()
        .map(BlockRef::kind)
        .collect::<Vec<_>>()
    else {
        return Err(unexpected(
            "expected encoded-path and remote image blocks",
            parsed.document().blocks().collect::<Vec<_>>(),
        ));
    };

    assert_eq!(encoded.source().to_string(), "already%20encoded.png");
    assert_eq!(
        remote.source().to_string(),
        "https://example.com/remote image.png"
    );
    Ok(())
}

#[test]
fn media_macro_targets_reject_edge_whitespace() -> TestResult {
    let parsed = parse(
        "image:: leading.png[]\n\naudio:: leading.mp3[]\n\nvideo:: leading.mp4[]\n\nimage::trailing.png []\n",
        &Options::default(),
    )?;
    assert!(parsed.document().blocks().all(|block| !matches!(
        block.kind(),
        BlockKind::Image(_) | BlockKind::Audio(_) | BlockKind::Video(_)
    )));

    for input in ["image: leading.png[]", "image:trailing.png []"] {
        let parsed = parse_inline(input, &Options::default())?;
        assert!(
            parsed
                .content()
                .inlines()
                .all(|inline| !matches!(inline.kind(), InlineKind::Image(_)))
        );
    }

    Ok(())
}
