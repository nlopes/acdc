use acdc_parser::{
    Options,
    ast::{BlockKind, BlockRef, InlineKind, InlineRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

#[test]
fn escaped_attribute_references_keep_complete_source_spans() -> Result<(), Error> {
    for escaped in [r"\{value}", r"{value\}", r"\{value\}"] {
        for newline in ["\n", "\r\n"] {
            let source = format!("= T{newline}:value: *Bold*{newline}{newline}α {escaped} ω.");
            let parsed = parse(&source, &Options::default())?;
            // Byte offsets refer to the grammar input after line-ending normalization.
            let source = parsed.source();
            let [BlockKind::Paragraph(paragraph)] = &*parsed
                .document()
                .blocks()
                .map(BlockRef::kind)
                .collect::<Vec<_>>()
            else {
                return Err("expected paragraph".into());
            };
            let raw = paragraph
                .content()
                .inlines()
                .find_map(|node| {
                    if let InlineKind::Raw(raw) = node.kind() {
                        Some(raw)
                    } else {
                        None
                    }
                })
                .ok_or("missing literal reference")?;
            assert_eq!(raw.text(), "{value}");
            assert_eq!(
                &source[raw.location().absolute_start()..=raw.location().absolute_end()],
                escaped,
                "raw={raw:?}; source={source:?}"
            );
            assert_eq!(raw.location().start().line(), 4);
            assert_eq!(raw.location().end().line(), 4);
            assert_eq!(raw.location().start().column(), 3);
            assert_eq!(
                raw.location().end().column(),
                u32::try_from(escaped.len())? + 2
            );
            let Some(InlineKind::Plain(tail)) =
                paragraph.content().inlines().last().map(InlineRef::kind)
            else {
                return Err("expected trailing text".into());
            };
            let last = source
                .get(tail.location().absolute_end()..)
                .and_then(|text| text.chars().next())
                .ok_or("invalid trailing endpoint")?;
            assert_eq!(
                &source[tail.location().absolute_start()
                    ..tail.location().absolute_end() + last.len_utf8()],
                tail.text()
            );
        }
    }
    Ok(())
}

#[test]
fn escaped_attribute_values_do_not_register_their_macros() -> Result<(), Error> {
    let parsed = parse(
        include_str!("../../fixtures/tests/attribute_brace_escapes.adoc"),
        &Options::default(),
    )?;
    assert_eq!(parsed.document().footnotes().count(), 1);
    assert_eq!(
        parsed
            .document()
            .footnotes()
            .next()
            .and_then(acdc_parser::ast::FootnoteDefinitionRef::id),
        Some("n")
    );
    let mut terms = 0;
    for block in parsed.document().blocks() {
        if let BlockKind::Section(section) = block.kind() {
            for block in section.blocks() {
                if let BlockKind::Paragraph(paragraph) = block.kind() {
                    for node in paragraph.content().inlines() {
                        if let InlineKind::IndexTerm(term) = node.kind() {
                            assert!(term.term().inlines().all(|node| matches!(node.kind(),
                                InlineKind::Raw(raw) if raw.text() == "{value}")));
                            terms += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(terms, 3);
    Ok(())
}
