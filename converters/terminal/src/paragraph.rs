//! Paragraph rendering for terminal output.
//!
//! Handles regular paragraphs and terminal presentations for styled paragraphs.

use std::io::{BufWriter, Write};

#[cfg(feature = "pre-spec-subs")]
use acdc_converters_core::substitutions::effective_subs_flags;
use acdc_converters_core::{
    inlines_to_string,
    visitor::{Visitor, WritableVisitor},
};
use acdc_parser::{BlockMetadata, CaptionKind, InlineNode, Paragraph};
use crossterm::{
    QueueableCommand,
    style::{Attribute, PrintStyledContent, SetAttribute, Stylize},
};

use crate::{Error, TerminalVisitor};

impl<W: Write> TerminalVisitor<'_, '_, W> {
    /// Render a regular or styled paragraph.
    pub(crate) fn render_paragraph(&mut self, para: &Paragraph) -> Result<(), Error> {
        #[cfg(feature = "pre-spec-subs")]
        {
            // Resolve `[subs="…"]` once per paragraph so inline rendering knows
            // whether to apply typography. Verse/literal/listing/source styles
            // are verbatim contexts; everything else uses the NORMAL baseline.
            // Snapshot/restore via the processor's shared cell so nested renders
            // (and sub-visitors that clone the processor) don't leak state.
            let is_verbatim = matches!(
                para.metadata.style,
                Some("verse" | "literal" | "listing" | "source")
            );
            let previous_subs = self.processor.current_subs.replace(effective_subs_flags(
                para.metadata.substitutions.as_ref(),
                is_verbatim,
            ));

            let result = self.render_paragraph_inner(para);

            self.processor.current_subs.set(previous_subs);
            result
        }
        #[cfg(not(feature = "pre-spec-subs"))]
        self.render_paragraph_inner(para)
    }

    fn render_paragraph_inner(&mut self, para: &Paragraph) -> Result<(), Error> {
        if let Some(style) = para.metadata.style {
            match style {
                "quote" => return self.render_quote_paragraph(para),
                "verse" => return self.render_verse_paragraph(para),
                "example" => return self.render_example_paragraph(para),
                "abstract" => return self.render_abstract_paragraph(para),
                "literal" | "listing" | "source" => {
                    return self.render_literal_paragraph(para);
                }
                _ => {}
            }
        }

        // Regular paragraph rendering
        self.render_captioned_title_with_wrapper(
            &para.title,
            &para.metadata,
            CaptionKind::for_style(para.metadata.style),
            "",
            "",
        )?;
        self.render_paragraph_content(para)?;
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
        let mut temp_visitor =
            TerminalVisitor::new(inner, self.processor.clone(), self.diagnostics.reborrow());

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
        self.render_attribution(&para.metadata)?;

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
        self.render_attribution(&para.metadata)?;

        // End marker with three dots
        let w = self.writer_mut();
        let end_marker = "• • •".magenta().bold();
        QueueableCommand::queue(w, PrintStyledContent(end_marker))?;
        writeln!(w)?;

        Ok(())
    }

    fn render_example_paragraph(&mut self, para: &Paragraph) -> Result<(), Error> {
        self.render_captioned_title_with_wrapper(
            &para.title,
            &para.metadata,
            Some(CaptionKind::Example),
            "",
            "\n",
        )?;
        self.writer_mut().queue(PrintStyledContent("│ ".cyan()))?;
        self.render_paragraph_content(para)?;
        writeln!(self.writer_mut())?;
        Ok(())
    }

    fn render_abstract_paragraph(&mut self, para: &Paragraph) -> Result<(), Error> {
        self.render_title_with_wrapper(&para.title, "", "\n")?;
        self.writer_mut()
            .queue(PrintStyledContent("ABSTRACT ".magenta().bold()))?;
        self.writer_mut().queue(SetAttribute(Attribute::Italic))?;
        self.render_paragraph_content(para)?;
        self.writer_mut().queue(SetAttribute(Attribute::NoItalic))?;
        writeln!(self.writer_mut())?;
        Ok(())
    }

    fn render_literal_paragraph(&mut self, para: &Paragraph) -> Result<(), Error> {
        self.render_captioned_title_with_wrapper(
            &para.title,
            &para.metadata,
            CaptionKind::for_style(para.metadata.style),
            "\n",
            "\n",
        )?;

        let separator = "─"
            .repeat(20)
            .with(self.processor.appearance.colors.label_listing);
        let w = self.writer_mut();
        writeln!(w, "{separator}")?;
        let content = extract_plain_text(&para.content);
        write!(w, "{content}")?;
        if !content.ends_with('\n') {
            writeln!(w)?;
        }
        writeln!(w, "{separator}")?;
        Ok(())
    }

    fn render_paragraph_content(&mut self, para: &Paragraph) -> Result<(), Error> {
        let roles = &para.metadata.roles;
        let strong = roles.iter().any(|role| matches!(*role, "lead" | "big"));
        let dim = roles.contains(&"small");
        let italic = roles.contains(&"subtitle");
        let underline = roles.contains(&"underline");
        let crossed_out = roles.contains(&"line-through");
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
        self.visit_inline_nodes(&para.content)?;
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

    pub(crate) fn render_attribution(&mut self, metadata: &BlockMetadata<'_>) -> Result<(), Error> {
        let attribution = metadata.attribution.as_ref().map(|a| inlines_to_string(a));
        let citation = metadata.citetitle.as_ref().map(|c| inlines_to_string(c));

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
