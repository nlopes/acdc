//! Paragraph edits preserve source snapshots and reject conflicting metadata.

use acdc_parser::{
    Block, BlockMetadata, CompatibilityMode, Location, Options, Paragraph, ParseResult, Plain,
    Title,
    ast::{ContentRef, ParagraphRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

const SOURCE: &str = "[#paragraph.note]\n.A *title*\nText *bold* footnote:[Note].\n";

fn paragraph(parsed: &ParseResult) -> Result<Paragraph<'_>, Error> {
    let block = parsed.document().blocks().next().ok_or("paragraph")?;
    let Block::Paragraph(paragraph) = Block::from(block) else {
        return Err("paragraph".into());
    };
    Ok(paragraph)
}

fn round_trip(mut paragraph: Paragraph<'_>) -> Result<Paragraph<'_>, Error> {
    let metadata = paragraph.take_metadata();
    let title = paragraph.take_title();
    let content = paragraph.take_content();
    assert!(ParagraphRef::from(&paragraph).title().is_none());
    assert!(ParagraphRef::from(&paragraph).content().is_empty());
    paragraph.set_metadata(metadata)?;
    paragraph.set_title(title);
    paragraph.set_content(content);
    Ok(paragraph)
}

#[rstest::rstest]
fn transfers_preserve_all_values_and_the_original_source_snapshot(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        SOURCE,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let original = paragraph(&parsed)?;
    let before = serde_json::to_value(&original)?;
    let source = original.source_text().ok_or("source snapshot")?;
    let rebuilt = round_trip(original.clone())?;
    assert_eq!(rebuilt, original);
    assert_eq!(serde_json::to_value(&rebuilt)?, before);
    assert_eq!(rebuilt.source_text(), Some(source));
    assert!(std::ptr::eq(
        rebuilt.source_text().ok_or("source")?.as_ptr(),
        source.as_ptr()
    ));
    Ok(())
}

#[rstest::rstest]
fn content_transfers_outlive_the_old_owner_and_keep_source_lifetimes(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        SOURCE,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let expected = paragraph(&parsed)?;
    let content = {
        let mut local = expected.clone();
        let content = local.take_content();
        drop(local);
        content
    };
    let mut rebuilt = expected.clone();
    rebuilt.set_content(content);
    assert_eq!(rebuilt, expected);
    assert!(!ParagraphRef::from(&rebuilt).content().is_empty());
    Ok(())
}

#[rstest::rstest]
fn rejected_metadata_leaves_content_title_location_and_source_unchanged(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        SOURCE,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let mut edited = paragraph(&parsed)?;
    let expected = edited.clone();
    for name in [
        "title",
        "id",
        "roles",
        "opts",
        "style",
        "caption",
        "reftext",
        "subs",
        "attribution",
        "citetitle",
    ] {
        let mut metadata = edited.clone_metadata();
        metadata.attributes.set(name.into(), "shadow".into());
        assert!(edited.set_metadata(metadata).is_err(), "{name}");
        assert_eq!(edited, expected, "{name}");
        assert_eq!(edited.source_text(), expected.source_text());
    }
    Ok(())
}

#[rstest::rstest]
fn body_edits_keep_source_provenance_and_do_not_rewrite_the_parse(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        SOURCE,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let before = serde_json::to_value(parsed.document())?;
    let mut edited = paragraph(&parsed)?;
    let source = edited.source_text();
    let location = Location::from(ParagraphRef::from(&edited).location());
    edited.set_content([]);
    edited.push_inline(acdc_parser::InlineNode::PlainText(Plain {
        content: "Changed",
        location: Location::default(),
        escaped: false,
    }));
    edited.set_title(Title::new(Vec::new()));
    edited.set_location(location.clone());
    let read = ParagraphRef::from(&edited);
    assert_eq!(read.content().inlines().count(), 1);
    assert!(read.title().is_some_and(ContentRef::is_empty));
    assert_eq!(Location::from(read.location()), location);
    assert_eq!(edited.source_text(), source);
    assert_eq!(serde_json::to_value(parsed.document())?, before);
    assert_eq!(parsed.document().footnotes().count(), 1);
    Ok(())
}

#[test]
fn constructed_paragraphs_keep_absent_and_empty_states() -> Result<(), Error> {
    let mut paragraph = Paragraph::new(Vec::new(), Location::default());
    assert!(!paragraph.take_title().is_present());
    assert!(paragraph.source_text().is_none());
    assert_eq!(paragraph.take_content().len(), 0);
    paragraph.set_metadata(BlockMetadata::default())?;
    paragraph.set_title(Title::new(Vec::new()));
    assert!(paragraph.take_title().is_present());
    assert!(ParagraphRef::from(&paragraph).title().is_none());
    paragraph.set_content([]);
    assert!(paragraph.source_text().is_none());
    Ok(())
}
