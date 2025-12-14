use std::io::Write;

use acdc_converters_common::{substitutions::strip_backslash_escapes, visitor::WritableVisitor};
use acdc_parser::{Button, CrossReference, InlineMacro, InlineNode};
use crossterm::{
    QueueableCommand,
    style::{
        Attribute, Color, Print, PrintStyledContent, ResetColor, SetAttribute, SetBackgroundColor,
        SetForegroundColor, Stylize,
    },
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
            let text = strip_backslash_escapes(&p.content);
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
            // Strip backslash escapes (e.g., \^ -> ^) for plain text
            let text = strip_backslash_escapes(&p.content);
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
            w.queue(SetAttribute(Attribute::NoBold))?;
        }
        InlineNode::HighlightText(h) => {
            let w = visitor.writer_mut();
            w.queue(SetForegroundColor(Color::Black))?;
            w.queue(SetBackgroundColor(Color::Yellow))?;
            visitor.visit_inline_nodes(&h.content)?;
            let w = visitor.writer_mut();
            w.queue(ResetColor)?;
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

fn maybe_render_osc8_link<W: Write + ?Sized>(
    target: &str,
    text: &str,
    w: &mut W,
    processor: &Processor,
) -> Result<(), crate::Error> {
    if processor.appearance.capabilities.osc8_links {
        w.queue(Print(
            format!("\x1B]8;;{target}\x1B\\{text}\x1B]8;;\x1B",)
                .with(processor.appearance.colors.link),
        ))?;
    } else {
        w.queue(PrintStyledContent(
            target.with(processor.appearance.colors.link),
        ))?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Options, TerminalVisitor};
    use acdc_converters_common::visitor::Visitor;
    use acdc_parser::{
        Anchor, BlockMetadata, Bold, CrossReference, CurvedApostrophe, CurvedQuotation,
        DocumentAttributes, ElementAttributes, Form, Highlight, Image, InlineMacro, Italic,
        Keyboard, LineBreak, Link, Location, Monospace, Paragraph, Plain, Source,
        StandaloneCurvedApostrophe, Subscript, Superscript,
    };

    /// Create simple plain text inline node for testing
    fn create_plain_text(content: &str) -> InlineNode {
        InlineNode::PlainText(Plain {
            content: content.to_string(),
            location: Location::default(),
        })
    }

    /// Create test processor with default options
    fn create_test_processor() -> Processor {
        use crate::Appearance;
        use std::{cell::Cell, rc::Rc};
        let options = Options::default();
        let document_attributes = DocumentAttributes::default();
        let appearance = Appearance::detect();
        Processor {
            options,
            document_attributes,
            toc_entries: vec![],
            example_counter: Rc::new(Cell::new(0)),
            appearance,
        }
    }

    /// Helper to render a paragraph with inline nodes and return the output
    fn render_paragraph(inlines: Vec<InlineNode>) -> Result<String, Error> {
        let paragraph = Paragraph {
            content: inlines,
            location: Location::default(),
            metadata: BlockMetadata::default(),
            title: Vec::new(),
        };

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

        // Check for presence of components rather than exact format with braces
        assert!(output.contains('x'), "Should contain base text");
        assert!(
            output.contains("^{") && output.contains('2'),
            "Should render superscript notation"
        );
        Ok(())
    }

    #[test]
    fn test_subscript_text() -> Result<(), Error> {
        let subscript = InlineNode::SubscriptText(Subscript {
            content: vec![create_plain_text("n")],
            role: None,
            id: None,
            form: Form::Constrained,
            location: Location::default(),
        });

        let output = render_paragraph(vec![create_plain_text("a"), subscript])?;

        // Check for presence of components rather than exact format with braces
        assert!(output.contains('a'), "Should contain base text");
        assert!(
            output.contains("_{") && output.contains('n'),
            "Should render subscript notation"
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
        let link = InlineNode::Macro(InlineMacro::Link(Link {
            target: Source::Name("https://example.com".to_string()),
            text: None,
            attributes: ElementAttributes::default(),
            location: Location::default(),
        }));

        let output = render_paragraph(vec![link])?;
        assert!(
            output.contains("https://example.com"),
            "Should render link URL"
        );
        Ok(())
    }

    #[test]
    fn test_image_macro_placeholder() -> Result<(), Error> {
        let image = InlineNode::Macro(InlineMacro::Image(Box::new(Image {
            source: Source::Name("logo.png".to_string()),
            title: Vec::new(),
            metadata: BlockMetadata::default(),
            location: Location::default(),
        })));

        let output = render_paragraph(vec![image])?;
        assert!(
            output.contains("[Image: logo.png]"),
            "Should render image placeholder"
        );
        Ok(())
    }

    #[test]
    fn test_keyboard_macro() -> Result<(), Error> {
        let kbd = InlineNode::Macro(InlineMacro::Keyboard(Keyboard {
            keys: vec!["Ctrl".to_string(), "C".to_string()],
            location: Location::default(),
        }));

        let output = render_paragraph(vec![kbd])?;
        assert!(
            output.contains("[Ctrl+C]"),
            "Should render keyboard shortcut"
        );
        Ok(())
    }

    #[test]
    fn test_cross_reference_with_text() -> Result<(), Error> {
        let xref = InlineNode::Macro(InlineMacro::CrossReference(CrossReference {
            target: "section-id".to_string(),
            text: Some("See Section 1".to_string()),
            location: Location::default(),
        }));

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
        let xref = InlineNode::Macro(InlineMacro::CrossReference(CrossReference {
            target: "section-id".to_string(),
            text: None,
            location: Location::default(),
        }));

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
            InlineNode::InlineAnchor(Anchor {
                id: "anchor-id".to_string(),
                xreflabel: None,
                location: Location::default(),
            }),
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
