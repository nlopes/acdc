use acdc_parser::{
    Options,
    ast::{BlockKind, BlockRef, ContentRef, IndexTermRef, InlineKind, InlineRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

fn index_term(nodes: ContentRef<'_>) -> Result<IndexTermRef<'_>, Error> {
    nodes
        .inlines()
        .find_map(|node| {
            if let InlineKind::IndexTerm(term) = node.kind() {
                Some(term)
            } else if let InlineKind::Link(link) = node.kind() {
                index_term(link.text()).ok()
            } else {
                None
            }
        })
        .ok_or_else(|| "missing index term".into())
}

fn label_text<'doc>(nodes: impl IntoIterator<Item = InlineRef<'doc>>) -> Result<String, Error> {
    nodes
        .into_iter()
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
fn outer_macro_escapes_preserve_registration_time_anchor_text() -> Result<(), Error> {
    let parsed = parse(r"((Escaped \[[id]] anchor))", &Options::default())?;
    let [BlockKind::Paragraph(paragraph)] = &*parsed
        .document()
        .blocks()
        .map(BlockRef::kind)
        .collect::<Vec<_>>()
    else {
        return Err("expected paragraph".into());
    };
    let term = index_term(paragraph.content())?;
    assert_eq!(label_text(term.term().inlines())?, "Escaped [[id]] anchor");
    assert_eq!(
        label_text(term.catalog().term().inlines())?,
        r"Escaped \[[id]] anchor"
    );
    assert!(parsed.document().references().is_empty());
    Ok(())
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
                    let [BlockKind::Paragraph(paragraph)] = &*parsed
                        .document()
                        .blocks()
                        .map(BlockRef::kind)
                        .collect::<Vec<_>>()
                    else {
                        return Err("expected paragraph".into());
                    };
                    let term = index_term(paragraph.content())?;
                    let registered_escapes = count - usize::from(named && bracket == ']');
                    let display_escapes =
                        registered_escapes.saturating_sub(usize::from(linked && bracket == ']'));
                    for (nodes, remaining) in [
                        (term.term(), display_escapes),
                        (term.catalog().term(), registered_escapes),
                    ] {
                        assert_eq!(
                            label_text(nodes.inlines())?,
                            format!("é {}{bracket} café", "\\".repeat(remaining)),
                            "{source}"
                        );
                        let bracket_node = nodes
                            .inlines()
                            .find(|node| {
                                label_text(std::iter::once(*node))
                                    .is_ok_and(|text| text.ends_with(bracket))
                            })
                            .ok_or("missing bracket text")?;
                        let location = bracket_node.location();
                        assert_eq!(
                            &source[location.absolute_start()..=location.absolute_end()],
                            format!("{escapes}{bracket}")
                        );
                        for node in nodes.inlines() {
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
            }
        }
    }
    Ok(())
}

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
            let Some(BlockKind::Paragraph(paragraph)) =
                parsed.document().blocks().last().map(BlockRef::kind)
            else {
                return Err("expected paragraph".into());
            };
            let term = index_term(paragraph.content())?;
            assert_eq!(label_text(term.term().inlines())?, display, "{source}");
            let catalog = if syntax.starts_with("link:") && subs.starts_with("attributes") {
                r"Value \] label"
            } else {
                catalog
            };
            assert_eq!(
                label_text(term.catalog().term().inlines())?,
                catalog,
                "{source}"
            );
        }
    }
    Ok(())
}
