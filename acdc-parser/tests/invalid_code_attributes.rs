use acdc_parser::{Block, InlineNode, Options, parse};

type Error = Box<dyn std::error::Error>;

#[test]
fn invalid_code_boundaries_do_not_hide_attribute_references() -> Result<(), Error> {
    for (before, after) in [
        ("A", "B"),
        ("A", ""),
        ("", "B"),
        ("é", "日"),
        ("_", "X"),
        (":", ""),
        (";", ""),
        ("}", ""),
        ("", "\""),
    ] {
        for (value, expected) in [("café", "café"), ("", ""), ("{sp}", " ")] {
            for newline in ["\n", "\r\n"] {
                let body = format!("α {before}`{{word}}`{after} ω.");
                let source = format!(":word: {value}{newline}{newline}{body}");
                let parsed = parse(&source, &Options::default())?;
                let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
                    return Err("expected one paragraph".into());
                };
                let [InlineNode::PlainText(text)] = paragraph.content.as_slice() else {
                    return Err(format!("expected literal backticks: {source:?}").into());
                };
                assert_eq!(text.content, body.replace("{word}", expected), "{source:?}");
                assert_eq!(
                    parsed
                        .source()
                        .get(text.location.absolute_start..=text.location.absolute_end),
                    Some(body.as_str())
                );
            }
        }
    }
    Ok(())
}

#[test]
fn invalid_code_edges_and_escapes_expand_attributes() -> Result<(), Error> {
    for (body, expected) in [
        ("`{word} `", "`café `"),
        ("` {word}`", "` café`"),
        ("`{word}\t`", "`café\t`"),
        ("`{word}\n`", "`café\n`"),
        ("\\`{word}`", "`café`"),
        ("\\``{word}``", "``café``"),
        ("A`\\{word}`B", "A`{word}`B"),
        ("A`{word\\}`B", "A`{word}`B"),
    ] {
        let source = format!(":word: café\n\nα {body} ω.");
        let parsed = parse(&source, &Options::default())?;
        let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
            return Err("expected one paragraph".into());
        };
        let mut text = String::new();
        for node in &paragraph.content {
            if let InlineNode::PlainText(plain) = node {
                text.push_str(plain.content);
            } else if let InlineNode::RawText(raw) = node {
                text.push_str(raw.content);
            } else {
                return Err(format!("expected literal code syntax: {source:?}: {node:?}").into());
            }
        }
        assert_eq!(text, format!("α {expected} ω."), "{source:?}");
    }
    Ok(())
}

#[test]
fn deferred_code_uses_the_later_valid_closer() -> Result<(), Error> {
    for rejected in ["`B", " `", "\t`", "\n`"] {
        let code_source = format!("`{{word}}{rejected} then +{{word}}+`");
        let source = format!(":word: café\n\nα {code_source}**End** ω.");
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
            .ok_or("missing code")?;
        let [InlineNode::PlainText(text), InlineNode::RawText(raw)] = code.content.as_slice()
        else {
            return Err(format!(
                "expected ordinary text and protected reference: {:?}",
                code.content
            )
            .into());
        };
        assert_eq!(text.content, format!("café{rejected} then "));
        assert_eq!(raw.content, "{word}");
        assert_eq!(
            source.get(code.location.absolute_start..=code.location.absolute_end),
            Some(code_source.as_str())
        );
        assert_eq!(
            source.get(raw.location.absolute_start..=raw.location.absolute_end),
            Some("{word}")
        );
        let bold = paragraph
            .content
            .iter()
            .find(|node| matches!(node, InlineNode::BoldText(_)))
            .ok_or("missing following bold span")?;
        let location = bold.location();
        assert_eq!(
            source.get(location.absolute_start..=location.absolute_end),
            Some("**End**")
        );
    }
    Ok(())
}

#[test]
fn expanded_references_do_not_create_new_code_boundaries() -> Result<(), Error> {
    for body in [
        "A`{empty}`B | A`{sp}`B | A`{unknown}`B.",
        "A`\\{word}`B | A`{word\\}`B.",
    ] {
        let source = format!(":word: café\n:empty:\n\n{body}");
        let parsed = parse(&source, &Options::default())?;
        let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
            return Err("expected paragraph".into());
        };
        assert!(
            !paragraph
                .content
                .iter()
                .any(|node| matches!(node, InlineNode::MonospaceText(_))),
            "{body}: {:?}",
            paragraph.content
        );
    }
    let source = ":word: café\n\n[red]`{word}`X | [red]`{word}`.";
    let parsed = parse(source, &Options::default())?;
    let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
        return Err("expected paragraph".into());
    };
    let [InlineNode::MonospaceText(code), InlineNode::PlainText(tail)] =
        paragraph.content.as_slice()
    else {
        return Err(format!(
            "expected one code span and its tail: {:?}",
            paragraph.content
        )
        .into());
    };
    assert_eq!(tail.content, "café`.");
    assert_eq!(
        source.get(code.location.absolute_start..=code.location.absolute_end),
        Some("`{word}`X | [red]`")
    );
    Ok(())
}

#[test]
fn invalid_code_expansion_retains_include_origins() -> Result<(), Error> {
    let parsed = acdc_parser::parse_file(
        "fixtures/tests/subs_invalid_code_attributes.adoc",
        &Options::builder()
            .with_safe_mode(acdc_parser::SafeMode::Unsafe)
            .build()?,
    )?;
    let text = parsed
        .document()
        .blocks
        .iter()
        .find_map(|block| {
            let Block::Section(section) = block else {
                return None;
            };
            section.content.iter().find_map(|block| {
                let Block::Paragraph(paragraph) = block else {
                    return None;
                };
                paragraph.content.iter().find_map(|node| {
                    let InlineNode::PlainText(text) = node else {
                        return None;
                    };
                    text.content.starts_with("B44 ").then_some(text)
                })
            })
        })
        .ok_or("missing included content")?;
    assert_eq!(text.content, "B44 A`late`B.");
    let source = include_str!("../fixtures/tests/includes/invalid_code_attributes.adoc");
    assert_eq!(
        source.get(text.location.absolute_start..=text.location.absolute_end),
        Some("B44 A`{word}`B.")
    );
    assert!(
        text.location
            .start
            .file
            .as_ref()
            .and_then(|files| files.last())
            .is_some_and(|file| file.ends_with("includes/invalid_code_attributes.adoc"))
    );
    assert_eq!(text.location.start.file, text.location.end.file);
    Ok(())
}
