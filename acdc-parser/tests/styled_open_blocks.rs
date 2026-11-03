use acdc_parser::{
    AdmonitionVariant, Block, CompatibilityMode, DelimitedBlockType, Options, parse,
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
    let [Block::DelimitedBlock(block)] = parsed.document().blocks.as_slice() else {
        return Err("expected one delimited block".into());
    };
    assert!(
        match expected {
            "example" => matches!(block.inner, DelimitedBlockType::DelimitedExample(_)),
            "quote" => matches!(block.inner, DelimitedBlockType::DelimitedQuote(_)),
            "verse" => matches!(block.inner, DelimitedBlockType::DelimitedVerse(_)),
            "sidebar" => matches!(block.inner, DelimitedBlockType::DelimitedSidebar(_)),
            "pass" => matches!(block.inner, DelimitedBlockType::DelimitedPass(_)),
            "comment" => matches!(block.inner, DelimitedBlockType::DelimitedComment(_)),
            _ => false,
        },
        "expected {expected}: {block:?}"
    );
    assert_eq!(block.delimiter, "--");
    let open = block
        .open_delimiter_location
        .as_ref()
        .ok_or("missing opener")?;
    let close = block
        .close_delimiter_location
        .as_ref()
        .ok_or("missing closer")?;
    assert_eq!(&source[open.absolute_start..=open.absolute_end], "--");
    assert_eq!(&source[close.absolute_start..=close.absolute_end], "--");
    assert_eq!(block.location.absolute_start, 0);
    assert_eq!(block.location.absolute_end + 1, source.len());
    if matches!(expected, "quote" | "verse") {
        assert!(block.metadata.attribution.is_some());
        assert!(block.metadata.citetitle.is_some());
    }
    assert_eq!(parsed.document().footnotes.len(), footnotes);
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
    let [Block::Admonition(block)] = parsed.document().blocks.as_slice() else {
        return Err("expected one admonition".into());
    };
    assert_eq!(block.variant, variant);
    assert_eq!(block.blocks.len(), 1);
    assert_eq!(block.location.absolute_start, 0);
    assert_eq!(block.location.absolute_end + 1, source.len());
    assert_eq!(parsed.document().footnotes.len(), 1);
    assert_eq!(parsed.warnings(), []);
    Ok(())
}
