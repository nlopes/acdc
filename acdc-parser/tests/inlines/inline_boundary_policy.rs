use acdc_parser::{
    CompatibilityMode, Options,
    ast::{BlockKind, BlockRef, InlineKind},
    parse,
};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
fn escaped_macros_keep_their_catalogs_inactive(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        include_str!("../../fixtures/tests/compatibility_inline_escapes.adoc"),
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let [note] = &*parsed.document().footnotes().collect::<Vec<_>>() else {
        return Err("expected only the active note".into());
    };
    assert_eq!(note.id(), Some("active"));
    assert!(!parsed.document().references().contains("inactive-anchor"));
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    Ok(())
}

#[rstest::rstest]
#[case("é*word*", false)]
#[case("日*word*", false)]
#[case("word*word*", false)]
#[case("_*word*_", false)]
#[case("“*Bold*”", true)]
#[case("́*Mark*", true)]
#[case("*Mark*́", true)]
fn constrained_boundaries_keep_the_shared_character_policy(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[case] source: &str,
    #[case] expected: bool,
) -> Result<(), Error> {
    let parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let [BlockKind::Paragraph(paragraph)] = &*parsed
        .document()
        .blocks()
        .map(BlockRef::kind)
        .collect::<Vec<_>>()
    else {
        return Err("expected one paragraph".into());
    };
    let bold = paragraph
        .content()
        .inlines()
        .find(|node| matches!(node.kind(), InlineKind::Bold(_)));
    assert_eq!(bold.is_some(), expected);
    if let Some(bold) = bold {
        let authored = source
            .get(bold.location().absolute_start()..=bold.location().absolute_end())
            .ok_or("invalid bold source range")?;
        assert!(authored.starts_with('*') && authored.ends_with('*'));
    }
    Ok(())
}
