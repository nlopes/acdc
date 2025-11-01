use std::io::Write;

use acdc_converters_common::visitor::WritableVisitor;
use acdc_parser::{Button, CrossReference, InlineMacro, InlineNode};
use crossterm::{
    QueueableCommand,
    style::{PrintStyledContent, Stylize},
};

use crate::{Error, Processor};

/// Helper to render inline nodes to a string buffer.
///
/// This is used to render styled text (bold, italic, etc.) where crossterm
/// requires the full text upfront to apply styling.
fn render_inline_nodes_to_string(
    nodes: &[InlineNode],
    processor: &Processor,
) -> Result<String, Error> {
    let mut buffer = std::io::BufWriter::new(Vec::new());
    for node in nodes {
        render_inline_node_to_writer(node, &mut buffer, processor)?;
    }
    buffer.flush()?;
    // SAFETY: We only write valid UTF-8 through write! macros and plain text from parser
    Ok(String::from_utf8(buffer.into_inner()?)
        .expect("Terminal inline rendering produced invalid UTF-8")
        .trim()
        .to_string())
}

/// Helper to render a single inline node directly to a writer
fn render_inline_node_to_writer<W: Write>(
    node: &InlineNode,
    w: &mut W,
    processor: &Processor,
) -> Result<(), Error> {
    match node {
        InlineNode::PlainText(p) => {
            write!(w, "{}", p.content)?;
        }
        InlineNode::RawText(r) => {
            write!(w, "{}", r.content)?;
        }
        InlineNode::VerbatimText(v) => {
            write!(w, "{}", v.content)?;
        }
        InlineNode::ItalicText(i) => {
            for inner in &i.content {
                render_inline_node_to_writer(inner, w, processor)?;
            }
        }
        InlineNode::BoldText(b) => {
            for inner in &b.content {
                render_inline_node_to_writer(inner, w, processor)?;
            }
        }
        InlineNode::HighlightText(h) => {
            for inner in &h.content {
                render_inline_node_to_writer(inner, w, processor)?;
            }
        }
        InlineNode::MonospaceText(m) => {
            for inner in &m.content {
                render_inline_node_to_writer(inner, w, processor)?;
            }
        }
        InlineNode::Macro(m) => {
            render_inline_macro_to_writer(m, w, processor)?;
        }
        InlineNode::SuperscriptText(s) => {
            write!(w, "^{{")?;
            for inner in &s.content {
                render_inline_node_to_writer(inner, w, processor)?;
            }
            write!(w, "}}")?;
        }
        InlineNode::SubscriptText(s) => {
            write!(w, "_{{")?;
            for inner in &s.content {
                render_inline_node_to_writer(inner, w, processor)?;
            }
            write!(w, "}}")?;
        }
        InlineNode::CurvedQuotationText(c) => {
            write!(w, "\u{201C}")?;
            for inner in &c.content {
                render_inline_node_to_writer(inner, w, processor)?;
            }
            write!(w, "\u{201D}")?;
        }
        InlineNode::CurvedApostropheText(c) => {
            write!(w, "\u{2018}")?;
            for inner in &c.content {
                render_inline_node_to_writer(inner, w, processor)?;
            }
            write!(w, "\u{2019}")?;
        }
        InlineNode::StandaloneCurvedApostrophe(_) => {
            write!(w, "\u{2019}")?;
        }
        InlineNode::LineBreak(_) => {
            writeln!(w)?;
        }
        InlineNode::InlineAnchor(_) => {
            // Anchors are invisible
        }
        _ => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                format!("Unsupported inline node in buffer: {node:?}"),
            )
            .into());
        }
    }
    Ok(())
}

/// Internal implementation for visiting inline nodes
#[allow(clippy::too_many_lines)]
pub(crate) fn visit_inline_node<V: WritableVisitor<Error = Error>>(
    node: &InlineNode,
    visitor: &mut V,
    processor: &Processor,
) -> Result<(), crate::Error> {
    match node {
        InlineNode::PlainText(p) => {
            let w = visitor.writer_mut();
            write!(w, "{}", p.content)?;
        }
        InlineNode::ItalicText(i) => {
            let text = render_inline_nodes_to_string(&i.content, processor)?;
            let w = visitor.writer_mut();
            w.queue(PrintStyledContent(text.italic()))?;
        }
        InlineNode::BoldText(b) => {
            let text = render_inline_nodes_to_string(&b.content, processor)?;
            let w = visitor.writer_mut();
            w.queue(PrintStyledContent(text.bold()))?;
        }
        InlineNode::HighlightText(h) => {
            let text = render_inline_nodes_to_string(&h.content, processor)?;
            let w = visitor.writer_mut();
            w.queue(PrintStyledContent(text.black().on_yellow()))?;
        }
        InlineNode::MonospaceText(m) => {
            let text = render_inline_nodes_to_string(&m.content, processor)?;
            let w = visitor.writer_mut();
            w.queue(PrintStyledContent(text.black().on_grey()))?;
        }
        InlineNode::Macro(m) => {
            let w = visitor.writer_mut();
            render_inline_macro_to_writer(m, w, processor)?;
        }
        InlineNode::InlineAnchor(_) => {
            // Anchors are invisible in terminal output
        }
        InlineNode::RawText(r) => {
            let w = visitor.writer_mut();
            write!(w, "{}", r.content)?;
        }
        InlineNode::VerbatimText(v) => {
            let w = visitor.writer_mut();
            write!(w, "{}", v.content)?;
        }
        InlineNode::SuperscriptText(s) => {
            let w = visitor.writer_mut();
            write!(w, "^{{")?;
            visitor.visit_inline_nodes(&s.content)?;
            let w = visitor.writer_mut();
            write!(w, "}}")?;
        }
        InlineNode::SubscriptText(s) => {
            let w = visitor.writer_mut();
            write!(w, "_{{")?;
            visitor.visit_inline_nodes(&s.content)?;
            let w = visitor.writer_mut();
            write!(w, "}}")?;
        }
        InlineNode::CurvedQuotationText(c) => {
            let w = visitor.writer_mut();
            write!(w, "\u{201C}")?;
            visitor.visit_inline_nodes(&c.content)?;
            let w = visitor.writer_mut();
            write!(w, "\u{201D}")?;
        }
        InlineNode::CurvedApostropheText(c) => {
            let w = visitor.writer_mut();
            write!(w, "\u{2018}")?;
            visitor.visit_inline_nodes(&c.content)?;
            let w = visitor.writer_mut();
            write!(w, "\u{2019}")?;
        }
        InlineNode::StandaloneCurvedApostrophe(_) => {
            let w = visitor.writer_mut();
            write!(w, "\u{2019}")?;
        }
        InlineNode::LineBreak(_) => {
            let w = visitor.writer_mut();
            writeln!(w)?;
        }
        _ => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                format!("Unsupported inline node in terminal: {node:?}"),
            )
            .into());
        }
    }
    Ok(())
}

fn render_inline_macro_to_writer<W: Write + ?Sized>(
    inline_macro: &InlineMacro,
    w: &mut W,
    processor: &Processor,
) -> Result<(), crate::Error> {
    match inline_macro {
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
        InlineMacro::Button(b) => render_button(b, w, processor)?,
        InlineMacro::CrossReference(xref) => render_cross_reference(xref, w)?,
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
                format!("Unsupported inline macro in terminal: {inline_macro:?}"),
            )
            .into());
        }
    }
    Ok(())
}

fn render_button<W: Write + ?Sized>(
    button: &Button,
    w: &mut W,
    processor: &Processor,
) -> Result<(), crate::Error> {
    if processor.document_attributes.contains_key("experimental") {
        w.queue(PrintStyledContent(
            format!("[{}]", button.label).white().bold(),
        ))?;
    } else {
        // If the no-button attribute is set, just render the label as plain text
        w.queue(PrintStyledContent(
            format!("btn:[{}]", button.label.clone()).white(),
        ))?;
    }
    Ok(())
}

fn render_cross_reference<W: Write + ?Sized>(
    xref: &CrossReference,
    w: &mut W,
) -> Result<(), crate::Error> {
    if let Some(text) = &xref.text {
        // Render custom text with subtle styling to indicate it's a cross-reference
        w.queue(PrintStyledContent(text.clone().blue().underlined()))?;
    } else {
        // Render target in brackets with styling
        w.queue(PrintStyledContent(
            format!("[{}]", xref.target).blue().underlined(),
        ))?;
    }
    Ok(())
}
