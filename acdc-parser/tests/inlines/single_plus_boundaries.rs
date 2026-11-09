use acdc_parser::{
    Options,
    ast::{BlockKind, BlockRef, ContentRef, InlineKind, InlineRef, LocationRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

fn collect_text<'doc>(
    nodes: ContentRef<'doc>,
    text: &mut String,
    protected: &mut Vec<(String, LocationRef<'doc>)>,
) -> Result<(), Error> {
    for node in nodes.inlines() {
        if let InlineKind::Plain(plain) = node.kind() {
            text.push_str(plain.text());
        } else if let InlineKind::Raw(raw) = node.kind() {
            text.push_str(raw.text());
            protected.push((raw.text().to_owned(), raw.location()));
        } else if let InlineKind::Monospace(code) = node.kind() {
            collect_text(code.content(), text, protected)?;
        } else if matches!(node.kind(), InlineKind::LineBreak(_)) {
            text.push('\n');
        } else {
            return Err(format!("unexpected inline: {node:?}").into());
        }
    }
    Ok(())
}

fn assert_source_span(
    source: &str,
    location: LocationRef<'_>,
    expected: &str,
) -> Result<(), Error> {
    let last = source
        .get(location.absolute_end()..)
        .and_then(|tail| tail.chars().next())
        .ok_or("invalid source endpoint")?;
    let span = source
        .get(location.absolute_start()..location.absolute_end() + last.len_utf8())
        .ok_or("invalid source span")?;
    assert_eq!(span.replace("\r\n", "\n"), expected);
    for (offset, position) in [
        (location.absolute_start(), location.start()),
        (location.absolute_end(), location.end()),
    ] {
        let prefix = source.get(..offset).ok_or("invalid character boundary")?;
        assert_eq!(
            position.line() as usize,
            prefix.bytes().filter(|byte| *byte == b'\n').count() + 1
        );
        assert_eq!(
            position.column() as usize,
            prefix
                .rsplit('\n')
                .next()
                .unwrap_or_default()
                .chars()
                .count()
                + 1
        );
    }
    Ok(())
}

#[test]
fn single_plus_empty_and_unmatched_candidates_remain_literal() -> Result<(), Error> {
    for source in ["++", "Text'+", "Text +unfinished"] {
        let parsed = parse(source, &Options::default())?;
        let [BlockKind::Paragraph(paragraph)] = &*parsed
            .document()
            .blocks()
            .map(BlockRef::kind)
            .collect::<Vec<_>>()
        else {
            return Err("expected one paragraph".into());
        };
        let mut text = String::new();
        let mut protected = Vec::new();
        collect_text(paragraph.content(), &mut text, &mut protected)?;
        assert_eq!(text, source);
        assert!(protected.is_empty(), "{source:?}");
    }
    Ok(())
}

#[test]
fn single_plus_invalid_candidates_keep_references_and_later_openers_visible() -> Result<(), Error> {
    for (body, expected, raw) in [
        ("A+{word}+B", "A+café+B", None),
        ("+ {word} +", "+ café +", None),
        ("+{word}+", "{word}", Some("{word}")),
        (
            "A+{word}+B | + {word} + | +{word}+",
            "A+caféB | + {word} + | +{word}",
            Some("B | + {word} + | +{word}"),
        ),
        (
            "+{word} + then +{word}+",
            "{word} + then +{word}",
            Some("{word} + then +{word}"),
        ),
        (
            "+{word}\n+ then +{word}+",
            "{word}\n+ then +{word}",
            Some("{word}\n+ then +{word}"),
        ),
        ("A`+{word}+`B", "A`{word}`B", Some("{word}")),
        ("`+{word}+`", "{word}", Some("{word}")),
    ] {
        for newline in ["\n", "\r\n"] {
            let source = format!(":word: café\n\nα {body} ω.").replace('\n', newline);
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
            let mut protected = Vec::new();
            collect_text(paragraph.content(), &mut text, &mut protected)?;
            assert_eq!(text, format!("α {expected} ω."), "{source:?}");
            assert_eq!(protected.len(), usize::from(raw.is_some()), "{source:?}");
            if let (Some(expected_raw), [(content, location)]) = (raw, protected.as_slice()) {
                assert_eq!(content, expected_raw, "{source:?}");
                assert_source_span(parsed.preprocessed_source(), *location, expected_raw)?;
            }
            assert_eq!(parsed.document().footnotes().count(), 0);
            assert!(parsed.document().references().iter().next().is_none());
        }
    }
    Ok(())
}

#[test]
fn single_plus_content_edges_distinguish_ascii_whitespace_from_nbsp() -> Result<(), Error> {
    for edge in [" ", "\t", "\n", "\u{b}", "\u{c}", "\u{a0}"] {
        for (left, right) in [(edge, ""), ("", edge)] {
            for newline in ["\n", "\r\n"] {
                let source =
                    format!(":word: café\n\nα +{left}{{word}}{right}+ ω.").replace('\n', newline);
                let parsed = parse(&source, &Options::default())?;
                let [BlockKind::Paragraph(paragraph)] = &*parsed
                    .document()
                    .blocks()
                    .map(BlockRef::kind)
                    .collect::<Vec<_>>()
                else {
                    return Err(format!("expected one paragraph: {source:?}").into());
                };
                let mut text = String::new();
                let mut protected = Vec::new();
                collect_text(paragraph.content(), &mut text, &mut protected)?;
                let recognized = edge == "\u{a0}";
                let expected = if recognized {
                    format!("α {left}{{word}}{right} ω.")
                } else if left == "\n" {
                    "α\ncafé+ ω.".to_owned()
                } else {
                    format!("α +{left}café{right}+ ω.")
                };
                assert_eq!(text, expected, "{source:?}");
                assert_eq!(protected.len(), usize::from(recognized), "{source:?}");
                for (content, location) in &protected {
                    assert_source_span(parsed.preprocessed_source(), *location, content)?;
                }
            }
        }
    }
    Ok(())
}

#[test]
fn escaped_single_plus_and_wrappers_use_the_same_closing_rules() -> Result<(), Error> {
    for (body, expected) in [
        ("\\+{word}+", "+café+"),
        ("A\\+{word}+", "A+café+"),
        ("A\\+{word}+B", "A\\+café+B"),
        ("é\\+{word} +", "é\\+café +"),
        ("'\\+{word}+'", "'+café+'"),
        ("\\+{word} + then +{word}+", "+café + then +café+"),
        ("\\++before +{word}+ after++", "+before +{word} after++"),
        ("\\+++before +{word}+ after+++", "++before +{word} after+++"),
        (
            "\\pass:[before '+{word}+' after]",
            "pass:[before '{word}' after]",
        ),
    ] {
        let source = format!(":word: café\n\nα {body} ω.");
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
        let mut protected = Vec::new();
        collect_text(paragraph.content(), &mut text, &mut protected)?;
        assert_eq!(text, format!("α {expected} ω."), "{source:?}");
        for (content, location) in &protected {
            assert_source_span(parsed.preprocessed_source(), *location, content)?;
        }
    }
    Ok(())
}

#[test]
fn disabled_single_plus_stages_remain_disabled() -> Result<(), Error> {
    let body = "A+{word}+B | '+{word}+'.";
    for (subs, expected) in [("attributes", "A+café+B | '+café+'."), ("none", body)] {
        let source = format!(":word: café\n\n[subs=\"{subs}\"]\n{body}");
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
            return Err("disabled macro syntax must remain plain text".into());
        };
        assert_eq!(text.text(), expected);
    }
    let source = ":word: café\n\n[subs=\"-attributes\"]\nA+{word}+B | +{word}+.";
    let parsed = parse(source, &Options::default())?;
    let [BlockKind::Paragraph(paragraph)] = &*parsed
        .document()
        .blocks()
        .map(BlockRef::kind)
        .collect::<Vec<_>>()
    else {
        return Err("expected one paragraph".into());
    };
    let mut text = String::new();
    let mut protected = Vec::new();
    collect_text(paragraph.content(), &mut text, &mut protected)?;
    assert_eq!(text, "A+{word}B | +{word}.");
    assert_eq!(protected.len(), 1);
    Ok(())
}

#[test]
fn single_plus_source_spans_retain_include_origins() -> Result<(), Error> {
    let parsed = acdc_parser::parse_file(
        "fixtures/tests/subs_single_plus_boundaries.adoc",
        &Options::builder()
            .with_safe_mode(acdc_parser::SafeMode::Unsafe)
            .build()?,
    )?;
    let included_source = include_str!("../../fixtures/tests/includes/single-plus.adoc");
    let mut checked = 0;
    for block in parsed.document().blocks() {
        let BlockKind::Section(section) = block.kind() else {
            continue;
        };
        for block in section.blocks() {
            let BlockKind::Paragraph(paragraph) = block.kind() else {
                continue;
            };
            let Some(InlineKind::Plain(first)) =
                paragraph.content().inlines().next().map(InlineRef::kind)
            else {
                continue;
            };
            if !first.text().starts_with("S37 ") && !first.text().starts_with("S38 ") {
                continue;
            }
            let mut text = String::new();
            let mut protected = Vec::new();
            collect_text(paragraph.content(), &mut text, &mut protected)?;
            let [(content, location)] = protected.as_slice() else {
                return Err("expected one included passthrough".into());
            };
            assert_source_span(included_source, *location, content)?;
            assert!(
                location
                    .start()
                    .include_chain()
                    .and_then(Iterator::last)
                    .is_some_and(|file| file.ends_with("includes/single-plus.adoc"))
            );
            assert_eq!(
                location
                    .start()
                    .include_chain()
                    .map(Iterator::collect::<Vec<_>>),
                location
                    .end()
                    .include_chain()
                    .map(Iterator::collect::<Vec<_>>)
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 2);
    Ok(())
}

#[test]
fn single_plus_boundaries_accept_punctuation_and_reject_words() -> Result<(), Error> {
    for (before, after, recognized) in [
        ("", "", true),
        ("'", "'", true),
        ("`", ".", true),
        ("(", "`", true),
        ("}", ")", true),
        ("«", "»", true),
        ("—", "—", true),
        (":", ".", false),
        (";", ".", false),
        ("A", ".", false),
        ("é", ".", false),
        ("", "日", false),
        ("_", "", false),
    ] {
        let source = format!(":word: café\n\nα {before}+{{word}}+{after} ω.");
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
        let mut protected = Vec::new();
        collect_text(paragraph.content(), &mut text, &mut protected)?;
        let expected = if recognized { "{word}" } else { "+café+" };
        assert_eq!(
            text,
            format!("α {before}{expected}{after} ω."),
            "{source:?}"
        );
        assert_eq!(protected.len(), usize::from(recognized), "{source:?}");
    }
    Ok(())
}

#[test]
fn single_plus_unicode_word_boundaries_preserve_references() -> Result<(), Error> {
    for (character, word) in [
        ("\u{301}", true),
        ("e\u{301}", true),
        ("\u{903}", true),
        ("\u{20dd}", true),
        ("\u{203f}", true),
        ("\u{200c}", true),
        ("\u{200d}", true),
        ("²", false),
        ("¼", false),
        ("Ⅰ", true),
        ("١", true),
        ("é", true),
        ("日", true),
        ("7", true),
    ] {
        for (before, after) in [(character, ""), ("", character)] {
            let source = format!(":word: café\n\nα {before}+{{word}}+{after} ω.");
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
            let mut protected = Vec::new();
            collect_text(paragraph.content(), &mut text, &mut protected)?;
            let expected = if word { "+café+" } else { "{word}" };
            assert_eq!(
                text,
                format!("α {before}{expected}{after} ω."),
                "{source:?}"
            );
            assert_eq!(protected.len(), usize::from(!word), "{source:?}");
            for (content, location) in &protected {
                assert_source_span(parsed.preprocessed_source(), *location, content)?;
            }
        }
    }
    Ok(())
}

#[test]
fn plus_prefixed_text_does_not_split_an_ordinary_paragraph() -> Result<(), Error> {
    for newline in ["\n", "\r\n"] {
        let source = "Before.\n+ text\nAfter.".replace('\n', newline);
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
        collect_text(paragraph.content(), &mut text, &mut Vec::new())?;
        assert_eq!(text, "Before.\n+ text\nAfter.");
    }
    Ok(())
}

#[test]
fn standalone_plus_still_attaches_list_continuations() -> Result<(), Error> {
    for continuation in ["+", "+ ", "+\t"] {
        for newline in ["\n", "\r\n"] {
            let source = format!(
                "* Entry\n{continuation}\nFirst paragraph.\n{continuation}\nSecond paragraph."
            )
            .replace('\n', newline);
            let parsed = parse(&source, &Options::default())?;
            let [BlockKind::UnorderedList(list)] = &*parsed
                .document()
                .blocks()
                .map(BlockRef::kind)
                .collect::<Vec<_>>()
            else {
                return Err(format!("expected one list: {source:?}").into());
            };
            let [item] = &*list.items().collect::<Vec<_>>() else {
                return Err("expected one list item".into());
            };
            let [BlockKind::Paragraph(first), BlockKind::Paragraph(second)] =
                &*item.blocks().map(BlockRef::kind).collect::<Vec<_>>()
            else {
                return Err(format!("expected two attached paragraphs: {source:?}").into());
            };
            for (paragraph, expected) in
                [(first, "First paragraph."), (second, "Second paragraph.")]
            {
                let mut text = String::new();
                collect_text(paragraph.content(), &mut text, &mut Vec::new())?;
                assert_eq!(text, expected, "{source:?}");
            }
        }
    }
    Ok(())
}

#[test]
fn single_plus_protected_macros_do_not_register_catalog_entries() -> Result<(), Error> {
    let content = "link:https://example.org[Site] anchor:target[] footnote:[Note] ((Term))";
    let source = format!("α `+{content}+`B.");
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
    let mut protected = Vec::new();
    collect_text(paragraph.content(), &mut text, &mut protected)?;
    assert_eq!(text, format!("α `{content}`B."));
    assert_eq!(protected.len(), 1);
    assert_eq!(parsed.document().footnotes().count(), 0);
    assert!(parsed.document().references().iter().next().is_none());
    Ok(())
}
