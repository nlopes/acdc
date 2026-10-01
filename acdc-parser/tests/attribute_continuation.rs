use acdc_parser::{Block, DocumentAttributeValue, InlineNode, Options, parse};

type Error = Box<dyn std::error::Error>;

fn text(nodes: &[InlineNode<'_>]) -> Result<String, Error> {
    nodes
        .iter()
        .map(|node| {
            if let InlineNode::PlainText(text) = node {
                Ok(text.content)
            } else if let InlineNode::RawText(text) = node {
                Ok(text.content)
            } else {
                Err(format!("unexpected text node: {node:?}").into())
            }
        })
        .collect()
}

// Fixture JSON omits header attribute values. Force a conditional pass too,
// so both preprocessing paths preserve the same declaration.
#[test]
fn attribute_continuation_preserves_literal_backslashes_and_following_source() -> Result<(), Error>
{
    for separator in [" ", "   ", "\t", " \t "] {
        for newline in ["\n", "\r\n"] {
            for force_preprocessing in [false, true] {
                let directives = if force_preprocessing {
                    format!("ifdef::value[]{newline}endif::[]{newline}")
                } else {
                    String::new()
                };
                let source = format!(
                    "= Attributes{newline}:value:{separator}\\{newline}{directives}{newline}é [{{value}}].{newline}"
                );
                let parsed = parse(&source, &Options::default())?;
                assert_eq!(
                    parsed
                        .document()
                        .attributes
                        .get("value")
                        .and_then(DocumentAttributeValue::text),
                    Some("\\"),
                    "{source:?}"
                );
                let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
                    return Err(format!("unexpected blocks: {:?}", parsed.document().blocks).into());
                };
                assert_eq!(text(&paragraph.content)?, "é [\\].");
                let expected_line = if force_preprocessing { 6 } else { 4 };
                assert_eq!(paragraph.location.start.line, expected_line);
                assert_eq!(paragraph.location.start.column, 1);
                assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
            }
        }
    }
    Ok(())
}

#[test]
fn attribute_continuation_preserves_a_literal_backslash_at_eof() -> Result<(), Error> {
    for separator in [" ", "   ", "\t", " \t "] {
        let source = format!(":value:{separator}\\");
        let parsed = parse(&source, &Options::default())?;
        assert_eq!(
            parsed
                .document()
                .attributes
                .get("value")
                .and_then(DocumentAttributeValue::text),
            Some("\\")
        );
        assert!(parsed.document().blocks.is_empty());
    }
    Ok(())
}

#[test]
fn attribute_continuation_preserves_caller_locks_and_does_not_swallow_body_text()
-> Result<(), Error> {
    let source = ":value: \\\nFollowing paragraph.\n\nValue [{value}].\n";
    for (options, expected) in [
        (Options::default(), "Value [\\]."),
        (
            Options::builder()
                .with_attribute("value", "caller")
                .build()?,
            "Value [caller].",
        ),
    ] {
        let parsed = parse(source, &options)?;
        let [Block::Paragraph(first), Block::Paragraph(second)] =
            parsed.document().blocks.as_slice()
        else {
            return Err("expected two paragraphs after the declaration".into());
        };
        assert_eq!(text(&first.content)?, "Following paragraph.");
        assert_eq!(text(&second.content)?, expected);
        assert_eq!(first.location.start.line, 2);
        assert_eq!(second.location.start.line, 4);
    }
    Ok(())
}

#[test]
fn attribute_continuation_keeps_colon_prefixed_lines_in_the_value() -> Result<(), Error> {
    let source = "= Attributes\n:value: First \\\n:other: Still value text\n\nAfter [{value}].\n";
    let parsed = parse(source, &Options::default())?;
    assert_eq!(
        parsed
            .document()
            .attributes
            .get("value")
            .and_then(DocumentAttributeValue::text),
        Some("First :other: Still value text")
    );
    assert!(!parsed.document().attributes.contains_key("other"));
    let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
        return Err("expected only the paragraph following the continued declaration".into());
    };
    assert_eq!(
        text(&paragraph.content)?,
        "After [First :other: Still value text]."
    );
    assert_eq!(paragraph.location.start.line, 5);
    Ok(())
}
