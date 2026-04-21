use std::io::Write;

use acdc_converters_core::visitor::{Visitor, WritableVisitor};
use acdc_parser::{Admonition, AdmonitionVariant, AttributeValue};
use crossterm::{
    QueueableCommand,
    style::{PrintStyledContent, Stylize},
};

use crate::{Error, TerminalVisitor};

impl<W: Write> TerminalVisitor<'_, W> {
    /// Visit an admonition block (NOTE, TIP, IMPORTANT, WARNING, CAUTION).
    ///
    /// Renders with bold caption and left border.
    pub(crate) fn render_admonition(&mut self, admon: &Admonition) -> Result<(), Error> {
        use std::io::BufWriter;

        let processor = self.processor.clone();

        let w = self.writer_mut();
        writeln!(w)?;

        // Get icon, caption attribute, and theme color for this admonition type
        let (icon, caption_attr, color) = match admon.variant {
            AdmonitionVariant::Note => (
                "ℹ️ ",
                "note-caption",
                processor.appearance.colors.admon_note,
            ),
            AdmonitionVariant::Tip => ("💡", "tip-caption", processor.appearance.colors.admon_tip),
            AdmonitionVariant::Important => (
                "❗",
                "important-caption",
                processor.appearance.colors.admon_important,
            ),
            AdmonitionVariant::Warning => (
                "⚠️ ",
                "warning-caption",
                processor.appearance.colors.admon_warning,
            ),
            AdmonitionVariant::Caution => (
                "🔥",
                "caution-caption",
                processor.appearance.colors.admon_caution,
            ),
        };

        let caption = processor
            .document_attributes
            .get(caption_attr)
            .and_then(|v| match v {
                AttributeValue::String(s) => Some(s.clone()),
                AttributeValue::Bool(_) | AttributeValue::None | _ => None,
            })
            .ok_or(Error::InvalidAdmonitionCaption(caption_attr.to_string()))?;

        // Border character based on terminal capabilities
        let border = if processor.appearance.capabilities.unicode {
            "│"
        } else {
            "|"
        };

        // Header line with icon, bold caption, and left border
        write!(w, "{} {icon}", border.with(color))?;
        let styled_caption = format!("{caption}:").bold();
        QueueableCommand::queue(w, PrintStyledContent(styled_caption))?;

        // Title on same line if present
        if admon.title.is_empty() {
            writeln!(w)?;
        } else {
            write!(w, " ")?;
            let mut title_buffer = Vec::new();
            let title_processor = processor.clone();
            let mut title_visitor = TerminalVisitor::new(&mut title_buffer, title_processor);
            title_visitor.visit_inline_nodes(&admon.title)?;

            let title_text = String::from_utf8_lossy(&title_buffer);
            let w = self.writer_mut();
            writeln!(w, "{}", title_text.trim())?;
        }

        // Render content blocks with left border
        for block in &admon.blocks {
            let buffer = Vec::new();
            let inner = BufWriter::new(buffer);
            let mut temp_visitor = TerminalVisitor::new(inner, processor.clone());
            temp_visitor.visit_block(block)?;

            let buffer = temp_visitor
                .into_writer()
                .into_inner()
                .map_err(std::io::IntoInnerError::into_error)?;

            let content = String::from_utf8_lossy(&buffer);

            // Word-wrap content to fit within the "│ " prefix
            let available = processor.terminal_width.saturating_sub(2);
            let wrapped = crate::wrap::wrap_ansi_text(&content, available);

            let w = self.writer_mut();
            for line in wrapped.lines() {
                write!(w, "{} ", border.with(color))?;
                writeln!(w, "{line}")?;
            }
        }

        // End border
        let w = self.writer_mut();
        writeln!(w, "{}", border.with(color))?;

        Ok(())
    }
}
