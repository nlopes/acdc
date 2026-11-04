use acdc_parser::{CompatibilityMode, Options, SymbolKind, ast::InlineKind, parse};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
#[case("--pass:[]", SymbolKind::SpacedEmDash, true)]
#[case("pass:[]--", SymbolKind::SpacedEmDash, true)]
#[case("indexterm2:[--]pass:[]", SymbolKind::SpacedEmDash, true)]
#[case("pass:[]indexterm2:[--]", SymbolKind::SpacedEmDash, true)]
#[case("left-pass:[]-right", SymbolKind::WordBoundedEmDash, false)]
#[case("left --pass:[] right", SymbolKind::SpacedEmDash, false)]
#[case("left pass:[]-- right", SymbolKind::SpacedEmDash, false)]
#[case("Sam'pass:[]s", SymbolKind::CurvedApostrophe, false)]
fn removed_profiles_keep_typography_in_the_parser(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values("{}", "**{}**", "footnote:n[{}]", "link:outer.html[{}]")] context: &str,
    #[case] body: &str,
    #[case] kind: SymbolKind,
    #[case] requires_paragraph: bool,
) -> Result<(), Error> {
    let source = format!("Earlier.\n\n{}", context.replace("{}", body));
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
    let expected = if requires_paragraph && context != "{}" {
        vec![]
    } else {
        vec![kind]
    };
    assert_eq!(
        symbols
            .iter()
            .map(|symbol| symbol.kind())
            .collect::<Vec<_>>(),
        expected,
        "{source}"
    );
    for symbol in symbols {
        let loc = symbol.location();
        let authored = source
            .get(loc.absolute_start()..=loc.absolute_end())
            .ok_or("source span")?;
        assert!(
            !authored.contains("pass:"),
            "symbol consumed unrelated source: {authored}"
        );
    }
    assert_eq!(parsed.warnings(), []);
    Ok(())
}

#[rstest::rstest]
#[case("left-pass:[]-right", "-")]
#[case("left --pass:[] right", " --")]
#[case("left pass:[]-- right", "-- ")]
#[case("Sam'pass:[]s", "'")]
fn symbols_after_empty_profiles_keep_their_source_owner(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[case] body: &str,
    #[case] authored: &str,
) -> Result<(), Error> {
    let source = format!("Earlier.\n\nα🚀 {body}");
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
        return Err("expected one symbol".into());
    };
    let loc = symbol.location();
    assert_eq!(
        source.get(loc.absolute_start()..=loc.absolute_end()),
        Some(authored)
    );
    Ok(())
}

#[rstest::rstest]
#[case("left-pass:[]-right")]
#[case("--pass:[]")]
#[case("Sam'pass:[]s")]
fn empty_profiles_do_not_enable_disabled_replacements(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[case] body: &str,
) -> Result<(), Error> {
    let parsed = parse(
        &format!("[subs=-replacements]\n{body}"),
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert!(
        !parsed
            .document()
            .content_occurrences()
            .flat_map(|field| field.content().inlines())
            .any(|node| matches!(node.kind(), InlineKind::Symbol(_)))
    );
    Ok(())
}
