use acdc_parser::{
    CompatibilityMode, Options,
    ast::{AttributeValueRef, BlockKind},
    parse,
};

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
    let [block] = &*parsed.document().blocks().collect::<Vec<_>>() else {
        return Err("expected one delimited block".into());
    };
    let (actual, retained_delimiter, open, close) = if let BlockKind::Literal(node) = block.kind() {
        (
            "literal",
            node.delimiter(),
            node.open_delimiter_location(),
            node.close_delimiter_location(),
        )
    } else if let BlockKind::Listing(node) = block.kind() {
        (
            "listing",
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
    } else {
        return Err("expected a supported delimited block".into());
    };
    assert_eq!(actual, expected, "{block:?}");
    assert_eq!(retained_delimiter, delimiter);
    assert_eq!(block.location().absolute_start(), 0);
    assert_eq!(block.location().absolute_end() + 1, source.len());
    let open = open.ok_or("missing opener")?;
    let close = close.ok_or("missing closer")?;
    assert_eq!(
        &source[open.absolute_start()..=open.absolute_end()],
        delimiter
    );
    assert_eq!(
        &source[close.absolute_start()..=close.absolute_end()],
        delimiter
    );
    if style == "source,rust" && expected == "listing" {
        assert_eq!(
            block
                .attributes()
                .ok_or("missing attributes")?
                .get("language"),
            Some(AttributeValueRef::Text("rust"))
        );
    }
    if expected != "sidebar" {
        assert!(parsed.document().footnotes().next().is_none());
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
    assert_eq!(parsed.document().footnotes().count(), 1);
    assert_eq!(parsed.warnings(), []);
    Ok(())
}
