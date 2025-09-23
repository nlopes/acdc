use std::io::Write;

use acdc_parser::{Button, CrossReference, Footnote, InlineMacro, InlineNode};
use crossterm::{
    QueueableCommand,
    style::{PrintStyledContent, Stylize},
};

use crate::{Processor, Render};

impl Render for InlineNode {
    fn render<W: Write>(&self, w: &mut W, processor: &Processor) -> std::io::Result<()> {
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
                    .try_for_each(|node| node.render(&mut inner, processor))?;
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
                    .try_for_each(|node| node.render(&mut inner, processor))?;
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
                    .try_for_each(|node| node.render(&mut inner, processor))?;
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
                    .try_for_each(|node| node.render(&mut inner, processor))?;
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
                m.render(w, processor)?;
                Ok(())
            }
            unknown => unimplemented!("GAH: {:?}", unknown),
        }
    }
}

impl Render for InlineMacro {
    fn render<W: Write>(&self, w: &mut W, processor: &Processor) -> std::io::Result<()> {
        match self {
            InlineMacro::Link(l) => write!(w, "{}", l.target)?,
            InlineMacro::Url(u) => write!(w, "{}", u.target)?,
            InlineMacro::Autolink(a) => write!(w, "{}", a.url)?,
            InlineMacro::Footnote(footnote) => {
                // Render footnote as superscript number in terminal
                // For terminal output, we'll show [n] format since true superscript is limited
                w.queue(PrintStyledContent(
                    format!("[{}]", footnote.number).cyan().bold(),
                ))?;
            }
            InlineMacro::Button(b) => b.render(w, processor)?,
            InlineMacro::CrossReference(xref) => xref.render(w, processor)?,
            unknown => todo!("{unknown:?}"),
        }
        Ok(())
    }
}

impl Render for Footnote {
    fn render<W: Write>(&self, w: &mut W, processor: &Processor) -> std::io::Result<()> {
        // Render footnote entry: [n] footnote content
        w.queue(PrintStyledContent(
            format!("[{}]", self.number).cyan().bold(),
        ))?;
        write!(w, " ")?;

        // Render the footnote content
        for node in &self.content {
            node.render(w, processor)?;
        }

        Ok(())
    }
}

impl Render for Button {
    fn render<W: Write>(&self, w: &mut W, processor: &Processor) -> std::io::Result<()> {
        if processor
            .options
            .document_attributes
            .contains_key("experimental")
        {
            w.queue(PrintStyledContent(
                format!("[{}]", self.label).white().bold(),
            ))?;
        } else {
            // If the no-button attribute is set, just render the label as plain text
            w.queue(PrintStyledContent(
                format!("btn:[{}]", self.label.clone()).white(),
            ))?;
            return Ok(());
        }
        Ok(())
    }
}

impl Render for CrossReference {
    fn render<W: Write>(&self, w: &mut W, _processor: &Processor) -> std::io::Result<()> {
        if let Some(text) = &self.text {
            // Render custom text with subtle styling to indicate it's a cross-reference
            w.queue(PrintStyledContent(text.clone().blue().underlined()))?;
        } else {
            // Render target in brackets with styling
            w.queue(PrintStyledContent(
                format!("[{}]", self.target).blue().underlined(),
            ))?;
        }
        Ok(())
    }
}
