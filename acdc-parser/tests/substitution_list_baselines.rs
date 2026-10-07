use acdc_parser::{Options, parse};

type Error = Box<dyn std::error::Error>;

#[cfg(feature = "pre-spec-subs")]
#[test]
fn parsed_substitution_entries_preserve_source_spelling() -> Result<(), Error> {
    use acdc_parser::{Block, NORMAL, SubstitutionSpec, VERBATIM};

    for (value, expected) in [
        ("verbatim,-macros", vec!["verbatim", "-macros"]),
        ("normal", vec!["normal"]),
        ("verbatim", vec!["verbatim"]),
        ("none,+quotes", vec!["none", "+quotes"]),
        ("none,none", vec!["none", "none"]),
        ("none,", vec!["none", ""]),
        ("+none", vec!["+none"]),
        ("-none", vec!["-none"]),
        ("none+", vec!["none+"]),
        ("a,q,c", vec!["a", "q", "c"]),
        ("specialcharacters", vec!["specialcharacters"]),
        ("quotes,quotes", vec!["quotes", "quotes"]),
        ("quotes,+attributes", vec!["quotes", "+attributes"]),
        ("+quotes,attributes", vec!["+quotes", "attributes"]),
        ("-normal,attributes+", vec!["-normal", "attributes+"]),
        ("typo,+quotes", vec!["typo", "+quotes"]),
        (",quotes,,", vec!["", "quotes", "", ""]),
        (" verbatim , -macros ", vec!["verbatim", "-macros"]),
        ("{chosen}", vec!["verbatim", "-macros"]),
    ] {
        let source = format!(":chosen: verbatim,-macros\n\n[subs=\"{value}\"]\nContent.\n");
        let parsed = parse(&source, &Options::default())?;
        let Some(Block::Paragraph(paragraph)) = parsed.document().blocks.first() else {
            return Err("expected a paragraph".into());
        };
        let spec = paragraph
            .metadata
            .substitutions
            .as_ref()
            .ok_or("missing substitutions")?;
        let SubstitutionSpec::Source(entries) = spec else {
            return Err("expected authored substitution entries".into());
        };
        assert_eq!(entries, &expected, "{value}");
        assert_eq!(
            serde_json::to_value(spec)?,
            serde_json::to_value(&expected)?,
            "{value}"
        );
        for baseline in [NORMAL, VERBATIM, &[]] {
            let _ = spec.resolve(baseline);
        }
        assert_eq!(
            serde_json::to_value(spec)?,
            serde_json::to_value(&expected)?,
            "{value}"
        );
    }
    Ok(())
}

#[cfg(feature = "pre-spec-subs")]
#[test]
fn empty_substitution_specs_remain_distinct_from_block_defaults() -> Result<(), Error> {
    use acdc_parser::{Block, NORMAL, SubstitutionSpec, VERBATIM};

    for metadata in [
        "",
        "[subs=none]",
        r#"[subs="none"]"#,
        "[subs='none']",
        "[subs=\"  none\t \" ]",
        r#"[subs=""]"#,
        "[subs=\" \t \" ]",
        r#"[subs="{disabled}"]"#,
        r#"[subs="{blank}"]"#,
    ] {
        for newline in ["\n", "\r\n"] {
            let source = format!(
                ":disabled: none{newline}:blank:{newline}{newline}\
                 {metadata}{newline}Content.{newline}"
            );
            let parsed = parse(&source, &Options::default())?;
            let Some(Block::Paragraph(paragraph)) = parsed.document().blocks.first() else {
                return Err("expected a paragraph".into());
            };
            let json = serde_json::to_value(&paragraph.metadata)?;
            if metadata.is_empty() {
                assert!(paragraph.metadata.substitutions.is_none());
                assert!(json.get("substitutions").is_none());
            } else {
                let spec = paragraph
                    .metadata
                    .substitutions
                    .as_ref()
                    .ok_or("missing explicit empty substitutions")?;
                assert_eq!(spec, &SubstitutionSpec::Explicit(Vec::new()), "{source}");
                assert_eq!(
                    json.get("substitutions"),
                    Some(&serde_json::json!([])),
                    "{source}"
                );
                for baseline in [NORMAL, VERBATIM, &[]] {
                    assert!(spec.resolve(baseline).is_empty(), "{source}");
                }
            }
        }
    }
    Ok(())
}

#[cfg(feature = "pre-spec-subs")]
#[test]
fn typed_substitution_constructors_keep_their_existing_contract() -> Result<(), Error> {
    use acdc_parser::{NORMAL, Substitution, SubstitutionOp, SubstitutionSpec, VERBATIM};

    let explicit = SubstitutionSpec::Explicit(vec![Substitution::SpecialChars]);
    assert_eq!(explicit.resolve(NORMAL), [Substitution::SpecialChars]);
    assert_eq!(
        serde_json::to_value(&explicit)?,
        serde_json::json!(["special_chars"])
    );
    let modifiers = SubstitutionSpec::Modifiers(vec![
        SubstitutionOp::Append(Substitution::Normal),
        SubstitutionOp::Remove(Substitution::Callouts),
    ]);
    assert_eq!(modifiers.resolve(VERBATIM), NORMAL);
    assert_eq!(
        serde_json::to_value(&modifiers)?,
        serde_json::json!(["+normal", "-callouts"])
    );
    Ok(())
}

#[test]
fn plain_first_substitution_lists_do_not_register_disabled_macros() -> Result<(), Error> {
    for subs in [
        "none",
        "",
        "quotes,+attributes",
        "none,+attributes",
        "quotes,-quotes,+attributes",
        "unknown,+attributes",
        ",attributes+",
    ] {
        for newline in ["\n", "\r\n"] {
            let source = format!(
                ":value: anchor:from-value[]footnote:[Attribute note.]{newline}{newline}\
                 [subs=\"{subs}\"]{newline}\
                 {{value}} anchor:direct[]footnote:[Direct note.]{newline}"
            );
            let parsed = parse(&source, &Options::default())?;
            let expected = if cfg!(feature = "pre-spec-subs") {
                0
            } else {
                2
            };
            assert_eq!(parsed.document().footnotes.len(), expected, "{source}");
            assert_eq!(parsed.document().references.len(), expected, "{source}");
        }
    }
    Ok(())
}

#[test]
fn mixed_substitution_lists_keep_enabled_macro_order() -> Result<(), Error> {
    for (subs, expected) in [
        ("attributes,+macros", 2),
        ("macros,+attributes", 1),
        ("macros,attributes+", 2),
        ("+quotes,attributes", 2),
        ("+none,attributes", 2),
    ] {
        let source = format!(
            ":value: anchor:from-value[]footnote:[Attribute note.]\n\n\
             [subs=\"{subs}\"]\n\
             {{value}} anchor:direct[]footnote:[Direct note.]\n"
        );
        let parsed = parse(&source, &Options::default())?;
        let expected = if cfg!(feature = "pre-spec-subs") {
            expected
        } else {
            2
        };
        assert_eq!(parsed.document().footnotes.len(), expected, "{subs}");
        assert_eq!(parsed.document().references.len(), expected, "{subs}");
        assert!(
            parsed.document().references.contains_key("direct"),
            "{subs}"
        );
        assert_eq!(
            parsed.document().references.contains_key("from-value"),
            expected == 2,
            "{subs}"
        );
    }
    Ok(())
}
