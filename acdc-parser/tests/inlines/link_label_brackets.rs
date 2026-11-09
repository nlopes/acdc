use acdc_parser::{
    Options,
    ast::{BlockKind, BlockRef, ContentRef, InlineKind},
    parse,
};

type Error = Box<dyn std::error::Error>;

fn label_text(nodes: ContentRef<'_>) -> Result<String, Error> {
    nodes
        .inlines()
        .map(|node| {
            if let InlineKind::Plain(text) = node.kind() {
                Ok(text.text())
            } else if let InlineKind::Raw(text) = node.kind() {
                Ok(text.text())
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
            let [BlockKind::Paragraph(paragraph)] = &*parsed
                .document()
                .blocks()
                .map(BlockRef::kind)
                .collect::<Vec<_>>()
            else {
                return Err("expected paragraph".into());
            };
            let nodes = paragraph.content().inlines().collect::<Vec<_>>();
            let [prefix, node, suffix] = nodes.as_slice() else {
                return Err(format!("expected one link between text: {nodes:?}").into());
            };
            assert!(matches!(prefix.kind(), InlineKind::Plain(_)));
            assert!(matches!(suffix.kind(), InlineKind::Plain(_)));
            let text = if let InlineKind::Link(link) = node.kind() {
                link.text()
            } else if let InlineKind::Url(url) = node.kind() {
                url.text()
            } else if let InlineKind::Mailto(mailto) = node.kind() {
                mailto.text()
            } else {
                return Err("unexpected macro".into());
            };
            assert_eq!(
                label_text(text)?,
                format!("é {}] café", "\\".repeat(count - 1)),
                "{source}"
            );
            let bracket = text
                .inlines()
                .find_map(|node| {
                    if let InlineKind::Raw(raw) = node.kind() {
                        Some(raw)
                    } else {
                        None
                    }
                })
                .ok_or("missing escaped bracket")?;
            assert_eq!(
                &source[bracket.location().absolute_start()..=bracket.location().absolute_end()],
                format!("{escapes}]")
            );
            for node in text.inlines() {
                let location = node.location();
                for (offset, position) in [
                    (location.absolute_start(), location.start()),
                    (location.absolute_end(), location.end()),
                ] {
                    assert!(source.is_char_boundary(offset));
                    assert_eq!(position.line(), 1);
                    assert_eq!(
                        position.column() as usize,
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
        let [BlockKind::Paragraph(paragraph)] = &*parsed
            .document()
            .blocks()
            .map(BlockRef::kind)
            .collect::<Vec<_>>()
        else {
            return Err("expected paragraph".into());
        };
        let link = paragraph
            .content()
            .inlines()
            .find_map(|node| {
                if let InlineKind::Link(link) = node.kind() {
                    Some(link)
                } else {
                    None
                }
            })
            .ok_or("missing link")?;
        assert_eq!(label_text(link.text())?, "é ] café");
        let first = link
            .text()
            .inlines()
            .next()
            .ok_or("missing label")?
            .location();
        assert_eq!(
            first.absolute_start(),
            source.find('é').ok_or("missing source label")?
        );
        let last = link
            .text()
            .inlines()
            .last()
            .ok_or("missing label")?
            .location();
        let end = source.find("café").ok_or("missing source tail")? + "caf".len();
        assert_eq!(last.absolute_end(), end);
        assert_eq!(
            last.end().column() as usize,
            source[..end].chars().count() + 1
        );
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
        let [BlockKind::Paragraph(paragraph)] = &*parsed
            .document()
            .blocks()
            .map(BlockRef::kind)
            .collect::<Vec<_>>()
        else {
            return Err("expected paragraph".into());
        };
        let link = paragraph
            .content()
            .inlines()
            .find_map(|node| {
                if let InlineKind::Link(link) = node.kind() {
                    Some(link)
                } else {
                    None
                }
            })
            .ok_or("missing link")?;
        assert_eq!(label_text(link.text())?, "é ]\ncafé");
        let bracket = link
            .text()
            .inlines()
            .find_map(|node| {
                if let InlineKind::Raw(raw) = node.kind()
                    && raw.text() == "]"
                {
                    Some(raw)
                } else {
                    None
                }
            })
            .ok_or("missing escaped bracket")?;
        assert_eq!(
            &source[bracket.location().absolute_start()..=bracket.location().absolute_end()],
            if passthrough { r"\\]" } else { r"\]" }
        );
        let tail = link
            .text()
            .inlines()
            .last()
            .ok_or("missing label tail")?
            .location();
        assert_eq!((tail.end().line(), tail.end().column()), (2, 4));
        assert_eq!(
            tail.absolute_end(),
            source.find("café").ok_or("missing source tail")? + 3
        );
    }
    Ok(())
}
