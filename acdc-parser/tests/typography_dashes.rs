use acdc_parser::{CompatibilityMode, Options, SymbolKind, ast::InlineKind, parse};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
#[case("left--right", vec![SymbolKind::WordBoundedEmDash])]
#[case("left -- right", vec![SymbolKind::SpacedEmDash])]
#[case("α--β a_--_b 1--2", vec![SymbolKind::WordBoundedEmDash; 3])]
#[case("left --- right; left----right", vec![])]
#[case("left --right; left-- right", vec![])]
#[case("*left -- right*", vec![SymbolKind::SpacedEmDash])]
#[case("`left--right`", vec![SymbolKind::WordBoundedEmDash])]
#[case("link:manual.html[left -- right]", vec![SymbolKind::SpacedEmDash])]
#[case("*-- word* and *word --*", vec![])]
#[case("`-- word` and `word --`", vec![])]
#[case("link:manual.html[-- word]", vec![])]
#[case("-- -- --", vec![SymbolKind::SpacedEmDash; 2])]
#[case("-- -- -- --", vec![SymbolKind::SpacedEmDash; 2])]
#[case("-- word", vec![SymbolKind::SpacedEmDash])]
#[case("word --", vec![SymbolKind::SpacedEmDash])]
#[case("*-- -- --*", vec![SymbolKind::SpacedEmDash])]
#[case("left -- -- -- right", vec![SymbolKind::SpacedEmDash; 2])]
#[case("left --\nright", vec![SymbolKind::SpacedEmDash])]
#[case("left\n-- right", vec![SymbolKind::SpacedEmDash])]
#[case(r"left\--right; left \-- right", vec![])]
#[case("pass:r[left--right]", vec![SymbolKind::WordBoundedEmDash])]
#[case("pass:r,q[*left -- right*]", vec![SymbolKind::SpacedEmDash])]
#[case("+left--right+ and pass:q[left -- right]", vec![])]
#[case("[subs=none]\nleft -- right; left--right", vec![])]
#[case("prefix indexterm2:[--] -- indexterm2:[--] tail", vec![SymbolKind::SpacedEmDash])]
fn dashes_are_semantic_only_in_their_enabled_source_context(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[case] body: &str,
    #[case] expected: Vec<SymbolKind>,
) -> Result<(), Error> {
    let parsed = parse(
        body,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let symbols = parsed
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
    assert_eq!(symbols, expected, "{body}");
    assert_eq!(parsed.warnings(), []);
    Ok(())
}

#[rstest::rstest]
#[case("left{join}right", false)]
#[case("{letter}--right", false)]
#[case("left--{letter}", false)]
#[case("left{empty}--right", false)]
#[case("left--{empty}right", false)]
#[case("left -- {letter}", true)]
fn dash_context_belongs_to_the_replacement_stage(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values(
        "specialchars,replacements,attributes",
        "specialchars,attributes,replacements"
    )]
    subs: &str,
    #[case] body: &str,
    #[case] authored: bool,
) -> Result<(), Error> {
    let source = format!(":join: --\n:letter: s\n:empty:\n\n[subs=\"{subs}\"]\n{body}.");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let symbols = parsed
        .document_ref()
        .content_occurrences()
        .flat_map(|field| field.content().inlines())
        .filter(|node| matches!(node.kind(), InlineKind::Symbol(_)))
        .count();
    assert_eq!(
        symbols,
        usize::from(authored || subs == "specialchars,attributes,replacements"),
        "{source}"
    );
    assert_eq!(parsed.warnings(), []);
    Ok(())
}

#[rstest::rstest]
fn dash_locations_retain_consumed_whitespace(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values("--", " -- ", " --\n", "\n-- ")] token: &str,
    #[values("*{}*", "link:manual.html[{}]")] context: &str,
) -> Result<(), Error> {
    let body = context.replace("{}", &format!("left{token}right"));
    let source = format!("Earlier.\n\nα🚀 {body}.");
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
    let [symbol] = symbols.as_slice() else {
        return Err(format!("expected one symbol: {symbols:?}").into());
    };
    let start = source.find(token).ok_or("token")?;
    assert_eq!(symbol.location().absolute_start(), start);
    assert_eq!(symbol.location().absolute_end(), start + token.len() - 1);
    assert_eq!(parsed.warnings(), []);
    Ok(())
}

#[rstest::rstest]
#[case("+macros,+replacements", "", 0)]
#[case("specialchars,replacements,callouts,macros", "", 1)]
#[case("+macros,+replacements", ",line-comment=--", 0)]
#[case("+macros,+replacements", ",line-comment=#", 1)]
#[case("+macros,+replacements", ",line-comment=", 1)]
fn callout_guards_are_not_reinterpreted_by_later_replacements(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[case] subs: &str,
    #[case] attributes: &str,
    #[case] expected: usize,
) -> Result<(), Error> {
    let source = format!(
        "[source,text,subs=\"{subs}\"{attributes}]\n----\ncode -- <1>\n----\n<1> Description.\n"
    );
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let symbols = parsed
        .document_ref()
        .content_occurrences()
        .flat_map(|field| field.content().inlines())
        .filter(|node| matches!(node.kind(), InlineKind::Symbol(_)))
        .count();
    assert_eq!(symbols, expected, "{source}");
    assert_eq!(parsed.warnings(), []);
    Ok(())
}

#[rstest::rstest]
#[case("pass:r[-- -- --]", 2)]
#[case("pass:r[-- word]", 1)]
#[case("pass:r[word --]", 1)]
#[case("pass:r,q[*-- word*] and pass:q,r[*-- word*]", 0)]
#[case("pass:r,a[{empty}-- word] and pass:r,a[word --{empty}]", 0)]
#[case("pass:a,r[{empty}-- word] and pass:a,r[word --{empty}]", 2)]
#[case(
    "[subs=\"specialchars,replacements,attributes\"]\n{empty}-- word; word --{empty}",
    0
)]
fn dash_boundaries_belong_to_the_owning_substitution_scope(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[case] body: &str,
    #[case] expected: usize,
) -> Result<(), Error> {
    let source = format!(":empty:\n\n{body}\n");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let symbols = parsed
        .document_ref()
        .content_occurrences()
        .flat_map(|field| field.content().inlines())
        .filter(|node| matches!(node.kind(), InlineKind::Symbol(_)))
        .count();
    assert_eq!(symbols, expected, "{source}");
    assert_eq!(parsed.warnings(), []);
    Ok(())
}
