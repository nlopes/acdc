use acdc_parser::{
    CompatibilityMode, Options, StemNotation, WarningKind,
    ast::{BlockKind, ContentRef, InlineKind},
    parse,
};

type Error = Box<dyn std::error::Error>;

fn paragraph(parsed: &acdc_parser::ParseResult) -> Result<ContentRef<'_>, Error> {
    parsed
        .document()
        .blocks()
        .find_map(|block| {
            if let BlockKind::Paragraph(block) = block.kind() {
                Some(block.content())
            } else {
                None
            }
        })
        .ok_or_else(|| "missing paragraph".into())
}

#[rstest::rstest]
fn unsupported_inline_substitutions_preserve_the_complete_source(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values("stem", "asciimath", "latexmath")] prefix: &str,
    #[values("attributes", "quotes", "+attributes", "quotes,attributes")] controls: &str,
) -> Result<(), Error> {
    let authored =
        format!("{prefix}:{controls}[{{value}} *x* footnote:note[Discarded.] pass:q[*Raw*]]");
    let source = format!(":value: expanded\n\nπ Before {authored} after {{value}}.");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let mut text = String::new();
    for node in paragraph(&parsed)?.inlines() {
        if let InlineKind::Plain(plain) = node.kind() {
            text.push_str(plain.text());
        } else if let InlineKind::Raw(raw) = node.kind() {
            text.push_str(raw.text());
        } else {
            return Err(format!("unsupported source produced an inline node: {node:?}").into());
        }
    }
    assert_eq!(text, format!("π Before {authored} after expanded."));
    assert_eq!(parsed.document().footnotes().count(), 0);
    let [warning] = parsed.warnings() else {
        return Err("expected one STEM recovery warning".into());
    };
    assert!(matches!(warning.kind, WarningKind::ContentRecovery { .. }));
    let location = &warning
        .location
        .as_ref()
        .ok_or("missing warning source")?
        .location;
    let start = source.find(&authored).ok_or("missing authored source")?;
    assert_eq!(
        (location.absolute_start, location.absolute_end),
        (start, start + authored.len() - 1)
    );
    Ok(())
}

#[rstest::rstest]
#[case("stem:[]", "", StemNotation::Asciimath)]
#[case("latexmath:[]", "", StemNotation::Latexmath)]
#[case(r"stem:[\[x\]]", "[x]", StemNotation::Asciimath)]
#[case(r"latexmath:[a\\]", "a\\", StemNotation::Latexmath)]
#[case("latexmath:[$x$]", "x", StemNotation::Latexmath)]
#[case("latexmath:[$$x$$]", "$x$", StemNotation::Latexmath)]
#[case("asciimath:[$x$]", "$x$", StemNotation::Asciimath)]
#[case("stem:[{value} *x* --]", "{value} *x* --", StemNotation::Asciimath)]
#[case("stem:[x pass:[y]]", "x y", StemNotation::Asciimath)]
fn literal_formula_contract_is_shared_between_modes(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[case] authored: &str,
    #[case] expected: &str,
    #[case] notation: StemNotation,
) -> Result<(), Error> {
    let source = format!(":value: expanded\n\nπ {authored} ω.");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let stem = paragraph(&parsed)?
        .inlines()
        .find_map(|node| {
            if let InlineKind::Stem(stem) = node.kind() {
                Some(stem)
            } else {
                None
            }
        })
        .ok_or("missing STEM macro")?;
    assert_eq!(stem.text(), expected);
    assert_eq!(stem.notation(), notation);
    assert_eq!(
        source.get(stem.location().absolute_start()..=stem.location().absolute_end()),
        Some(authored)
    );
    assert_eq!(parsed.warnings(), []);
    Ok(())
}

#[rstest::rstest]
fn unsupported_lists_do_not_warn_when_macros_are_disabled(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = ":value: expanded\n\n[subs=attributes]\nstem:attributes[{value}]";
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert_eq!(parsed.warnings(), []);
    assert!(paragraph(&parsed)?.inlines().any(
        |node| matches!(node.kind(), InlineKind::Plain(plain) if plain.text().contains("expanded"))
    ));
    Ok(())
}

#[rstest::rstest]
#[case("attributes", "expanded", 0)]
#[case("+attributes", "expanded", 0)]
#[case("none", "{value}", 0)]
#[case("-attributes", "{value}", 0)]
#[case("quotes,attributes", "expanded", 1)]
fn block_formula_substitutions_are_shared_and_keep_the_authored_body(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[case] substitutions: &str,
    #[case] expected: &str,
    #[case] warnings: usize,
) -> Result<(), Error> {
    let source =
        format!(":value: expanded\n\n[stem,subs=\"{substitutions}\"]\n++++\n{{value}} *x*\n++++\n");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let block = parsed
        .document()
        .blocks()
        .find_map(|block| {
            if let BlockKind::Stem(block) = block.kind() {
                Some(block)
            } else {
                None
            }
        })
        .ok_or("missing STEM block")?;
    let formula = block;
    assert_eq!(formula.text(), format!("{expected} *x*"));
    assert_eq!(block.source_text(), Some("{value} *x*\n"));
    assert_eq!(parsed.warnings().len(), warnings);
    Ok(())
}
