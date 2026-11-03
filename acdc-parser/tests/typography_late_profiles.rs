use acdc_parser::{CompatibilityMode, Options, SymbolKind, ast::InlineKind, parse};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
#[case("pass:r,a[{word}']s", vec![])]
#[case("pass:a,r[{word}']s", vec![SymbolKind::CurvedApostrophe])]
#[case("Sampass:r,a['{tail}]", vec![])]
#[case("Sampass:a,r['{tail}]", vec![SymbolKind::CurvedApostrophe])]
#[case("pass:r,a[{mark}]", vec![])]
#[case("pass:a,r[{mark}]", vec![SymbolKind::Copyright])]
#[case("pass:r,a[left{dash}]right", vec![])]
#[case("pass:a,r[left{dash}]right", vec![SymbolKind::WordBoundedEmDash])]
#[case("pass:r,a[Sam']s", vec![SymbolKind::CurvedApostrophe])]
#[case("Sampass:r,a['s]", vec![SymbolKind::CurvedApostrophe])]
#[case("pass:r,a[--]", vec![SymbolKind::SpacedEmDash])]
#[case("pass:r,a[(C) {word}]", vec![SymbolKind::Copyright])]
#[case("pass:r,a[(C){word}']s", vec![SymbolKind::Copyright])]
#[case("pass:r,a['{word} Sam']s", vec![SymbolKind::CurvedApostrophe])]
#[case("pass:r,a[{word}'s and Sam']s", vec![SymbolKind::CurvedApostrophe])]
#[case(r"Sampass:r,a[\'s]", vec![])]
#[case(r"pass:r,a[Sam\']s", vec![])]
#[case(r"pass:r,a[{word}\']s", vec![])]
#[case("pass:r,a[pré']été", vec![SymbolKind::CurvedApostrophe])]
#[case("pass:r,a[word_--]tail", vec![SymbolKind::WordBoundedEmDash])]
#[case("pass:r,a[---]tail", vec![])]
#[case("pass:r,a[Sam'{empty}]s", vec![])]
#[case("pass:a,r[Sam'{empty}]s", vec![SymbolKind::CurvedApostrophe])]
#[case("pass:c,r,a[-> {word}]", vec![SymbolKind::RightArrow])]
#[case("pass:r,a,c[-> {word}]", vec![])]
#[case("pass:r,a[-&gt; {word}]", vec![SymbolKind::RightArrow])]
#[case("pass:r,a[{arrow}]", vec![])]
#[case("pass:a,c,r[{arrow}]", vec![])]
#[case("pass:a,r[{arrow}]", vec![SymbolKind::RightArrow])]
#[case(r"pass:r,a[\-&gt; {word}]", vec![])]
#[case(r"pass:r,a[-> {word}\]]", vec![])]
fn profiles_do_not_run_replacements_after_later_attributes(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values("{}", "**{}**", "footnote:n[{}]", "link:outer.html[{}]")] context: &str,
    #[case] body: &str,
    #[case] expected: Vec<SymbolKind>,
) -> Result<(), Error> {
    let source = format!(
        ":word: Sam\n:tail: s\n:mark: (C)\n:dash: --\n:arrow: ->\n\nEarlier.\n\nα🚀 {}",
        context.replace("{}", body)
    );
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
    assert_eq!(kinds, expected, "{source}");
    assert_eq!(parsed.warnings(), []);
    Ok(())
}

#[rstest::rstest]
#[case(r"pass:r,a,m[link:target.html[(C)\]]", true, false, false, 1)]
#[case(r"pass:r,a,m[footnote:n[(C)\]]", false, true, false, 1)]
#[case(r"pass:r,m[footnote:n[(C)\]]", false, true, false, 1)]
#[case("pass:r,a,q[*{word} (C)*]", false, false, true, 1)]
#[case(r"pass:r,a,q,m[*link:target.html[(C)\]*]", true, false, true, 1)]
#[case(r"pass:m,r[footnote:n[(C)\]]", false, true, false, 0)]
fn resolved_symbols_keep_later_macro_and_formatting_structure(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[case] body: &str,
    #[case] link: bool,
    #[case] note: bool,
    #[case] bold: bool,
    #[case] copyrights: usize,
) -> Result<(), Error> {
    let source = format!(":word: Sam\n\n{body}");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let kinds = parsed
        .document_ref()
        .content_occurrences()
        .flat_map(|field| field.content().inlines())
        .map(acdc_parser::ast::InlineRef::kind)
        .collect::<Vec<_>>();
    assert_eq!(kinds.iter().filter(|kind| matches!(kind, InlineKind::Symbol(symbol) if symbol.kind() == SymbolKind::Copyright)).count(), copyrights);
    assert_eq!(
        kinds
            .iter()
            .filter(|kind| matches!(kind, InlineKind::Link(_)))
            .count(),
        usize::from(link)
    );
    assert_eq!(
        kinds
            .iter()
            .filter(|kind| matches!(kind, InlineKind::Footnote(_)))
            .count(),
        usize::from(note)
    );
    assert_eq!(
        kinds
            .iter()
            .filter(|kind| matches!(kind, InlineKind::Bold(_)))
            .count(),
        usize::from(bold)
    );
    assert_eq!(parsed.document_ref().footnotes().count(), usize::from(note));
    assert_eq!(parsed.warnings(), []);
    Ok(())
}
