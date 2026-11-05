use acdc_parser::{
    CompatibilityMode, InlineMacro, InlineNode, Options,
    ast::{BlockKind, BlockRef},
    parse, parse_inline,
};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
#[case(include_str!("../fixtures/tests/compatibility_standalone_inlines.adoc"), &[1, 1])]
#[case(include_str!("../fixtures/tests/compatibility_standalone_inline_recovery.adoc"), &[])]
fn standalone_fragments_use_document_inline_semantics(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[case] source: &str,
    #[case] expected_footnotes: &[u32],
) -> Result<(), Error> {
    let source = source.trim_end_matches('\n');
    let options = Options::builder().with_compatibility_mode(mode).build()?;
    let document = parse(source, &options)?;
    let [BlockKind::Paragraph(_)] = &*document
        .document()
        .blocks()
        .map(BlockRef::kind)
        .collect::<Vec<_>>()
    else {
        return Err("expected one paragraph".into());
    };
    let fragment = parse_inline(source, &options)?;
    let fragment_inlines = acdc_parser::ContentAttribute::from(fragment.content()).into_inlines();
    assert_eq!(
        &serde_json::to_value(fragment.content())?,
        serde_json::to_value(document.document())?
            .pointer("/blocks/0/inlines")
            .ok_or("missing paragraph inlines")?
    );
    assert_eq!(fragment.warnings(), document.warnings());
    let footnotes: Vec<_> = fragment_inlines
        .as_slice()
        .iter()
        .filter_map(|node| {
            if let InlineNode::Macro(InlineMacro::Footnote(note)) = node {
                Some(acdc_parser::ast::FootnoteRef::from(note).number())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(footnotes, expected_footnotes);
    Ok(())
}

#[rstest::rstest]
#[case("")]
#[case(" ")]
#[case("\n")]
#[case(" \t\n π \n \t")]
fn standalone_fragments_preserve_surrounding_whitespace(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[case] source: &str,
) -> Result<(), Error> {
    let parsed = parse_inline(
        source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let parsed_inlines = acdc_parser::ContentAttribute::from(parsed.content()).into_inlines();
    let mut text = String::new();
    for node in parsed_inlines.as_slice() {
        let InlineNode::PlainText(plain) = node else {
            return Err("expected literal whitespace".into());
        };
        text.push_str(acdc_parser::ast::PlainRef::from(plain).text());
        assert_eq!(
            &source[acdc_parser::ast::PlainRef::from(plain)
                .location()
                .absolute_start()
                ..=acdc_parser::ast::PlainRef::from(plain)
                    .location()
                    .absolute_end()],
            acdc_parser::ast::PlainRef::from(plain).text()
        );
    }
    assert_eq!(text, source);
    Ok(())
}

#[rstest::rstest]
fn standalone_attribute_expansion_obeys_the_selected_mode(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let options = Options::builder()
        .with_compatibility_mode(mode)
        .with_attribute("word", "*selected*")
        .build()?;
    let source = "π {word} (C)";
    let parsed = parse_inline(source, &options)?;
    let parsed_inlines = acdc_parser::ContentAttribute::from(parsed.content()).into_inlines();
    assert!(
        parsed_inlines
            .as_slice()
            .iter()
            .any(|node| matches!(node, InlineNode::Symbol(_)))
    );
    let bold = parsed_inlines
        .as_slice()
        .iter()
        .any(|node| matches!(node, InlineNode::BoldText(_)));
    assert_eq!(bold, mode == CompatibilityMode::Draft);
    for node in parsed_inlines.as_slice() {
        let location = node.location();
        assert!(location.absolute_start <= location.absolute_end);
        assert!(location.absolute_end < source.len());
    }
    Ok(())
}
