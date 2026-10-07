use acdc_parser::{Block, DelimitedBlockType, InlineNode, Options, parse};

type Error = Box<dyn std::error::Error>;

#[test]
fn removed_groups_do_not_register_disabled_macros() -> Result<(), Error> {
    for subs in [
        "-normal",
        "-normal,attributes+",
        "-normal,+quotes",
        "+quotes,-normal",
        "-normal,+normal,-normal",
        "-normal,-verbatim,+attributes",
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
fn readded_group_members_preserve_macro_registration_order() -> Result<(), Error> {
    for (subs, expected) in [
        ("-normal,+macros", 1),
        ("-normal,+attributes,+macros", 2),
        ("-normal,+macros,+attributes", 1),
        ("-normal,+normal", 2),
        ("-normal,+normal,-macros", 0),
        ("-verbatim,+verbatim", 2),
        ("-normal,+normal,-normal,+macros", 1),
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
        assert_eq!(
            parsed.document().references.contains_key("direct"),
            expected > 0,
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

#[test]
fn removed_verbatim_group_controls_callouts_in_every_code_block_kind() -> Result<(), Error> {
    for (subs, enabled) in [
        ("-verbatim", false),
        ("-verbatim,+callouts", true),
        ("-verbatim,+verbatim", true),
        ("-verbatim,+verbatim,-verbatim", false),
        ("-verbatim,+specialchars", false),
        ("-normal", true),
        ("-normal,-verbatim", false),
    ] {
        for newline in ["\n", "\r\n"] {
            for (style, delimiter, indent) in [
                ("source,text,", "----", ""),
                ("", "....", ""),
                ("source,text,", "", ""),
                ("listing,", "", ""),
                ("literal,", "", ""),
                ("", "", " "),
            ] {
                let content = format!("{indent}café <1>");
                let body = if delimiter.is_empty() {
                    content
                } else {
                    format!("{delimiter}{newline}{content}{newline}{delimiter}")
                };
                let source = format!("[{style}subs=\"{subs}\"]{newline}{body}{newline}");
                let parsed = parse(&source, &Options::default())?;
                let nodes = match parsed.document().blocks.first() {
                    Some(Block::Paragraph(paragraph)) => &paragraph.content,
                    Some(Block::DelimitedBlock(block)) => {
                        let (DelimitedBlockType::DelimitedListing(nodes)
                        | DelimitedBlockType::DelimitedLiteral(nodes)) = &block.inner
                        else {
                            return Err("expected code block".into());
                        };
                        nodes
                    }
                    _ => return Err("expected code content".into()),
                };
                let callouts = nodes
                    .iter()
                    .filter_map(|node| {
                        if let InlineNode::CalloutRef(callout) = node {
                            Some(callout)
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>();
                let expected = enabled || !cfg!(feature = "pre-spec-subs");
                assert_eq!(callouts.len(), usize::from(expected), "{source}");
                for callout in &callouts {
                    assert_eq!(
                        parsed
                            .source()
                            .get(callout.location.absolute_start..=callout.location.absolute_end),
                        Some("<1>"),
                        "{source}"
                    );
                }
                if !expected {
                    let text = nodes
                        .iter()
                        .filter_map(|node| {
                            if let InlineNode::PlainText(text) = node {
                                Some(text.content)
                            } else if let InlineNode::VerbatimText(text) = node {
                                Some(text.content)
                            } else {
                                None
                            }
                        })
                        .collect::<String>();
                    assert!(text.contains("café <1>"), "{source}: {text:?}");
                }
            }
        }
    }
    Ok(())
}
