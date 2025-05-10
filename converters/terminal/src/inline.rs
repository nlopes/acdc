use std::io::Write;

use crossterm::{
    QueueableCommand,
    style::{PrintStyledContent, Stylize},
};

use crate::Render;

impl Render for acdc_parser::InlineNode {
    fn render(&self, w: &mut impl Write) -> std::io::Result<()> {
        match self {
            acdc_parser::InlineNode::PlainText(p) => {
                write!(w, "{}", p.content.clone())
            }
            acdc_parser::InlineNode::ItalicText(i) => {
                // ItalicText is a wrapper around a Vec<InlineNode>
                //
                // We need to render each node in the Vec<InlineNode> and then italicize the result
                // before writing it to the writer
                //
                // We can use a BufWriter to buffer the result of the italicized content
                // before writing it to the writer
                let mut inner = std::io::BufWriter::new(Vec::new());
                i.content
                    .iter()
                    .try_for_each(|node| node.render(&mut inner))?;
                inner.flush()?;
                w.queue(PrintStyledContent(
                    String::from_utf8(inner.get_ref().clone())
                        .unwrap_or_default()
                        .trim()
                        .italic(),
                ))?;
                Ok(())
            }
            acdc_parser::InlineNode::BoldText(b) => {
                let mut inner = std::io::BufWriter::new(Vec::new());
                b.content
                    .iter()
                    .try_for_each(|node| node.render(&mut inner))?;
                inner.flush()?;
                w.queue(PrintStyledContent(
                    String::from_utf8(inner.get_ref().clone())
                        .unwrap_or_default()
                        .trim()
                        .bold(),
                ))?;
                Ok(())
            }
            acdc_parser::InlineNode::HighlightText(h) => {
                let mut inner = std::io::BufWriter::new(Vec::new());
                h.content
                    .iter()
                    .try_for_each(|node| node.render(&mut inner))?;
                inner.flush()?;
                w.queue(PrintStyledContent(
                    String::from_utf8(inner.get_ref().clone())
                        .unwrap_or_default()
                        .trim()
                        .black()
                        .on_yellow(),
                ))?;
                Ok(())
            }
            acdc_parser::InlineNode::MonospaceText(m) => {
                let mut inner = std::io::BufWriter::new(Vec::new());
                m.content
                    .iter()
                    .try_for_each(|node| node.render(&mut inner))?;
                inner.flush()?;
                w.queue(PrintStyledContent(
                    String::from_utf8(inner.get_ref().clone())
                        .unwrap_or_default()
                        .trim()
                        .black()
                        .on_grey(),
                ))?;
                Ok(())
            }
            // implement macro link
            acdc_parser::InlineNode::Macro(m) => {
                m.render(w)?;
                Ok(())
            }
            unknown => unimplemented!("GAH: {:?}", unknown),
        }
    }
}

impl Render for acdc_parser::InlineMacro {
    fn render(&self, w: &mut impl Write) -> std::io::Result<()> {
        match self {
            acdc_parser::InlineMacro::Link(l) => write!(w, "{}", l.target)?,
            unknown => unimplemented!("GAH: {:?}", unknown),
        }
        Ok(())
    }
}
