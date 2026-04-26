//! Paragraph rendering for terminal output.
//!
//! Handles regular paragraphs and styled paragraphs (quote, verse, literal).

use std::io::{BufWriter, Write};

use acdc_converters_core::visitor::{Visitor, WritableVisitor};
use acdc_parser::{InlineNode, Paragraph, inlines_to_string};
use crossterm::{
    QueueableCommand,
    style::{PrintStyledContent, Stylize},
};

use crate::{Error, TerminalVisitor};

impl<W: Write> TerminalVisitor<'_, W> {
    /// Visit a paragraph, handling styled paragraphs (quote, verse, literal).
    pub(crate) fn render_paragraph(&mut self, para: &Paragraph) -> Result<(), Error> {
        // Check for styled paragraphs
        if let Some(style) = para.metadata.style {
            match style {
                "quote" => return self.render_quote_paragraph(para),
                "verse" => return self.render_verse_paragraph(para),
                "literal" | "listing" | "source" => {
                    return self.render_literal_paragraph(para);
                }
                _ => {}
            }
        }

        // Regular paragraph rendering
        self.visit_inline_nodes(&para.title)?;
        self.visit_inline_nodes(&para.content)?;
        let w = self.writer_mut();
        writeln!(w)?;
        Ok(())
    }

    /// Render a quote-styled paragraph with indentation and italic styling.
    fn render_quote_paragraph(&mut self, para: &Paragraph) -> Result<(), Error> {
        // Render title if present
        self.render_title_with_wrapper(&para.title, "", "\n")?;

        // Render content to temporary buffer for processing
        let buffer = Vec::new();
        let inner = BufWriter::new(buffer);
        let mut temp_visitor = TerminalVisitor::new(inner, self.processor.clone());

        temp_visitor.visit_inline_nodes(&para.content)?;

        let buffer = temp_visitor
            .into_writer()
            .into_inner()
            .map_err(std::io::IntoInnerError::into_error)?;

        let content = String::from_utf8_lossy(&buffer);
        let w = self.writer_mut();
        QueueableCommand::queue(w, PrintStyledContent(content.italic()))?;
        writeln!(w)?;

        // Render attribution if present
        self.render_para_attribution(para)?;

        // Add final newline
        let w = self.writer_mut();
        writeln!(w)?;

        Ok(())
    }

    /// Render a verse-styled paragraph preserving line breaks.
    fn render_verse_paragraph(&mut self, para: &Paragraph) -> Result<(), Error> {
        let w = self.writer_mut();

        // Start marker with "VERSE" label
        let styled_label = "VERSE".magenta().bold();
        QueueableCommand::queue(w, PrintStyledContent(styled_label))?;
        writeln!(w)?;

        self.render_title_with_wrapper(&para.title, "", "\n\n")?;

        // Render verse content
        self.visit_inline_nodes(&para.content)?;
        let w = self.writer_mut();
        writeln!(w)?;

        // Render attribution if present
        self.render_para_attribution(para)?;

        // End marker with three dots
        let w = self.writer_mut();
        let end_marker = "• • •".magenta().bold();
        QueueableCommand::queue(w, PrintStyledContent(end_marker))?;
        writeln!(w)?;

        Ok(())
    }

    /// Render a literal-styled paragraph with preformatted text.
    fn render_literal_paragraph(&mut self, para: &Paragraph) -> Result<(), Error> {
        // Title if present
        if !para.title.is_empty() {
            self.render_title_with_wrapper(&para.title, "\n", "\n")?;
        }

        let processor = self.processor.clone();

        // Simple top separator (mdcat style)
        let color = processor.appearance.colors.label_listing;
        let separator = "─".repeat(20).with(color);
        let w = self.writer_mut();
        writeln!(w, "{separator}")?;

        // Render literal content - extract plain text
        let content = extract_plain_text(&para.content);
        write!(w, "{content}")?;
        if !content.ends_with('\n') {
            writeln!(w)?;
        }

        // Bottom separator
        writeln!(w, "{separator}")?;

        Ok(())
    }

    /// Render attribution for quote/verse paragraphs.
    fn render_para_attribution(&mut self, para: &Paragraph) -> Result<(), Error> {
        let attribution = para
            .metadata
            .attribution
            .as_ref()
            .map(|a| inlines_to_string(a));
        let citation = para
            .metadata
            .citetitle
            .as_ref()
            .map(|c| inlines_to_string(c));

        if attribution.is_some() || citation.is_some() {
            let w = self.writer_mut();

            // Format: "— Author" or "— Citation, Author" or just "— Citation"
            let styled_dash = "—".dim();
            QueueableCommand::queue(w, PrintStyledContent(styled_dash))?;
            write!(w, " ")?;

            if let Some(ref author) = attribution {
                let styled_author = author.as_str().dim().italic();
                QueueableCommand::queue(w, PrintStyledContent(styled_author))?;
            }

            if let Some(ref cite) = citation {
                if attribution.is_some() {
                    write!(w, ", ")?;
                }
                let styled_cite = cite.as_str().dim().italic();
                QueueableCommand::queue(w, PrintStyledContent(styled_cite))?;
            }

            writeln!(w)?;
        }

        Ok(())
    }
}

fn extract_plain_text(inlines: &[InlineNode]) -> String {
    crate::extract_inline_text(inlines, "\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use acdc_parser::{Bold, Form, Italic, LineBreak, Location, Plain};

    fn plain(s: &str) -> InlineNode<'_> {
        InlineNode::PlainText(Plain {
            content: s,
            location: Location::default(),
            escaped: false,
        })
    }

    fn bold(nodes: Vec<InlineNode>) -> InlineNode {
        InlineNode::BoldText(Bold {
            role: None,
            id: None,
            form: Form::Constrained,
            content: nodes,
            location: Location::default(),
        })
    }

    fn italic(nodes: Vec<InlineNode>) -> InlineNode {
        InlineNode::ItalicText(Italic {
            role: None,
            id: None,
            form: Form::Constrained,
            content: nodes,
            location: Location::default(),
        })
    }

    #[test]
    fn extract_bold_text_from_literal() {
        let inlines = [bold(vec![plain("important")])];
        assert_eq!(extract_plain_text(&inlines), "important");
    }

    #[test]
    fn extract_nested_formatting() {
        let inlines = [bold(vec![italic(vec![plain("nested")])])];
        assert_eq!(extract_plain_text(&inlines), "nested");
    }

    #[test]
    fn extract_mixed_plain_and_formatted() {
        let inlines = [plain("before "), bold(vec![plain("bold")]), plain(" after")];
        assert_eq!(extract_plain_text(&inlines), "before bold after");
    }

    #[test]
    fn extract_line_break_as_newline() {
        let inlines = [
            plain("first"),
            InlineNode::LineBreak(LineBreak {
                location: Location::default(),
            }),
            plain("second"),
        ];
        assert_eq!(extract_plain_text(&inlines), "first\nsecond");
    }
}
