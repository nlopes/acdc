use acdc_parser::{CompatibilityMode, Error, Options, WarningKind, parse};

type TestError = Box<dyn std::error::Error>;

#[rstest::rstest]
fn trailing_macro_text_remains_a_literal_paragraph(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values(
        "image::diagram.png[]",
        "audio::sound.wav[]",
        "video::movie.mp4[]",
        "toc::[]"
    )]
    block: &str,
    #[values(" discarded λ", "suffix")] tail: &str,
) -> Result<(), TestError> {
    let source = format!("Earlier.\n\n{block}{tail}");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert!(parsed.source_recovery().is_none());
    assert_eq!(parsed.warnings(), []);
    let parsed_block = parsed
        .document_ref()
        .blocks()
        .nth(1)
        .ok_or("missing second block")?;
    let acdc_parser::ast::BlockKind::Paragraph(paragraph) = parsed_block.kind() else {
        return Err("malformed block macro must remain a paragraph".into());
    };
    let text = paragraph
        .content()
        .inlines()
        .map(|node| {
            let acdc_parser::ast::InlineKind::Plain(text) = node.kind() else {
                return Err("malformed block macro must stay plain text");
            };
            Ok(text.text())
        })
        .collect::<Result<String, &str>>()?;
    assert_eq!(text, format!("{block}{tail}"));
    Ok(())
}

#[rstest::rstest]
fn whitespace_after_complete_macros_remains_valid(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values(
        "image::diagram.png[]",
        "audio::sound.wav[]",
        "video::movie.mp4[]",
        "toc::[]"
    )]
    source: &str,
    #[values(" \t", " \t\n")] whitespace: &str,
) -> Result<(), TestError> {
    let source = format!("{source}{whitespace}");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert!(parsed.source_recovery().is_none());
    assert_eq!(parsed.warnings(), []);
    let block = parsed
        .document_ref()
        .blocks()
        .next()
        .ok_or("missing complete macro")?;
    assert!(!matches!(
        block.kind(),
        acdc_parser::ast::BlockKind::Paragraph(_)
    ));
    Ok(())
}

#[rstest::rstest]
fn unresolved_xrefs_keep_their_structure_without_source_recovery(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), TestError> {
    let parsed = parse(
        "See <<missing>>.",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    assert!(
        parsed
            .warnings()
            .iter()
            .any(|warning| matches!(warning.kind, WarningKind::UnresolvedReference { .. }))
    );
    assert!(parsed.source_recovery().is_none());
    Ok(())
}

#[rstest::rstest]
fn malformed_conditional_errors_remain_fatal(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), TestError> {
    let result = parse(
        "ifdef::a[]\nBody.\nendif::b[]",
        &Options::builder().with_compatibility_mode(mode).build()?,
    );
    assert!(matches!(result, Err(Error::InvalidConditionalDirective(_))));
    Ok(())
}
