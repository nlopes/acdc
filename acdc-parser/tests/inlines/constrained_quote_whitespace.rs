use acdc_parser::{
    FormattingForm, Options,
    ast::{BlockKind, BlockRef, InlineKind, InlineRef},
    parse,
};

type Error = Box<dyn std::error::Error>;

fn is_quote(node: InlineRef<'_>) -> bool {
    matches!(
        node.kind(),
        InlineKind::Bold(_)
            | InlineKind::Italic(_)
            | InlineKind::Highlight(_)
            | InlineKind::Monospace(_)
    )
}

#[test]
fn constrained_quotes_reject_ascii_whitespace_at_both_content_edges() -> Result<(), Error> {
    for marker in ["*", "_", "#", "`"] {
        for space in [" ", "\t", "\n", "\r\n", "\u{b}", "\u{c}"] {
            for content in [format!("café{space}"), format!("{space}café")] {
                let source = format!("P {marker}{content}{marker}. Q.\n");
                let parsed = parse(&source, &Options::default())?;
                let [BlockKind::Paragraph(paragraph)] = &*parsed
                    .document()
                    .blocks()
                    .map(BlockRef::kind)
                    .collect::<Vec<_>>()
                else {
                    return Err(format!("expected one paragraph: {source:?}").into());
                };
                assert!(!paragraph.content().inlines().any(is_quote), "{source:?}");
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
                let [BlockKind::Paragraph(paragraph)] = &*parsed
                    .document()
                    .blocks()
                    .map(BlockRef::kind)
                    .collect::<Vec<_>>()
                else {
                    return Err(format!("expected one paragraph: {source:?}").into());
                };
                let spans = paragraph
                    .content()
                    .inlines()
                    .filter(|node| is_quote(*node))
                    .collect::<Vec<_>>();
                let [quote] = spans.as_slice() else {
                    return Err(format!("expected one complete quote: {source:?}").into());
                };
                let span = quote.location();
                assert_eq!(
                    &parsed.preprocessed_source()[span.absolute_start()..=span.absolute_end()],
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
            let [BlockKind::Paragraph(paragraph)] = &*parsed
                .document()
                .blocks()
                .map(BlockRef::kind)
                .collect::<Vec<_>>()
            else {
                return Err("expected one paragraph".into());
            };
            assert!(paragraph.content().inlines().any(is_quote), "{source:?}");
        }
        for content in ["café ", " café", "café\t"] {
            let source = format!("P {marker}{marker}{content}{marker}{marker} Q.\n");
            let parsed = parse(&source, &Options::default())?;
            let [BlockKind::Paragraph(paragraph)] = &*parsed
                .document()
                .blocks()
                .map(BlockRef::kind)
                .collect::<Vec<_>>()
            else {
                return Err("expected one paragraph".into());
            };
            assert!(paragraph.content().inlines().any(is_quote), "{source:?}");
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
                let [BlockKind::Paragraph(paragraph)] = &*parsed
                    .document()
                    .blocks()
                    .map(BlockRef::kind)
                    .collect::<Vec<_>>()
                else {
                    return Err(format!("expected one paragraph: {source:?}").into());
                };
                let quote = paragraph
                    .content()
                    .inlines()
                    .find(|node| is_quote(*node))
                    .ok_or_else(|| format!("expected an unconstrained span: {source:?}"))?;
                let form = if let InlineKind::Bold(text) = quote.kind() {
                    text.form()
                } else if let InlineKind::Italic(text) = quote.kind() {
                    text.form()
                } else if let InlineKind::Highlight(text) = quote.kind() {
                    text.form()
                } else if let InlineKind::Monospace(text) = quote.kind() {
                    text.form()
                } else {
                    return Err("expected formatting".into());
                };
                assert_eq!(form, FormattingForm::Unconstrained, "{source:?}");
                let span = quote.location();
                assert_eq!(
                    &parsed.preprocessed_source()[span.absolute_start()..=span.absolute_end()],
                    quoted.replace("\r\n", "\n")
                );
            }
        }
        let source = format!("P {marker}first\n\nsecond{marker}.\n");
        let parsed = parse(&source, &Options::default())?;
        assert_eq!(parsed.document().blocks().count(), 2, "{source:?}");
        for block in parsed.document().blocks() {
            let BlockKind::Paragraph(paragraph) = block.kind() else {
                return Err("expected separate paragraphs".into());
            };
            assert!(!paragraph.content().inlines().any(is_quote), "{source:?}");
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
            let [BlockKind::Paragraph(paragraph)] = &*parsed
                .document()
                .blocks()
                .map(BlockRef::kind)
                .collect::<Vec<_>>()
            else {
                return Err("expected one paragraph".into());
            };
            let quote = paragraph
                .content()
                .inlines()
                .find(|node| is_quote(*node))
                .ok_or_else(|| format!("expected formatting before expansion: {source:?}"))?;
            let span = quote.location();
            assert_eq!(
                &parsed.preprocessed_source()[span.absolute_start()..=span.absolute_end()],
                format!("{marker}{content}{marker}"),
                "{source:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn constrained_quotes_reject_whitespace_from_earlier_attribute_expansion() -> Result<(), Error> {
    for marker in ["*", "_", "#", "`"] {
        for content in ["café{sp}", "{sp}café", "café {empty}", "{empty} café"] {
            let source = format!(
                "= Edges\n:empty:\n\n[subs=\"attributes,quotes\"]\nP {marker}{content}{marker}.\n"
            );
            let parsed = parse(&source, &Options::default())?;
            let [BlockKind::Paragraph(paragraph)] = &*parsed
                .document()
                .blocks()
                .map(BlockRef::kind)
                .collect::<Vec<_>>()
            else {
                return Err("expected one paragraph".into());
            };
            assert!(!paragraph.content().inlines().any(is_quote), "{source:?}");
        }
    }
    Ok(())
}
