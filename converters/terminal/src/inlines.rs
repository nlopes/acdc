use std::io::Write;

use acdc_converters_core::{
    substitutions::{restore_escaped_patterns, strip_backslash_escapes},
    visitor::WritableVisitor,
};
use acdc_parser::{Button, CrossReference, InlineMacro, InlineNode};
use crossterm::{
    QueueableCommand,
    style::{
        Attribute, Color, Print, PrintStyledContent, ResetColor, SetAttribute, SetBackgroundColor,
        SetForegroundColor, Stylize,
    },
};

use crate::{Error, Processor};

/// Try to convert a character to its Unicode superscript equivalent.
/// Only digits and a few reliable symbols are mapped; letters are not included
/// because Unicode superscript coverage for letters is incomplete and inconsistent.
fn to_superscript(c: char) -> Option<char> {
    match c {
        '0' => Some('\u{2070}'),
        '1' => Some('\u{00B9}'),
        '2' => Some('\u{00B2}'),
        '3' => Some('\u{00B3}'),
        '4' => Some('\u{2074}'),
        '5' => Some('\u{2075}'),
        '6' => Some('\u{2076}'),
        '7' => Some('\u{2077}'),
        '8' => Some('\u{2078}'),
        '9' => Some('\u{2079}'),
        '+' => Some('\u{207A}'),
        '-' => Some('\u{207B}'),
        '=' => Some('\u{207C}'),
        '(' => Some('\u{207D}'),
        ')' => Some('\u{207E}'),
        _ => None,
    }
}

/// Try to convert a character to its Unicode subscript equivalent.
/// Only digits and a few reliable symbols are mapped; letters are not included
/// because Unicode subscript coverage for letters is incomplete and inconsistent.
fn to_subscript(c: char) -> Option<char> {
    match c {
        '0' => Some('\u{2080}'),
        '1' => Some('\u{2081}'),
        '2' => Some('\u{2082}'),
        '3' => Some('\u{2083}'),
        '4' => Some('\u{2084}'),
        '5' => Some('\u{2085}'),
        '6' => Some('\u{2086}'),
        '7' => Some('\u{2087}'),
        '8' => Some('\u{2088}'),
        '9' => Some('\u{2089}'),
        '+' => Some('\u{208A}'),
        '-' => Some('\u{208B}'),
        '=' => Some('\u{208C}'),
        '(' => Some('\u{208D}'),
        ')' => Some('\u{208E}'),
        _ => None,
    }
}

/// Try to convert a string entirely to Unicode superscript characters.
/// Returns `None` if any character cannot be converted.
fn try_to_unicode_superscript(text: &str) -> Option<String> {
    text.chars().map(to_superscript).collect()
}

/// Try to convert a string entirely to Unicode subscript characters.
/// Returns `None` if any character cannot be converted.
fn try_to_unicode_subscript(text: &str) -> Option<String> {
    text.chars().map(to_subscript).collect()
}

/// Render super/subscript content: try Unicode conversion first, fall back to
/// dim-styled text to subtly indicate super/subscript.
fn render_script_text<W: Write>(
    nodes: &[InlineNode],
    w: &mut W,
    processor: &Processor,
    converter: fn(&str) -> Option<String>,
) -> Result<(), Error> {
    // Collect plain text to attempt Unicode conversion
    let plain: Option<String> = nodes
        .iter()
        .map(|n| match n {
            InlineNode::PlainText(p) => Some(p.content.as_str()),
            InlineNode::RawText(_)
            | InlineNode::VerbatimText(_)
            | InlineNode::BoldText(_)
            | InlineNode::ItalicText(_)
            | InlineNode::MonospaceText(_)
            | InlineNode::HighlightText(_)
            | InlineNode::SubscriptText(_)
            | InlineNode::SuperscriptText(_)
            | InlineNode::CurvedQuotationText(_)
            | InlineNode::CurvedApostropheText(_)
            | InlineNode::StandaloneCurvedApostrophe(_)
            | InlineNode::LineBreak(_)
            | InlineNode::InlineAnchor(_)
            | InlineNode::Macro(_)
            | InlineNode::CalloutRef(_)
            | _ => None,
        })
        .collect::<Option<Vec<_>>>()
        .map(|parts| parts.join(""));

    if let Some(ref text) = plain
        && let Some(converted) = converter(text)
    {
        write!(w, "{converted}")?;
        return Ok(());
    }

    // Fall back to dim-styled text
    w.queue(SetAttribute(Attribute::Dim))?;
    for node in nodes {
        render_inline_node_to_writer(node, w, processor)?;
    }
    w.queue(SetAttribute(Attribute::NormalIntensity))?;
    Ok(())
}

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
    Ok(String::from_utf8(buffer.into_inner()?)?.trim().to_string())
}

/// Helper to render a single inline node directly to a writer
fn render_inline_node_to_writer<W: Write>(
    node: &InlineNode,
    w: &mut W,
    processor: &Processor,
) -> Result<(), Error> {
    match node {
        InlineNode::PlainText(p) => {
            // Strip backslash escapes (e.g., \^ -> ^) for plain text
            // Also restore escaped patterns (e.g., \... -> ...)
            let text = restore_escaped_patterns(&strip_backslash_escapes(&p.content));
            write!(w, "{text}")?;
        }
        InlineNode::RawText(r) => {
            write!(w, "{}", r.content)?;
        }
        InlineNode::VerbatimText(v) => {
            // Verbatim text preserves backslashes
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
            if h.role.as_deref() == Some("underline") {
                // Underline role: just render content (no highlight styling in buffer)
                for inner in &h.content {
                    render_inline_node_to_writer(inner, w, processor)?;
                }
            } else {
                for inner in &h.content {
                    render_inline_node_to_writer(inner, w, processor)?;
                }
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
            render_script_text(&s.content, w, processor, try_to_unicode_superscript)?;
        }
        InlineNode::SubscriptText(s) => {
            render_script_text(&s.content, w, processor, try_to_unicode_subscript)?;
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
        InlineNode::CalloutRef(callout) => {
            // Render callout reference as (N)
            write!(w, "({})", callout.number)?;
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
            // Strip backslash escapes (e.g., \^ -> ^) for plain text
            // Also restore escaped patterns (e.g., \... -> ...)
            let text = restore_escaped_patterns(&strip_backslash_escapes(&p.content));
            let w = visitor.writer_mut();
            write!(w, "{text}")?;
        }
        InlineNode::ItalicText(i) => {
            let w = visitor.writer_mut();
            w.queue(SetAttribute(Attribute::Italic))?;
            visitor.visit_inline_nodes(&i.content)?;
            let w = visitor.writer_mut();
            w.queue(SetAttribute(Attribute::NoItalic))?;
        }
        InlineNode::BoldText(b) => {
            let w = visitor.writer_mut();
            w.queue(SetAttribute(Attribute::Bold))?;
            visitor.visit_inline_nodes(&b.content)?;
            let w = visitor.writer_mut();
            w.queue(SetAttribute(Attribute::NormalIntensity))?;
        }
        InlineNode::HighlightText(h) => {
            if h.role.as_deref() == Some("underline") {
                let w = visitor.writer_mut();
                w.queue(SetAttribute(Attribute::Underlined))?;
                visitor.visit_inline_nodes(&h.content)?;
                let w = visitor.writer_mut();
                w.queue(SetAttribute(Attribute::NoUnderline))?;
            } else {
                let w = visitor.writer_mut();
                w.queue(SetForegroundColor(Color::Black))?;
                w.queue(SetBackgroundColor(Color::Yellow))?;
                visitor.visit_inline_nodes(&h.content)?;
                let w = visitor.writer_mut();
                w.queue(ResetColor)?;
            }
        }
        InlineNode::MonospaceText(m) => {
            let w = visitor.writer_mut();
            w.queue(SetForegroundColor(
                processor.appearance.colors.inline_monospace,
            ))?;
            visitor.visit_inline_nodes(&m.content)?;
            let w = visitor.writer_mut();
            w.queue(ResetColor)?;
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
            let text = render_inline_nodes_to_string(&s.content, processor)?;
            let w = visitor.writer_mut();
            if let Some(converted) = try_to_unicode_superscript(&text) {
                write!(w, "{converted}")?;
            } else {
                w.queue(SetAttribute(Attribute::Dim))?;
                write!(w, "{text}")?;
                w.queue(SetAttribute(Attribute::NormalIntensity))?;
            }
        }
        InlineNode::SubscriptText(s) => {
            let text = render_inline_nodes_to_string(&s.content, processor)?;
            let w = visitor.writer_mut();
            if let Some(converted) = try_to_unicode_subscript(&text) {
                write!(w, "{converted}")?;
            } else {
                w.queue(SetAttribute(Attribute::Dim))?;
                write!(w, "{text}")?;
                w.queue(SetAttribute(Attribute::NormalIntensity))?;
            }
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
        InlineNode::CalloutRef(callout) => {
            // Render callout reference as bold (N)
            let w = visitor.writer_mut();
            w.queue(crossterm::style::SetAttribute(
                crossterm::style::Attribute::Bold,
            ))?;
            write!(w, "({})", callout.number)?;
            w.queue(crossterm::style::SetAttribute(
                crossterm::style::Attribute::NormalIntensity,
            ))?;
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

fn maybe_render_osc8_link<W: Write + ?Sized>(
    target: &str,
    text: &str,
    w: &mut W,
    processor: &Processor,
) -> Result<(), crate::Error> {
    if processor.appearance.capabilities.osc8_links {
        w.queue(Print(
            format!("\x1B]8;;{target}\x1B\\{text}\x1B]8;;\x1B\\",)
                .with(processor.appearance.colors.link),
        ))?;
    } else {
        // Non-OSC8: show "text (url)" with text styled and URL dim
        w.queue(PrintStyledContent(
            text.with(processor.appearance.colors.link),
        ))?;
        if text != target {
            w.queue(PrintStyledContent(format!(" ({target})").dim()))?;
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
        InlineMacro::Link(l) => {
            let target = l.target.clone();
            let text = match l.text.clone() {
                Some(text) => text,
                None => target.to_string(),
            };
            maybe_render_osc8_link(target.clone().to_string().as_str(), &text, w, processor)?;
        }
        InlineMacro::Url(u) => {
            maybe_render_osc8_link(
                u.target.to_string().as_str(),
                &render_inline_nodes_to_string(&u.text, processor)?,
                w,
                processor,
            )?;
        }
        InlineMacro::Mailto(m) => {
            maybe_render_osc8_link(
                m.target.to_string().as_str(),
                &render_inline_nodes_to_string(&m.text, processor)?,
                w,
                processor,
            )?;
        }
        InlineMacro::Autolink(a) => {
            let target = a.url.to_string();
            maybe_render_osc8_link(&target, &target, w, processor)?;
        }
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
        InlineMacro::IndexTerm(it) => {
            // Collect entry for index catalog rendering
            processor.add_index_entry(it.kind.clone());

            // Flow terms (visible): output the term text
            // Concealed terms (hidden): output nothing
            if it.is_visible() {
                write!(w, "{}", it.term())?;
            }
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
    if xref.text.is_empty() {
        // Render target in brackets with styling
        w.queue(PrintStyledContent(
            format!("[{}]", xref.target).blue().underlined(),
        ))?;
    } else {
        // Render custom text with subtle styling to indicate it's a cross-reference
        let text = acdc_parser::inlines_to_string(&xref.text);
        w.queue(PrintStyledContent(text.blue().underlined()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Options, TerminalVisitor};
    use acdc_converters_core::visitor::Visitor;
    use acdc_parser::{
        Anchor, Bold, CrossReference, CurvedApostrophe, CurvedQuotation, DocumentAttributes, Form,
        Highlight, Image, InlineMacro, Italic, Keyboard, LineBreak, Link, Location, Monospace,
        Paragraph, Plain, Source, StandaloneCurvedApostrophe, Subscript, Superscript,
    };

    /// Create simple plain text inline node for testing
    fn create_plain_text(content: &str) -> InlineNode {
        InlineNode::PlainText(Plain {
            content: content.to_string(),
            location: Location::default(),
            escaped: false,
        })
    }

    /// Create test processor with default options
    fn create_test_processor() -> Processor {
        use crate::Appearance;
        use acdc_converters_core::section::{
            AppendixTracker, PartNumberTracker, SectionNumberTracker,
        };
        use std::{cell::Cell, rc::Rc};
        let options = Options::default();
        let document_attributes = DocumentAttributes::default();
        let appearance = Appearance::detect();
        let section_number_tracker = SectionNumberTracker::new(&document_attributes);
        let part_number_tracker =
            PartNumberTracker::new(&document_attributes, section_number_tracker.clone());
        let appendix_tracker =
            AppendixTracker::new(&document_attributes, section_number_tracker.clone());
        Processor {
            options,
            document_attributes,
            toc_entries: vec![],
            example_counter: Rc::new(Cell::new(0)),
            appearance,
            section_number_tracker,
            part_number_tracker,
            appendix_tracker,
            terminal_width: crate::FALLBACK_TERMINAL_WIDTH,
            index_entries: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
            has_valid_index_section: false,
            list_indent: std::rc::Rc::new(std::cell::Cell::new(0)),
        }
    }

    /// Helper to render a paragraph with inline nodes and return the output
    fn render_paragraph(inlines: Vec<InlineNode>) -> Result<String, Error> {
        let paragraph = Paragraph::new(inlines, Location::default());

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut visitor = TerminalVisitor::new(buffer, processor);
        visitor.visit_paragraph(&paragraph)?;
        let output = visitor.into_writer();

        Ok(String::from_utf8_lossy(&output).to_string())
    }

    #[test]
    fn test_plain_text() -> Result<(), Error> {
        let output = render_paragraph(vec![create_plain_text("Hello, world!")])?;
        assert!(
            output.contains("Hello, world!"),
            "Should contain plain text"
        );
        Ok(())
    }

    #[test]
    fn test_bold_text() -> Result<(), Error> {
        let bold = InlineNode::BoldText(Bold {
            content: vec![create_plain_text("bold text")],
            role: None,
            id: None,
            form: Form::Constrained,
            location: Location::default(),
        });

        let output = render_paragraph(vec![bold])?;
        // Bold text should contain ANSI bold escape codes
        assert!(
            output.contains("bold text"),
            "Should contain bold text content"
        );
        assert!(output.contains("\x1b[1m"), "Should contain ANSI bold code");
        Ok(())
    }

    #[test]
    fn test_italic_text() -> Result<(), Error> {
        let italic = InlineNode::ItalicText(Italic {
            content: vec![create_plain_text("italic text")],
            role: None,
            id: None,
            form: Form::Constrained,
            location: Location::default(),
        });

        let output = render_paragraph(vec![italic])?;
        // Italic text should contain ANSI italic escape codes
        assert!(
            output.contains("italic text"),
            "Should contain italic text content"
        );
        assert!(
            output.contains("\x1b[3m"),
            "Should contain ANSI italic code"
        );
        Ok(())
    }

    #[test]
    fn test_monospace_text() -> Result<(), Error> {
        let monospace = InlineNode::MonospaceText(Monospace {
            content: vec![create_plain_text("monospace text")],
            role: None,
            id: None,
            form: Form::Constrained,
            location: Location::default(),
        });

        let output = render_paragraph(vec![monospace])?;
        assert!(
            output.contains("monospace text"),
            "Should contain monospace text content"
        );
        // Monospace uses black text on grey background (codes 30 and 100)
        assert!(
            output.contains("\x1b["),
            "Should contain ANSI escape codes for styling"
        );
        Ok(())
    }

    #[test]
    fn test_mixed_formatting() -> Result<(), Error> {
        let output = render_paragraph(vec![
            create_plain_text("Normal "),
            InlineNode::BoldText(Bold {
                content: vec![create_plain_text("bold")],
                role: None,
                id: None,
                form: Form::Constrained,
                location: Location::default(),
            }),
            create_plain_text(" and "),
            InlineNode::ItalicText(Italic {
                content: vec![create_plain_text("italic")],
                role: None,
                id: None,
                form: Form::Constrained,
                location: Location::default(),
            }),
        ])?;

        assert!(output.contains("Normal"), "Should contain normal text");
        assert!(output.contains("bold"), "Should contain bold text");
        assert!(output.contains("italic"), "Should contain italic text");
        Ok(())
    }

    #[test]
    fn test_highlight_text() -> Result<(), Error> {
        let highlight = InlineNode::HighlightText(Highlight {
            content: vec![create_plain_text("highlighted")],
            role: None,
            id: None,
            form: Form::Constrained,
            location: Location::default(),
        });

        let output = render_paragraph(vec![highlight])?;
        assert!(
            output.contains("highlighted"),
            "Should contain highlighted text"
        );
        // Highlight uses yellow background (code 43 or 103)
        assert!(output.contains("\x1b["), "Should contain ANSI escape codes");
        Ok(())
    }

    #[test]
    fn test_superscript_text() -> Result<(), Error> {
        let superscript = InlineNode::SuperscriptText(Superscript {
            content: vec![create_plain_text("2")],
            role: None,
            id: None,
            form: Form::Constrained,
            location: Location::default(),
        });

        let output = render_paragraph(vec![create_plain_text("x"), superscript])?;

        assert!(output.contains('x'), "Should contain base text");
        assert!(
            output.contains('\u{00B2}'),
            "Should render '2' as Unicode superscript '²'"
        );
        Ok(())
    }

    #[test]
    fn test_subscript_text() -> Result<(), Error> {
        let subscript = InlineNode::SubscriptText(Subscript {
            content: vec![create_plain_text("2")],
            role: None,
            id: None,
            form: Form::Constrained,
            location: Location::default(),
        });

        let output = render_paragraph(vec![create_plain_text("a"), subscript])?;

        assert!(output.contains('a'), "Should contain base text");
        assert!(
            output.contains('\u{2082}'),
            "Should render '2' as Unicode subscript '₂'"
        );
        Ok(())
    }

    #[test]
    fn test_subscript_fallback_for_letters() -> Result<(), Error> {
        let subscript = InlineNode::SubscriptText(Subscript {
            content: vec![create_plain_text("n")],
            role: None,
            id: None,
            form: Form::Constrained,
            location: Location::default(),
        });

        let output = render_paragraph(vec![create_plain_text("a"), subscript])?;
        assert!(
            output.contains("\x1b[2m"),
            "Letters should fall back to dim styling, got: {output:?}"
        );
        assert!(
            output.contains('n'),
            "Should contain the original text, got: {output:?}"
        );
        Ok(())
    }

    #[test]
    fn test_superscript_fallback_for_letters() -> Result<(), Error> {
        let superscript = InlineNode::SuperscriptText(Superscript {
            content: vec![create_plain_text("abc")],
            role: None,
            id: None,
            form: Form::Constrained,
            location: Location::default(),
        });

        let output = render_paragraph(vec![superscript])?;
        assert!(
            output.contains("\x1b[2m"),
            "Letters should fall back to dim styling, got: {output:?}"
        );
        assert!(
            output.contains("abc"),
            "Should contain the original text, got: {output:?}"
        );
        Ok(())
    }

    #[test]
    fn test_curved_quotation_text() -> Result<(), Error> {
        let quoted = InlineNode::CurvedQuotationText(CurvedQuotation {
            content: vec![create_plain_text("quoted text")],
            role: None,
            id: None,
            form: Form::Constrained,
            location: Location::default(),
        });

        let output = render_paragraph(vec![quoted])?;
        assert!(
            output.contains("\u{201C}"),
            "Should contain opening curly quote"
        );
        assert!(
            output.contains("\u{201D}"),
            "Should contain closing curly quote"
        );
        assert!(output.contains("quoted text"), "Should contain quoted text");
        Ok(())
    }

    #[test]
    fn test_curved_apostrophe_text() -> Result<(), Error> {
        let apostrophe = InlineNode::CurvedApostropheText(CurvedApostrophe {
            content: vec![create_plain_text("text")],
            role: None,
            id: None,
            form: Form::Constrained,
            location: Location::default(),
        });

        let output = render_paragraph(vec![apostrophe])?;
        assert!(
            output.contains("\u{2018}"),
            "Should contain opening curly apostrophe"
        );
        assert!(
            output.contains("\u{2019}"),
            "Should contain closing curly apostrophe"
        );
        Ok(())
    }

    #[test]
    fn test_standalone_curved_apostrophe() -> Result<(), Error> {
        let apostrophe = InlineNode::StandaloneCurvedApostrophe(StandaloneCurvedApostrophe {
            location: Location::default(),
        });

        let output = render_paragraph(vec![apostrophe])?;
        assert!(
            output.contains("\u{2019}"),
            "Should contain curly apostrophe"
        );
        Ok(())
    }

    #[test]
    fn test_link_macro() -> Result<(), Error> {
        let link = InlineNode::Macro(InlineMacro::Link(Link::new(
            Source::Name("https://example.com".to_string()),
            Location::default(),
        )));

        let output = render_paragraph(vec![link])?;
        assert!(
            output.contains("https://example.com"),
            "Should render link URL"
        );
        Ok(())
    }

    #[test]
    fn test_image_macro_placeholder() -> Result<(), Error> {
        let image = InlineNode::Macro(InlineMacro::Image(Box::new(Image::new(
            Source::Name("logo.png".to_string()),
            Location::default(),
        ))));

        let output = render_paragraph(vec![image])?;
        assert!(
            output.contains("[Image: logo.png]"),
            "Should render image placeholder"
        );
        Ok(())
    }

    #[test]
    fn test_keyboard_macro() -> Result<(), Error> {
        let kbd = InlineNode::Macro(InlineMacro::Keyboard(Keyboard::new(
            vec!["Ctrl".to_string(), "C".to_string()],
            Location::default(),
        )));

        let output = render_paragraph(vec![kbd])?;
        assert!(
            output.contains("[Ctrl+C]"),
            "Should render keyboard shortcut"
        );
        Ok(())
    }

    #[test]
    fn test_cross_reference_with_text() -> Result<(), Error> {
        let xref = InlineNode::Macro(InlineMacro::CrossReference(
            CrossReference::new("section-id", Location::default())
                .with_text(vec![create_plain_text("See Section 1")]),
        ));

        let output = render_paragraph(vec![xref])?;
        assert!(
            output.contains("See Section 1"),
            "Should render xref custom text"
        );
        assert!(
            output.contains("\x1b["),
            "Should contain ANSI codes for styling"
        );
        Ok(())
    }

    #[test]
    fn test_cross_reference_without_text() -> Result<(), Error> {
        let xref = InlineNode::Macro(InlineMacro::CrossReference(CrossReference::new(
            "section-id",
            Location::default(),
        )));

        let output = render_paragraph(vec![xref])?;
        assert!(
            output.contains("[section-id]"),
            "Should render xref with target in brackets"
        );
        Ok(())
    }

    #[test]
    fn test_line_break() -> Result<(), Error> {
        let output = render_paragraph(vec![
            create_plain_text("First line"),
            InlineNode::LineBreak(LineBreak {
                location: Location::default(),
            }),
            create_plain_text("Second line"),
        ])?;

        // Line break should create a newline
        assert!(output.contains("First line"), "Should contain first line");
        assert!(output.contains("Second line"), "Should contain second line");
        // Should have newline between them
        assert!(output.lines().count() >= 2, "Should have multiple lines");
        Ok(())
    }

    #[test]
    fn test_inline_anchor_invisible() -> Result<(), Error> {
        let output = render_paragraph(vec![
            create_plain_text("Before"),
            InlineNode::InlineAnchor(Anchor::new("anchor-id".to_string(), Location::default())),
            create_plain_text("After"),
        ])?;

        // Anchor should be invisible
        assert!(
            output.contains("Before"),
            "Should contain text before anchor"
        );
        assert!(output.contains("After"), "Should contain text after anchor");
        assert!(
            !output.contains("anchor-id"),
            "Anchor ID should not be visible"
        );
        Ok(())
    }

    #[test]
    fn test_highlight_text_with_underline_role() -> Result<(), Error> {
        let highlight = InlineNode::HighlightText(Highlight {
            content: vec![create_plain_text("underlined text")],
            role: Some("underline".to_string()),
            id: None,
            form: Form::Constrained,
            location: Location::default(),
        });

        let output = render_paragraph(vec![highlight])?;
        assert!(
            output.contains("underlined text"),
            "Should contain the text"
        );
        // Should use ANSI underline attribute (\x1b[4m), NOT highlight colors
        assert!(
            output.contains("\x1b[4m"),
            "Should contain ANSI underline code, got: {output:?}"
        );
        // Should NOT contain yellow background (highlight styling)
        assert!(
            !output.contains("\x1b[48;5;11m"),
            "Should NOT use highlight background color for underline role"
        );
        Ok(())
    }

    #[test]
    fn test_superscript_renders_unicode() -> Result<(), Error> {
        let superscript = InlineNode::SuperscriptText(Superscript {
            content: vec![create_plain_text("2")],
            role: None,
            id: None,
            form: Form::Constrained,
            location: Location::default(),
        });

        let output = render_paragraph(vec![create_plain_text("x"), superscript])?;
        assert!(output.contains('x'), "Should contain base text");
        assert!(
            output.contains('\u{00B2}'),
            "Should render '2' as Unicode superscript '²', got: {output:?}"
        );
        Ok(())
    }

    #[test]
    fn test_subscript_renders_unicode() -> Result<(), Error> {
        let subscript = InlineNode::SubscriptText(Subscript {
            content: vec![create_plain_text("2")],
            role: None,
            id: None,
            form: Form::Constrained,
            location: Location::default(),
        });

        let output = render_paragraph(vec![create_plain_text("H"), subscript])?;
        assert!(output.contains('H'), "Should contain base text");
        assert!(
            output.contains('\u{2082}'),
            "Should render '2' as Unicode subscript '₂', got: {output:?}"
        );
        Ok(())
    }

    #[test]
    fn test_superscript_fallback_for_unsupported_chars() -> Result<(), Error> {
        // Characters without Unicode superscript equivalents should fall back
        let superscript = InlineNode::SuperscriptText(Superscript {
            content: vec![create_plain_text("@")],
            role: None,
            id: None,
            form: Form::Constrained,
            location: Location::default(),
        });

        let output = render_paragraph(vec![superscript])?;
        assert!(
            output.contains("\x1b[2m"),
            "Should fall back to dim styling for unsupported chars, got: {output:?}"
        );
        assert!(
            output.contains('@'),
            "Should contain the original text, got: {output:?}"
        );
        Ok(())
    }

    #[test]
    fn test_subscript_fallback_for_unsupported_chars() -> Result<(), Error> {
        let subscript = InlineNode::SubscriptText(Subscript {
            content: vec![create_plain_text("@")],
            role: None,
            id: None,
            form: Form::Constrained,
            location: Location::default(),
        });

        let output = render_paragraph(vec![subscript])?;
        assert!(
            output.contains("\x1b[2m"),
            "Should fall back to dim styling for unsupported chars, got: {output:?}"
        );
        assert!(
            output.contains('@'),
            "Should contain the original text, got: {output:?}"
        );
        Ok(())
    }

    #[test]
    fn test_nested_formatting() -> Result<(), Error> {
        // Test bold text containing italic text
        let nested = InlineNode::BoldText(Bold {
            content: vec![InlineNode::ItalicText(Italic {
                content: vec![create_plain_text("bold italic")],
                role: None,
                id: None,
                form: Form::Constrained,
                location: Location::default(),
            })],
            role: None,
            id: None,
            form: Form::Constrained,
            location: Location::default(),
        });

        let output = render_paragraph(vec![nested])?;
        assert!(
            output.contains("bold italic"),
            "Should contain nested formatted text"
        );
        // Should have escape codes for both bold and italic
        assert!(output.contains("\x1b["), "Should contain ANSI escape codes");
        Ok(())
    }
}
