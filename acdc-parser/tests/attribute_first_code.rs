use acdc_parser::{Block, InlineNode, Options, parse};

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
        assert_eq!(parsed.document().footnotes.len(), 1, "{subs}");
        assert_eq!(parsed.document().references.len(), 1, "{subs}");
        assert!(
            parsed.document().references.contains_key("target"),
            "{subs}"
        );
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
                let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
                    return Err("expected one paragraph".into());
                };
                let code = paragraph
                    .content
                    .iter()
                    .find(|node| matches!(node, InlineNode::MonospaceText(_)));
                let attributes_first = cfg!(feature = "pre-spec-subs")
                    && matches!(
                        subs,
                        "attributes,quotes" | "attributes+" | "-attributes,attributes+"
                    );
                assert_eq!(code.is_none(), attributes_first, "{source:?}");
                if let Some(code) = code {
                    let span = code.location();
                    assert_eq!(
                        &parsed.source()[span.absolute_start..=span.absolute_end],
                        format!("`{content}`"),
                        "{source:?}"
                    );
                } else {
                    assert!(
                        paragraph
                            .content
                            .iter()
                            .all(|node| matches!(node, InlineNode::PlainText(_))),
                        "{source:?}"
                    );
                    let text = paragraph
                        .content
                        .iter()
                        .filter_map(|node| {
                            if let InlineNode::PlainText(text) = node {
                                Some(text.content)
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

#[cfg(feature = "pre-spec-subs")]
#[test]
fn attribute_first_code_uses_a_later_valid_closer_with_complete_locations() -> Result<(), Error> {
    for newline in ["\n", "\r\n"] {
        for gap in ["; ".to_owned(), format!(";{newline}")] {
            let quote = format!("`{{word}}{{sp}}`{gap}tail`");
            let source = format!(
                "= Code{newline}:word: café{newline}{newline}[subs=\"attributes,quotes\"]{newline}α {quote}**End** ω.{newline}"
            );
            let parsed = parse(&source, &Options::default())?;
            let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
                return Err("expected one paragraph".into());
            };
            let code = paragraph
                .content
                .iter()
                .find_map(|node| {
                    if let InlineNode::MonospaceText(code) = node {
                        Some(code)
                    } else {
                        None
                    }
                })
                .ok_or("missing code through the last backtick")?;
            let span = &code.location;
            assert_eq!(
                &parsed.source()[span.absolute_start..=span.absolute_end],
                quote.replace("\r\n", "\n")
            );
            let word = code.content.first().ok_or("missing code content")?;
            let word_span = word.location();
            assert_eq!(
                &parsed.source()[word_span.absolute_start..=word_span.absolute_end],
                format!("{{word}}{{sp}}`{}tail", gap.replace("\r\n", "\n"))
            );
            let sibling = paragraph
                .content
                .iter()
                .find(|node| matches!(node, InlineNode::BoldText(_)))
                .ok_or("missing following bold span")?;
            let span = sibling.location();
            assert_eq!(
                &parsed.source()[span.absolute_start..=span.absolute_end],
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
        let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
            return Err("expected one paragraph".into());
        };
        if cfg!(feature = "pre-spec-subs") {
            let [InlineNode::PlainText(text)] = paragraph.content.as_slice() else {
                return Err("expected literal backticks".into());
            };
            assert_eq!(text.content, "P `café`.");
        } else {
            assert!(
                paragraph
                    .content
                    .iter()
                    .any(|node| matches!(node, InlineNode::MonospaceText(_)))
            );
        }
    }
    Ok(())
}

#[cfg(feature = "pre-spec-subs")]
#[test]
fn attribute_first_code_preserves_passthrough_and_macro_locations() -> Result<(), Error> {
    use acdc_parser::InlineMacro;

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
            let [Block::Paragraph(paragraph)] = document.blocks.as_slice() else {
                return Err("expected one paragraph".into());
            };
            let code = paragraph
                .content
                .iter()
                .find_map(|node| {
                    if let InlineNode::MonospaceText(code) = node {
                        Some(code)
                    } else {
                        None
                    }
                })
                .ok_or("missing code span")?;
            let span = &code.location;
            assert_eq!(
                &parsed.source()[span.absolute_start..=span.absolute_end],
                quote
            );
            let raw = code.content.first().ok_or("missing protected reference")?;
            let InlineNode::RawText(raw) = raw else {
                return Err(format!("expected raw reference: {raw:?}").into());
            };
            assert_eq!(raw.content, "{word}");
            assert_eq!(
                &parsed.source()[raw.location.absolute_start..=raw.location.absolute_end],
                "{word}"
            );
            assert_eq!(document.footnotes.len(), 1);
            assert!(document.references.contains_key("target"));
            for (expected, present) in [
                (
                    "footnote:[Only once.]",
                    code.content
                        .iter()
                        .find(|node| matches!(node, InlineNode::Macro(InlineMacro::Footnote(_)))),
                ),
                (
                    "indexterm2:[Term]",
                    code.content
                        .iter()
                        .find(|node| matches!(node, InlineNode::Macro(InlineMacro::IndexTerm(_)))),
                ),
            ] {
                let span = present
                    .ok_or_else(|| format!("missing {expected}"))?
                    .location();
                assert_eq!(
                    &parsed.source()[span.absolute_start..=span.absolute_end],
                    expected
                );
            }
            let raw_siblings = paragraph
                .content
                .iter()
                .filter_map(|node| {
                    if let InlineNode::RawText(raw) = node {
                        Some(raw.content)
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
