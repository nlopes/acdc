use acdc_parser::{Block, InlineNode, Options, parse};

type Error = Box<dyn std::error::Error>;

#[cfg(feature = "pre-spec-subs")]
#[test]
fn disabled_passthroughs_use_the_enclosing_attribute_stage() -> Result<(), Error> {
    for subs in [
        "attributes",
        "-macros",
        "attributes,quotes",
        "quotes,attributes",
    ] {
        for body in [
            "+{word}+",
            "++{word}++",
            "+++{word}+++",
            "pass:[{word}]",
            "pass:a[{word}]",
            "pass:q[{word}]",
            "pass:n[{word}]",
            "\\+{word}+",
            "\\++{word}++",
            "\\+++{word}+++",
            "\\pass:[{word}]",
            "pass:[one [nested {word}]]",
            "pass:[{word}",
            "+{word}",
        ] {
            for newline in ["\n", "\r\n"] {
                let source =
                    format!(":word: café{newline}{newline}[subs=\"{subs}\"]{newline}α {body} ω.");
                let parsed = parse(&source, &Options::default())?;
                let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
                    return Err("expected one paragraph".into());
                };
                let [InlineNode::PlainText(text)] = paragraph.content.as_slice() else {
                    return Err(format!(
                        "expected literal macro syntax: {source}: {:?}",
                        paragraph.content
                    )
                    .into());
                };
                assert_eq!(
                    text.content,
                    format!("α {} ω.", body.replace("{word}", "café")),
                    "{source}"
                );
                assert_eq!(
                    parsed
                        .source()
                        .get(text.location.absolute_start..=text.location.absolute_end),
                    Some(format!("α {body} ω.").as_str())
                );
                assert!(parsed.document().footnotes.is_empty());
                assert!(parsed.document().references.is_empty());
            }
        }
    }
    Ok(())
}

#[cfg(feature = "pre-spec-subs")]
#[test]
fn disabled_pass_substitution_lists_cannot_enable_attributes() -> Result<(), Error> {
    for subs in ["none", "quotes", "specialcharacters,quotes"] {
        for body in [
            "+{word}+",
            "++{word}++",
            "+++{word}+++",
            "pass:[{word}]",
            "pass:a[{word}]",
            "pass:normal[{word}]",
        ] {
            let source = format!(":word: café\n\n[subs=\"{subs}\"]\n{body}");
            let parsed = parse(&source, &Options::default())?;
            let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
                return Err("expected one paragraph".into());
            };
            let [InlineNode::PlainText(text)] = paragraph.content.as_slice() else {
                return Err("expected literal macro syntax".into());
            };
            assert_eq!(text.content, body, "{subs}");
        }
    }
    Ok(())
}

#[cfg(feature = "pre-spec-subs")]
#[test]
fn disabled_passthroughs_expand_inside_code_without_losing_source_spans() -> Result<(), Error> {
    for subs in ["attributes,quotes", "quotes,attributes"] {
        for marker in ["`", "``"] {
            for body in [
                "+{word}+",
                "++{word}++",
                "+++{word}+++",
                "pass:[{word}]",
                "pass:a[{word}]",
            ] {
                let code_source = format!("{marker}{body}{marker}");
                let source = format!(":word: café\n\n[subs=\"{subs}\"]\nα {code_source}**End**.");
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
                    .ok_or("missing code span")?;
                let [InlineNode::PlainText(text)] = code.content.as_slice() else {
                    return Err("expected literal passthrough syntax inside code".into());
                };
                assert_eq!(text.content, body.replace("{word}", "café"), "{source}");
                assert_eq!(
                    source.get(code.location.absolute_start..=code.location.absolute_end),
                    Some(code_source.as_str())
                );
                assert_eq!(
                    source.get(text.location.absolute_start..=text.location.absolute_end),
                    Some(body)
                );
                let bold = paragraph
                    .content
                    .iter()
                    .find(|node| matches!(node, InlineNode::BoldText(_)))
                    .ok_or("missing following formatting")?;
                let span = bold.location();
                assert_eq!(
                    source.get(span.absolute_start..=span.absolute_end),
                    Some("**End**")
                );
            }
        }
    }
    Ok(())
}

#[test]
fn enabled_passthroughs_still_protect_attribute_references() -> Result<(), Error> {
    for body in ["+{word}+", "++{word}++", "+++{word}+++", "pass:[{word}]"] {
        let source = format!(":word: café\n\n{body}");
        let parsed = parse(&source, &Options::default())?;
        let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
            return Err("expected one paragraph".into());
        };
        let [InlineNode::RawText(raw)] = paragraph.content.as_slice() else {
            return Err("expected protected text".into());
        };
        assert_eq!(raw.content, "{word}");
    }
    Ok(())
}

#[cfg(feature = "pre-spec-subs")]
#[test]
fn disabled_passthrough_counters_use_the_existing_warning_policy() -> Result<(), Error> {
    for subs in ["attributes", "none"] {
        for body in [
            "+{counter:n}+",
            "++{counter:n}++",
            "+++{counter:n}+++",
            "pass:[{counter:n}]",
            "pass:a[{counter:n}]",
            "\\+++{counter:n}+++",
        ] {
            let source = format!("[subs=\"{subs}\"]\nα {body} ω.");
            let parsed = parse(&source, &Options::default())?;
            let warnings = parsed
                .warnings()
                .iter()
                .filter(|warning| warning.to_string().contains("Counters"))
                .collect::<Vec<_>>();
            assert_eq!(
                warnings.len(),
                usize::from(subs == "attributes"),
                "{source}"
            );
            if let [warning] = warnings.as_slice() {
                let location = &warning
                    .source_location()
                    .ok_or("missing counter location")?
                    .location;
                // Counter diagnostics use an exclusive source endpoint.
                assert_eq!(
                    source.get(location.absolute_start..location.absolute_end),
                    Some("{counter:n}"),
                    "{source}"
                );
            }
        }
    }
    Ok(())
}

#[cfg(feature = "pre-spec-subs")]
#[test]
fn disabled_passthrough_expansion_keeps_catalogs_empty_and_include_origins() -> Result<(), Error> {
    let parsed = acdc_parser::parse_file(
        "fixtures/tests/subs_disabled_passthrough_attributes.adoc",
        &Options::default(),
    )?;
    let document = parsed.document();
    assert!(document.footnotes.is_empty());
    assert!(!document.references.contains_key("target"));
    let paragraph = document.blocks.iter().find_map(|block| {
        if let Block::Section(section) = block {
            section.content.iter().find_map(|block| {
                if let Block::Paragraph(paragraph) = block {
                    paragraph.content.iter().any(|node| matches!(node, InlineNode::PlainText(text) if text.content.starts_with("D30 "))).then_some(paragraph)
                } else { None }
            })
        } else { None }
    }).ok_or("missing included paragraph")?;
    let [InlineNode::PlainText(text)] = paragraph.content.as_slice() else {
        return Err("expected literal included content".into());
    };
    assert_eq!(text.content, "D30 +late+ | pass:[late].");
    let source = include_str!("../fixtures/tests/includes/disabled_passthrough_attributes.adoc");
    assert_eq!(
        source.get(text.location.absolute_start..=text.location.absolute_end),
        Some("D30 +{word}+ | pass:[{word}].")
    );
    assert!(
        text.location
            .start
            .file
            .as_ref()
            .and_then(|files| files.last())
            .is_some_and(|file| file.ends_with("includes/disabled_passthrough_attributes.adoc"))
    );
    assert_eq!(text.location.start.file, text.location.end.file);
    Ok(())
}
