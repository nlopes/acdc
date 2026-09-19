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
    for style in ["source,rust", "listing", "literal"] {
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
