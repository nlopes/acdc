use std::io::Write;

use acdc_parser::{InlineMacro, InlineNode};
use crossterm::{
    QueueableCommand,
    style::{PrintStyledContent, Stylize},
};

use crate::Render;

impl Render for InlineNode {
    fn render(&self, w: &mut impl Write) -> std::io::Result<()> {
        match self {
            InlineNode::PlainText(p) => {
                write!(w, "{}", p.content.clone())
            }
            InlineNode::ItalicText(i) => {
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
            InlineNode::BoldText(b) => {
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
            InlineNode::HighlightText(h) => {
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
            InlineNode::MonospaceText(m) => {
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
            InlineNode::Macro(m) => {
                m.render(w)?;
                Ok(())
            }
            unknown => unimplemented!("GAH: {:?}", unknown),
        }
    }
}

impl Render for InlineMacro {
    fn render(&self, w: &mut impl Write) -> std::io::Result<()> {
        match self {
            InlineMacro::Link(l) => write!(w, "{}", l.target)?,
            InlineMacro::Url(u) => write!(w, "{}", u.target)?,
            InlineMacro::Autolink(a) => write!(w, "{}", a.url)?,
            unknown => todo!("{unknown:?}"),
        }
        Ok(())
    }
}
