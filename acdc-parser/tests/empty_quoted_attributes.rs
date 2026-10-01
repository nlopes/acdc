use acdc_parser::{Block, InlineNode, Options, parse};

type Error = Box<dyn std::error::Error>;

// Byte offsets currently use line-ending-normalized source. Check that each
// inclusive span still covers references removed by inline substitution.
#[test]
fn empty_quoted_attributes_preserve_original_source_spans() -> Result<(), Error> {
    for marker in ["*", "_", "`", "#", "^", "~", "**", "__", "``", "##"] {
        for references in ["{blank}", "{blank}{blank}"] {
            for newline in ["\n", "\r\n"] {
                let quoted = format!("{marker}{references}{marker}");
                let source =
                    format!("= Empty{newline}:blank:{newline}{newline}α {quoted} ω.{newline}");
                let parsed = parse(&source, &Options::default())?;
                let [Block::Paragraph(paragraph)] = parsed.document().blocks.as_slice() else {
                    return Err("expected a paragraph".into());
                };
                let [
                    InlineNode::PlainText(_),
                    formatted,
                    InlineNode::PlainText(_),
                ] = paragraph.content.as_slice()
                else {
                    return Err(
                        format!("expected one formatted span: {:?}", paragraph.content).into(),
                    );
                };
                assert!(!matches!(formatted, InlineNode::PlainText(_)), "{source:?}");
                let location = formatted.location();
                assert_eq!(
                    &parsed.source()[location.absolute_start..=location.absolute_end],
                    quoted,
                    "{source:?}"
                );
                assert_eq!(location.start.line, 4);
                assert_eq!(location.end.line, 4);
                assert_eq!(location.start.column, 3);
                assert_eq!(location.end.column as usize, 2 + quoted.len());
            }
        }
    }
    Ok(())
}
