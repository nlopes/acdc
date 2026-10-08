use acdc_parser::{Block, DelimitedBlockType, InlineNode, Options, parse};

type Error = Box<dyn std::error::Error>;

fn inline_text(nodes: &[InlineNode<'_>]) -> String {
    nodes.iter().fold(String::new(), |mut text, node| {
        if let InlineNode::PlainText(node) = node {
            text.push_str(node.content);
        } else if let InlineNode::VerbatimText(node) = node {
            text.push_str(node.content);
        } else if let InlineNode::CalloutRef(callout) = node {
            text.push('<');
            text.push_str(&callout.number.to_string());
            text.push('>');
        }
        text
    })
}

#[test]
fn callout_processing_preserves_terminal_newlines() -> Result<(), Error> {
    let parsed = parse("----\none <1>\n\n----\n", &Options::default())?;
    let Some(Block::DelimitedBlock(block)) = parsed.document().blocks.first() else {
        return Err("expected a delimited block".into());
    };
    let DelimitedBlockType::DelimitedListing(nodes) = &block.inner else {
        return Err("expected a listing block".into());
    };

    assert_eq!(inline_text(nodes), "one <1>\n");
    Ok(())
}

#[test]
fn explicit_verbatim_paragraph_styles_preserve_indentation() -> Result<(), Error> {
    for style in ["source,rust", "listing", "literal", "verse"] {
        let source = format!("[{style}]\n  indented\n");
        let parsed = parse(&source, &Options::default())?;
        let Some(Block::Paragraph(paragraph)) = parsed.document().blocks.first() else {
            return Err(format!("expected a {style} paragraph").into());
        };

        assert_eq!(paragraph.metadata.style, style.split(',').next());
        assert_eq!(inline_text(&paragraph.content), "  indented");
    }
    Ok(())
}

#[test]
fn styled_paragraph_comment_callouts_retain_source_spans() -> Result<(), Error> {
    for style in ["source,text", "listing", "literal"] {
        let source =
            format!("[{style}]\ncode\n// é comment <1>\n// last line\n\n<1> Explanation.\n");
        let parsed = parse(&source, &Options::default())?;
        assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
        let [Block::Paragraph(paragraph), Block::CalloutList(_)] =
            parsed.document().blocks.as_slice()
        else {
            return Err("expected code and its callout explanation".into());
        };
        assert_eq!(
            inline_text(&paragraph.content),
            "code\n// é comment <1>\n// last line"
        );
        let callout = paragraph
            .content
            .iter()
            .find_map(|node| {
                if let InlineNode::CalloutRef(callout) = node {
                    Some(callout)
                } else {
                    None
                }
            })
            .ok_or("missing callout")?;
        assert_eq!(
            source.get(callout.location.absolute_start..=callout.location.absolute_end),
            Some("<1>")
        );
        assert_eq!(callout.location.start.line, 3);
        assert_eq!(callout.location.start.column, 14);
    }
    Ok(())
}

#[test]
fn explicit_verbatim_styles_override_block_syntax() -> Result<(), Error> {
    for style in ["source,text", "listing", "literal", "verse"] {
        for content in [
            "term;; value",
            "term:: value",
            "* bullet",
            ". ordered",
            "NOTE: ordinary text",
            "image::missing.png[]",
            "audio::missing.ogg[]",
            "video::missing.mp4[]",
            "toc::[]",
            "<<<",
            "'''",
            "> quote",
            "<1> marker",
        ] {
            let source = format!("[{style}]\n{content}\n");
            let parsed = parse(&source, &Options::default())?;
            let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
                return Err(format!("expected one styled paragraph for {source:?}").into());
            };
            assert_eq!(paragraph.metadata.style, style.split(',').next());
            assert_eq!(inline_text(&paragraph.content), content, "{source:?}");
            assert!(
                parsed.warnings().is_empty(),
                "{source:?}: {:?}",
                parsed.warnings()
            );
        }
    }
    Ok(())
}

#[test]
fn styled_paragraph_boundaries_preserve_source_and_macro_ownership() -> Result<(), Error> {
    let content = "é first <1>\nterm;; footnote:[Inactive.]\n----\n[[inactive]]\n[.role]\n== Heading\n----\nlast <2>";
    for ending in ["", "\n"] {
        let source = format!(
            "[#code]\n.Caption\n[source,text]\n{content}\n\n<1> First.\n<2> Last.\n\nAfter.{ending}"
        );
        let parsed = parse(&source, &Options::default())?;
        assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
        assert_eq!(parsed.document().footnotes, []);
        let [
            Block::Paragraph(paragraph),
            Block::CalloutList(_),
            Block::Paragraph(after),
        ] = parsed.document().blocks.as_slice()
        else {
            return Err(
                "expected a source paragraph, explanations, and following paragraph".into(),
            );
        };
        assert_eq!(inline_text(&paragraph.content), content);
        assert_eq!(inline_text(&after.content), "After.");
        for node in &paragraph.content {
            if let InlineNode::CalloutRef(callout) = node {
                assert_eq!(
                    parsed
                        .source()
                        .get(callout.location.absolute_start..=callout.location.absolute_end),
                    Some(if callout.number == 1 { "<1>" } else { "<2>" })
                );
            }
        }
    }
    Ok(())
}

#[cfg(feature = "pre-spec-subs")]
#[test]
fn styled_paragraph_precedence_registers_enabled_footnotes_once() -> Result<(), Error> {
    let source = include_str!("../fixtures/tests/subs_verbatim_precedence.adoc");
    let parsed = parse(source, &Options::default())?;
    let [note] = parsed.document().footnotes.as_slice() else {
        return Err("expected only the enabled footnote".into());
    };
    assert_eq!(
        source.get(note.location.absolute_start..=note.location.absolute_end),
        Some("footnote:used[One note.]")
    );
    assert!(
        parsed
            .warnings()
            .iter()
            .all(|warning| !warning.kind.to_string().starts_with("no callout"))
    );
    Ok(())
}

#[test]
fn indented_callouts_keep_source_spans_after_dedenting() -> Result<(), Error> {
    for newline in ["\n", "\r\n"] {
        for ending in ["", newline] {
            for indent in ["", " "] {
                let source = format!(
                    ".Caption{newline} é <1>{newline}{indent}中 <!--2-->{newline}{newline}<1> First.{newline}<2> Second.{ending}"
                );
                let parsed = parse(&source, &Options::default())?;
                assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
                let [Block::Paragraph(paragraph), Block::CalloutList(_)] =
                    parsed.document().blocks.as_slice()
                else {
                    return Err("expected a literal paragraph and callout list".into());
                };
                assert_eq!(paragraph.metadata.style, Some("literal"));
                assert_eq!(
                    inline_text(&paragraph.content),
                    if indent.is_empty() {
                        " é <1>\n中 <!--<2>-->"
                    } else {
                        "é <1>\n中 <!--<2>-->"
                    },
                    "{source:?}"
                );
                let callouts = paragraph
                    .content
                    .iter()
                    .filter_map(|node| {
                        if let InlineNode::CalloutRef(callout) = node {
                            Some(callout)
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>();
                assert_eq!(callouts.len(), 2);
                for (callout, (number, text, column)) in callouts
                    .iter()
                    .zip([(1, "<1>", 4), (2, "2", 7 + indent.len())])
                {
                    assert_eq!(callout.number, number);
                    assert_eq!(
                        parsed
                            .source()
                            .get(callout.location.absolute_start..=callout.location.absolute_end),
                        Some(text),
                        "{source:?}"
                    );
                    assert_eq!(callout.location.start.line, u32::try_from(number + 1)?);
                    assert_eq!(callout.location.start.column, u32::try_from(column)?);
                }
            }
        }
    }
    Ok(())
}

#[test]
fn indented_callouts_do_not_register_escaped_or_disabled_xml_markers() -> Result<(), Error> {
    for setting in ["", "[line-comment=]\n", "[line-comment=%]\n"] {
        let source =
            format!("{setting} é \\<.>\n xml <!--.-->\n next <.>\n\n<.> First.\n<.> Second.\n");
        let parsed = parse(&source, &Options::default())?;
        if setting.is_empty() {
            assert!(parsed.warnings().is_empty(), "{:?}", parsed.warnings());
        } else {
            let [warning] = parsed.warnings() else {
                return Err("expected one unmatched item warning".into());
            };
            assert_eq!(warning.kind.to_string(), "no callout found for <2>");
            let location = &warning
                .location
                .as_ref()
                .ok_or("missing location")?
                .location;
            assert_eq!(
                source.get(location.absolute_start..=location.absolute_end),
                Some("<.> Second.")
            );
        }
    }
    Ok(())
}

#[cfg(feature = "pre-spec-subs")]
#[test]
fn indented_callouts_respect_disabled_substitutions() -> Result<(), Error> {
    for subs in ["-callouts", "none", "specialcharacters"] {
        let source = format!("[subs={subs}]\n é \\<1>\n xml <!--1-->\n\n<1> Unmatched.\n");
        let parsed = parse(&source, &Options::default())?;
        let Some(Block::Paragraph(paragraph)) = parsed.document().blocks.first() else {
            return Err("expected a literal paragraph".into());
        };
        assert_eq!(inline_text(&paragraph.content), "é \\<1>\nxml <!--1-->");
        assert!(
            parsed
                .warnings()
                .iter()
                .any(|warning| warning.kind.to_string() == "no callout found for <1>")
        );
    }
    Ok(())
}

#[test]
fn delimiter_scanning_preserves_unicode_and_nonclosing_runs() -> Result<(), Error> {
    for delimiter in ["----", "....", "```"] {
        let content = format!("é中 {delimiter} inside a line\n\n{delimiter}{delimiter}\nlast λ");
        let source = format!("{delimiter}\n{content}\n{delimiter}\n\nAfter.\n");
        let parsed = parse(&source, &Options::default())?;
        assert_eq!(parsed.warnings(), []);
        let Some(Block::DelimitedBlock(block)) = parsed.document().blocks.first() else {
            return Err("expected a delimited block".into());
        };
        let (DelimitedBlockType::DelimitedListing(nodes)
        | DelimitedBlockType::DelimitedLiteral(nodes)) = &block.inner
        else {
            return Err("expected a verbatim block".into());
        };
        assert_eq!(inline_text(nodes), content);
        let closing = block
            .close_delimiter_location
            .as_ref()
            .ok_or("expected a closing delimiter")?;
        assert_eq!(closing.absolute_start, delimiter.len() + content.len() + 2);
        let Some(Block::Paragraph(after)) = parsed.document().blocks.get(1) else {
            return Err("expected the following paragraph".into());
        };
        assert_eq!(inline_text(&after.content), "After.");
    }
    Ok(())
}
