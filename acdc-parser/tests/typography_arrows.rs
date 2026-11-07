use acdc_parser::{CompatibilityMode, Options, SymbolKind, ast::InlineKind, parse};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
#[case("", "-> => <- <=", true)]
#[case("", "*->* `=>` footnote:[<-] link:guide.html[<=]", true)]
#[case("", "pass:c,r[-> => <- <=]", true)]
#[case("", "pass:r,c[-> => <- <=]", false)]
#[case("", "pass:r[-> => <- <=]", false)]
#[case("", "pass:c[-> => <- <=]", false)]
#[case("[subs=none]\n", "-> => <- <=", false)]
#[case("[subs=\"replacements,specialchars\"]\n", "-> => <- <=", false)]
#[case("[subs=\"specialchars,replacements\"]\n", "-> => <- <=", true)]
#[case("", "-&gt; =&gt; &lt;- &lt;=", false)]
#[case("", r"\-> \=> \<- \<=", false)]
#[case("", r"pass:c,r[\-> \=> \<- \<=]", false)]
#[case("", "pass:a,c,r[{right} {double-right} {left} {double-left}]", false)]
#[case("", "pass:c,r,a[{right} {double-right} {left} {double-left}]", false)]
fn source_arrows_are_resolved_in_the_requested_order(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[case] settings: &str,
    #[case] body: &str,
    #[case] expected: bool,
) -> Result<(), Error> {
    let source =
        format!(":right: ->\n:double-right: =>\n:left: <-\n:double-left: <=\n\n{settings}{body}\n");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let kinds = parsed
        .document()
        .content_occurrences()
        .flat_map(|field| field.content().inlines())
        .filter_map(|node| {
            if let InlineKind::Symbol(symbol) = node.kind() {
                Some(symbol.kind())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        kinds,
        if expected {
            vec![
                SymbolKind::RightArrow,
                SymbolKind::RightDoubleArrow,
                SymbolKind::LeftArrow,
                SymbolKind::LeftDoubleArrow,
            ]
        } else {
            vec![]
        },
        "{source}"
    );
    assert_eq!(parsed.warnings(), []);
    Ok(())
}

#[rstest::rstest]
#[case("<->", "->", SymbolKind::RightArrow)]
#[case("<=>", "=>", SymbolKind::RightDoubleArrow)]
#[case("->", "->", SymbolKind::RightArrow)]
#[case("=>", "=>", SymbolKind::RightDoubleArrow)]
#[case("<-", "<-", SymbolKind::LeftArrow)]
#[case("<=", "<=", SymbolKind::LeftDoubleArrow)]
fn source_arrow_locations_cover_authored_tokens(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values("{}", "*{}*", "footnote:[{}]", "link:guide.html[{}]", "pass:c,r[{}]")] context: &str,
    #[case] text: &str,
    #[case] token: &str,
    #[case] kind: SymbolKind,
) -> Result<(), Error> {
    let source = format!("Earlier.\n\nα🚀 {}.", context.replace("{}", text));
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let symbols = parsed
        .document()
        .content_occurrences()
        .flat_map(|field| field.content().inlines())
        .filter_map(|node| {
            if let InlineKind::Symbol(symbol) = node.kind() {
                Some(symbol)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let [symbol] = symbols.as_slice() else {
        return Err(format!("expected one symbol: {symbols:?}").into());
    };
    let start = source.rfind(token).ok_or("authored token")?;
    assert_eq!(symbol.kind(), kind);
    assert_eq!(symbol.location().absolute_start(), start, "{source}");
    assert_eq!(
        symbol.location().absolute_end(),
        start + token.len() - 1,
        "{source}"
    );
    assert_eq!(parsed.warnings(), []);
    Ok(())
}

#[rstest::rstest]
#[case("callouts,specialchars,replacements,macros", false)]
#[case("specialcharacters,replacements,macros", true)]
fn earlier_xml_callout_guards_cannot_become_arrows(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[case] subs: &str,
    #[case] arrow: bool,
) -> Result<(), Error> {
    let source = format!("[subs=\"{subs}\"]\n----\npass:r[(C)] <!--1-->\n----\n");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let kinds = parsed
        .document()
        .content_occurrences()
        .flat_map(|field| field.content().inlines())
        .filter_map(|node| {
            if let InlineKind::Symbol(symbol) = node.kind() {
                Some(symbol.kind())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(kinds.contains(&SymbolKind::RightArrow), arrow, "{source}");
    Ok(())
}
