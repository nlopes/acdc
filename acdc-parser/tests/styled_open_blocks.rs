use acdc_parser::{
    AdmonitionVariant, CompatibilityMode, Options,
    ast::{BlockKind, BlockRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
#[case("example", "example", 1)]
#[case("quote,Author,Citation", "quote", 1)]
#[case("verse,Author,Citation", "verse", 1)]
#[case("sidebar", "sidebar", 1)]
#[case("pass", "pass", 0)]
#[case("comment", "comment", 0)]
fn styled_open_blocks_select_one_content_model(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[case] style: &str,
    #[case] expected: &str,
    #[case] footnotes: usize,
) -> Result<(), Error> {
    let source = format!("[{style}]\n--\n*Body* footnote:body[Body note]\n--");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let [block] = &*parsed.document_ref().blocks().collect::<Vec<_>>() else {
        return Err("expected one delimited block".into());
    };
    let (actual, retained_delimiter, open, close) = if let BlockKind::Example(node) = block.kind() {
        (
            "example",
            node.delimiter(),
            node.open_delimiter_location(),
            node.close_delimiter_location(),
        )
    } else if let BlockKind::Quote(node) = block.kind() {
        (
            "quote",
            node.delimiter(),
            node.open_delimiter_location(),
            node.close_delimiter_location(),
        )
    } else if let BlockKind::Verse(node) = block.kind() {
        (
            "verse",
            node.delimiter(),
            node.open_delimiter_location(),
            node.close_delimiter_location(),
        )
    } else if let BlockKind::Sidebar(node) = block.kind() {
        (
            "sidebar",
            node.delimiter(),
            node.open_delimiter_location(),
            node.close_delimiter_location(),
        )
    } else if let BlockKind::Passthrough(node) = block.kind() {
        (
            "pass",
            node.delimiter(),
            node.open_delimiter_location(),
            node.close_delimiter_location(),
        )
    } else if let BlockKind::DelimitedComment(node) = block.kind() {
        (
            "comment",
            node.delimiter(),
            node.open_delimiter_location(),
            node.close_delimiter_location(),
        )
    } else {
        return Err("expected a supported delimited block".into());
    };
    assert_eq!(actual, expected, "{block:?}");
    assert_eq!(retained_delimiter, "--");
    let open = open.ok_or("missing opener")?;
    let close = close.ok_or("missing closer")?;
    assert_eq!(&source[open.absolute_start()..=open.absolute_end()], "--");
    assert_eq!(&source[close.absolute_start()..=close.absolute_end()], "--");
    assert_eq!(block.location().absolute_start(), 0);
    assert_eq!(block.location().absolute_end() + 1, source.len());
    if matches!(expected, "quote" | "verse") {
        assert!(
            block
                .attributes()
                .ok_or("missing attributes")?
                .contains("attribution")
        );
        assert!(
            block
                .attributes()
                .ok_or("missing attributes")?
                .contains("citetitle")
        );
    }
    assert_eq!(parsed.document_ref().footnotes().count(), footnotes);
    assert_eq!(parsed.warnings(), []);
    Ok(())
}

#[rstest::rstest]
#[case("NOTE", AdmonitionVariant::Note)]
#[case("TIP", AdmonitionVariant::Tip)]
#[case("IMPORTANT", AdmonitionVariant::Important)]
#[case("CAUTION", AdmonitionVariant::Caution)]
#[case("WARNING", AdmonitionVariant::Warning)]
fn styled_open_admonitions_register_body_once(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[case] style: &str,
    #[case] variant: AdmonitionVariant,
) -> Result<(), Error> {
    let source = format!("[{style}]\n--\nfootnote:body[Body note]\n--");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let [BlockKind::Admonition(block)] = &*parsed
        .document_ref()
        .blocks()
        .map(BlockRef::kind)
        .collect::<Vec<_>>()
    else {
        return Err("expected one admonition".into());
    };
    assert_eq!(block.variant(), variant);
    assert_eq!(block.blocks().count(), 1);
    assert_eq!(block.location().absolute_start(), 0);
    assert_eq!(block.location().absolute_end() + 1, source.len());
    assert_eq!(parsed.document_ref().footnotes().count(), 1);
    assert_eq!(parsed.warnings(), []);
    Ok(())
}
