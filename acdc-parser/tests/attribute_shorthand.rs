use acdc_parser::{
    CompatibilityMode, Options,
    ast::{AttributeValueRef, BlockKind, BlockRef, ElementIdRef, InlineKind},
    parse,
};

type Error = Box<dyn std::error::Error>;

#[test]
fn quoted_and_incomplete_shorthands_follow_the_selected_mode() -> Result<(), Error> {
    for source in ["\"note.demo\"", "'note.demo'", "note..demo", "note#"] {
        for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
            let input = format!("[{source}]\nText.");
            let parsed = parse(
                &input,
                &Options::builder().with_compatibility_mode(mode).build()?,
            )?;
            let Some(BlockKind::Paragraph(block)) =
                parsed.document().blocks().next().map(BlockRef::kind)
            else {
                return Err("expected a paragraph".into());
            };
            if mode == CompatibilityMode::Draft {
                assert_eq!(block.style(), Some(source.trim_matches(['\'', '"'])));
                assert_eq!(block.roles().iter().collect::<Vec<_>>(), [] as [&str; 0]);
                assert!(block.id().is_none());
            } else {
                assert_eq!(block.style(), Some("note"));
                assert_eq!(block.roles().contains("demo"), source.contains("demo"));
            }
        }
    }
    Ok(())
}

#[test]
fn draft_anchor_shorthand_keeps_reftext_and_uses_the_last_id() -> Result<(), Error> {
    let source = "[[early,Read, this\\] label]note#late.demo%ready]\nText.\n\nSee <<late>>.";
    let parsed = parse(
        source,
        &Options::builder()
            .with_compatibility_mode(CompatibilityMode::Draft)
            .build()?,
    )?;
    let Some(BlockKind::Paragraph(block)) = parsed.document().blocks().next().map(BlockRef::kind)
    else {
        return Err("expected a paragraph".into());
    };
    assert_eq!(block.style(), Some("note"));
    assert_eq!(block.id().map(ElementIdRef::as_str), Some("late"));
    assert_eq!(block.roles().iter().collect::<Vec<_>>(), ["demo"]);
    assert_eq!(block.options().iter().collect::<Vec<_>>(), ["ready"]);
    assert_eq!(
        block
            .attributes()
            .get("reftext")
            .and_then(|value| if let AttributeValueRef::Content(content) = value {
                content.inlines().next()
            } else {
                None
            })
            .and_then(|node| if let InlineKind::Plain(plain) = node.kind() {
                Some(plain.text())
            } else {
                None
            }),
        Some("Read, this] label")
    );
    assert!(parsed.document().references().contains("late"));
    assert!(!parsed.document().references().contains("early"));
    let location = block.id().ok_or("missing ID")?.location();
    assert_eq!(
        source.get(location.absolute_start()..=location.absolute_end()),
        Some("late")
    );
    Ok(())
}

#[test]
fn none_is_ignored_only_in_asciidoctor_named_block_attributes() -> Result<(), Error> {
    let source = "[role=old,role=None,title=Before,title=None,custom=old,custom=None]\nText.";
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let parsed = parse(
            source,
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let Some(BlockKind::Paragraph(block)) =
            parsed.document().blocks().next().map(BlockRef::kind)
        else {
            return Err("expected a paragraph".into());
        };
        let expected = if mode == CompatibilityMode::Draft {
            "None"
        } else {
            "old"
        };
        assert_eq!(
            block.attributes().get("custom").and_then(|value| {
                if let AttributeValueRef::Text(text) = value {
                    Some(text)
                } else {
                    None
                }
            }),
            Some(expected)
        );
        assert_eq!(
            block.roles().contains("None"),
            mode == CompatibilityMode::Draft
        );
        let title = serde_json::to_value(parsed.document())?
            .pointer("/blocks/0/title")
            .cloned()
            .ok_or("missing title")?;
        assert_eq!(
            title
                .pointer("/0/value")
                .and_then(serde_json::Value::as_str)
                .ok_or("missing title")?,
            if mode == CompatibilityMode::Draft {
                "None"
            } else {
                "Before"
            }
        );
    }
    Ok(())
}

#[test]
fn expanded_shorthand_ids_point_to_the_authored_reference() -> Result<(), Error> {
    let source = ":identity: café\n\n[note#{identity}.role]\nText.";
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let parsed = parse(
            source,
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let Some(BlockKind::Paragraph(block)) =
            parsed.document().blocks().next().map(BlockRef::kind)
        else {
            return Err("expected a paragraph".into());
        };
        let id = block.id().ok_or("missing ID")?;
        assert_eq!(id.as_str(), "café");
        assert_eq!(
            source.get(id.location().absolute_start()..=id.location().absolute_end()),
            Some("{identity}")
        );
    }
    Ok(())
}

#[test]
fn preprocessing_and_block_matching_share_shorthand_rules() -> Result<(), Error> {
    for header in ["", ":force-slow-path: yes\n\n"] {
        for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
            let source = format!("{header}[\"source.demo\"]\nprintf 'café'\n// source comment\n");
            let parsed = parse(
                &source,
                &Options::builder().with_compatibility_mode(mode).build()?,
            )?;
            let Some(BlockKind::Paragraph(block)) =
                parsed.document().blocks().next().map(BlockRef::kind)
            else {
                return Err("expected a paragraph".into());
            };
            assert_eq!(
                block
                    .source_text()
                    .is_some_and(|text| text.contains("// source comment")),
                mode == CompatibilityMode::Asciidoctor
            );
        }
        let source = format!("{header}[[id,Label]source.demo]\nprintf 'café'\n// source comment\n");
        let parsed = parse(
            &source,
            &Options::builder()
                .with_compatibility_mode(CompatibilityMode::Draft)
                .build()?,
        )?;
        let Some(BlockKind::Paragraph(block)) =
            parsed.document().blocks().next().map(BlockRef::kind)
        else {
            return Err("expected a source paragraph".into());
        };
        assert_eq!(block.style(), Some("source"));
        assert_eq!(block.source_text(), Some("printf 'café'"));
    }
    Ok(())
}

#[test]
fn draft_anchor_shorthand_marks_discrete_headings() -> Result<(), Error> {
    let parsed = parse(
        "[[heading]discrete.demo]\n== Heading\n\nText.",
        &Options::builder()
            .with_compatibility_mode(CompatibilityMode::Draft)
            .build()?,
    )?;
    assert!(matches!(
        parsed.document().blocks().next().map(BlockRef::kind),
        Some(BlockKind::DiscreteHeader(_))
    ));
    Ok(())
}

#[test]
fn a_named_none_style_follows_the_mode_during_heading_lookahead() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let parsed = parse(
            "[discrete]\n[style=None]\n== Heading\n\nText.",
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        assert_eq!(
            matches!(
                parsed.document().blocks().next().map(BlockRef::kind),
                Some(BlockKind::DiscreteHeader(_))
            ),
            mode == CompatibilityMode::Asciidoctor
        );
    }
    Ok(())
}
