use acdc_parser::{CompatibilityMode, Options, Substitution, SymbolKind, ast::InlineKind, parse};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
#[case("βx-&gt;", Some(SymbolKind::RightArrow), "-&gt;")]
#[case("-&gt;", Some(SymbolKind::RightArrow), "-&gt;")]
#[case("=&gt;", Some(SymbolKind::RightDoubleArrow), "=&gt;")]
#[case("&lt;-", Some(SymbolKind::LeftArrow), "&lt;-")]
#[case("&lt;=", Some(SymbolKind::LeftDoubleArrow), "&lt;=")]
#[case(r"\-&gt;", None, "")]
#[case(r"\=&gt;", None, "")]
#[case(r"\&lt;-", None, "")]
#[case(r"\&lt;=", None, "")]
#[case("&lt;-&gt;", Some(SymbolKind::RightArrow), "-&gt;")]
#[case("&lt;=&gt;", Some(SymbolKind::RightDoubleArrow), "=&gt;")]
#[case(r"&lt;\-&gt;", Some(SymbolKind::LeftArrow), r"&lt;\-")]
#[case(r"&lt;\=&gt;", Some(SymbolKind::LeftDoubleArrow), r"&lt;\=")]
#[case(r"\&lt;-&gt;", Some(SymbolKind::RightArrow), "-&gt;")]
fn encoded_arrows_have_semantic_values_and_authored_spans(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values("r", "r,c", "c,r")] profile: &str,
    #[case] body: &str,
    #[case] expected: Option<SymbolKind>,
    #[case] token: &str,
) -> Result<(), Error> {
    let source = format!("Earlier.\n\nα🚀 pass:{profile}[{body}] ω.");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let symbols = parsed
        .document_ref()
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
    let expected = expected.filter(|_| profile != "c,r");
    assert_eq!(
        symbols.iter().map(|s| s.kind()).collect::<Vec<_>>(),
        expected.into_iter().collect::<Vec<_>>(),
        "{source}"
    );
    if let [symbol] = symbols.as_slice() {
        let start = source.rfind(token).ok_or("authored arrow")?;
        assert_eq!(symbol.location().absolute_start(), start, "{source}");
        assert_eq!(
            symbol.location().absolute_end(),
            start + token.len() - 1,
            "{source}"
        );
    }
    assert_eq!(parsed.warnings(), []);
    Ok(())
}

#[rstest::rstest]
#[case("a,r", true)]
#[case("r,a", false)]
#[case("a,r,c", true)]
#[case("r,a,c", false)]
fn encoded_arrows_cannot_use_later_attribute_fragments(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[case] profile: &str,
    #[case] enabled: bool,
) -> Result<(), Error> {
    let source = format!(":left: &lt;-\n:tail: gt;\n\nα🚀 pass:{profile}[-&{{tail}} {{left}}] ω.");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let kinds = parsed
        .document_ref()
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
        if enabled {
            vec![SymbolKind::RightArrow]
        } else {
            vec![]
        },
        "{source}"
    );
    if !enabled {
        assert!(parsed.document_ref().content_occurrences().flat_map(|field| field.content().inlines()).any(|node|matches!(node.kind(),InlineKind::Raw(raw) if raw.text().contains('&') && !raw.substitutions().any(|sub|sub==Substitution::Replacements))));
    }
    assert_eq!(parsed.warnings(), []);
    Ok(())
}

#[rstest::rstest]
#[case("replacements", true)]
#[case("quotes,replacements", true)]
#[case("quotes,replacements,specialchars", true)]
#[case("specialcharacters,quotes,replacements", false)]
fn encoded_arrows_are_resolved_in_block_profiles(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values("{}", "----\n{}\n----")] context: &str,
    #[case] profile: &str,
    #[case] enabled: bool,
) -> Result<(), Error> {
    let body = "α🚀 -&gt; =&gt; &lt;- &lt;=";
    let source = format!("[subs=\"{profile}\"]\n{}", context.replace("{}", body));
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let kinds = parsed
        .document_ref()
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
        if enabled {
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
fn ordinary_encoded_arrows_preserve_literal_characters(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = "βx -&gt; =&gt; &lt;- &lt;=";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let text = parsed
        .document_ref()
        .content_occurrences()
        .flat_map(|field| field.content().inlines())
        .map(|node| {
            if let InlineKind::Plain(text) = node.kind() {
                Ok(text.text())
            } else if let InlineKind::Raw(text) = node.kind() {
                Ok(text.text())
            } else {
                Err(format!("unexpected inline {:?}", node.kind()))
            }
        })
        .collect::<Result<String, _>>()?;
    assert_eq!(text, "βx -> => <- <=");
    assert_eq!(parsed.warnings(), []);
    Ok(())
}
