use acdc_parser::{
    CompatibilityMode, Options,
    ast::{AttributeValueRef, BlockKind, BlockRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

fn is_kind(block: BlockRef<'_>, expected: &str) -> bool {
    match expected {
        "paragraph" => matches!(block.kind(), BlockKind::Paragraph(_)),
        "unordered" => matches!(block.kind(), BlockKind::UnorderedList(_)),
        "ordered" => matches!(block.kind(), BlockKind::OrderedList(_)),
        "description" => matches!(block.kind(), BlockKind::DescriptionList(_)),
        "image" => matches!(block.kind(), BlockKind::Image(_)),
        "audio" => matches!(block.kind(), BlockKind::Audio(_)),
        "video" => matches!(block.kind(), BlockKind::Video(_)),
        "admonition" => matches!(block.kind(), BlockKind::Admonition(_)),
        _ => false,
    }
}

#[rstest::rstest]
#[case("literal", "* First item\n* Second item", "unordered")]
#[case("listing", ". First item\n. Second item", "ordered")]
#[case("verse", "term:: Description", "description")]
#[case("source,text", "image::sample.png[Sample]", "image")]
#[case("literal", "audio::sample.ogg[]", "audio")]
#[case("listing", "video::sample.mp4[]", "video")]
#[case("source,text", "NOTE: Important text", "admonition")]
fn draft_matches_structure_before_style(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[case] style: &str,
    #[case] content: &str,
    #[case] draft_kind: &str,
) -> Result<(), Error> {
    let source = format!("[{style}]\n{content}");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let [block] = &*parsed.document_ref().blocks().collect::<Vec<_>>() else {
        return Err("expected one block".into());
    };
    let expected = if mode == CompatibilityMode::Draft {
        draft_kind
    } else {
        "paragraph"
    };
    assert!(is_kind(*block, expected), "expected {expected}: {block:?}");
    let location = block.location();
    assert_eq!(location.absolute_start(), 0);
    assert_eq!(parsed.warnings(), []);
    Ok(())
}

#[rstest::rstest]
#[case(
    "source,text",
    "Ordinary paragraph before a delimiter\n----\nseparate listing body\n----"
)]
#[case("verse", "Ordinary paragraph before metadata\n[quote]\nNext paragraph")]
#[case(
    "literal",
    "Ordinary paragraph before a heading\n== Section title\nsection body"
)]
#[case(
    "source,text",
    " indented content before a delimiter\n----\nseparate listing body\n----"
)]
fn draft_styled_paragraphs_keep_structural_boundaries(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[case] style: &str,
    #[case] content: &str,
) -> Result<(), Error> {
    let source = format!("[{style}]\n{content}");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert_eq!(
        parsed.document_ref().blocks().count(),
        if mode == CompatibilityMode::Draft {
            2
        } else {
            1
        }
    );
    let first = parsed
        .document_ref()
        .blocks()
        .next()
        .ok_or("missing paragraph")?;
    let end = if mode == CompatibilityMode::Draft {
        source.find('\n').ok_or("missing metadata newline")?
            + 1
            + content.find('\n').ok_or("missing content newline")?
    } else {
        source.len()
    };
    assert_eq!(first.location().absolute_start(), 0);
    assert_eq!(first.location().absolute_end() + 1, end);
    assert_eq!(parsed.warnings(), []);
    Ok(())
}

#[rstest::rstest]
fn structural_lookahead_registers_the_selected_title_once(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = "[source,title='footnote:one[Title note]']\nimage::sample.png[Sample]";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert_eq!(parsed.document_ref().footnotes().count(), 1);
    assert_eq!(parsed.warnings(), []);
    Ok(())
}

#[test]
fn draft_media_mapping_does_not_retain_source_only_attributes() -> Result<(), Error> {
    let parsed = parse(
        "[source,rust]\nimage::sample.png[Alt]",
        &Options::builder()
            .with_compatibility_mode(CompatibilityMode::Draft)
            .build()?,
    )?;
    let [BlockKind::Image(image)] = &*parsed
        .document_ref()
        .blocks()
        .map(BlockRef::kind)
        .collect::<Vec<_>>()
    else {
        return Err("expected an image".into());
    };
    assert!(image.attributes().get("language").is_none());
    assert_eq!(
        image.attributes().get("width"),
        Some(AttributeValueRef::Text("rust"))
    );
    Ok(())
}
