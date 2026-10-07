use acdc_parser::{Block, InlineMacro, InlineNode, Options, WarningKind, parse, parse_inline};

type Error = Box<dyn std::error::Error>;

fn formatted_content<'a>(node: &'a InlineNode<'_>, marker: &str) -> Option<&'a [InlineNode<'a>]> {
    match (marker, node) {
        ("``" | "`", InlineNode::MonospaceText(span)) => Some(&span.content),
        ("**" | "*", InlineNode::BoldText(span)) => Some(&span.content),
        ("__" | "_", InlineNode::ItalicText(span)) => Some(&span.content),
        ("##" | "#", InlineNode::HighlightText(span)) => Some(&span.content),
        _ => None,
    }
}

#[test]
fn formatting_preserves_whitespace_and_its_source_span() -> Result<(), Error> {
    for marker in ["``", "**", "__", "##"] {
        for (body, expected) in [
            (" ", " "),
            ("  ", "  "),
            ("\t", "\t"),
            ("\n", "\n"),
            ("\u{a0}", "\u{a0}"),
            ("\u{2003}", "\u{2003}"),
            ("{sp}", " "),
            ("{sp}{sp}", "  "),
            ("{nbsp}", "\u{a0}"),
        ] {
            for newline in ["\n", "\r\n"] {
                let source = format!("α A{marker}{body}{marker}B.\n").replace('\n', newline);
                let parsed = parse(&source, &Options::default())?;
                let Some(Block::Paragraph(paragraph)) = parsed.document().blocks.first() else {
                    return Err("expected a paragraph".into());
                };
                let span = paragraph
                    .content
                    .iter()
                    .find(|node| formatted_content(node, marker).is_some())
                    .ok_or("missing formatted span")?;
                let nodes = formatted_content(span, marker).ok_or("missing content")?;
                let [InlineNode::PlainText(text)] = nodes else {
                    return Err(
                        format!("expected one whitespace node: {source:?}: {nodes:?}").into(),
                    );
                };
                assert_eq!(text.content, expected, "{source:?}");
                let start = parsed.source().find(marker).ok_or("missing marker")? + marker.len();
                let end = start + body.char_indices().last().ok_or("empty body")?.0;
                assert_eq!(text.location.absolute_start, start, "{source:?}");
                assert_eq!(text.location.absolute_end, end, "{source:?}");
                let prefix = parsed.source().get(..end).ok_or("invalid endpoint")?;
                assert_eq!(
                    text.location.end.line,
                    u32::try_from(prefix.bytes().filter(|byte| *byte == b'\n').count() + 1)?
                );
                assert_eq!(
                    text.location.end.column,
                    u32::try_from(
                        prefix
                            .rsplit('\n')
                            .next()
                            .ok_or("missing line")?
                            .chars()
                            .count()
                            + 1
                    )?
                );
            }
        }
    }
    Ok(())
}

#[test]
fn whitespace_and_empty_attributes_keep_constrained_quote_order() -> Result<(), Error> {
    for marker in ["`", "*", "_", "#"] {
        for subs in ["", "quotes,attributes", "attributes,quotes"] {
            for (body, expected) in [("{sp}", " "), ("{empty}", "")] {
                let metadata = if subs.is_empty() {
                    String::new()
                } else {
                    format!("[subs=\"{subs}\"]\n")
                };
                let source = format!(":empty:\n\n{metadata}A {marker}{body}{marker} B.");
                let parsed = parse(&source, &Options::default())?;
                let Some(Block::Paragraph(paragraph)) = parsed.document().blocks.first() else {
                    return Err("expected a paragraph".into());
                };
                let content = paragraph
                    .content
                    .iter()
                    .find_map(|node| formatted_content(node, marker));
                if cfg!(feature = "pre-spec-subs") && subs == "attributes,quotes" {
                    assert!(content.is_none(), "{source}");
                } else {
                    let content = content.ok_or("missing formatting before attribute expansion")?;
                    if expected.is_empty() {
                        assert!(content.is_empty(), "{source}");
                    } else {
                        let [InlineNode::PlainText(text)] = content else {
                            return Err(format!("missing whitespace: {source}: {content:?}").into());
                        };
                        assert_eq!(text.content, expected, "{source}");
                    }
                }
            }
        }
    }
    Ok(())
}

#[test]
fn blank_block_expansions_keep_their_existing_empty_content() -> Result<(), Error> {
    for body in ["{sp}", "{sp}{sp}", "{nbsp}", "{empty}"] {
        let source = format!(":empty:\n\n{body}\n\nAfter.");
        let parsed = parse(&source, &Options::default())?;
        let [Block::Paragraph(blank), Block::Paragraph(after)] =
            parsed.document().blocks.as_slice()
        else {
            return Err("expected the blank paragraph and its following sibling".into());
        };
        assert!(blank.content.is_empty(), "{source}");
        assert!(!after.content.is_empty());
    }
    Ok(())
}

#[test]
fn inline_api_preserves_whitespace_inside_formatting() -> Result<(), Error> {
    for marker in ["``", "**", "__", "##"] {
        let source = format!("A{marker} {marker}B");
        let parsed = parse_inline(&source, &Options::default())?;
        let nodes = parsed
            .inlines()
            .iter()
            .find_map(|node| formatted_content(node, marker))
            .ok_or("missing formatted span")?;
        let [InlineNode::PlainText(text)] = nodes else {
            return Err("expected one whitespace node".into());
        };
        assert_eq!(text.content, " ");
    }
    Ok(())
}

#[test]
fn inline_whitespace_scope_preserves_footnote_conflict_warnings() -> Result<(), Error> {
    for marker in ["", "*", "`", "_"] {
        for body in ["First.", "Second.", ""] {
            let source =
                format!("{marker}footnote:n[First.]{marker} {marker}footnote:n[{body}]{marker}");
            let parsed = parse_inline(&source, &Options::default())?;
            let warnings = parsed
                .warnings()
                .iter()
                .filter(|warning| matches!(warning.kind, WarningKind::ConflictingFootnote { .. }))
                .collect::<Vec<_>>();
            assert_eq!(warnings.len(), usize::from(body == "Second."), "{source}");
            if let [warning] = warnings.as_slice() {
                let WarningKind::ConflictingFootnote { first, .. } = &warning.kind else {
                    return Err("expected a footnote conflict".into());
                };
                let repeated = &warning
                    .source_location()
                    .ok_or("missing conflict location")?
                    .location;
                assert_eq!(
                    source.get(first.location.absolute_start..=first.location.absolute_end),
                    Some("footnote:n[First.]")
                );
                assert_eq!(
                    source.get(repeated.absolute_start..=repeated.absolute_end),
                    Some("footnote:n[Second.]")
                );
            }
        }
    }
    Ok(())
}

#[test]
fn formatted_whitespace_remains_inside_link_labels() -> Result<(), Error> {
    for marker in ["``", "**", "__", "##"] {
        let source = format!("link:https://example.org[A{marker}{{sp}}{marker}B]");
        let parsed = parse_inline(&source, &Options::default())?;
        let [InlineNode::Macro(InlineMacro::Link(link))] = parsed.inlines() else {
            return Err("expected one link".into());
        };
        let nodes = link
            .text
            .iter()
            .find_map(|node| formatted_content(node, marker))
            .ok_or("missing formatted label")?;
        let [InlineNode::PlainText(text)] = nodes else {
            return Err("missing whitespace in link label".into());
        };
        assert_eq!(text.content, " ");
        assert_eq!(
            source.get(text.location.absolute_start..=text.location.absolute_end),
            Some("{sp}")
        );
    }
    Ok(())
}

#[test]
fn whitespace_macro_labels_keep_only_their_source_span() -> Result<(), Error> {
    for target in [
        "link:https://example.org",
        "https://example.org",
        "mailto:a@example.org",
        "xref:target",
    ] {
        for body in [" ", "  ", "\t", "{sp}", "{sp}{sp}"] {
            let source = format!("{target}[{body}]");
            let parsed = parse(&source, &Options::default())?;
            let Some(Block::Paragraph(paragraph)) = parsed.document().blocks.first() else {
                return Err("expected a paragraph".into());
            };
            let [InlineNode::Macro(inline_macro)] = paragraph.content.as_slice() else {
                return Err("expected one macro".into());
            };
            let content = if let InlineMacro::Link(link) = inline_macro {
                &link.text
            } else if let InlineMacro::Url(url) = inline_macro {
                &url.text
            } else if let InlineMacro::Mailto(mailto) = inline_macro {
                &mailto.text
            } else if let InlineMacro::CrossReference(xref) = inline_macro {
                &xref.text
            } else {
                return Err("unexpected macro".into());
            };
            let [InlineNode::PlainText(text)] = content.as_slice() else {
                return Err("expected a whitespace label".into());
            };
            assert_eq!(text.content, body.replace("{sp}", " "), "{source}");
            assert_eq!(
                source.get(text.location.absolute_start..=text.location.absolute_end),
                Some(body),
                "{source}"
            );
            assert_eq!(
                text.location.end.column as usize,
                target.len() + 1 + body.len(),
                "{source}"
            );
        }
    }
    Ok(())
}
