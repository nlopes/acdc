use acdc_parser::{
    Options,
    ast::{BlockKind, BlockRef, InlineKind, InlineRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

#[test]
fn prepend_modifiers_register_attribute_macros_once() -> Result<(), Error> {
    for subs in [
        "attributes+",
        "attributes+,attributes+",
        "normal+,attributes+",
    ] {
        let source = format!(
            ":value: anchor:target[]footnote:[Only note.]\n\n[subs=\"{subs}\"]\n{{value}}\n"
        );
        let parsed = parse(&source, &Options::default())?;
        assert_eq!(parsed.document().footnotes().count(), 1, "{subs}");
        assert_eq!(parsed.document().references().len(), 1, "{subs}");
        assert!(parsed.document().references().contains("target"), "{subs}");
    }
    Ok(())
}

#[test]
fn attribute_first_code_checks_expanded_content_and_keeps_default_order() -> Result<(), Error> {
    for subs in [
        "attributes,quotes",
        "quotes,attributes",
        "attributes+",
        "+attributes",
        "-attributes,attributes+",
        "normal",
    ] {
        for content in [
            "{empty}",
            "{empty}{empty}",
            "café{sp}",
            "{sp}café",
            "café {empty}",
            "{empty} café",
        ] {
            for newline in ["\n", "\r\n"] {
                let source = format!(
                    "= Code{newline}:empty:{newline}{newline}[subs=\"{subs}\"]{newline}α `{content}` ω.{newline}"
                );
                let parsed = parse(&source, &Options::default())?;
                let [BlockKind::Paragraph(paragraph)] = &*parsed
                    .document()
                    .blocks()
                    .map(BlockRef::kind)
                    .collect::<Vec<_>>()
                else {
                    return Err("expected one paragraph".into());
                };
                let code = paragraph
                    .content()
                    .inlines()
                    .find(|node| matches!(node.kind(), InlineKind::Monospace(_)));
                let attributes_first = matches!(
                    subs,
                    "attributes,quotes" | "attributes+" | "-attributes,attributes+"
                );
                assert_eq!(code.is_none(), attributes_first, "{source:?}");
                if let Some(code) = code {
                    let span = code.location();
                    assert_eq!(
                        &parsed.source()[span.absolute_start()..=span.absolute_end()],
                        format!("`{content}`"),
                        "{source:?}"
                    );
                } else {
                    assert!(
                        paragraph
                            .content()
                            .inlines()
                            .all(|node| matches!(node.kind(), InlineKind::Plain(_))),
                        "{source:?}"
                    );
                    let text = paragraph
                        .content()
                        .inlines()
                        .filter_map(|node| {
                            if let InlineKind::Plain(text) = node.kind() {
                                Some(text.text())
                            } else {
                                None
                            }
                        })
                        .collect::<String>();
                    assert_eq!(
                        text,
                        format!(
                            "α `{}` ω.",
                            content.replace("{empty}", "").replace("{sp}", " ")
                        )
                    );
                }
            }
        }
    }
    Ok(())
}

#[test]
fn attribute_first_code_uses_a_later_valid_closer_with_complete_locations() -> Result<(), Error> {
    for newline in ["\n", "\r\n"] {
        for gap in ["; ".to_owned(), format!(";{newline}")] {
            let quote = format!("`{{word}}{{sp}}`{gap}tail`");
            let source = format!(
                "= Code{newline}:word: café{newline}{newline}[subs=\"attributes,quotes\"]{newline}α {quote}**End** ω.{newline}"
            );
            let parsed = parse(&source, &Options::default())?;
            let [BlockKind::Paragraph(paragraph)] = &*parsed
                .document()
                .blocks()
                .map(BlockRef::kind)
                .collect::<Vec<_>>()
            else {
                return Err("expected one paragraph".into());
            };
            let code = paragraph
                .content()
                .inlines()
                .find_map(|node| {
                    if let InlineKind::Monospace(code) = node.kind() {
                        Some(code)
                    } else {
                        None
                    }
                })
                .ok_or("missing code through the last backtick")?;
            let span = code.location();
            assert_eq!(
                &parsed.source()[span.absolute_start()..=span.absolute_end()],
                quote.replace("\r\n", "\n")
            );
            let word = code
                .content()
                .inlines()
                .next()
                .ok_or("missing code content")?;
            let word_span = word.location();
            assert_eq!(
                &parsed.source()[word_span.absolute_start()..=word_span.absolute_end()],
                format!("{{word}}{{sp}}`{}tail", gap.replace("\r\n", "\n"))
            );
            let sibling = paragraph
                .content()
                .inlines()
                .find(|node| matches!(node.kind(), InlineKind::Bold(_)))
                .ok_or("missing following bold span")?;
            let span = sibling.location();
            assert_eq!(
                &parsed.source()[span.absolute_start()..=span.absolute_end()],
                "**End**"
            );
        }
    }
    Ok(())
}

#[test]
fn attribute_first_code_expands_references_when_quotes_are_disabled() -> Result<(), Error> {
    for subs in ["attributes", "-quotes"] {
        let source = format!("= Code\n:word: café\n\n[subs=\"{subs}\"]\nP `{{word}}`.\n");
        let parsed = parse(&source, &Options::default())?;
        let [BlockKind::Paragraph(paragraph)] = &*parsed
            .document()
            .blocks()
            .map(BlockRef::kind)
            .collect::<Vec<_>>()
        else {
            return Err("expected one paragraph".into());
        };
        let [InlineKind::Plain(text)] = &*paragraph
            .content()
            .inlines()
            .map(InlineRef::kind)
            .collect::<Vec<_>>()
        else {
            return Err("expected literal backticks".into());
        };
        assert_eq!(text.text(), "P `café`.");
    }
    Ok(())
}

#[test]
fn attribute_first_code_preserves_passthrough_and_macro_locations() -> Result<(), Error> {
    for newline in ["\n", "\r\n"] {
        for marker in ["`", "``"] {
            let quote = format!(
                "{marker}+{{word}}+ {{word}} anchor:target[]footnote:[Only once.]indexterm2:[Term]{marker}"
            );
            let source = format!(
                "= Code{newline}:word: café{newline}{newline}[subs=\"attributes,quotes,macros\"]{newline}α pass:[before] {quote} pass:[after] ω.{newline}"
            );
            let parsed = parse(&source, &Options::default())?;
            let document = parsed.document();
            let [BlockKind::Paragraph(paragraph)] =
                &*document.blocks().map(BlockRef::kind).collect::<Vec<_>>()
            else {
                return Err("expected one paragraph".into());
            };
            let code = paragraph
                .content()
                .inlines()
                .find_map(|node| {
                    if let InlineKind::Monospace(code) = node.kind() {
                        Some(code)
                    } else {
                        None
                    }
                })
                .ok_or("missing code span")?;
            let span = code.location();
            assert_eq!(
                &parsed.source()[span.absolute_start()..=span.absolute_end()],
                quote
            );
            let raw = code
                .content()
                .inlines()
                .next()
                .ok_or("missing protected reference")?;
            let InlineKind::Raw(raw) = raw.kind() else {
                return Err(format!("expected raw reference: {raw:?}").into());
            };
            assert_eq!(raw.text(), "{word}");
            assert_eq!(
                &parsed.source()[raw.location().absolute_start()..=raw.location().absolute_end()],
                "{word}"
            );
            assert_eq!(document.footnotes().count(), 1);
            assert!(document.references().contains("target"));
            for (expected, present) in [
                (
                    "footnote:[Only once.]",
                    code.content()
                        .inlines()
                        .find(|node| matches!(node.kind(), InlineKind::Footnote(_))),
                ),
                (
                    "indexterm2:[Term]",
                    code.content()
                        .inlines()
                        .find(|node| matches!(node.kind(), InlineKind::IndexTerm(_))),
                ),
            ] {
                let span = present
                    .ok_or_else(|| format!("missing {expected}"))?
                    .location();
                assert_eq!(
                    &parsed.source()[span.absolute_start()..=span.absolute_end()],
                    expected
                );
            }
            let raw_siblings = paragraph
                .content()
                .inlines()
                .filter_map(|node| {
                    if let InlineKind::Raw(raw) = node.kind() {
                        Some(raw.text())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            assert_eq!(raw_siblings, ["before", "after"]);
        }
    }
    Ok(())
}
