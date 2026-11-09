use acdc_parser::{
    Options,
    ast::{BlockKind, BlockRef, ContentRef, InlineKind, InlineRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

fn label(nodes: ContentRef<'_>) -> Result<ContentRef<'_>, Error> {
    nodes
        .inlines()
        .find_map(|node| {
            if let InlineKind::Link(link) = node.kind() {
                Some(link.text())
            } else if let InlineKind::UrlMacro(url) = node.kind() {
                Some(url.text())
            } else if let InlineKind::Mailto(mailto) = node.kind() {
                Some(mailto.text())
            } else {
                None
            }
        })
        .ok_or_else(|| "missing link".into())
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
fn link_label_quotes_keep_literal_quotes_without_attribute_parsing() -> Result<(), Error> {
    for prefix in [
        "link:https://example.org",
        "https://example.org",
        "mailto:test@example.org",
    ] {
        for quote in ['\'', '"'] {
            let source = format!("α {prefix}[{quote}é \\] café{quote}] ω.\n");
            let parsed = parse(&source, &Options::default())?;
            let [BlockKind::Paragraph(paragraph)] = &*parsed
                .document()
                .blocks()
                .map(BlockRef::kind)
                .collect::<Vec<_>>()
            else {
                return Err("expected paragraph".into());
            };
            assert_eq!(
                label_text(label(paragraph.content())?.inlines())?,
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
                let [BlockKind::Paragraph(paragraph)] = &*parsed
                    .document()
                    .blocks()
                    .map(BlockRef::kind)
                    .collect::<Vec<_>>()
                else {
                    return Err("expected paragraph".into());
                };
                let text = label(paragraph.content())?;
                let remaining = "\\".repeat(count - 1);
                assert_eq!(
                    label_text(text.inlines())?,
                    format!("é {remaining}{quote}café{remaining}{quote} fin"),
                    "{source}"
                );
                for node in text.inlines() {
                    let location = node.location();
                    for (offset, position) in [
                        (location.absolute_start(), location.start()),
                        (location.absolute_end(), location.end()),
                    ] {
                        assert!(source.is_char_boundary(offset), "{source}");
                        assert_eq!(position.line(), 1);
                        assert_eq!(
                            position.column() as usize,
                            source[..offset].chars().count() + 1
                        );
                    }
                    if let InlineKind::Raw(raw) = node.kind() {
                        assert_eq!(
                            &source[location.absolute_start()..=location.absolute_end()],
                            format!("{escapes}{quote}")
                        );
                        assert_eq!(raw.text(), format!("{remaining}{quote}"));
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
        let [BlockKind::Paragraph(paragraph)] = &*parsed
            .document()
            .blocks()
            .map(BlockRef::kind)
            .collect::<Vec<_>>()
        else {
            return Err("expected paragraph".into());
        };
        let text = label(paragraph.content())?;
        assert_eq!(label_text(text.inlines())?, "é \"café\"\nfin");
        assert_eq!(
            text.inlines()
                .next()
                .ok_or("empty label")?
                .location()
                .absolute_start(),
            source.find('é').ok_or("missing label")?
        );
        let tail = text.inlines().last().ok_or("empty label")?.location();
        assert_eq!((tail.end().line(), tail.end().column()), (2, 3));
        assert_eq!(
            tail.absolute_end(),
            source.find("fin").ok_or("missing label tail")? + 2
        );
        for node in text.inlines() {
            if let InlineKind::Raw(raw) = node.kind()
                && raw.text() == "\""
            {
                assert_eq!(
                    &source[raw.location().absolute_start()..=raw.location().absolute_end()],
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
        let source = format!(
            "{substitutions}{}\n\nReuse footnote:note[].\n",
            r#"link:https://example.org["Label ((One \"quote\")) footnote:note[Note \"quote\".]",role=test]"#
        );
        let parsed = parse(&source, &Options::default())?;
        let Some(BlockKind::Paragraph(paragraph)) =
            parsed.document().blocks().next().map(BlockRef::kind)
        else {
            return Err("expected paragraph".into());
        };
        let text = label(paragraph.content())?;
        let term = text
            .inlines()
            .find_map(|node| {
                if let InlineKind::IndexTerm(term) = node.kind() {
                    Some(term)
                } else {
                    None
                }
            })
            .ok_or("missing index term")?;
        assert_eq!(label_text(term.term().inlines())?, "One \"quote\"");
        assert_eq!(
            label_text(term.catalog().term().inlines())?,
            r#"One \"quote\""#
        );
        let [note] = &*parsed.document().footnotes().collect::<Vec<_>>() else {
            return Err("expected one registered footnote".into());
        };
        assert_eq!(label_text(note.content().inlines())?, r#"Note \"quote\"."#);
    }
    Ok(())
}

#[test]
fn link_label_quotes_keep_nested_macro_delimiters() -> Result<(), Error> {
    let source =
        "link:https://example.org[Label (((term,sub))) and <<target,Display>> tail,role=green]";
    let parsed = parse(source, &Options::default())?;
    let [BlockKind::Paragraph(paragraph)] = &*parsed
        .document()
        .blocks()
        .map(BlockRef::kind)
        .collect::<Vec<_>>()
    else {
        return Err("expected paragraph".into());
    };
    let text = label(paragraph.content())?;
    let term = text
        .inlines()
        .find_map(|node| {
            if let InlineKind::IndexTerm(term) = node.kind() {
                Some(term)
            } else {
                None
            }
        })
        .ok_or("missing concealed term")?;
    assert!(!term.is_visible());
    assert_eq!(label_text(term.term().inlines())?, "term");
    assert_eq!(
        label_text(term.secondary().ok_or("missing secondary term")?.inlines())?,
        "sub"
    );
    let xref = text
        .inlines()
        .find_map(|node| {
            if let InlineKind::CrossReference(xref) = node.kind() {
                Some(xref)
            } else {
                None
            }
        })
        .ok_or("missing nested reference")?;
    assert_eq!(xref.target(), "target");
    assert_eq!(label_text(xref.text().inlines())?, "Display");
    let tail = text.inlines().last().ok_or("missing label tail")?;
    assert_eq!(label_text(std::iter::once(tail))?, " tail");
    Ok(())
}
