use acdc_parser::{Block, IndexTerm, InlineMacro, InlineNode, Options, parse};

type Error = Box<dyn std::error::Error>;

fn index_term<'n, 'a>(nodes: &'n [InlineNode<'a>]) -> Result<&'n IndexTerm<'a>, Error> {
    nodes
        .iter()
        .find_map(|node| {
            if let InlineNode::Macro(InlineMacro::IndexTerm(term)) = node {
                Some(term.as_ref())
            } else if let InlineNode::Macro(InlineMacro::Link(link)) = node {
                index_term(&link.text).ok()
            } else {
                None
            }
        })
        .ok_or_else(|| "missing index term".into())
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

// JSON fixtures omit the registration-time catalog label. Check it separately
// from the display label, including inclusive source spans after unescaping.
#[test]
fn index_bracket_escapes_follow_delimiters_and_keep_source_spans() -> Result<(), Error> {
    for count in 1_usize..=4 {
        for bracket in ['[', ']'] {
            for named in [false, true] {
                for linked in [false, true] {
                    let escapes = "\\".repeat(count);
                    let label = format!("é {escapes}{bracket} café");
                    let term = if named {
                        format!("indexterm2:[{label}]")
                    } else {
                        format!("(({label}))")
                    };
                    let source = if linked {
                        format!("α link:https://example.org[{term}] ω.\n")
                    } else {
                        format!("α {term} ω.\n")
                    };
                    let parsed = parse(&source, &Options::default())
                        .map_err(|error| format!("{source}: {error}"))?;
                    let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
                        return Err("expected paragraph".into());
                    };
                    let term = index_term(&paragraph.content)?;
                    let registered_escapes = count - usize::from(named && bracket == ']');
                    let display_escapes =
                        registered_escapes.saturating_sub(usize::from(linked && bracket == ']'));
                    for (nodes, remaining) in [
                        (term.term(), display_escapes),
                        (term.catalog_entry().term(), registered_escapes),
                    ] {
                        assert_eq!(
                            label_text(nodes)?,
                            format!("é {}{bracket} café", "\\".repeat(remaining)),
                            "{source}"
                        );
                        let bracket_node = nodes
                            .iter()
                            .find(|node| {
                                label_text(std::slice::from_ref(node))
                                    .is_ok_and(|text| text.ends_with(bracket))
                            })
                            .ok_or("missing bracket text")?;
                        let location = bracket_node.location();
                        assert_eq!(
                            &source[location.absolute_start..=location.absolute_end],
                            format!("{escapes}{bracket}")
                        );
                        for node in nodes {
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
            }
        }
    }
    Ok(())
}

#[cfg(feature = "pre-spec-subs")]
#[test]
fn index_bracket_escapes_ignore_values_introduced_after_macros() -> Result<(), Error> {
    for syntax in [
        "indexterm2:[{label}]",
        "link:https://example.org[(({label}))]",
    ] {
        for (subs, display, catalog) in [
            ("attributes,macros", "Value ] label", "Value ] label"),
            ("macros,attributes", r"Value \] label", "{label}"),
        ] {
            let source = format!(":label: Value \\] label\n\n[subs=\"{subs}\"]\n{syntax}\n");
            let parsed = parse(&source, &Options::default())?;
            let Some(Block::Paragraph(paragraph)) = parsed.document().blocks.last() else {
                return Err("expected paragraph".into());
            };
            let term = index_term(&paragraph.content)?;
            assert_eq!(label_text(term.term())?, display, "{source}");
            let catalog = if syntax.starts_with("link:") && subs.starts_with("attributes") {
                r"Value \] label"
            } else {
                catalog
            };
            assert_eq!(
                label_text(term.catalog_entry().term())?,
                catalog,
                "{source}"
            );
        }
    }
    Ok(())
}
