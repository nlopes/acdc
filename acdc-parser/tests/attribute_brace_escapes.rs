use acdc_parser::{Block, InlineMacro, InlineNode, Options, parse};

type Error = Box<dyn std::error::Error>;

#[test]
fn escaped_attribute_references_keep_complete_source_spans() -> Result<(), Error> {
    for escaped in [r"\{value}", r"{value\}", r"\{value\}"] {
        for newline in ["\n", "\r\n"] {
            let source = format!("= T{newline}:value: *Bold*{newline}{newline}α {escaped} ω.");
            let parsed = parse(&source, &Options::default())?;
            // Byte offsets refer to the grammar input after line-ending normalization.
            let source = parsed.source();
            let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
                return Err("expected paragraph".into());
            };
            let raw = paragraph
                .content
                .iter()
                .find_map(|node| {
                    if let InlineNode::RawText(raw) = node {
                        Some(raw)
                    } else {
                        None
                    }
                })
                .ok_or("missing literal reference")?;
            assert_eq!(raw.content, "{value}");
            assert_eq!(
                &source[raw.location.absolute_start..=raw.location.absolute_end],
                escaped,
                "raw={raw:?}; source={source:?}"
            );
            assert_eq!(raw.location.start.line, 4);
            assert_eq!(raw.location.end.line, 4);
            assert_eq!(raw.location.start.column, 3);
            assert_eq!(raw.location.end.column, u32::try_from(escaped.len())? + 2);
            let Some(InlineNode::PlainText(tail)) = paragraph.content.last() else {
                return Err("expected trailing text".into());
            };
            let last = source
                .get(tail.location.absolute_end..)
                .and_then(|text| text.chars().next())
                .ok_or("invalid trailing endpoint")?;
            assert_eq!(
                &source[tail.location.absolute_start..tail.location.absolute_end + last.len_utf8()],
                tail.content
            );
        }
    }
    Ok(())
}

#[test]
fn escaped_attribute_values_do_not_register_their_macros() -> Result<(), Error> {
    let parsed = parse(
        include_str!("../fixtures/tests/attribute_brace_escapes.adoc"),
        &Options::default(),
    )?;
    assert_eq!(parsed.document().footnotes.len(), 1);
    assert_eq!(
        parsed.document().footnotes.first().and_then(|note| note.id),
        Some("n")
    );
    let mut terms = 0;
    for block in &parsed.document().blocks {
        if let Block::Section(section) = block {
            for block in &section.content {
                if let Block::Paragraph(paragraph) = block {
                    for node in &paragraph.content {
                        if let InlineNode::Macro(InlineMacro::IndexTerm(term)) = node {
                            assert!(term.term().iter().all(|node| matches!(node,
                                InlineNode::RawText(raw) if raw.content == "{value}")));
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
