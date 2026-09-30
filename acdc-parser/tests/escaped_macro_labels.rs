use acdc_parser::{Block, InlineMacro, InlineNode, Options, parse};

type Error = Box<dyn std::error::Error>;

#[test]
fn escaped_macro_brackets_preserve_unicode_source_spans() -> Result<(), Error> {
    for count in 1..=4 {
        for bracket in ['[', ']'] {
            let escapes = format!("{}{bracket}", "\\".repeat(count));
            for (prefix, suffix) in [
                ("indexterm2:[", "]"),
                ("indexterm:[", "]"),
                ("((", "))"),
                ("https://example.org[", "]"),
                ("mailto:user@example.org[", "]"),
                ("xref:target[", "]"),
                ("footnote:[", "]"),
                ("anchor:target[", "]"),
            ] {
                let literal = format!("{prefix}é {escapes} café{suffix}");
                let source = format!("α \\{literal} ω.\n");
                let parsed = parse(&source, &Options::default())?;
                let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
                    return Err("expected one paragraph".into());
                };
                let mut text = String::new();
                for node in &paragraph.content {
                    let InlineNode::PlainText(plain) = node else {
                        return Err(format!("escaped macro created a node: {node:?}").into());
                    };
                    text.push_str(plain.content);
                    if plain.content.contains(&escapes) {
                        let location = node.location();
                        assert!(
                            source[location.absolute_start..=location.absolute_end]
                                .contains(&escapes),
                            "{source}: {location:?}"
                        );
                    }
                    for (offset, position) in [
                        (node.location().absolute_start, &node.location().start),
                        (node.location().absolute_end, &node.location().end),
                    ] {
                        assert!(source.is_char_boundary(offset));
                        assert_eq!(position.line, 1);
                        assert_eq!(
                            position.column as usize,
                            source[..offset].chars().count() + 1
                        );
                    }
                }
                assert_eq!(text, format!("α {literal} ω."));
            }
        }
    }
    Ok(())
}

// Raw nodes preserve the complete escaped source span, although display text is shorter.
#[test]
fn active_macro_brackets_consume_only_their_delimiter_escape() -> Result<(), Error> {
    for count in 1..=4 {
        for bracket in ['[', ']'] {
            let escapes = format!("{}{bracket}", "\\".repeat(count));
            for prefix in ["footnote:", "xref:target"] {
                let source = format!("α {prefix}[é {escapes} café] ω.\n");
                let parsed = parse(&source, &Options::default())?;
                let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
                    return Err("expected one paragraph".into());
                };
                let content = paragraph
                    .content
                    .iter()
                    .find_map(|node| {
                        if let InlineNode::Macro(InlineMacro::Footnote(note)) = node {
                            Some(&note.content)
                        } else if let InlineNode::Macro(InlineMacro::CrossReference(xref)) = node {
                            Some(&xref.text)
                        } else {
                            None
                        }
                    })
                    .ok_or("missing active macro")?;
                let mut text = String::new();
                for node in content {
                    if let InlineNode::PlainText(plain) = node {
                        text.push_str(plain.content);
                    } else if let InlineNode::RawText(raw) = node {
                        text.push_str(raw.content);
                        assert_eq!(
                            &source[raw.location.absolute_start..=raw.location.absolute_end],
                            escapes,
                        );
                    } else {
                        return Err("unexpected formatting".into());
                    }
                }
                let remaining = count - usize::from(bracket == ']');
                assert_eq!(text, format!("é {}{bracket} café", "\\".repeat(remaining)));
            }
        }
    }
    Ok(())
}

#[test]
fn formatted_macro_escapes_keep_original_source_spans() -> Result<(), Error> {
    for source in [
        r#"α link:https://example.org["*é \] café*",role=green] ω."#,
        r"α pass:q,m[link:https://example.org[*é \\] café*\]] ω.",
        r#"α pass:q,m[link:https://example.org["*é \"café\" fin*",role=green\]] ω."#,
    ] {
        let parsed = parse(source, &Options::default())?;
        let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
            return Err("expected paragraph".into());
        };
        let content = paragraph
            .content
            .iter()
            .find_map(|node| {
                if let InlineNode::Macro(InlineMacro::Link(link)) = node {
                    Some(&link.text)
                } else {
                    None
                }
            })
            .ok_or("missing link")?;
        let content = content
            .iter()
            .find_map(|node| {
                if let InlineNode::BoldText(bold) = node {
                    Some(&bold.content)
                } else {
                    None
                }
            })
            .ok_or_else(|| format!("missing bold label: {source}"))?;
        let mut found = 0;
        for node in content {
            if let InlineNode::RawText(raw) = node
                && (raw.content.contains(']') || raw.content == "\"")
            {
                let location = &raw.location;
                let slice = &source[location.absolute_start..=location.absolute_end];
                assert!(slice.starts_with('\\'), "{source}: {slice:?}, {location:?}");
                assert!(slice.ends_with(raw.content), "{source}: {slice:?}, {raw:?}");
                assert_eq!(
                    location.start.column as usize,
                    source[..location.absolute_start].chars().count() + 1
                );
                assert_eq!(
                    location.end.column as usize,
                    source[..location.absolute_end].chars().count() + 1
                );
                found += 1;
            }
        }
        assert!(found > 0, "{source}");
    }
    Ok(())
}
