use acdc_parser::{CompatibilityMode, Options, SymbolKind, ast::InlineKind, parse};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
#[case("(C{close}", SymbolKind::Copyright, true)]
#[case(".{dots}", SymbolKind::Ellipsis, true)]
#[case("left{join}right", SymbolKind::WordBoundedEmDash, true)]
#[case("Sam'{tail}", SymbolKind::CurvedApostrophe, true)]
#[case("Sam'{empty}s", SymbolKind::CurvedApostrophe, true)]
#[case("Sam{empty}'s", SymbolKind::CurvedApostrophe, true)]
#[case("left{empty}--right", SymbolKind::WordBoundedEmDash, true)]
#[case("left--{empty}right", SymbolKind::WordBoundedEmDash, true)]
#[case("(C{empty})", SymbolKind::Copyright, true)]
#[case("{backslash}(C)", SymbolKind::Copyright, false)]
fn raw_fragment_tokens_obey_attribute_order(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values("a,r", "r,a")] subs: &str,
    #[case] body: &str,
    #[case] kind: SymbolKind,
    #[case] early: bool,
) -> Result<(), Error> {
    let source =
        format!(":tail: s\n:join: --\n:dots: ..\n:close: )\n:empty:\n\npass:{subs}[{body}]\n");
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
        if (subs == "a,r") == early {
            vec![kind]
        } else {
            vec![]
        },
        "{source}"
    );
    assert_eq!(parsed.warnings(), []);
    Ok(())
}

#[rstest::rstest]
#[case("(C{close}", "(C{close}")]
#[case(".{dots}", ".{dots}")]
#[case("left{join}right", "{join}")]
#[case("Sam'{tail}", "'")]
#[case("Sam'{empty}s", "'")]
#[case("left{empty}--right", "--")]
fn split_symbols_retain_complete_authored_spans(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values("{}", "*{}*", "footnote:[{}]", "link:guide.html[{}]")] context: &str,
    #[case] body: &str,
    #[case] token: &str,
) -> Result<(), Error> {
    let body = context.replace("{}", &format!("pass:a,r[{body}]"));
    let source =
        format!(":tail: s\n:join: --\n:dots: ..\n:close: )\n:empty:\n\nEarlier.\n\nα🚀 {body}.");
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

#[rstest::rstest]
#[case(r"pass:a,r[\(C{close}]")]
#[case(r"pass:a,r[\.{dots}]")]
#[case(r"pass:a,r[left\{join}right]")]
#[case(r"pass:q,r[*left*--right]")]
#[case(r"pass:r,q[left--*right*]")]
fn raw_fragment_escape_and_formatting_barriers_remain_literal(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[case] body: &str,
) -> Result<(), Error> {
    let source = format!(":join: --\n:dots: ..\n:close: )\n\n{body}.");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert!(
        parsed
            .document_ref()
            .content_occurrences()
            .flat_map(|field| field.content().inlines())
            .all(|node| !matches!(node.kind(), InlineKind::Symbol(_))),
        "{source}"
    );
    assert_eq!(parsed.warnings(), []);
    Ok(())
}
