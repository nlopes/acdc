use std::io::Write;

use acdc_parser::{Button, CrossReference, Footnote, InlineMacro, InlineNode};
use crossterm::{
    QueueableCommand,
    style::{PrintStyledContent, Stylize},
};

use crate::{Processor, Render};

impl Render for InlineNode {
    type Error = crate::Error;

    #[allow(clippy::too_many_lines)]
    fn render<W: Write>(&self, w: &mut W, processor: &Processor) -> Result<(), Self::Error> {
        match self {
            InlineNode::PlainText(p) => {
                write!(w, "{}", p.content.clone())?;
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
            }
            // implement macro link
            InlineNode::Macro(m) => {
                m.render(w, processor)?;
            }
            InlineNode::InlineAnchor(_) => {
                // Anchors are invisible in terminal output
            }
            InlineNode::RawText(r) => {
                write!(w, "{}", r.content)?;
            }
            InlineNode::VerbatimText(v) => {
                write!(w, "{}", v.content)?;
            }
            InlineNode::SuperscriptText(s) => {
                // Terminal doesn't support true superscript, use ^{} notation
                write!(w, "^{{")?;
                for node in &s.content {
                    node.render(w, processor)?;
                }
                write!(w, "}}")?;
            }
            InlineNode::SubscriptText(s) => {
                // Terminal doesn't support true subscript, use _{} notation
                write!(w, "_{{")?;
                for node in &s.content {
                    node.render(w, processor)?;
                }
                write!(w, "}}")?;
            }
            InlineNode::CurvedQuotationText(c) => {
                write!(w, "\u{201C}")?; // Left double quotation mark
                for node in &c.content {
                    node.render(w, processor)?;
                }
                write!(w, "\u{201D}")?; // Right double quotation mark
            }
            InlineNode::CurvedApostropheText(c) => {
                write!(w, "\u{2018}")?; // Left single quotation mark
                for node in &c.content {
                    node.render(w, processor)?;
                }
                write!(w, "\u{2019}")?; // Right single quotation mark
            }
            InlineNode::StandaloneCurvedApostrophe(_) => {
                write!(w, "\u{2019}")?; // Right single quotation mark
            }
            InlineNode::LineBreak(_) => {
                writeln!(w)?;
            }
            _ => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Unsupported,
                    format!("Unsupported inline node in terminal: {self:?}"),
                )
                .into());
            }
        }
        Ok(())
    }
}

impl Render for InlineMacro {
    type Error = crate::Error;

    fn render<W: Write>(&self, w: &mut W, processor: &Processor) -> Result<(), Self::Error> {
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
            InlineMacro::Pass(p) => {
                // Pass content through as-is
                if let Some(ref text) = p.text {
                    write!(w, "{text}")?;
                }
            }
            InlineMacro::Image(img) => {
                // Terminal can't display images, show alt text or path
                write!(w, "[Image: {}]", img.source)?;
            }
            InlineMacro::Icon(icon) => {
                // Terminal can't display icons, show icon name
                write!(w, "[Icon: {}]", icon.target)?;
            }
            InlineMacro::Keyboard(kbd) => {
                // Show keyboard shortcuts with brackets
                write!(w, "[")?;
                for (i, key) in kbd.keys.iter().enumerate() {
                    if i > 0 {
                        write!(w, "+")?;
                    }
                    write!(w, "{key}")?;
                }
                write!(w, "]")?;
            }
            InlineMacro::Menu(menu) => {
                // Show menu path
                write!(w, "{}", menu.target)?;
                for item in &menu.items {
                    write!(w, " > {item}")?;
                }
            }
            InlineMacro::Stem(stem) => {
                // Show stem content as-is (terminal can't render math)
                write!(w, "[{}]", stem.content)?;
            }
            _ => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Unsupported,
                    format!("Unsupported inline macro in terminal: {self:?}"),
                )
                .into());
            }
        }
        Ok(())
    }
}

impl Render for Footnote {
    type Error = crate::Error;

    fn render<W: Write>(&self, w: &mut W, processor: &Processor) -> Result<(), Self::Error> {
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
    type Error = crate::Error;

    fn render<W: Write>(&self, w: &mut W, processor: &Processor) -> Result<(), Self::Error> {
        if processor.document_attributes.contains_key("experimental") {
            w.queue(PrintStyledContent(
                format!("[{}]", self.label).white().bold(),
            ))?;
        } else {
            // If the no-button attribute is set, just render the label as plain text
            w.queue(PrintStyledContent(
                format!("btn:[{}]", self.label.clone()).white(),
            ))?;
        }
        Ok(())
    }
}

impl Render for CrossReference {
    type Error = crate::Error;

    fn render<W: Write>(&self, w: &mut W, _processor: &Processor) -> Result<(), Self::Error> {
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
