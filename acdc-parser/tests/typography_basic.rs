use acdc_parser::{CompatibilityMode, Options, SymbolKind, ast::InlineKind, parse};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
#[case("", "(C) (R) (TM)", vec![SymbolKind::Copyright, SymbolKind::RegisteredTrademark, SymbolKind::Trademark])]
#[case("", "Wait...", vec![SymbolKind::Ellipsis])]
#[case("", "Sam's text", vec![SymbolKind::CurvedApostrophe])]
#[case("", "left--right", vec![SymbolKind::WordBoundedEmDash])]
#[case("", "left -- right", vec![SymbolKind::SpacedEmDash])]
#[case("", r"\(C) \(R) \(TM) \... word\--word Sam\'s", vec![])]
#[case("[subs=none]\n", "(C) ... left--right Sam's", vec![])]
#[case("[subs=\"specialchars,macros\"]\n", "(C) ... left--right Sam's", vec![])]
#[case("", "*Bold (C)*", vec![SymbolKind::Copyright])]
#[case("", "link:https://example.org[Label (TM)]", vec![SymbolKind::Trademark])]
#[case("", r"\footnote:n[Note (C) *Literal*]", vec![SymbolKind::Copyright])]
#[case("", "`(C) ...`", vec![SymbolKind::Copyright, SymbolKind::Ellipsis])]
#[case("", "+(C) ...+", vec![])]
#[case("", "pass:r[(C) ...]", vec![SymbolKind::Copyright, SymbolKind::Ellipsis])]
#[case("", "pass:q[*Bold (C)*]", vec![])]
#[case("", "pass:r,q[*Bold (C)*]", vec![SymbolKind::Copyright])]
#[case("", "pass:q,r[*Bold (C)*]", vec![SymbolKind::Copyright])]
#[case("", "pass:r,a,q[*{value}*]", vec![])]
#[case("", r"pass:r,q[Sam\'s \(C) \...]", vec![])]
#[case("", r"pass:q,r[Sam\'s \(C) \...]", vec![])]
fn typography_is_resolved_before_a_converter_reads_the_tree(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[case] settings: &str,
    #[case] body: &str,
    #[case] expected: Vec<SymbolKind>,
) -> Result<(), Error> {
    let source = format!("{settings}{body}\n");
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
                Some(symbol.kind())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(symbols, expected);
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    Ok(())
}

#[rstest::rstest]
#[case("attributes,replacements,specialchars", true)]
#[case("replacements,attributes,specialchars", false)]
fn late_attribute_text_cannot_create_a_symbol(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[case] subs: &str,
    #[case] expected: bool,
) -> Result<(), Error> {
    let source = format!(":value: (C)\n\n[subs=\"{subs}\"]\nπ {{value}}.");
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
    assert_eq!(!symbols.is_empty(), expected);
    if let [symbol] = symbols.as_slice() {
        let start = source.rfind("{value}").ok_or("missing reference")?;
        assert_eq!(symbol.location().absolute_start(), start);
        assert_eq!(
            symbol.location().absolute_end(),
            start + "{value}".len() - 1
        );
        assert_eq!(symbol.text(), "©");
    }
    Ok(())
}

#[rstest::rstest]
fn typography_does_not_register_a_note_twice(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = "π footnote:n[Note (C).]";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let [note] = parsed.document().footnotes.as_slice() else {
        return Err("expected one registered note".into());
    };
    assert_eq!(note.id.as_deref(), Some("n"));
    assert!(
        note.content
            .iter()
            .any(|node| matches!(node, acdc_parser::InlineNode::Symbol(_)))
    );
    Ok(())
}

#[rstest::rstest]
fn symbols_retain_authored_ranges(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values("(C)", "(R)", "(TM)", "...", "'", "--")] token: &str,
) -> Result<(), Error> {
    let source = format!("Earlier.\n\nα🚀 *left{token}right*.");
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
    let start = source.find(token).ok_or("authored token")?;
    assert_eq!(symbol.location().absolute_start(), start);
    assert_eq!(symbol.location().absolute_end(), start + token.len() - 1);
    assert_eq!(symbol.location().start().line(), 3);
    assert_eq!(
        symbol.location().start().column() as usize,
        source[source.rfind('\n').ok_or("line")? + 1..start]
            .chars()
            .count()
            + 1
    );
    Ok(())
}
