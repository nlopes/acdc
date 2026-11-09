use acdc_parser::{
    CompatibilityMode, Options, SafeMode,
    ast::{BlockKind, InlineKind, StructuralRef},
    parse, parse_file,
};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
fn quote_characters_are_retained_in_image_targets(
    #[values(
        "'photo.png",
        "\"photo.png",
        "'photo.png'",
        "\"photo.png\"",
        "photo's.png",
        "photo\"s.png",
        "'photo name.png",
        "\"photo name.png",
        "dir/'photo.png",
        "dir/\"photo.png",
        "'é東京.png",
        "%27photo.png",
        "%22photo.png"
    )]
    target: &str,
    #[values(false, true)] block: bool,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let macro_name = if block { "image::" } else { "image:" };
    let macro_source = format!("{macro_name}{target}[\"Alt, label\",title=\"Tip\"]");
    let source = if block {
        format!("{macro_source}\n")
    } else {
        format!("α {macro_source} ω\n")
    };
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let owner = parsed
        .document()
        .blocks()
        .next()
        .ok_or("image or paragraph")?;
    let (actual, location) = if block {
        let BlockKind::Image(image) = owner.kind() else {
            return Err("image block".into());
        };
        (image.source().to_string(), owner.location())
    } else {
        let BlockKind::Paragraph(paragraph) = owner.kind() else {
            return Err("paragraph".into());
        };
        let inline = paragraph
            .content()
            .inlines()
            .find(|inline| matches!(inline.kind(), InlineKind::Image(_)))
            .ok_or("inline image")?;
        let InlineKind::Image(image) = inline.kind() else {
            return Err("inline image".into());
        };
        (image.source().to_string(), inline.location())
    };
    assert_eq!(actual, target);
    assert_eq!(
        source.get(location.absolute_start()..=location.absolute_end()),
        Some(macro_source.as_str())
    );
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    Ok(())
}

#[rstest::rstest]
#[case("  α image:'é.png[Alt]\n", "image:'é.png[Alt]", "'é.png", 2, 5)]
#[case("  image::\"é.png[Alt]\n", "image::\"é.png[Alt]", "\"é.png", 2, 3)]
#[case(
    "  [cols=a,options=noheader]\n  |===\n  |\n  α image:'é.png[Alt]\n  |===\n",
    "image:'é.png[Alt]",
    "'é.png",
    5,
    5
)]
#[case(
    "  [cols=a,options=noheader]\n  |===\n  |\n  image::\"é.png[Alt]\n  |===\n",
    "image::\"é.png[Alt]",
    "\"é.png",
    5,
    3
)]
fn quoted_images_keep_original_locations_in_selected_includes_and_cells(
    #[case] body: &str,
    #[case] macro_source: &str,
    #[case] target: &str,
    #[case] line: u32,
    #[case] column: u32,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    let main = directory.path().join("main.adoc");
    let included = directory.path().join("body.adoc");
    std::fs::write(
        &main,
        format!(
            "include::body.adoc[lines=2..{},indent=0]\n",
            body.lines().count() + 1
        ),
    )?;
    std::fs::write(&included, format!("// skipped\n{body}"))?;
    let parsed = parse_file(
        &main,
        &Options::builder()
            .with_safe_mode(SafeMode::Unsafe)
            .with_compatibility_mode(mode)
            .build()?,
    )?;
    let mut images = Vec::new();
    for node in parsed.document().descendants() {
        let StructuralRef::Block(block) = node else {
            continue;
        };
        if let BlockKind::Image(image) = block.kind() {
            images.push((image.source().to_string(), block.location()));
        } else if let BlockKind::Paragraph(paragraph) = block.kind() {
            for inline in paragraph.content().inlines() {
                if let InlineKind::Image(image) = inline.kind() {
                    images.push((image.source().to_string(), inline.location()));
                }
            }
        }
    }
    let [(actual, location)] = images.as_slice() else {
        return Err("one included image".into());
    };
    assert_eq!(actual, target);
    let origin = parsed.resolve_location(*location);
    assert_eq!(origin.file.as_ref(), Some(&included));
    assert_eq!(origin.location.start.line, line);
    assert_eq!(origin.location.start.column, column);
    assert_eq!(origin.location.end.line, line);
    assert_eq!(
        origin.location.end.column,
        column + u32::try_from(macro_source.chars().count())? - 1
    );
    Ok(())
}

#[rstest::rstest]
fn quote_characters_are_retained_in_block_audio_targets(
    #[values("'audio clip.mp3", "\"audio.mp3")] target: &str,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = format!("audio::{target}[title=\"Audio\"]");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let owner = parsed.document().blocks().next().ok_or("audio block")?;
    let BlockKind::Audio(audio) = owner.kind() else {
        return Err("audio block".into());
    };
    assert_eq!(audio.source().to_string(), target);
    let location = owner.location();
    assert_eq!(
        source.get(location.absolute_start()..=location.absolute_end()),
        Some(source.as_str())
    );
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    Ok(())
}

#[rstest::rstest]
fn escaped_and_incomplete_quoted_image_macros_remain_text(
    #[values(
        r"\image:'file.png[Alt]",
        r#"\image::"file.png[Alt]"#,
        "image:'file.png[Alt",
        r#"image::"file.png[Alt"#
    )]
    source: &str,
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    for node in parsed.document().descendants() {
        if let StructuralRef::Block(block) = node {
            assert!(!matches!(block.kind(), BlockKind::Image(_)));
            if let BlockKind::Paragraph(paragraph) = block.kind() {
                assert!(
                    paragraph
                        .content()
                        .inlines()
                        .all(|inline| !matches!(inline.kind(), InlineKind::Image(_)))
                );
            }
        }
    }
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    Ok(())
}
