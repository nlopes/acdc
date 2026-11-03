//! Simple admonition bodies expose their original paragraph without changing the tree.

use acdc_parser::{
    Admonition, AdmonitionVariant, Block, BlockMetadata, CompatibilityMode, Location, Options,
    Paragraph, Title,
    ast::{AdmonitionRef, BlockKind, ContentRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
fn simple_admonitions_keep_paragraph_attributes_content_and_locations(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = ".A note\n[#note,role=alert,subs=\"normal,-replacements\"]\nNOTE: A *café* note footnote:[One note].\n\n[WARNING]\n====\nFirst.\n\nSecond.\n====\n";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let before = serde_json::to_value(parsed.document())?;
    let mut admonitions = parsed.document_ref().blocks().filter_map(|block| {
        if let BlockKind::Admonition(admonition) = block.kind() {
            Some(admonition)
        } else {
            None
        }
    });
    let note = admonitions.next().ok_or("note")?;
    let paragraph = note.simple_paragraph().ok_or("simple paragraph")?;
    assert_eq!(note.id().ok_or("admonition ID")?.as_str(), "note");
    assert!(note.roles().contains("alert"));
    assert!(paragraph.id().is_none());
    assert!(paragraph.roles().is_empty());
    assert!(paragraph.attributes().get("subs").is_some());
    let BlockKind::Paragraph(original) = note.blocks().next().ok_or("body")?.kind() else {
        return Err("expected paragraph".into());
    };
    assert_eq!(paragraph.content(), original.content());
    assert_eq!(paragraph.location(), original.location());
    assert_eq!(paragraph.title(), original.title());
    assert!(
        admonitions
            .next()
            .ok_or("compound warning")?
            .simple_paragraph()
            .is_none()
    );
    assert!(admonitions.next().is_none());
    assert_eq!(parsed.document().footnotes.len(), 1);
    assert_eq!(serde_json::to_value(parsed.document())?, before);
    Ok(())
}

#[test]
fn constructed_admonitions_require_an_explicit_paragraph_form() -> Result<(), Error> {
    let metadata = BlockMetadata::new()
        .with_roles(vec!["alert"])
        .with_options(vec!["unbreakable"]);
    let paragraph = Paragraph::new(Vec::new(), Location::default())
        .with_metadata(metadata.clone())?
        .with_title(Title::new(Vec::new()));
    let admonition = Admonition::from_paragraph(
        AdmonitionVariant::Tip,
        paragraph.clone(),
        Location::default(),
    );
    let selected = AdmonitionRef::from(&admonition)
        .simple_paragraph()
        .ok_or("constructed simple paragraph")?;
    assert!(selected.content().is_empty());
    assert!(selected.title().is_none());
    assert!(
        AdmonitionRef::from(&admonition)
            .title()
            .is_some_and(ContentRef::is_empty)
    );
    assert!(AdmonitionRef::from(&admonition).roles().contains("alert"));

    for blocks in [
        Vec::new(),
        vec![
            Block::Paragraph(paragraph.clone()),
            Block::Paragraph(paragraph),
        ],
        vec![Block::Admonition(Admonition::new(
            AdmonitionVariant::Note,
            Vec::new(),
            Location::default(),
        ))],
        vec![Block::Paragraph(Paragraph::new(
            Vec::new(),
            Location::default(),
        ))],
    ] {
        let owner = Admonition::new(AdmonitionVariant::Tip, blocks, Location::default())
            .with_metadata(metadata.clone())?;
        assert!(AdmonitionRef::from(&owner).simple_paragraph().is_none());
    }
    Ok(())
}
