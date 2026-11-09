use acdc_parser::{
    CompatibilityMode, Options,
    ast::{BlockKind, BlockRef, ContentRef, InlineKind, InlineRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
#[case("missing", "{future}")]
#[case("escaped", "{name}")]
#[case("alias", "{future}")]
fn formatted_values_keep_missing_and_escaped_references_literal(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[case] name: &str,
    #[case] expected: &str,
) -> Result<(), Error> {
    let source = format!(
        "= T\n:name: Early\n:missing: pass:q,a[*{{future}}*]\n:escaped: pass:q,a[*\\{{name}}*]\n:alias: {{missing}}\n\n:future: Later\n:name: Later\n\n{{{name}}}\n"
    );
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let Some(BlockKind::Paragraph(paragraph)) =
        parsed.document().blocks().last().map(BlockRef::kind)
    else {
        return Err("missing paragraph".into());
    };
    let [InlineKind::Bold(bold)] = &*paragraph
        .content()
        .inlines()
        .map(InlineRef::kind)
        .collect::<Vec<_>>()
    else {
        return Err(format!("missing bold span: {:?}", paragraph.content()).into());
    };
    let text = bold
        .content()
        .inlines()
        .filter_map(|node| {
            if let InlineKind::Raw(text) = node.kind() {
                Some(text.text())
            } else if let InlineKind::Plain(text) = node.kind() {
                Some(text.text())
            } else {
                None
            }
        })
        .collect::<String>();
    assert_eq!(text, expected);
    assert_eq!(
        source.get(bold.location().absolute_start()..=bold.location().absolute_end()),
        Some(format!("{{{name}}}").as_str())
    );
    assert_eq!(parsed.warnings(), []);
    Ok(())
}

fn active_stage(nodes: ContentRef<'_>, stage: &str) -> usize {
    nodes.inlines().filter(|node| match stage {
        "q" => matches!(node.kind(), InlineKind::Bold(_)),
        "m" => matches!(node.kind(), InlineKind::UrlMacro(_) | InlineKind::Link(_) | InlineKind::Autolink(_)),
        "r" => matches!(node.kind(), InlineKind::Symbol(symbol) if symbol.kind() == acdc_parser::SymbolKind::Copyright),
        "p" => matches!(node.kind(), InlineKind::LineBreak(_)),
        _ => false,
    }).count()
}

#[rstest::rstest]
#[case("q", "*Bold*")]
#[case("m", "https://example.org")]
#[case("r", "(C)")]
#[case("p", "first +\nsecond")]
fn formatted_values_apply_stages_in_declared_order(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[case] stage: &str,
    #[case] input: &str,
) -> Result<(), Error> {
    let source = format!(
        "= T\n:first: pass:{stage},a[{{input}}]\n:second: pass:a,{stage}[{{input}}]\n\n:input: Changed\n\n{{first}}\n\n{{second}}\n"
    );
    let parsed = parse(
        &source,
        &Options::builder()
            .with_compatibility_mode(mode)
            .with_default_attribute("input", input)
            .build()?,
    )?;
    let paragraphs: Vec<_> = parsed
        .document()
        .blocks()
        .filter_map(|block| {
            let BlockKind::Paragraph(paragraph) = block.kind() else {
                return None;
            };
            Some(paragraph)
        })
        .collect();
    assert_eq!(paragraphs.len(), 2);
    assert_eq!(
        active_stage(
            paragraphs
                .first()
                .ok_or("missing first paragraph")?
                .content(),
            stage
        ),
        0
    );
    assert_eq!(
        active_stage(
            paragraphs
                .last()
                .ok_or("missing second paragraph")?
                .content(),
            stage
        ),
        1
    );
    assert_eq!(
        parsed
            .document()
            .attributes()
            .get("first")
            .and_then(acdc_parser::DocumentAttributeValue::text),
        Some(input)
    );
    assert_eq!(parsed.warnings(), []);
    Ok(())
}

#[rstest::rstest]
fn formatted_values_preserve_macro_stage_bodies_without_unused_registrations(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        include_str!("../../fixtures/tests/compatibility_formatted_attribute_snapshots.adoc"),
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let [note] = &*parsed.document().footnotes().collect::<Vec<_>>() else {
        return Err("expected only the used named note".into());
    };
    let text = note
        .content()
        .inlines()
        .filter_map(|node| {
            if let InlineKind::Plain(text) = node.kind() {
                Some(text.text())
            } else if let InlineKind::Raw(text) = node.kind() {
                Some(text.text())
            } else {
                None
            }
        })
        .collect::<String>();
    assert_eq!(text, "{name}");
    assert!(!parsed.document().references().contains("unused"));
    assert_eq!(parsed.warnings(), []);
    Ok(())
}
