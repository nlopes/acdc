use acdc_parser::{Block, InlineMacro, InlineNode, Location, Options, parse};

type Error = Box<dyn std::error::Error>;

fn source_span<'s>(source: &'s str, location: &Location) -> Result<&'s str, Error> {
    let start = location.absolute_start;
    let end = location.absolute_end;
    let last = source
        .get(end..)
        .and_then(|tail| tail.chars().next())
        .ok_or("invalid inclusive end")?;
    for (offset, position) in [(start, &location.start), (end, &location.end)] {
        let prefix = source.get(..offset).ok_or("invalid character boundary")?;
        let line = prefix.chars().filter(|&c| c == '\n').count() + 1;
        let column = prefix
            .rsplit('\n')
            .next()
            .ok_or("missing line")?
            .chars()
            .count()
            + 1;
        assert_eq!(
            (position.line as usize, position.column as usize),
            (line, column)
        );
    }
    source
        .get(start..end + last.len_utf8())
        .ok_or_else(|| "invalid source span".into())
}

fn assert_literal_spans(source: &str, nodes: &[InlineNode<'_>]) -> Result<(), Error> {
    for node in nodes {
        if let InlineNode::PlainText(text) = node {
            assert_eq!(source_span(source, &text.location)?, text.content);
        } else if let InlineNode::RawText(raw) = node {
            assert_eq!(source_span(source, &raw.location)?, raw.content);
        } else if let InlineNode::BoldText(bold) = node {
            assert_literal_spans(source, &bold.content)?;
        } else if let InlineNode::Macro(InlineMacro::IndexTerm(term)) = node {
            assert_literal_spans(source, term.term())?;
            assert_literal_spans(source, term.catalog_entry().term())?;
        } else {
            return Err(format!("unexpected node: {node:?}").into());
        }
    }
    Ok(())
}

#[test]
fn passthrough_fragments_have_exact_inclusive_source_spans() -> Result<(), Error> {
    for source in [
        include_str!("../fixtures/tests/passthrough_at_line_start.adoc"),
        "α +β+ γ\n+δ+ ε",
        "((Before +é+))",
        "*Before pass:[é] after*",
        "pass:[one]pass:[two]",
        "pass:[one\n two] suffix\npass:[three] end",
        "pass:quotes[*bold*]",
        "pass:quotes[*E* and *é*]",
        "pass:macros[((Word café))]",
        "pass:quotes,macros[((Term *café*))]",
        "pass:macros,quotes[((Term *café*))]",
    ] {
        let parsed = parse(source, &Options::default())?;
        let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
            return Err("expected one paragraph".into());
        };
        assert_literal_spans(source, &paragraph.content)?;
    }
    Ok(())
}

#[test]
fn passthrough_newline_ends_before_the_next_delimiter() -> Result<(), Error> {
    let source = include_str!("../fixtures/tests/passthrough_at_line_start.adoc");
    let parsed = parse(source, &Options::default())?;
    let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
        return Err("expected one paragraph".into());
    };
    let text = paragraph
        .content
        .iter()
        .find_map(|node| {
            if let InlineNode::PlainText(text) = node {
                (text.content == " or\n").then_some(text)
            } else {
                None
            }
        })
        .ok_or("missing newline fragment")?;
    assert_eq!((text.location.end.line, text.location.end.column), (1, 48));
    assert_eq!(source_span(source, &text.location)?, " or\n");
    Ok(())
}

#[test]
fn passthrough_hardbreak_maps_to_its_marker_without_the_newline() -> Result<(), Error> {
    for (body, marker) in [
        ("first +\nsecond", " +"),
        ("first {plus}\nsecond", " {plus}"),
    ] {
        let source = format!(":plus: +\n\npass:a,p[{body}]");
        let parsed = parse(&source, &Options::default())?;
        let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
            return Err("expected one paragraph".into());
        };
        let mut breaks = 0;
        for node in &paragraph.content {
            if let InlineNode::LineBreak(line_break) = node {
                assert_eq!(source_span(&source, &line_break.location)?, marker);
                breaks += 1;
            } else {
                assert_literal_spans(&source, std::slice::from_ref(node))?;
            }
        }
        assert_eq!(breaks, 1);
    }
    Ok(())
}

#[test]
fn passthrough_formatted_attribute_maps_to_the_complete_reference() -> Result<(), Error> {
    for value in ["*é*", "*abcdef*", "*a much longer value*"] {
        for stages in ["attributes,quotes,macros", "attributes,macros,quotes"] {
            let source = format!(":value: {value}\n\npass:{stages}[((Term {{value}}))]");
            let parsed = parse(&source, &Options::default())?;
            let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
                return Err("expected one paragraph".into());
            };
            let [InlineNode::Macro(InlineMacro::IndexTerm(term))] = paragraph.content.as_slice()
            else {
                return Err("expected one index term".into());
            };
            let bold = term
                .term()
                .iter()
                .find_map(|node| {
                    if let InlineNode::BoldText(bold) = node {
                        Some(bold)
                    } else {
                        None
                    }
                })
                .ok_or("missing formatted attribute")?;
            assert_eq!(source_span(&source, &bold.location)?, "{value}");
            let [InlineNode::RawText(raw)] = bold.content.as_slice() else {
                return Err("missing formatted attribute text".into());
            };
            assert_eq!(source_span(&source, &raw.location)?, "{value}");
        }
    }
    Ok(())
}

#[test]
fn passthrough_escape_does_not_shift_a_following_attribute_span() -> Result<(), Error> {
    for value in ["é", "Expanded"] {
        let source = format!(":name: {value}\n\npass:a,m[\\((Literal {{name}}))]");
        let parsed = parse(&source, &Options::default())?;
        let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
            return Err("expected one paragraph".into());
        };
        let [
            InlineNode::RawText(prefix),
            InlineNode::RawText(attribute),
            InlineNode::RawText(suffix),
        ] = paragraph.content.as_slice()
        else {
            return Err("expected escaped prefix, attribute, and suffix".into());
        };
        assert_eq!(prefix.content, "((Literal ");
        assert_eq!(source_span(&source, &prefix.location)?, "\\((Literal ");
        assert_eq!(attribute.content, value);
        assert_eq!(source_span(&source, &attribute.location)?, "{name}");
        assert_eq!(suffix.content, "))");
        assert_eq!(source_span(&source, &suffix.location)?, "))");
    }
    Ok(())
}
