use acdc_parser::{
    Options,
    ast::{BlockKind, BlockRef, InlineKind},
    parse,
};

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
                let [BlockKind::Paragraph(paragraph)] = &*parsed
                    .document()
                    .blocks()
                    .map(BlockRef::kind)
                    .collect::<Vec<_>>()
                else {
                    return Err("expected one paragraph".into());
                };
                let mut text = String::new();
                for node in paragraph.content().inlines() {
                    let InlineKind::Plain(plain) = node.kind() else {
                        return Err(format!("escaped macro created a node: {node:?}").into());
                    };
                    text.push_str(plain.text());
                    if plain.text().contains(&escapes) {
                        let location = node.location();
                        assert!(
                            source[location.absolute_start()..=location.absolute_end()]
                                .contains(&escapes),
                            "{source}: {location:?}"
                        );
                    }
                    for (offset, position) in [
                        (node.location().absolute_start(), node.location().start()),
                        (node.location().absolute_end(), node.location().end()),
                    ] {
                        assert!(source.is_char_boundary(offset));
                        assert_eq!(position.line(), 1);
                        assert_eq!(
                            position.column() as usize,
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
                let [BlockKind::Paragraph(paragraph)] = &*parsed
                    .document()
                    .blocks()
                    .map(BlockRef::kind)
                    .collect::<Vec<_>>()
                else {
                    return Err("expected one paragraph".into());
                };
                let content = paragraph
                    .content()
                    .inlines()
                    .find_map(|node| {
                        if let InlineKind::Footnote(note) = node.kind() {
                            note.content()
                        } else if let InlineKind::CrossReference(xref) = node.kind() {
                            Some(xref.text())
                        } else {
                            None
                        }
                    })
                    .ok_or("missing active macro")?;
                let mut text = String::new();
                for node in content.inlines() {
                    if let InlineKind::Plain(plain) = node.kind() {
                        text.push_str(plain.text());
                    } else if let InlineKind::Raw(raw) = node.kind() {
                        text.push_str(raw.text());
                        assert_eq!(
                            &source
                                [raw.location().absolute_start()..=raw.location().absolute_end()],
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
        let [BlockKind::Paragraph(paragraph)] = &*parsed
            .document()
            .blocks()
            .map(BlockRef::kind)
            .collect::<Vec<_>>()
        else {
            return Err("expected paragraph".into());
        };
        let content = paragraph
            .content()
            .inlines()
            .find_map(|node| {
                if let InlineKind::Link(link) = node.kind() {
                    Some(link.text())
                } else {
                    None
                }
            })
            .ok_or("missing link")?;
        let content = content
            .inlines()
            .find_map(|node| {
                if let InlineKind::Bold(bold) = node.kind() {
                    Some(bold.content())
                } else {
                    None
                }
            })
            .ok_or_else(|| format!("missing bold label: {source}"))?;
        let mut found = 0;
        for node in content.inlines() {
            if let InlineKind::Raw(raw) = node.kind()
                && (raw.text().contains(']') || raw.text() == "\"")
            {
                let location = raw.location();
                let slice = &source[location.absolute_start()..=location.absolute_end()];
                assert!(slice.starts_with('\\'), "{source}: {slice:?}, {location:?}");
                assert!(slice.ends_with(raw.text()), "{source}: {slice:?}, {raw:?}");
                assert_eq!(
                    location.start().column() as usize,
                    source[..location.absolute_start()].chars().count() + 1
                );
                assert_eq!(
                    location.end().column() as usize,
                    source[..location.absolute_end()].chars().count() + 1
                );
                found += 1;
            }
        }
        assert!(found > 0, "{source}");
    }
    Ok(())
}
