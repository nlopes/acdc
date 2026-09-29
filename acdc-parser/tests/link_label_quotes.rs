use acdc_parser::{Block, InlineMacro, InlineNode, Options, parse};

type Error = Box<dyn std::error::Error>;

fn label<'n, 'a>(nodes: &'n [InlineNode<'a>]) -> Result<&'n [InlineNode<'a>], Error> {
    nodes
        .iter()
        .find_map(|node| {
            if let InlineNode::Macro(InlineMacro::Link(link)) = node {
                Some(link.text.as_slice())
            } else if let InlineNode::Macro(InlineMacro::Url(url)) = node {
                Some(url.text.as_slice())
            } else if let InlineNode::Macro(InlineMacro::Mailto(mailto)) = node {
                Some(mailto.text.as_slice())
            } else {
                None
            }
        })
        .ok_or_else(|| "missing link".into())
}

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
fn link_label_quotes_keep_literal_quotes_without_attribute_parsing() -> Result<(), Error> {
    for prefix in [
        "link:https://example.org",
        "https://example.org",
        "mailto:test@example.org",
    ] {
        for quote in ['\'', '"'] {
            let source = format!("α {prefix}[{quote}é \\] café{quote}] ω.\n");
            let parsed = parse(&source, &Options::default())?;
            let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
                return Err("expected paragraph".into());
            };
            assert_eq!(
                label_text(label(&paragraph.content)?)?,
                format!("{quote}é ] café{quote}")
            );
        }
    }
    Ok(())
}

#[test]
fn link_label_quotes_remove_one_escape_and_keep_source_spans() -> Result<(), Error> {
    for prefix in [
        "link:https://example.org",
        "https://example.org",
        "mailto:test@example.org",
    ] {
        for quote in ['\'', '"'] {
            for count in 1..=4 {
                let escapes = "\\".repeat(count);
                let source = format!(
                    "α {prefix}[{quote}é {escapes}{quote}café{escapes}{quote} fin{quote},role=test] ω.\n"
                );
                let parsed = parse(&source, &Options::default())?;
                let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
                    return Err("expected paragraph".into());
                };
                let text = label(&paragraph.content)?;
                let remaining = "\\".repeat(count - 1);
                assert_eq!(
                    label_text(text)?,
                    format!("é {remaining}{quote}café{remaining}{quote} fin"),
                    "{source}"
                );
                for node in text {
                    let location = node.location();
                    for (offset, position) in [
                        (location.absolute_start, &location.start),
                        (location.absolute_end, &location.end),
                    ] {
                        assert!(source.is_char_boundary(offset), "{source}");
                        assert_eq!(position.line, 1);
                        assert_eq!(
                            position.column as usize,
                            source[..offset].chars().count() + 1
                        );
                    }
                    if let InlineNode::RawText(raw) = node {
                        assert_eq!(
                            &source[location.absolute_start..=location.absolute_end],
                            format!("{escapes}{quote}")
                        );
                        assert_eq!(raw.content, format!("{remaining}{quote}"));
                    }
                }
            }
        }
    }
    Ok(())
}

#[test]
fn link_label_quotes_keep_multiline_passthrough_source_spans() -> Result<(), Error> {
    for source in [
        "α link:https://example.org[\"é \\\"café\\\"\nfin\",role=test] ω.\n",
        "α pass:m[link:https://example.org[\"é \\\"café\\\"\nfin\",role=test\\]] ω.\n",
    ] {
        let parsed = parse(source, &Options::default())?;
        let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
            return Err("expected paragraph".into());
        };
        let text = label(&paragraph.content)?;
        assert_eq!(label_text(text)?, "é \"café\"\nfin");
        assert_eq!(
            text.first().ok_or("empty label")?.location().absolute_start,
            source.find('é').ok_or("missing label")?
        );
        let tail = text.last().ok_or("empty label")?.location();
        assert_eq!((tail.end.line, tail.end.column), (2, 3));
        assert_eq!(
            tail.absolute_end,
            source.find("fin").ok_or("missing label tail")? + 2
        );
        for node in text {
            if let InlineNode::RawText(raw) = node
                && raw.content == "\""
            {
                assert_eq!(
                    &source[raw.location.absolute_start..=raw.location.absolute_end],
                    "\\\""
                );
            }
        }
    }
    Ok(())
}

#[test]
fn link_label_quotes_keep_index_and_footnote_registration_text() -> Result<(), Error> {
    for substitutions in ["", "[subs=\"macros,attributes,quotes,replacements\"]\n"] {
        if !cfg!(feature = "pre-spec-subs") && !substitutions.is_empty() {
            continue;
        }
        let source = format!(
            "{substitutions}{}\n\nReuse footnote:note[].\n",
            r#"link:https://example.org["Label ((One \"quote\")) footnote:note[Note \"quote\".]",role=test]"#
        );
        let parsed = parse(&source, &Options::default())?;
        let Some(Block::Paragraph(paragraph)) = parsed.document().blocks.first() else {
            return Err("expected paragraph".into());
        };
        let text = label(&paragraph.content)?;
        let term = text
            .iter()
            .find_map(|node| {
                if let InlineNode::Macro(InlineMacro::IndexTerm(term)) = node {
                    Some(term)
                } else {
                    None
                }
            })
            .ok_or("missing index term")?;
        assert_eq!(label_text(term.term())?, "One \"quote\"");
        assert_eq!(label_text(term.catalog_entry().term())?, r#"One \"quote\""#);
        let [note] = parsed.document().footnotes.as_slice() else {
            return Err("expected one registered footnote".into());
        };
        assert_eq!(label_text(&note.content)?, r#"Note \"quote\"."#);
    }
    Ok(())
}

#[test]
fn link_label_quotes_keep_nested_macro_delimiters() -> Result<(), Error> {
    let source =
        "link:https://example.org[Label (((term,sub))) and <<target,Display>> tail,role=green]";
    let parsed = parse(source, &Options::default())?;
    let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
        return Err("expected paragraph".into());
    };
    let text = label(&paragraph.content)?;
    let term = text
        .iter()
        .find_map(|node| {
            if let InlineNode::Macro(InlineMacro::IndexTerm(term)) = node {
                Some(term)
            } else {
                None
            }
        })
        .ok_or("missing concealed term")?;
    assert!(!term.is_visible());
    assert_eq!(label_text(term.term())?, "term");
    assert_eq!(
        label_text(term.secondary().ok_or("missing secondary term")?)?,
        "sub"
    );
    let xref = text
        .iter()
        .find_map(|node| {
            if let InlineNode::Macro(InlineMacro::CrossReference(xref)) = node {
                Some(xref)
            } else {
                None
            }
        })
        .ok_or("missing nested reference")?;
    assert_eq!(xref.target, "target");
    assert_eq!(label_text(&xref.text)?, "Display");
    let tail = text.last().ok_or("missing label tail")?;
    assert_eq!(label_text(std::slice::from_ref(tail))?, " tail");
    Ok(())
}
