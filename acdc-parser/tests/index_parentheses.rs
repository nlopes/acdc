use acdc_parser::{Block, InlineMacro, InlineNode, Options, parse};

type Error = Box<dyn std::error::Error>;

fn label(nodes: &[InlineNode<'_>]) -> Result<String, Error> {
    nodes
        .iter()
        .map(|node| {
            if let InlineNode::PlainText(text) = node {
                Ok(text.content)
            } else if let InlineNode::RawText(text) = node {
                Ok(text.content)
            } else {
                Err(format!("unexpected index label node: {node:?}").into())
            }
        })
        .collect()
}

// JSON omits registration-time labels. Check those and the exact span rather
// than rendered text, which can hide a parenthesis moved outside the term.
#[test]
fn index_parentheses_keep_complete_catalog_labels_and_source_spans() -> Result<(), Error> {
    for depth in 1..=8 {
        for extra in 0..=4 {
            for prefix in ["", "α Before "] {
                let content = format!("fn{}café{}", "(".repeat(depth), ")".repeat(depth));
                let markup = format!("(({content}))");
                let suffix = format!("{} after.", ")".repeat(extra));
                let source = format!("{prefix}{markup}{suffix}");
                let parsed = parse(&source, &Options::default())?;
                let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
                    return Err("expected paragraph".into());
                };
                let term = paragraph
                    .content
                    .iter()
                    .find_map(|node| {
                        if let InlineNode::Macro(InlineMacro::IndexTerm(term)) = node {
                            Some(term)
                        } else {
                            None
                        }
                    })
                    .ok_or("missing index term")?;
                assert!(term.is_visible(), "{source}");
                assert_eq!(label(term.term())?, content, "{source}");
                assert_eq!(label(term.catalog_entry().term())?, content, "{source}");
                let span = &term.location;
                assert_eq!(
                    source.get(span.absolute_start..=span.absolute_end),
                    Some(markup.as_str()),
                    "{source}"
                );
                assert_eq!(span.start.column as usize, prefix.chars().count() + 1);
                assert_eq!(
                    span.end.column as usize,
                    prefix.chars().count() + markup.chars().count()
                );
                let [InlineNode::PlainText(text)] = term.term() else {
                    return Err("expected complete plain label".into());
                };
                assert_eq!(
                    source.get(text.location.absolute_start..=text.location.absolute_end),
                    Some(content.as_str())
                );
                let tail = paragraph.content.last().ok_or("missing suffix")?;
                assert_eq!(label(std::slice::from_ref(tail))?, suffix, "{source}");
            }
        }
    }
    Ok(())
}

#[test]
fn index_parentheses_preserve_adjacent_terms_and_unbalanced_prose() -> Result<(), Error> {
    for (source, expected) in [
        ("((first)) and ((second))", ["first", "second"]),
        ("((Open (literal)) and ((Next))", ["Open (literal", "Next"]),
        ("((fn((x)) tail)) and ((Next))", ["fn((x)) tail", "Next"]),
    ] {
        let parsed = parse(source, &Options::default())?;
        let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
            return Err("expected paragraph".into());
        };
        let terms = paragraph
            .content
            .iter()
            .filter_map(|node| {
                if let InlineNode::Macro(InlineMacro::IndexTerm(term)) = node {
                    Some(label(term.term()))
                } else {
                    None
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(terms, expected, "{source}");
    }
    Ok(())
}

#[cfg(feature = "pre-spec-subs")]
#[test]
fn index_parentheses_keep_registration_time_attribute_values() -> Result<(), Error> {
    for (subs, registered) in [
        ("macros,attributes,replacements", "Late {symbol}"),
        ("attributes,macros,replacements", "Late (R)"),
    ] {
        let source = format!(":symbol: (R)\n\n[subs=\"{subs}\"]\n((Late {{symbol}})) after.");
        let parsed = parse(&source, &Options::default())?;
        let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
            return Err("expected paragraph".into());
        };
        let [InlineNode::Macro(InlineMacro::IndexTerm(term)), ..] = paragraph.content.as_slice()
        else {
            return Err("expected visible index term".into());
        };
        assert_eq!(label(term.term())?, "Late (R)", "{source}");
        assert_eq!(label(term.catalog_entry().term())?, registered, "{source}");
        assert_eq!(
            source.get(term.location.absolute_start..=term.location.absolute_end),
            Some("((Late {symbol}))")
        );
    }
    Ok(())
}
