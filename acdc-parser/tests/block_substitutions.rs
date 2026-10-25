use acdc_parser::{
    Block, CompatibilityMode, DelimitedBlockType, InlineNode, Options, SubstitutionSpec,
    WarningKind, parse,
};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
#[case::asciidoctor(CompatibilityMode::Asciidoctor)]
#[case::draft(CompatibilityMode::Draft)]
fn supported_block_substitutions_do_not_warn(#[case] mode: CompatibilityMode) -> Result<(), Error> {
    let parsed = parse(
        include_str!("../fixtures/tests/compatibility_block_substitutions.adoc"),
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
    assert!(parsed.source_recovery().is_none());
    assert_eq!(parsed.document().footnotes.len(), 0);
    Ok(())
}

#[rstest::rstest]
#[case::asciidoctor(CompatibilityMode::Asciidoctor)]
#[case::draft(CompatibilityMode::Draft)]
fn unsupported_substitutions_preserve_source_and_report_recovery(
    #[case] mode: CompatibilityMode,
) -> Result<(), Error> {
    let source = include_str!("../fixtures/tests/compatibility_unsupported_substitutions.adoc");
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
    assert_eq!(parsed.source(), source.trim_end());
    assert_eq!(parsed.document().footnotes.len(), 0);
    let [
        Block::Paragraph(first),
        Block::Paragraph(second),
        Block::DelimitedBlock(stem),
    ] = parsed.document().blocks.as_slice()
    else {
        return Err("expected two paragraphs and a STEM block".into());
    };
    assert_eq!(
        first.metadata.substitutions,
        Some(SubstitutionSpec::Source(vec![
            "header".into(),
            "+quotes".into(),
            "typo".into()
        ]))
    );
    assert!(matches!(
        first.content.first(),
        Some(InlineNode::BoldText(_))
    ));
    assert!(
        matches!(second.content.as_slice(), [InlineNode::PlainText(text)]
        if text.content == "*Literal formatting* expanded.")
    );
    let DelimitedBlockType::DelimitedStem(formula) = &stem.inner else {
        return Err("expected STEM content".into());
    };
    assert_eq!(formula.content, "expanded *x* footnote:[keep the source]");
    assert_eq!(
        stem.source_text(),
        Some("{value} *x* footnote:[keep the source]\n")
    );
    Ok(())
}
