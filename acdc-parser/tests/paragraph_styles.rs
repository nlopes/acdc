use acdc_parser::{
    CompatibilityMode, Options,
    ast::{BlockKind, BlockRef, ElementIdRef, InlineKind, InlineRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
fn pass_paragraphs_retain_raw_source_without_registering_macros(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values("", "  ")] indent: &str,
) -> Result<(), Error> {
    let body =
        format!("{indent}<b>*Raw* {{name}} footnote:n[Hidden]</b>\n{indent}// raw slash line");
    let source = format!(":name: value\n\n[pass]\n{body}");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let Some(BlockKind::Paragraph(block)) =
        parsed.document_ref().blocks().last().map(BlockRef::kind)
    else {
        return Err("expected paragraph source form".into());
    };
    let [InlineKind::Raw(raw)] = &*block
        .content()
        .inlines()
        .map(InlineRef::kind)
        .collect::<Vec<_>>()
    else {
        return Err("expected raw content".into());
    };
    let expected = if indent.is_empty() {
        body.split('\n').next().ok_or("missing body")?
    } else {
        &body
    };
    assert_eq!(raw.text(), expected);
    assert_eq!(raw.substitutions().collect::<Vec<_>>(), []);
    assert_eq!(
        &source[raw.location().absolute_start()..=raw.location().absolute_end()],
        expected
    );
    assert!(parsed.document_ref().footnotes().next().is_none());
    assert_eq!(block.source_text(), Some(expected));
    Ok(())
}

#[rstest::rstest]
fn simple_admonition_metadata_has_one_owner(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
    #[values(
        "[NOTE#owner.role,subs=none]\nBody *raw*",
        "[#owner.role,subs=none]\nNOTE: Body *raw*"
    )]
    body: &str,
) -> Result<(), Error> {
    let source = format!(".Title\n{body}");
    let parsed = parse(
        &source,
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let [BlockKind::Admonition(block)] = &*parsed
        .document_ref()
        .blocks()
        .map(BlockRef::kind)
        .collect::<Vec<_>>()
    else {
        return Err("expected admonition".into());
    };
    assert_eq!(block.id().map(ElementIdRef::as_str), Some("owner"));
    let [BlockKind::Paragraph(paragraph)] =
        &*block.blocks().map(BlockRef::kind).collect::<Vec<_>>()
    else {
        return Err("expected one body".into());
    };
    assert!(paragraph.id().is_none());
    assert_eq!(
        paragraph.roles().iter().collect::<Vec<_>>(),
        [] as [&str; 0]
    );
    assert!(paragraph.style().is_none());
    assert_eq!(
        paragraph.attributes().get("subs"),
        block.attributes().get("subs")
    );
    assert!(paragraph.title().is_none());
    Ok(())
}

#[rstest::rstest]
fn admonition_reads_distinguish_simple_and_compound_bodies(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    for (source, simple) in [
        ("NOTE: Body", true),
        ("[NOTE]\nBody", true),
        ("[NOTE]\n--\nBody\n--", false),
        ("[NOTE]\n====\nBody\n====", false),
    ] {
        let parsed = parse(
            source,
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let block = parsed
            .document_ref()
            .blocks()
            .next()
            .ok_or("missing block")?;
        let BlockKind::Admonition(admonition) = block.kind() else {
            return Err("expected admonition".into());
        };
        assert_eq!(admonition.simple_paragraph().is_some(), simple);
    }
    Ok(())
}
