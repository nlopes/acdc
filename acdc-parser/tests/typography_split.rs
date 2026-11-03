use acdc_parser::{CompatibilityMode, Options, SymbolKind, ast::InlineKind, parse};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
#[case("prefix-indexterm2:[-]tail", vec![SymbolKind::WordBoundedEmDash])]
#[case("left-[[between]]-right", vec![SymbolKind::WordBoundedEmDash])]
#[case("left-indexterm:[hidden]-right", vec![SymbolKind::WordBoundedEmDash])]
#[case("left indexterm2:[--] right", vec![SymbolKind::SpacedEmDash])]
#[case("left indexterm2:[-]indexterm2:[-] right", vec![SymbolKind::SpacedEmDash])]
#[case("indexterm2:[--] right", vec![SymbolKind::SpacedEmDash])]
#[case("left indexterm2:[--]", vec![SymbolKind::SpacedEmDash])]
#[case("-- indexterm2:[--] --", vec![SymbolKind::SpacedEmDash; 2])]
#[case("left indexterm2:[--] -- indexterm2:[--] right", vec![SymbolKind::SpacedEmDash])]
#[case("left-pass:[-]right", vec![])]
#[case("left indexterm2:[pass:r[--]] right", vec![SymbolKind::SpacedEmDash])]
#[case("left-indexterm2:[*two*]-right", vec![])]
#[case(r"left\-[[escaped]]-right", vec![])]
#[case("left-indexterm2:[--]right", vec![])]
#[case("left-[[profile]]-pass:[right]", vec![SymbolKind::WordBoundedEmDash])]
fn split_dashes_preserve_profiles_and_earlier_replacements(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values("{}", "**{}**", "footnote:n[{}]", "link:outer.html[{}]")] context: &str,
    #[case] body: &str,
    #[case] expected: Vec<SymbolKind>,
) -> Result<(), Error> {
    let source = format!("Earlier.\n\nα🚀 {}", context.replace("{}", body));
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
    let expected =
        if context != "{}" && (body == "indexterm2:[--] right" || body == "left indexterm2:[--]") {
            vec![]
        } else {
            expected
        };
    let expected = if context != "{}" && body == "-- indexterm2:[--] --" {
        vec![SymbolKind::SpacedEmDash]
    } else {
        expected
    };
    assert_eq!(kinds, expected, "{source}");
    assert_eq!(parsed.warnings(), []);
    Ok(())
}

#[rstest::rstest]
#[case("prefix-indexterm2:[-]tail", "-")]
#[case("left-[[a]]-right", "-")]
#[case("left indexterm2:[--] right", "--")]
#[case("left indexterm2:[-]indexterm2:[-] right", "-")]
#[case("left [[a]]-- right", "-- ")]
#[case("left --[[a]] right", " --")]
fn symbols_keep_only_their_owners_source_contribution(
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
        return Err("expected one symbol".into());
    };
    let location = symbol.location();
    assert_eq!(
        source.get(location.absolute_start()..=location.absolute_end()),
        Some(authored),
        "{source}"
    );
    for term in parsed
        .document_ref()
        .content_occurrences()
        .flat_map(|field| field.content().inlines())
        .filter_map(|node| {
            if let InlineKind::IndexTerm(term) = node.kind() {
                Some(term)
            } else {
                None
            }
        })
    {
        assert_eq!(
            term.catalog()
                .term()
                .inlines()
                .filter_map(|node| {
                    if let InlineKind::Symbol(symbol) = node.kind() {
                        Some(symbol.kind())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>(),
            if body == "left indexterm2:[--] right" {
                vec![SymbolKind::SpacedEmDash]
            } else {
                vec![]
            }
        );
        for child in term.term().inlines() {
            assert!(term.location().absolute_start() <= child.location().absolute_start());
            assert!(term.location().absolute_end() >= child.location().absolute_end());
        }
    }
    Ok(())
}

#[rstest::rstest]
#[case("left-{empty}indexterm2:[-]right")]
#[case("left-indexterm2:[-]{tail}")]
#[case("left-indexterm2:[{dash}]right")]
#[case("left-indexterm2:[-]{empty}right")]
fn split_tokens_obey_attribute_stage_order(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values("attributes,replacements,macros", "replacements,attributes,macros")] subs: &str,
    #[case] body: &str,
) -> Result<(), Error> {
    let source = format!(":empty:\n:tail: right\n:dash: -\n\n[subs=\"{subs}\"]\nα🚀 {body}.");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let count = parsed
        .document_ref()
        .content_occurrences()
        .flat_map(|field| field.content().inlines())
        .filter(|node| matches!(node.kind(), InlineKind::Symbol(_)))
        .count();
    assert_eq!(
        count,
        usize::from(subs.starts_with("attributes")),
        "{source}"
    );
    Ok(())
}
