use acdc_parser::{CompatibilityMode, Options, SymbolKind, ast::InlineKind, parse};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
#[case("{lead}'s")]
#[case("Sam'{tail}")]
#[case("{lead}'{tail}")]
#[case("Sam{empty}'s")]
#[case("Sam'{empty}s")]
#[case("é'{tail}")]
fn apostrophes_use_word_context_from_their_substitution_stage(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values(
        "specialchars,replacements,attributes",
        "specialchars,attributes,replacements",
        "specialchars,replacements,attributes,replacements"
    )]
    subs: &str,
    #[case] body: &str,
) -> Result<(), Error> {
    let source = format!(":lead: Sam\n:tail: s\n:empty:\n\n[subs=\"{subs}\"]\n{body}.");
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
    let expected = usize::from(subs == "specialchars,attributes,replacements");
    assert_eq!(symbols.len(), expected, "{source}");
    if let [symbol] = symbols.as_slice() {
        assert_eq!(symbol.kind(), SymbolKind::CurvedApostrophe);
        let start = source.rfind('\'').ok_or("apostrophe")?;
        assert_eq!(symbol.location().absolute_start(), start);
        assert_eq!(symbol.location().absolute_end(), start);
    }
    assert_eq!(parsed.warnings(), []);
    Ok(())
}

#[rstest::rstest]
fn empty_attributes_do_not_take_the_following_symbols_source_span(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values("(C)", "(R)", "(TM)", "...", "'")] token: &str,
    #[values(
        "{}",
        "*{}*",
        "`{}`",
        "footnote:[{}]",
        "link:manual.html[{}]",
        ".{}\nBody.",
        "== {}\n\nBody.",
        "image:photo.png[{}]"
    )]
    context: &str,
) -> Result<(), Error> {
    let body = context.replace("{}", &format!("π left{{empty}}{{empty}}{token}right"));
    let source = format!(":empty:\n\nEarlier.\n\n{body}");
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
    assert_eq!(symbol.location().absolute_start(), start, "{source}");
    assert_eq!(
        symbol.location().absolute_end(),
        start + token.len() - 1,
        "{source}"
    );
    assert_eq!(parsed.warnings(), []);
    Ok(())
}
