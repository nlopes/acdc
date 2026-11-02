use acdc_parser::{AttributeValue, Block, CompatibilityMode, DelimitedBlockType, Options, parse};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
#[case("literal", "----", "literal")]
#[case("listing", "....", "listing")]
#[case("source,rust", "....", "listing")]
#[case("literal", "--", "literal")]
#[case("listing", "--", "listing")]
#[case("source,rust", "--", "listing")]
#[case("sidebar", "----", "listing")]
#[case("source,rust", "****", "sidebar")]
fn delimited_style_selects_content_without_changing_source_form(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[case] style: &str,
    #[case] delimiter: &str,
    #[case] expected: &str,
) -> Result<(), Error> {
    let body = "*bold* footnote:body[Unselected note]";
    let source = format!("[{style}]\n{delimiter}\n{body}\n{delimiter}");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let [Block::DelimitedBlock(block)] = parsed.document().blocks.as_slice() else {
        return Err("expected one delimited block".into());
    };
    assert!(
        match expected {
            "literal" => matches!(block.inner, DelimitedBlockType::DelimitedLiteral(_)),
            "listing" => matches!(block.inner, DelimitedBlockType::DelimitedListing(_)),
            "sidebar" => matches!(block.inner, DelimitedBlockType::DelimitedSidebar(_)),
            _ => false,
        },
        "expected {expected}: {block:?}"
    );
    assert_eq!(block.delimiter, delimiter);
    assert_eq!(block.location.absolute_start, 0);
    assert_eq!(block.location.absolute_end + 1, source.len());
    let open = block
        .open_delimiter_location
        .as_ref()
        .ok_or("missing opener")?;
    let close = block
        .close_delimiter_location
        .as_ref()
        .ok_or("missing closer")?;
    assert_eq!(&source[open.absolute_start..=open.absolute_end], delimiter);
    assert_eq!(
        &source[close.absolute_start..=close.absolute_end],
        delimiter
    );
    if style == "source,rust" && expected == "listing" {
        assert_eq!(
            block.metadata.attributes.get("language"),
            Some(&AttributeValue::String("rust".into()))
        );
    }
    if expected != "sidebar" {
        assert_eq!(parsed.document().footnotes, []);
    }
    assert_eq!(parsed.warnings(), []);
    Ok(())
}

#[rstest::rstest]
#[case("literal", "----")]
#[case("source,rust", "....")]
fn transformed_verbatim_blocks_apply_explicit_substitutions_once(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[case] style: &str,
    #[case] delimiter: &str,
) -> Result<(), Error> {
    let source = format!(
        "[{style},subs=\"+quotes,+macros\"]\n{delimiter}\n*bold* footnote:body[Selected note]\n{delimiter}"
    );
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert_eq!(parsed.document().footnotes.len(), 1);
    assert_eq!(parsed.warnings(), []);
    Ok(())
}
