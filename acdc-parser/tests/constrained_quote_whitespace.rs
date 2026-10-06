use acdc_parser::{Block, Form, InlineNode, Options, parse};

type Error = Box<dyn std::error::Error>;

fn is_quote(node: &InlineNode<'_>) -> bool {
    matches!(
        node,
        InlineNode::BoldText(_)
            | InlineNode::ItalicText(_)
            | InlineNode::HighlightText(_)
            | InlineNode::MonospaceText(_)
    )
}

#[test]
fn constrained_quotes_reject_ascii_whitespace_at_both_content_edges() -> Result<(), Error> {
    for marker in ["*", "_", "#", "`"] {
        for space in [" ", "\t", "\n", "\r\n", "\u{b}", "\u{c}"] {
            for content in [format!("café{space}"), format!("{space}café")] {
                let source = format!("P {marker}{content}{marker}. Q.\n");
                let parsed = parse(&source, &Options::default())?;
                let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
                    return Err(format!("expected one paragraph: {source:?}").into());
                };
                assert!(!paragraph.content.iter().any(is_quote), "{source:?}");
            }
        }
    }
    Ok(())
}

#[test]
fn constrained_quotes_skip_invalid_closers_and_preserve_complete_source_spans() -> Result<(), Error>
{
    for marker in ["*", "_", "#", "`"] {
        for newline in ["\n", "\r\n"] {
            for gap in [" ", "\t", newline] {
                let quoted = format!("{marker}café{gap}{marker}; tail{marker}");
                let source = format!("P {quoted} Q.{newline}");
                let parsed = parse(&source, &Options::default())?;
                let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
                    return Err(format!("expected one paragraph: {source:?}").into());
                };
                let spans = paragraph
                    .content
                    .iter()
                    .filter(|node| is_quote(node))
                    .collect::<Vec<_>>();
                let [quote] = spans.as_slice() else {
                    return Err(format!("expected one complete quote: {source:?}").into());
                };
                let span = quote.location();
                assert_eq!(
                    &parsed.source()[span.absolute_start..=span.absolute_end],
                    quoted.replace("\r\n", "\n"),
                    "{source:?}"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn constrained_quotes_keep_non_ascii_spaces_and_unconstrained_controls() -> Result<(), Error> {
    for marker in ["*", "_", "#", "`"] {
        for content in ["café\u{a0}", "\u{a0}café", "café\u{2003}", "\u{2003}café"] {
            let source = format!("P {marker}{content}{marker} Q.\n");
            let parsed = parse(&source, &Options::default())?;
            let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
                return Err("expected one paragraph".into());
            };
            assert!(paragraph.content.iter().any(is_quote), "{source:?}");
        }
        for content in ["café ", " café", "café\t"] {
            let source = format!("P {marker}{marker}{content}{marker}{marker} Q.\n");
            let parsed = parse(&source, &Options::default())?;
            let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
                return Err("expected one paragraph".into());
            };
            assert!(paragraph.content.iter().any(is_quote), "{source:?}");
        }
    }
    Ok(())
}

#[test]
fn unconstrained_quotes_keep_multiline_content_and_complete_source_spans() -> Result<(), Error> {
    for marker in ["**", "__", "##", "``"] {
        for newline in ["\n", "\r\n"] {
            for content in [
                format!("café{newline}"),
                format!("{newline}café"),
                format!("café{newline}tail"),
            ] {
                let quoted = format!("{marker}{content}{marker}");
                let source = format!("P {quoted}. Q.{newline}");
                let parsed = parse(&source, &Options::default())?;
                let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
                    return Err(format!("expected one paragraph: {source:?}").into());
                };
                let quote = paragraph
                    .content
                    .iter()
                    .find(|node| is_quote(node))
                    .ok_or_else(|| format!("expected an unconstrained span: {source:?}"))?;
                let form = match quote {
                    InlineNode::BoldText(text) => &text.form,
                    InlineNode::ItalicText(text) => &text.form,
                    InlineNode::HighlightText(text) => &text.form,
                    InlineNode::MonospaceText(text) => &text.form,
                    InlineNode::PlainText(_)
                    | InlineNode::RawText(_)
                    | InlineNode::VerbatimText(_)
                    | InlineNode::SubscriptText(_)
                    | InlineNode::SuperscriptText(_)
                    | InlineNode::CurvedQuotationText(_)
                    | InlineNode::CurvedApostropheText(_)
                    | InlineNode::StandaloneCurvedApostrophe(_)
                    | InlineNode::LineBreak(_)
                    | InlineNode::InlineAnchor(_)
                    | InlineNode::Macro(_)
                    | InlineNode::CalloutRef(_)
                    | _ => return Err("expected formatting".into()),
                };
                assert_eq!(form, &Form::Unconstrained, "{source:?}");
                let span = quote.location();
                assert_eq!(
                    &parsed.source()[span.absolute_start..=span.absolute_end],
                    quoted.replace("\r\n", "\n")
                );
            }
        }
        let source = format!("P {marker}first\n\nsecond{marker}.\n");
        let parsed = parse(&source, &Options::default())?;
        assert_eq!(parsed.document().blocks.len(), 2, "{source:?}");
        for block in &parsed.document().blocks {
            let Block::Paragraph(paragraph) = block else {
                return Err("expected separate paragraphs".into());
            };
            assert!(!paragraph.content.iter().any(is_quote), "{source:?}");
        }
    }
    Ok(())
}

#[test]
fn constrained_quotes_check_whitespace_before_later_attribute_expansion() -> Result<(), Error> {
    for marker in ["*", "_", "#", "`"] {
        for content in ["café{sp}", "{sp}café", "café {empty}", "{empty} café"] {
            let source = format!("= Edges\n:empty:\n\nP {marker}{content}{marker}.\n");
            let parsed = parse(&source, &Options::default())?;
            let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
                return Err("expected one paragraph".into());
            };
            let quote = paragraph
                .content
                .iter()
                .find(|node| is_quote(node))
                .ok_or_else(|| format!("expected formatting before expansion: {source:?}"))?;
            let span = quote.location();
            assert_eq!(
                &parsed.source()[span.absolute_start..=span.absolute_end],
                format!("{marker}{content}{marker}"),
                "{source:?}"
            );
        }
    }
    Ok(())
}

#[cfg(feature = "pre-spec-subs")]
#[test]
fn constrained_quotes_reject_whitespace_from_earlier_attribute_expansion() -> Result<(), Error> {
    for marker in ["*", "_", "#", "`"] {
        for content in ["café{sp}", "{sp}café", "café {empty}", "{empty} café"] {
            let source = format!(
                "= Edges\n:empty:\n\n[subs=\"attributes,quotes\"]\nP {marker}{content}{marker}.\n"
            );
            let parsed = parse(&source, &Options::default())?;
            let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
                return Err("expected one paragraph".into());
            };
            assert!(!paragraph.content.iter().any(is_quote), "{source:?}");
        }
    }
    Ok(())
}
