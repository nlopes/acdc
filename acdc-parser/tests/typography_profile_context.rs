use acdc_parser::{CompatibilityMode, Options, SymbolKind, ast::InlineKind, parse};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
#[case("Sam'pass:[s]", Some(SymbolKind::CurvedApostrophe))]
#[case("pass:[Sam]'s", Some(SymbolKind::CurvedApostrophe))]
#[case("pass:r[Sam']s", Some(SymbolKind::CurvedApostrophe))]
#[case("Sampass:r['s]", Some(SymbolKind::CurvedApostrophe))]
#[case("left--pass:[right]", Some(SymbolKind::WordBoundedEmDash))]
#[case("pass:[left]--right", Some(SymbolKind::WordBoundedEmDash))]
#[case("pass:r[left--]right", Some(SymbolKind::WordBoundedEmDash))]
#[case("leftpass:r[--right]", Some(SymbolKind::WordBoundedEmDash))]
#[case("leftpass:[--]right", None)]
#[case("pass:[Sam']s", None)]
#[case("Sam'pass:[123]", None)]
#[case("Sam'pass:[_s]", None)]
#[case("Sam'pass:[*s*]", None)]
#[case("left--pass:[!]", None)]
#[case(r"Sam\'pass:[s]", None)]
#[case(r"pass:r[Sam\']s", None)]
#[case("Sam'[[between]]pass:[s]", Some(SymbolKind::CurvedApostrophe))]
#[case("Sam'((pass:[s]))", Some(SymbolKind::CurvedApostrophe))]
#[case("((pass:[Sam]))'s", Some(SymbolKind::CurvedApostrophe))]
#[case("left--((pass:[right]))", Some(SymbolKind::WordBoundedEmDash))]
fn complete_tokens_use_profile_text_as_word_context(
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
        symbols
            .iter()
            .map(|symbol| symbol.kind())
            .collect::<Vec<_>>(),
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
#[case("pass:r[Sam']s", Some(SymbolKind::CurvedApostrophe))]
#[case("Sampass:r['s]", Some(SymbolKind::CurvedApostrophe))]
#[case("pass:r[left--]right", Some(SymbolKind::WordBoundedEmDash))]
#[case("leftpass:r[--right]", Some(SymbolKind::WordBoundedEmDash))]
#[case("Sam'pass:[s]", None)]
#[case("left--pass:[right]", None)]
fn active_profiles_keep_their_own_replacement_setting(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[case] body: &str,
    #[case] kind: Option<SymbolKind>,
) -> Result<(), Error> {
    let source = format!("[subs=macros]\nα🚀 {body}.");
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
    assert_eq!(kinds, kind.into_iter().collect::<Vec<_>>(), "{source}");
    assert_eq!(parsed.warnings(), []);
    Ok(())
}

#[rstest::rstest]
#[case("replacements,attributes,macros", false)]
#[case("attributes,replacements,macros", true)]
fn later_outer_expansion_cannot_supply_word_context(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[case] substitutions: &str,
    #[case] curved: bool,
) -> Result<(), Error> {
    let source = format!(":tail: s\n\n[subs=\"{substitutions}\"]\nSam'pass:a[{{tail}}].");
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
    assert_eq!(count, usize::from(curved), "{source}");
    Ok(())
}
