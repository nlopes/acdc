use acdc_parser::{AttributeValue, Block, CompatibilityMode, Options, parse};

type Error = Box<dyn std::error::Error>;

#[rstest::rstest]
#[case::asciidoctor(CompatibilityMode::Asciidoctor, "α")]
#[case::draft(CompatibilityMode::Draft, "\"α\"tail")]
fn closing_quote_needs_a_separator_in_draft(
    #[case] mode: CompatibilityMode,
    #[case] expected: &str,
) -> Result<(), Error> {
    let parsed = parse(
        "[data=\"α\"tail,role=after]\nContent.",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let [Block::Paragraph(block)] = parsed.document().blocks.as_slice() else {
        return Err("expected a paragraph".into());
    };
    assert_eq!(
        block.metadata.attributes.get("data").and_then(|value| {
            if let AttributeValue::String(text) = value {
                Some(text.as_ref())
            } else {
                None
            }
        }),
        Some(expected)
    );
    assert_eq!(block.metadata.roles, ["after"]);
    Ok(())
}

#[test]
fn leading_whitespace_keeps_a_block_attribute_line_as_content() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let parsed = parse(
            "[ source]\nContent.",
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let [Block::Paragraph(block)] = parsed.document().blocks.as_slice() else {
            return Err("expected a paragraph".into());
        };
        assert_eq!(block.metadata.style, None);
    }
    Ok(())
}

#[rstest::rstest]
#[case::asciidoctor(CompatibilityMode::Asciidoctor, true)]
#[case::draft(CompatibilityMode::Draft, false)]
fn preprocessing_uses_the_same_quote_rules_on_fast_and_slow_paths(
    #[case] mode: CompatibilityMode,
    #[case] keeps_comment: bool,
) -> Result<(), Error> {
    for header in ["", ":force-slow-path: yes\n\n"] {
        let source = format!("{header}[\"source\"suffix]\nprintf 'café'\n// source comment\n");
        let parsed = parse(
            &source,
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let [Block::Paragraph(block)] = parsed.document().blocks.as_slice() else {
            return Err("expected a paragraph".into());
        };
        assert_eq!(
            block
                .source_text()
                .is_some_and(|text| text.contains("// source comment")),
            keeps_comment
        );
        assert_eq!(
            block.metadata.style,
            Some(if keeps_comment {
                "source"
            } else {
                "\"source\"suffix"
            })
        );
    }
    Ok(())
}

#[test]
fn rejected_attribute_lines_start_source_paragraph_content() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let parsed = parse(
            "[source]\n[ literal body]\n// source comment\n",
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let [Block::Paragraph(block)] = parsed.document().blocks.as_slice() else {
            return Err("expected a source paragraph".into());
        };
        assert_eq!(block.metadata.style, Some("source"));
        assert_eq!(
            block.source_text(),
            Some("[ literal body]\n// source comment")
        );
    }
    Ok(())
}

#[test]
fn quoted_unicode_content_retains_its_authored_location() -> Result<(), Error> {
    let source = "[data='α,β',title='*café*']\nText.";
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let parsed = parse(
            source,
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let [Block::Paragraph(block)] = parsed.document().blocks.as_slice() else {
            return Err("expected a paragraph".into());
        };
        let [title] = block.title.as_ref() else {
            return Err("expected a formatted title".into());
        };
        let location = title.location();
        assert_eq!(
            source.get(location.absolute_start..=location.absolute_end),
            Some("*café*")
        );
        assert_eq!((location.start.line, location.start.column), (1, 20));
        assert_eq!((location.end.line, location.end.column), (1, 25));
    }
    Ok(())
}

#[test]
fn toc_uses_block_macro_boundaries() -> Result<(), Error> {
    for mode in [CompatibilityMode::Asciidoctor, CompatibilityMode::Draft] {
        let parsed = parse(
            "toc::[ levels=2 ]",
            &Options::builder().with_compatibility_mode(mode).build()?,
        )?;
        let [block] = parsed.document().blocks.as_slice() else {
            return Err("expected one block".into());
        };
        assert_eq!(
            matches!(block, Block::TableOfContents(_)),
            mode == CompatibilityMode::Asciidoctor
        );
    }
    Ok(())
}
