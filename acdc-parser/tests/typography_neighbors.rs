use acdc_parser::{CompatibilityMode, Options, Substitution, SymbolKind, ast::InlineKind, parse};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
#[case("Sam'anchor:x[]s", Some(SymbolKind::CurvedApostrophe))]
#[case("Sam'indexterm:[hidden]s", Some(SymbolKind::CurvedApostrophe))]
#[case("Saindexterm2:[m]'s", Some(SymbolKind::CurvedApostrophe))]
#[case("indexterm2:[Sam]'s", Some(SymbolKind::CurvedApostrophe))]
#[case("Samindexterm2:['s]", Some(SymbolKind::CurvedApostrophe))]
#[case("Sam'indexterm2:[s]", Some(SymbolKind::CurvedApostrophe))]
#[case("left--anchor:x[]right", Some(SymbolKind::WordBoundedEmDash))]
#[case("left--indexterm:[hidden]right", Some(SymbolKind::WordBoundedEmDash))]
#[case("leftindexterm2:[--right]", Some(SymbolKind::WordBoundedEmDash))]
#[case("indexterm2:[left--]right", Some(SymbolKind::WordBoundedEmDash))]
#[case("leftindexterm2:[--]right", Some(SymbolKind::WordBoundedEmDash))]
#[case("Sam'indexterm2:[*s*]", None)]
#[case("Saindexterm2:[*m*]'s", None)]
#[case("Sam'link:guide.html[s]", None)]
#[case("*Sam*'s", None)]
#[case("Sam'anchor:x[]123", None)]
#[case("Sam'anchor:x[]_s", None)]
#[case("Sam'anchor:x[]", None)]
#[case("left--anchor:x[]!", None)]
#[case(r"Sam\'anchor:x[]s", None)]
fn neighbors_supply_word_context_without_moving_owned_text(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values("{}", "**{}**", "footnote:n[{}]", "link:outer.html[{}]")] context: &str,
    #[case] body: &str,
    #[case] kind: Option<SymbolKind>,
) -> Result<(), Error> {
    let source = format!("Earlier.\n\nα🚀 {}.", context.replace("{}", body));
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
    assert_eq!(
        symbols.iter().map(|s| s.kind()).collect::<Vec<_>>(),
        kind.into_iter().collect::<Vec<_>>(),
        "{source}"
    );
    if let [symbol] = symbols.as_slice() {
        let token = if kind == Some(SymbolKind::CurvedApostrophe) {
            "'"
        } else {
            "--"
        };
        let start = source.rfind(token).ok_or("authored token")?;
        assert_eq!(symbol.location().absolute_start(), start, "{source}");
        assert_eq!(
            symbol.location().absolute_end(),
            start + token.len() - 1,
            "{source}"
        );
    }
    assert_eq!(
        parsed.document_ref().footnotes().count(),
        usize::from(context.starts_with("footnote:"))
    );
    assert_eq!(parsed.warnings(), []);
    Ok(())
}

#[rstest::rstest]
fn resolved_visible_labels_preserve_the_registration_snapshot(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values("replacements,macros", "macros,replacements")] subs: &str,
) -> Result<(), Error> {
    let source = format!("[subs=\"{subs}\"]\nleftindexterm2:[--]right");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let term = parsed
        .document_ref()
        .content_occurrences()
        .flat_map(|field| field.content().inlines())
        .find_map(|node| {
            if let InlineKind::IndexTerm(term) = node.kind() {
                Some(term)
            } else {
                None
            }
        })
        .ok_or("index occurrence")?;
    assert!(
        term.term()
            .inlines()
            .any(|node| matches!(node.kind(), InlineKind::Symbol(_)))
    );
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
        if subs == "replacements,macros" {
            vec![SymbolKind::SpacedEmDash]
        } else {
            vec![]
        }
    );
    assert_eq!(parsed.warnings(), []);
    Ok(())
}

#[rstest::rstest]
#[case("Sam'anchor:x[]{tail}", SymbolKind::CurvedApostrophe, "'")]
#[case("{head}anchor:x[]'s", SymbolKind::CurvedApostrophe, "'")]
#[case("left--anchor:x[]{tail}", SymbolKind::WordBoundedEmDash, "--")]
#[case("{head}anchor:x[]--right", SymbolKind::WordBoundedEmDash, "--")]
fn neighboring_attribute_values_obey_the_parent_stage_order(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values("attributes,replacements,macros", "replacements,attributes,macros")] subs: &str,
    #[case] body: &str,
    #[case] kind: SymbolKind,
    #[case] token: &str,
) -> Result<(), Error> {
    let source = format!(":head: Sam\n:tail: s\n\n[subs=\"{subs}\"]\nα🚀 {body}.");
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
    if subs.starts_with("attributes") {
        assert_eq!(kinds, [kind], "{source}");
    } else {
        assert_eq!(kinds, [], "{source}");
        assert!(parsed.document_ref().content_occurrences().flat_map(|field|field.content().inlines())
            .any(|node| matches!(node.kind(),InlineKind::Raw(raw) if raw.text()==token && !raw.substitutions().any(|sub|sub==Substitution::Replacements))),"{source}");
    }
    assert_eq!(parsed.warnings(), []);
    Ok(())
}
