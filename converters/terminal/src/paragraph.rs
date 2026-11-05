//! Paragraph rendering for terminal output.
//!
//! Handles regular paragraphs and terminal presentations for styled paragraphs.

use std::io::{BufWriter, Write};

use acdc_converters_core::{InlineTextTransform, TraversalContext};
use acdc_parser::{
    CaptionKind,
    ast::{AttributeValueRef, AttributesRef, ContentRef, ParagraphRef},
};
use crossterm::{
    QueueableCommand,
    style::{Attribute, PrintStyledContent, SetAttribute, Stylize},
};

use crate::{Error, TerminalVisitor};

impl<'a, W: Write> TerminalVisitor<'a, '_, W> {
    /// Render a regular or styled paragraph.
    pub(crate) fn render_paragraph(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        para: ParagraphRef<'_>,
    ) -> Result<(), Error> {
        if let Some(style) = para.style() {
            match style {
                "quote" => return self.render_quote_paragraph(traversal, para),
                "verse" => return self.render_verse_paragraph(traversal, para),
                "example" => return self.render_example_paragraph(traversal, para),
                "sidebar" => {
                    return self.render_sidebar_content(
                        traversal,
                        para.title(),
                        |visitor, traversal| visitor.write_content(traversal, para.content()),
                    );
                }
                "pass" => {
                    self.write_content(traversal, para.content())?;
                    writeln!(self.writer_mut())?;
                    return Ok(());
                }
                "abstract" => return self.render_abstract_paragraph(traversal, para),
                "literal" | "listing" | "source" => {
                    return self.render_literal_paragraph(traversal, para);
                }
                _ => {}
            }
        }

        // Regular paragraph rendering
        self.render_captioned_title_with_wrapper(
            traversal,
            para.title(),
            para.attributes(),
            CaptionKind::for_style(para.style()),
            "",
            "",
        )?;
        self.render_paragraph_content(traversal, para)?;
        let w = self.writer_mut();
        writeln!(w)?;
        Ok(())
    }

    /// Render a quote-styled paragraph with indentation and italic styling.
    fn render_quote_paragraph(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        para: ParagraphRef<'_>,
    ) -> Result<(), Error> {
        // Render title if present
        self.render_title_with_wrapper(traversal, para.title(), "", "\n")?;

        // Render content to temporary buffer for processing
        let buffer = Vec::new();
        let inner = BufWriter::new(buffer);
        let mut temp_visitor =
            TerminalVisitor::new(inner, self.processor, self.diagnostics.reborrow());

        temp_visitor.write_content(traversal, para.content())?;

        let buffer = temp_visitor
            .into_writer()
            .into_inner()
            .map_err(std::io::IntoInnerError::into_error)?;

        let content = String::from_utf8_lossy(&buffer);
        let w = self.writer_mut();
        QueueableCommand::queue(w, PrintStyledContent(content.italic()))?;
        writeln!(w)?;

        // Render attribution if present
        self.render_attribution(para.attributes())?;

        // Add final newline
        let w = self.writer_mut();
        writeln!(w)?;

        Ok(())
    }

    /// Render a verse-styled paragraph preserving line breaks.
    fn render_verse_paragraph(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        para: ParagraphRef<'_>,
    ) -> Result<(), Error> {
        let w = self.writer_mut();

        // Start marker with "VERSE" label
        let styled_label = "VERSE".magenta().bold();
        QueueableCommand::queue(w, PrintStyledContent(styled_label))?;
        writeln!(w)?;

        self.render_title_with_wrapper(traversal, para.title(), "", "\n\n")?;

        // Render verse content
        self.write_content(traversal, para.content())?;
        let w = self.writer_mut();
        writeln!(w)?;

        // Render attribution if present
        self.render_attribution(para.attributes())?;

        // End marker with three dots
        let w = self.writer_mut();
        let end_marker = "• • •".magenta().bold();
        QueueableCommand::queue(w, PrintStyledContent(end_marker))?;
        writeln!(w)?;

        Ok(())
    }

    fn render_example_paragraph(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        para: ParagraphRef<'_>,
    ) -> Result<(), Error> {
        self.render_captioned_title_with_wrapper(
            traversal,
            para.title(),
            para.attributes(),
            Some(CaptionKind::Example),
            "",
            "\n",
        )?;
        self.writer_mut().queue(PrintStyledContent("│ ".cyan()))?;
        self.render_paragraph_content(traversal, para)?;
        writeln!(self.writer_mut())?;
        Ok(())
    }

    fn render_abstract_paragraph(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        para: ParagraphRef<'_>,
    ) -> Result<(), Error> {
        self.render_title_with_wrapper(traversal, para.title(), "", "\n")?;
        self.writer_mut()
            .queue(PrintStyledContent("ABSTRACT ".magenta().bold()))?;
        self.writer_mut().queue(SetAttribute(Attribute::Italic))?;
        self.render_paragraph_content(traversal, para)?;
        self.writer_mut().queue(SetAttribute(Attribute::NoItalic))?;
        writeln!(self.writer_mut())?;
        Ok(())
    }

    fn render_literal_paragraph(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        para: ParagraphRef<'_>,
    ) -> Result<(), Error> {
        self.render_captioned_title_with_wrapper(
            traversal,
            para.title(),
            para.attributes(),
            CaptionKind::for_style(para.style()),
            "\n",
            "\n",
        )?;

        acdc_converters_core::index::visit_index_inlines(
            para.content().inlines(),
            &mut |term, _| crate::inlines::register_index_term(term, self.processor, traversal),
        )?;

        let separator = "─"
            .repeat(20)
            .with(self.processor.appearance.colors.label_listing);
        let content =
            extract_plain_content(para.content(), crate::delimited::code_text_transform());
        let w = self.writer_mut();
        writeln!(w, "{separator}")?;
        write!(w, "{content}")?;
        if !content.ends_with('\n') {
            writeln!(w)?;
        }
        writeln!(w, "{separator}")?;
        Ok(())
    }

    fn render_paragraph_content(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        para: ParagraphRef<'_>,
    ) -> Result<(), Error> {
        let roles = para.roles();
        let strong = roles.iter().any(|role| matches!(role, "lead" | "big"));
        let dim = roles.iter().any(|role| role == "small");
        let italic = roles.iter().any(|role| role == "subtitle");
        let underline = roles.iter().any(|role| role == "underline");
        let crossed_out = roles.iter().any(|role| role == "line-through");
        if strong {
            self.writer_mut().queue(SetAttribute(Attribute::Bold))?;
        }
        if dim {
            self.writer_mut().queue(SetAttribute(Attribute::Dim))?;
        }
        if italic {
            self.writer_mut().queue(SetAttribute(Attribute::Italic))?;
        }
        if underline {
            self.writer_mut()
                .queue(SetAttribute(Attribute::Underlined))?;
        }
        if crossed_out {
            self.writer_mut()
                .queue(SetAttribute(Attribute::CrossedOut))?;
        }
        self.write_content(traversal, para.content())?;
        if crossed_out {
            self.writer_mut()
                .queue(SetAttribute(Attribute::NotCrossedOut))?;
        }
        if underline {
            self.writer_mut()
                .queue(SetAttribute(Attribute::NoUnderline))?;
        }
        if italic {
            self.writer_mut().queue(SetAttribute(Attribute::NoItalic))?;
        }
        if strong || dim {
            self.writer_mut()
                .queue(SetAttribute(Attribute::NormalIntensity))?;
        }
        Ok(())
    }

    pub(crate) fn render_attribution(
        &mut self,
        attributes: AttributesRef<'_>,
    ) -> Result<(), Error> {
        let transform = InlineTextTransform::default()
            .rendered()
            .decode_char_refs(true);
        let content = |name| match attributes.get(name) {
            Some(AttributeValueRef::Content(content)) => Some(transform.content_to_string(content)),
            _ => None,
        };
        let attribution = content("attribution");
        let citation = content("citetitle");

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

fn extract_plain_content(
    inlines: ContentRef<'_>,
    transform: InlineTextTransform<'_, '_>,
) -> String {
    transform
        .line_break("\n")
        .decode_char_refs(true)
        .content_to_string(inlines)
}

#[cfg(test)]
fn extract_plain_text(
    inlines: &[acdc_parser::InlineNode<'_>],
    transform: InlineTextTransform<'_, '_>,
) -> String {
    transform
        .line_break("\n")
        .decode_char_refs(true)
        .to_string(inlines)
}

#[cfg(test)]
mod tests {
    use super::*;
    use acdc_parser::{Bold, Form, InlineNode, Italic, LineBreak, Location, Plain};

    fn plain(s: &str) -> InlineNode<'_> {
        InlineNode::PlainText(Plain::new(s, Location::default()))
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
        assert_eq!(
            extract_plain_text(&inlines, InlineTextTransform::default()),
            "important"
        );
    }

    #[test]
    fn extract_nested_formatting() {
        let inlines = [bold(vec![italic(vec![plain("nested")])])];
        assert_eq!(
            extract_plain_text(&inlines, InlineTextTransform::default()),
            "nested"
        );
    }

    #[test]
    fn extract_mixed_plain_and_formatted() {
        let inlines = [plain("before "), bold(vec![plain("bold")]), plain(" after")];
        assert_eq!(
            extract_plain_text(&inlines, InlineTextTransform::default()),
            "before bold after"
        );
    }

    #[test]
    fn extract_line_break_as_newline() {
        let inlines = [
            plain("first"),
            InlineNode::LineBreak(LineBreak::new(Location::default())),
            plain("second"),
        ];
        assert_eq!(
            extract_plain_text(&inlines, InlineTextTransform::default()),
            "first\nsecond"
        );
    }
}
