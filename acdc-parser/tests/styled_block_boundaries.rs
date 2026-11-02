use acdc_parser::{AttributeValue, Block, CompatibilityMode, Options, parse};

type Error = Box<dyn std::error::Error>;

fn is_kind(block: &Block<'_>, expected: &str) -> bool {
    match expected {
        "paragraph" => matches!(block, Block::Paragraph(_)),
        "unordered" => matches!(block, Block::UnorderedList(_)),
        "ordered" => matches!(block, Block::OrderedList(_)),
        "description" => matches!(block, Block::DescriptionList(_)),
        "image" => matches!(block, Block::Image(_)),
        "audio" => matches!(block, Block::Audio(_)),
        "video" => matches!(block, Block::Video(_)),
        "admonition" => matches!(block, Block::Admonition(_)),
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
    let [block] = parsed.document().blocks.as_slice() else {
        return Err("expected one block".into());
    };
    let expected = if mode == CompatibilityMode::Draft {
        draft_kind
    } else {
        "paragraph"
    };
    assert!(is_kind(block, expected), "expected {expected}: {block:?}");
    let location = block.location();
    assert_eq!(location.absolute_start, 0);
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
        parsed.document().blocks.len(),
        if mode == CompatibilityMode::Draft {
            2
        } else {
            1
        }
    );
    let first = parsed
        .document()
        .blocks
        .first()
        .ok_or("missing paragraph")?;
    let end = if mode == CompatibilityMode::Draft {
        source.find('\n').ok_or("missing metadata newline")?
            + 1
            + content.find('\n').ok_or("missing content newline")?
    } else {
        source.len()
    };
    assert_eq!(first.location().absolute_start, 0);
    assert_eq!(first.location().absolute_end + 1, end);
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
    assert_eq!(parsed.document().footnotes.len(), 1);
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
    let [Block::Image(image)] = parsed.document().blocks.as_slice() else {
        return Err("expected an image".into());
    };
    assert!(!image.metadata.attributes.contains_key("language"));
    assert_eq!(
        image.metadata.attributes.get("width"),
        Some(&AttributeValue::String("rust".into()))
    );
    Ok(())
}
