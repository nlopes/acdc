use acdc_parser::{
    CompatibilityMode, Options, SubstitutionSpec, WarningKind,
    ast::{AttributeValueRef, BlockKind, BlockRef, InlineKind, InlineRef, SubstitutionSpecRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
#[case::asciidoctor(CompatibilityMode::Asciidoctor)]
#[case::draft(CompatibilityMode::Draft)]
fn supported_block_substitutions_do_not_warn(#[case] mode: CompatibilityMode) -> Result<(), Error> {
    let parsed = parse(
        include_str!("../../fixtures/tests/compatibility_block_substitutions.adoc"),
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    assert!(parsed.source_recovery().is_none());
    assert_eq!(parsed.document().footnotes().count(), 0);
    Ok(())
}

#[rstest::rstest]
#[case::asciidoctor(CompatibilityMode::Asciidoctor)]
#[case::draft(CompatibilityMode::Draft)]
fn unsupported_substitutions_preserve_source_and_report_recovery(
    #[case] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = include_str!("../../fixtures/tests/compatibility_unsupported_substitutions.adoc");
    let mut parsed = parse(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let warnings = parsed.take_warnings();
    assert_eq!(warnings.len(), 4, "{warnings:?}");
    for warning in warnings {
        assert!(matches!(warning.kind, WarningKind::ContentRecovery { .. }));
        assert!(warning.source_location().is_some());
    }
    assert!(parsed.source_recovery().is_some());
    assert_eq!(parsed.preprocessed_source(), source.trim_end());
    assert_eq!(parsed.document().footnotes().count(), 0);
    let [
        BlockKind::Paragraph(first),
        BlockKind::Paragraph(second),
        BlockKind::Stem(stem),
    ] = &*parsed
        .document()
        .blocks()
        .map(BlockRef::kind)
        .collect::<Vec<_>>()
    else {
        return Err("expected two paragraphs and a STEM block".into());
    };
    assert_eq!(
        first.attributes().get("subs"),
        Some(AttributeValueRef::Substitutions(SubstitutionSpecRef::from(
            &SubstitutionSpec::Source(vec!["header".into(), "+quotes".into(), "typo".into()])
        )))
    );
    assert!(matches!(
        first.content().inlines().next().map(InlineRef::kind),
        Some(InlineKind::Bold(_))
    ));
    assert!(
        matches!(&*second.content().inlines().map(InlineRef::kind).collect::<Vec<_>>(), [InlineKind::Plain(text)]
        if text.text() == "*Literal formatting* expanded.")
    );
    assert_eq!(stem.text(), "expanded *x* footnote:[keep the source]");
    assert_eq!(
        stem.source_text(),
        Some("{value} *x* footnote:[keep the source]\n")
    );
    Ok(())
}
