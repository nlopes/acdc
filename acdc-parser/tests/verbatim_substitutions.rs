use acdc_parser::{Block, InlineNode, Options, parse};

type Error = Box<dyn std::error::Error>;

// Snapshots record locations, but this asserts that dedenting display text does
// not also shorten the original inclusive source span.
#[cfg(feature = "pre-spec-subs")]
#[test]
fn verbatim_quotes_keep_indented_multiline_source_spans() -> Result<(), Error> {
    let source = "[subs=+quotes]\n *First\n   café* after.\n";
    let parsed = parse(source, &Options::default())?;
    let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
        return Err("expected literal paragraph".into());
    };
    let bold = paragraph
        .content
        .iter()
        .find_map(|node| {
            if let InlineNode::BoldText(bold) = node {
                Some(bold)
            } else {
                None
            }
        })
        .ok_or("missing bold text")?;
    let [InlineNode::PlainText(text)] = bold.content.as_slice() else {
        return Err("expected one formatted text fragment".into());
    };
    assert_eq!(text.content, "First\n  café");
    let end = text.location.absolute_end;
    let last = source
        .get(end..)
        .and_then(|s| s.chars().next())
        .ok_or("invalid end")?;
    assert_eq!(
        source.get(text.location.absolute_start..end + last.len_utf8()),
        Some("First\n   café")
    );
    assert_eq!(
        (text.location.start.line, text.location.start.column),
        (2, 3)
    );
    assert_eq!((text.location.end.line, text.location.end.column), (3, 7));
    Ok(())
}

#[cfg(not(feature = "pre-spec-subs"))]
#[test]
fn verbatim_substitutions_disabled_keeps_default_literal_text() -> Result<(), Error> {
    let parsed = parse(
        ":word: expanded\n\n[subs=\"quotes,attributes\"]\n *{word}*\n",
        &Options::default(),
    )?;
    let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
        return Err("expected literal paragraph".into());
    };
    let [InlineNode::PlainText(text)] = paragraph.content.as_slice() else {
        return Err("expected literal text".into());
    };
    assert_eq!(text.content, "*{word}*");
    Ok(())
}
