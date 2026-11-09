use acdc_parser::{
    CompatibilityMode, Options,
    ast::{BlockKind, BlockRef, ContentRef, InlineKind, StructuralRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
fn ordinary_attribute_order_follows_the_selected_mode(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        ":value: *Bold*\n:alias: {value}\n\n{alias}\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let Some(BlockKind::Paragraph(paragraph)) =
        parsed.document().blocks().next().map(BlockRef::kind)
    else {
        return Err("missing paragraph".into());
    };
    let nodes: Vec<_> = paragraph.content().inlines().collect();
    assert_eq!(nodes.len(), 1);
    let node = nodes.first().ok_or("missing content")?.kind();
    if matches!(node, InlineKind::Bold(_)) {
        assert_eq!(mode, CompatibilityMode::Draft);
    } else {
        let InlineKind::Plain(text) = node else {
            return Err("unexpected inline kind".into());
        };
        assert_eq!(mode, CompatibilityMode::Asciidoctor);
        assert_eq!(text.text(), "*Bold*");
    }
    Ok(())
}

#[rstest::rstest]
fn ordinary_order_preserves_single_registration_and_reference_ranges(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        include_str!("../../fixtures/tests/compatibility_inline_attribute_effects.adoc"),
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert_eq!(parsed.document().footnotes().count(), 1);
    assert_eq!(
        parsed
            .document()
            .footnotes()
            .next()
            .ok_or("missing note")?
            .id(),
        Some("n")
    );
    let bold = parsed
        .document()
        .descendants()
        .filter_map(|n| {
            let StructuralRef::Block(block) = n else {
                return None;
            };
            let BlockKind::Paragraph(p) = block.kind() else {
                return None;
            };
            Some(p.content())
        })
        .flat_map(ContentRef::inlines)
        .filter(|n| matches!(n.kind(), InlineKind::Bold(_)))
        .count();
    assert_eq!(
        bold,
        if mode == CompatibilityMode::Draft {
            3
        } else {
            0
        }
    );
    for block in parsed.document().blocks() {
        let BlockKind::Paragraph(paragraph) = block.kind() else {
            continue;
        };
        for node in paragraph.content().inlines() {
            if let InlineKind::Bold(value) = node.kind() {
                assert!(matches!(
                    parsed
                        .preprocessed_source()
                        .get(value.location().absolute_start()..=value.location().absolute_end()),
                    Some("{wrapped}" | "{term-wrapped}")
                ));
            }
        }
    }
    Ok(())
}
