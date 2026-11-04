use acdc_parser::{
    CompatibilityMode, Options,
    ast::{BlockKind, BlockRef, ElementIdRef, InlineKind},
    parse,
};

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
    let [BlockKind::Paragraph(block)] = &*parsed
        .document_ref()
        .blocks()
        .map(BlockRef::kind)
        .collect::<Vec<_>>()
    else {
        return Err("expected a paragraph source form".into());
    };
    assert_eq!(block.style(), Some(expected));
    assert_eq!(block.title().ok_or("missing title")?.inlines().len(), 1);
    assert_eq!(block.id().map(ElementIdRef::as_str), Some("identity"));
    assert_eq!(
        parsed.document_ref().footnotes().count(),
        usize::from(formatted)
    );
    assert_eq!(
        block
            .content()
            .inlines()
            .any(|node| matches!(node.kind(), InlineKind::Bold(_))),
        formatted
    );
    if matches!(expected, "quote" | "verse") {
        assert!(block.attributes().contains("attribution"));
        assert!(block.attributes().contains("citetitle"));
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
    let [BlockKind::Paragraph(block)] = &*parsed
        .document_ref()
        .blocks()
        .map(BlockRef::kind)
        .collect::<Vec<_>>()
    else {
        return Err("expected paragraph".into());
    };
    let first = block.content().inlines().next().ok_or("missing content")?;
    assert!(matches!(first.kind(), InlineKind::Plain(text) if text.text() == "é "));
    assert_eq!(first.location().start().column(), 3);
    let bold = block
        .content()
        .inlines()
        .find(|node| matches!(node.kind(), InlineKind::Bold(_)))
        .ok_or("missing bold")?;
    assert_eq!(
        &source[bold.location().absolute_start()..=bold.location().absolute_end()],
        "*Bold*"
    );
    assert_eq!(parsed.document_ref().footnotes().count(), 1);
    let note = parsed
        .document_ref()
        .footnotes()
        .next()
        .ok_or("missing note")?;
    assert_eq!(
        &source[note.location().absolute_start()..=note.location().absolute_end()],
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
    let [BlockKind::Paragraph(block)] = &*parsed
        .document_ref()
        .blocks()
        .map(BlockRef::kind)
        .collect::<Vec<_>>()
    else {
        return Err("expected paragraph".into());
    };
    let draft = mode == CompatibilityMode::Draft;
    assert_eq!(
        block.style(),
        Some(if draft { "normal" } else { "literal" })
    );
    assert_eq!(
        parsed.document_ref().footnotes().count(),
        usize::from(draft)
    );
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
    let [BlockKind::Admonition(block)] = &*parsed
        .document_ref()
        .blocks()
        .map(BlockRef::kind)
        .collect::<Vec<_>>()
    else {
        return Err("expected admonition".into());
    };
    assert_eq!(block.blocks().count(), 1);
    assert_eq!(parsed.document_ref().footnotes().count(), 1);
    assert_eq!(block.location().absolute_end() + 1, source.len());
    Ok(())
}
