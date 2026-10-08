use acdc_parser::{Block, InlineNode, Location, Options, parse};

type Error = Box<dyn std::error::Error>;

fn collect_text(
    nodes: &[InlineNode<'_>],
    text: &mut String,
    protected: &mut Vec<(String, Location)>,
) -> Result<(), Error> {
    for node in nodes {
        if let InlineNode::PlainText(plain) = node {
            text.push_str(plain.content);
        } else if let InlineNode::RawText(raw) = node {
            text.push_str(raw.content);
            protected.push((raw.content.to_owned(), raw.location.clone()));
        } else if let InlineNode::MonospaceText(code) = node {
            collect_text(&code.content, text, protected)?;
        } else if matches!(node, InlineNode::LineBreak(_)) {
            text.push('\n');
        } else {
            return Err(format!("unexpected inline: {node:?}").into());
        }
    }
    Ok(())
}

fn assert_source_span(source: &str, location: &Location, expected: &str) -> Result<(), Error> {
    let last = source
        .get(location.absolute_end..)
        .and_then(|tail| tail.chars().next())
        .ok_or("invalid source endpoint")?;
    let span = source
        .get(location.absolute_start..location.absolute_end + last.len_utf8())
        .ok_or("invalid source span")?;
    assert_eq!(span.replace("\r\n", "\n"), expected);
    for (offset, position) in [
        (location.absolute_start, &location.start),
        (location.absolute_end, &location.end),
    ] {
        let prefix = source.get(..offset).ok_or("invalid character boundary")?;
        assert_eq!(
            position.line as usize,
            prefix.bytes().filter(|byte| *byte == b'\n').count() + 1
        );
        assert_eq!(
            position.column as usize,
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
        let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
            return Err("expected one paragraph".into());
        };
        let mut text = String::new();
        let mut protected = Vec::new();
        collect_text(&paragraph.content, &mut text, &mut protected)?;
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
            let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
                return Err("expected one paragraph".into());
            };
            let mut text = String::new();
            let mut protected = Vec::new();
            collect_text(&paragraph.content, &mut text, &mut protected)?;
            assert_eq!(text, format!("α {expected} ω."), "{source:?}");
            assert_eq!(protected.len(), usize::from(raw.is_some()), "{source:?}");
            if let (Some(expected_raw), [(content, location)]) = (raw, protected.as_slice()) {
                assert_eq!(content, expected_raw, "{source:?}");
                assert_source_span(parsed.source(), location, expected_raw)?;
            }
            assert_eq!(parsed.document().footnotes, []);
            assert!(parsed.document().references.is_empty());
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
                let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
                    return Err(format!("expected one paragraph: {source:?}").into());
                };
                let mut text = String::new();
                let mut protected = Vec::new();
                collect_text(&paragraph.content, &mut text, &mut protected)?;
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
                    assert_source_span(parsed.source(), location, content)?;
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
        let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
            return Err("expected one paragraph".into());
        };
        let mut text = String::new();
        let mut protected = Vec::new();
        collect_text(&paragraph.content, &mut text, &mut protected)?;
        assert_eq!(text, format!("α {expected} ω."), "{source:?}");
        for (content, location) in &protected {
            assert_source_span(parsed.source(), location, content)?;
        }
    }
    Ok(())
}

#[cfg(feature = "pre-spec-subs")]
#[test]
fn disabled_single_plus_stages_remain_disabled() -> Result<(), Error> {
    let body = "A+{word}+B | '+{word}+'.";
    for (subs, expected) in [("attributes", "A+café+B | '+café+'."), ("none", body)] {
        let source = format!(":word: café\n\n[subs=\"{subs}\"]\n{body}");
        let parsed = parse(&source, &Options::default())?;
        let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
            return Err("expected one paragraph".into());
        };
        let [InlineNode::PlainText(text)] = paragraph.content.as_slice() else {
            return Err("disabled macro syntax must remain plain text".into());
        };
        assert_eq!(text.content, expected);
    }
    let source = ":word: café\n\n[subs=\"-attributes\"]\nA+{word}+B | +{word}+.";
    let parsed = parse(source, &Options::default())?;
    let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
        return Err("expected one paragraph".into());
    };
    let mut text = String::new();
    let mut protected = Vec::new();
    collect_text(&paragraph.content, &mut text, &mut protected)?;
    assert_eq!(text, "A+{word}B | +{word}.");
    assert_eq!(protected.len(), 1);
    Ok(())
}

#[test]
fn single_plus_source_spans_retain_include_origins() -> Result<(), Error> {
    let parsed = acdc_parser::parse_file(
        "fixtures/tests/subs_single_plus_boundaries.adoc",
        &Options::default(),
    )?;
    let included_source = include_str!("../fixtures/tests/includes/single-plus.adoc");
    let mut checked = 0;
    for block in &parsed.document().blocks {
        let Block::Section(section) = block else {
            continue;
        };
        for block in &section.content {
            let Block::Paragraph(paragraph) = block else {
                continue;
            };
            let Some(InlineNode::PlainText(first)) = paragraph.content.first() else {
                continue;
            };
            if !first.content.starts_with("S37 ") && !first.content.starts_with("S38 ") {
                continue;
            }
            let mut text = String::new();
            let mut protected = Vec::new();
            collect_text(&paragraph.content, &mut text, &mut protected)?;
            let [(content, location)] = protected.as_slice() else {
                return Err("expected one included passthrough".into());
            };
            assert_source_span(included_source, location, content)?;
            assert!(
                location
                    .start
                    .file
                    .as_ref()
                    .and_then(|files| files.last())
                    .is_some_and(|file| file.ends_with("includes/single-plus.adoc"))
            );
            assert_eq!(location.start.file, location.end.file);
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
        let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
            return Err("expected one paragraph".into());
        };
        let mut text = String::new();
        let mut protected = Vec::new();
        collect_text(&paragraph.content, &mut text, &mut protected)?;
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
            let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
                return Err("expected one paragraph".into());
            };
            let mut text = String::new();
            let mut protected = Vec::new();
            collect_text(&paragraph.content, &mut text, &mut protected)?;
            let expected = if word { "+café+" } else { "{word}" };
            assert_eq!(
                text,
                format!("α {before}{expected}{after} ω."),
                "{source:?}"
            );
            assert_eq!(protected.len(), usize::from(!word), "{source:?}");
            for (content, location) in &protected {
                assert_source_span(parsed.source(), location, content)?;
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
        let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
            return Err("expected one paragraph".into());
        };
        let mut text = String::new();
        collect_text(&paragraph.content, &mut text, &mut Vec::new())?;
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
            let [Block::UnorderedList(list)] = parsed.document().blocks.as_slice() else {
                return Err(format!("expected one list: {source:?}").into());
            };
            let [item] = list.items.as_slice() else {
                return Err("expected one list item".into());
            };
            let [Block::Paragraph(first), Block::Paragraph(second)] = item.blocks.as_slice() else {
                return Err(format!("expected two attached paragraphs: {source:?}").into());
            };
            for (paragraph, expected) in
                [(first, "First paragraph."), (second, "Second paragraph.")]
            {
                let mut text = String::new();
                collect_text(&paragraph.content, &mut text, &mut Vec::new())?;
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
    let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
        return Err("expected one paragraph".into());
    };
    let mut text = String::new();
    let mut protected = Vec::new();
    collect_text(&paragraph.content, &mut text, &mut protected)?;
    assert_eq!(text, format!("α `{content}`B."));
    assert_eq!(protected.len(), 1);
    assert_eq!(parsed.document().footnotes, []);
    assert!(parsed.document().references.is_empty());
    Ok(())
}
