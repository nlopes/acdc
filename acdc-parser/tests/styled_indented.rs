use acdc_parser::{Block, CompatibilityMode, InlineNode, Options, parse};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
#[case("normal", "normal", true)]
#[case("quote,Author,Citation", "quote", true)]
#[case("verse,Author,Citation", "verse", true)]
#[case("sidebar", "sidebar", true)]
#[case("example", "example", true)]
#[case("source,rust", "source", false)]
#[case("listing", "listing", false)]
#[case("literal", "literal", false)]
fn indented_styles_select_content_once(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[case] style: &str,
    #[case] expected: &str,
    #[case] formatted: bool,
) -> Result<(), Error> {
    let source = format!(".Title\n[{style},id=identity,role=role]\n  *Body* footnote:body[Note]\n");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let [Block::Paragraph(block)] = parsed.document().blocks.as_slice() else {
        return Err("expected a paragraph source form".into());
    };
    assert_eq!(block.metadata.style, Some(expected));
    assert_eq!(block.title.len(), 1);
    assert_eq!(block.metadata.id.as_ref().map(|id| id.id), Some("identity"));
    assert_eq!(parsed.document().footnotes.len(), usize::from(formatted));
    assert_eq!(
        block
            .content
            .iter()
            .any(|node| matches!(node, InlineNode::BoldText(_))),
        formatted
    );
    if matches!(expected, "quote" | "verse") {
        assert!(block.metadata.attribution.is_some());
        assert!(block.metadata.citetitle.is_some());
    }
    assert_eq!(block.source_text(), Some("  *Body* footnote:body[Note]"));
    assert_eq!(parsed.warnings(), []);
    Ok(())
}

#[rstest::rstest]
fn indented_normal_retains_nested_source_locations(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = "[normal]\n  é *Bold*\n    https://example.org[Link] footnote:n[Note]";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let [Block::Paragraph(block)] = parsed.document().blocks.as_slice() else {
        return Err("expected paragraph".into());
    };
    let first = block.content.first().ok_or("missing content")?;
    assert!(matches!(first, InlineNode::PlainText(text) if text.content == "é "));
    assert_eq!(first.location().start.column, 3);
    let bold = block
        .content
        .iter()
        .find(|node| matches!(node, InlineNode::BoldText(_)))
        .ok_or("missing bold")?;
    assert_eq!(
        &source[bold.location().absolute_start..=bold.location().absolute_end],
        "*Bold*"
    );
    assert_eq!(parsed.document().footnotes.len(), 1);
    let note = parsed.document().footnotes.first().ok_or("missing note")?;
    assert_eq!(
        &source[note.location.absolute_start..=note.location.absolute_end],
        "footnote:n[Note]"
    );
    Ok(())
}

#[rstest::rstest]
fn paragraph_alias_depends_on_compatibility_mode(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "[paragraph]\n  *Body* footnote:n[Note]",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let [Block::Paragraph(block)] = parsed.document().blocks.as_slice() else {
        return Err("expected paragraph".into());
    };
    let draft = mode == CompatibilityMode::Draft;
    assert_eq!(
        block.metadata.style,
        Some(if draft { "normal" } else { "literal" })
    );
    assert_eq!(parsed.document().footnotes.len(), usize::from(draft));
    Ok(())
}

#[rstest::rstest]
fn indented_admonitions_keep_one_body_and_registration(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = ".Title\n[NOTE#identity.role]\n  *Body* footnote:n[Note]";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let [Block::Admonition(block)] = parsed.document().blocks.as_slice() else {
        return Err("expected admonition".into());
    };
    assert_eq!(block.blocks.len(), 1);
    assert_eq!(parsed.document().footnotes.len(), 1);
    assert_eq!(block.location.absolute_end + 1, source.len());
    Ok(())
}
