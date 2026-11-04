use acdc_parser::{
    Options,
    ast::{BlockKind, BlockRef, InlineKind, InlineRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

fn label<'doc>(nodes: impl IntoIterator<Item = InlineRef<'doc>>) -> Result<String, Error> {
    nodes
        .into_iter()
        .map(|node| {
            if let InlineKind::Plain(text) = node.kind() {
                Ok(text.text())
            } else if let InlineKind::Raw(text) = node.kind() {
                Ok(text.text())
            } else if let InlineKind::Symbol(symbol) = node.kind() {
                Ok(symbol.kind().text())
            } else {
                Err(format!("unexpected index label node: {node:?}").into())
            }
        })
        .collect()
}

// Rendered text can hide a parenthesis moved outside the index term.
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
                let [BlockKind::Paragraph(paragraph)] = &*parsed
                    .document_ref()
                    .blocks()
                    .map(BlockRef::kind)
                    .collect::<Vec<_>>()
                else {
                    return Err("expected paragraph".into());
                };
                let term = paragraph
                    .content()
                    .inlines()
                    .find_map(|node| {
                        if let InlineKind::IndexTerm(term) = node.kind() {
                            Some(term)
                        } else {
                            None
                        }
                    })
                    .ok_or("missing index term")?;
                assert!(term.is_visible(), "{source}");
                assert_eq!(label(term.term().inlines())?, content, "{source}");
                assert_eq!(label(term.catalog().term().inlines())?, content, "{source}");
                let span = term.location();
                assert_eq!(
                    source.get(span.absolute_start()..=span.absolute_end()),
                    Some(markup.as_str()),
                    "{source}"
                );
                assert_eq!(span.start().column() as usize, prefix.chars().count() + 1);
                assert_eq!(
                    span.end().column() as usize,
                    prefix.chars().count() + markup.chars().count()
                );
                let [InlineKind::Plain(text)] = &*term
                    .term()
                    .inlines()
                    .map(InlineRef::kind)
                    .collect::<Vec<_>>()
                else {
                    return Err("expected complete plain label".into());
                };
                assert_eq!(
                    source.get(text.location().absolute_start()..=text.location().absolute_end()),
                    Some(content.as_str())
                );
                let tail = paragraph
                    .content()
                    .inlines()
                    .last()
                    .ok_or("missing suffix")?;
                assert_eq!(label(std::iter::once(tail))?, suffix, "{source}");
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
        let [BlockKind::Paragraph(paragraph)] = &*parsed
            .document_ref()
            .blocks()
            .map(BlockRef::kind)
            .collect::<Vec<_>>()
        else {
            return Err("expected paragraph".into());
        };
        let terms = paragraph
            .content()
            .inlines()
            .filter_map(|node| {
                if let InlineKind::IndexTerm(term) = node.kind() {
                    Some(label(term.term().inlines()))
                } else {
                    None
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(terms, expected, "{source}");
    }
    Ok(())
}

#[test]
fn index_parentheses_keep_registration_time_attribute_values() -> Result<(), Error> {
    for (subs, registered) in [
        ("macros,attributes,replacements", "Late {symbol}"),
        ("attributes,macros,replacements", "Late (R)"),
    ] {
        let source = format!(":symbol: (R)\n\n[subs=\"{subs}\"]\n((Late {{symbol}})) after.");
        let parsed = parse(&source, &Options::default())?;
        let [BlockKind::Paragraph(paragraph)] = &*parsed
            .document_ref()
            .blocks()
            .map(BlockRef::kind)
            .collect::<Vec<_>>()
        else {
            return Err("expected paragraph".into());
        };
        let [InlineKind::IndexTerm(term), ..] = &*paragraph
            .content()
            .inlines()
            .map(InlineRef::kind)
            .collect::<Vec<_>>()
        else {
            return Err("expected visible index term".into());
        };
        assert_eq!(label(term.term().inlines())?, "Late ®", "{source}");
        assert_eq!(
            label(term.catalog().term().inlines())?,
            registered,
            "{source}"
        );
        assert_eq!(
            source.get(term.location().absolute_start()..=term.location().absolute_end()),
            Some("((Late {symbol}))")
        );
    }
    Ok(())
}
