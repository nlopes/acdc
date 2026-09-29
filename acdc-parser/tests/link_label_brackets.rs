use acdc_parser::{Block, InlineMacro, InlineNode, Options, parse};

type Error = Box<dyn std::error::Error>;

fn label_text(nodes: &[InlineNode<'_>]) -> Result<String, Error> {
    nodes
        .iter()
        .map(|node| {
            if let InlineNode::PlainText(text) = node {
                Ok(text.content)
            } else if let InlineNode::RawText(text) = node {
                Ok(text.content)
            } else {
                Err(format!("unexpected label node: {node:?}").into())
            }
        })
        .collect()
}

#[test]
fn link_label_brackets_remove_one_escape_and_keep_source_spans() -> Result<(), Error> {
    for prefix in [
        "link:https://example.org",
        "https://example.org",
        "mailto:test@example.org",
    ] {
        for count in 1..=4 {
            let escapes = "\\".repeat(count);
            let source = format!("α {prefix}[é {escapes}] café] ω.\n");
            let parsed = parse(&source, &Options::default())?;
            let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
                return Err("expected paragraph".into());
            };
            let node = paragraph
                .content
                .iter()
                .find(|node| matches!(node, InlineNode::Macro(_)))
                .ok_or("missing link")?;
            let text = if let InlineNode::Macro(InlineMacro::Link(link)) = node {
                &link.text
            } else if let InlineNode::Macro(InlineMacro::Url(url)) = node {
                &url.text
            } else if let InlineNode::Macro(InlineMacro::Mailto(mailto)) = node {
                &mailto.text
            } else {
                return Err("unexpected macro".into());
            };
            assert_eq!(
                label_text(text)?,
                format!("é {}] café", "\\".repeat(count - 1)),
                "{source}"
            );
            let bracket = text
                .iter()
                .find_map(|node| {
                    if let InlineNode::RawText(raw) = node {
                        Some(raw)
                    } else {
                        None
                    }
                })
                .ok_or("missing escaped bracket")?;
            assert_eq!(
                &source[bracket.location.absolute_start..=bracket.location.absolute_end],
                format!("{escapes}]")
            );
            for node in text {
                let location = node.location();
                for (offset, position) in [
                    (location.absolute_start, &location.start),
                    (location.absolute_end, &location.end),
                ] {
                    assert!(source.is_char_boundary(offset));
                    assert_eq!(position.line, 1);
                    assert_eq!(
                        position.column as usize,
                        source[..offset].chars().count() + 1
                    );
                }
            }
        }
    }
    Ok(())
}

#[test]
fn link_label_brackets_keep_quoted_source_spans() -> Result<(), Error> {
    for quote in ['\'', '"'] {
        let source = format!("α link:https://example.org[{quote}é \\] café{quote},role=test] ω.\n");
        let parsed = parse(&source, &Options::default())?;
        let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
            return Err("expected paragraph".into());
        };
        let link = paragraph
            .content
            .iter()
            .find_map(|node| {
                if let InlineNode::Macro(InlineMacro::Link(link)) = node {
                    Some(link)
                } else {
                    None
                }
            })
            .ok_or("missing link")?;
        assert_eq!(label_text(&link.text)?, "é ] café");
        let first = link.text.first().ok_or("missing label")?.location();
        assert_eq!(
            first.absolute_start,
            source.find('é').ok_or("missing source label")?
        );
        let last = link.text.last().ok_or("missing label")?.location();
        let end = source.find("café").ok_or("missing source tail")? + "caf".len();
        assert_eq!(last.absolute_end, end);
        assert_eq!(last.end.column as usize, source[..end].chars().count() + 1);
    }
    Ok(())
}

#[test]
fn link_label_brackets_keep_multiline_passthrough_source_spans() -> Result<(), Error> {
    for passthrough in [false, true] {
        let source = if passthrough {
            "α pass:m[link:https://example.org[é \\\\]\ncafé\\]] ω.\n"
        } else {
            "α link:https://example.org[é \\]\ncafé] ω.\n"
        };
        let parsed = parse(source, &Options::default())?;
        let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
            return Err("expected paragraph".into());
        };
        let link = paragraph
            .content
            .iter()
            .find_map(|node| {
                if let InlineNode::Macro(InlineMacro::Link(link)) = node {
                    Some(link)
                } else {
                    None
                }
            })
            .ok_or("missing link")?;
        assert_eq!(label_text(&link.text)?, "é ]\ncafé");
        let bracket = link
            .text
            .iter()
            .find_map(|node| {
                if let InlineNode::RawText(raw) = node
                    && raw.content == "]"
                {
                    Some(raw)
                } else {
                    None
                }
            })
            .ok_or("missing escaped bracket")?;
        assert_eq!(
            &source[bracket.location.absolute_start..=bracket.location.absolute_end],
            if passthrough { r"\\]" } else { r"\]" }
        );
        let tail = link.text.last().ok_or("missing label tail")?.location();
        assert_eq!((tail.end.line, tail.end.column), (2, 4));
        assert_eq!(
            tail.absolute_end,
            source.find("café").ok_or("missing source tail")? + 3
        );
    }
    Ok(())
}
