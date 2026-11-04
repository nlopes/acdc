use acdc_parser::{CompatibilityMode, Options, SymbolKind, ast::InlineKind, parse};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
#[case("specialchars,replacements", true, true)]
#[case("replacements,specialchars", true, false)]
#[case("replacements", true, false)]
#[case("specialchars", false, false)]
#[case("none", false, false)]
#[case("+replacements", true, true)]
fn verbatim_profiles_expose_resolved_symbols(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values("listing", "literal", "paragraph", "source")] form: &str,
    #[case] subs: &str,
    #[case] replacements: bool,
    #[case] arrows: bool,
) -> Result<(), Error> {
    let body = "α🚀 (C) ... left--right Sam's ->";
    let source = match form {
        "listing" => format!("[subs=\"{subs}\"]\n----\n{body}\n----"),
        "literal" => format!("[subs=\"{subs}\"]\n....\n{body}\n...."),
        "paragraph" => format!("[literal,subs=\"{subs}\"]\n{body}"),
        "source" => format!("[source,rust,subs=\"{subs}\"]\n----\n{body}\n----"),
        _ => return Err("unknown block form".into()),
    };
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
    let mut expected = if replacements {
        vec![
            SymbolKind::Copyright,
            SymbolKind::Ellipsis,
            SymbolKind::WordBoundedEmDash,
            SymbolKind::CurvedApostrophe,
        ]
    } else {
        vec![]
    };
    if arrows {
        expected.push(SymbolKind::RightArrow);
    }
    assert_eq!(
        symbols.iter().map(|s| s.kind()).collect::<Vec<_>>(),
        expected,
        "{source}"
    );
    for symbol in symbols {
        let token = match symbol.kind() {
            SymbolKind::Copyright => "(C)",
            SymbolKind::Ellipsis => "...",
            SymbolKind::WordBoundedEmDash => "--",
            SymbolKind::CurvedApostrophe => "'",
            SymbolKind::RightArrow => "->",
            SymbolKind::SpacedEmDash
            | SymbolKind::RegisteredTrademark
            | SymbolKind::Trademark
            | SymbolKind::LeftArrow
            | SymbolKind::RightDoubleArrow
            | SymbolKind::LeftDoubleArrow
            | _ => return Err("unexpected symbol".into()),
        };
        let start =
            source.find(body).ok_or("block body")? + body.find(token).ok_or("authored token")?;
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
fn verbatim_only_replacements_preserve_other_escapes(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values(
        "specialchars,replacements",
        "replacements,specialchars",
        "replacements",
        "none"
    )]
    subs: &str,
) -> Result<(), Error> {
    let source = format!(
        "[subs=\"{subs}\"]\n----\n{}\n----",
        r"\(C) \... left\--right Sam\'s \-> \*literal\* \{attribute}"
    );
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert!(
        parsed
            .document()
            .content_occurrences()
            .flat_map(|field| field.content().inlines())
            .all(|node| !matches!(node.kind(), InlineKind::Symbol(_))),
        "{source}"
    );
    assert_eq!(parsed.warnings(), []);
    Ok(())
}
