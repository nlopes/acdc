use std::io::Write;

use acdc_converters_core::{
    TraversalContext,
    icon::IconMode,
    visitor::{Visitor, WritableVisitor},
};
use acdc_parser::{Admonition, AdmonitionVariant};
use crossterm::{
    QueueableCommand,
    style::{PrintStyledContent, Stylize},
};

use crate::{Error, TerminalVisitor};

impl<'a, W: Write> TerminalVisitor<'a, '_, W> {
    /// Visit an admonition block (NOTE, TIP, IMPORTANT, WARNING, CAUTION).
    ///
    /// Renders with bold caption and left border.
    pub(crate) fn render_admonition(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        admon: &'a Admonition<'a>,
    ) -> Result<(), Error> {
        use std::io::BufWriter;

        let processor = self.processor;

        // Get icon, caption attribute, and theme color for this admonition type
        let (glyph, caption_attr, color) = match admon.variant {
            AdmonitionVariant::Note => {
                ("ℹ️", "note-caption", processor.appearance.colors.admon_note)
            }
            AdmonitionVariant::Tip => ("💡", "tip-caption", processor.appearance.colors.admon_tip),
            AdmonitionVariant::Important => (
                "❗",
                "important-caption",
                processor.appearance.colors.admon_important,
            ),
            AdmonitionVariant::Warning => (
                "⚠️",
                "warning-caption",
                processor.appearance.colors.admon_warning,
            ),
            AdmonitionVariant::Caution => (
                "🔥",
                "caution-caption",
                processor.appearance.colors.admon_caution,
            ),
        };
        let icon = (IconMode::from_attributes(traversal) != IconMode::Text
            && processor.appearance.capabilities.unicode)
            .then_some(glyph);

        let caption = traversal
            .get(caption_attr)
            .and_then(|value| value.text())
            .ok_or(Error::InvalidAdmonitionCaption(caption_attr.to_string()))?
            .to_string();

        // Border character based on terminal capabilities
        let border = if processor.appearance.capabilities.unicode {
            "│"
        } else {
            "|"
        };

        // Header line with icon, bold caption, and left border
        let w = self.writer_mut();
        writeln!(w)?;
        write!(w, "{} ", border.with(color))?;
        if let Some(icon) = icon {
            write!(w, "{icon} ")?;
        }
        let styled_caption = format!("{caption}:").bold();
        QueueableCommand::queue(w, PrintStyledContent(styled_caption))?;

        // Title on same line if present
        if admon.title.is_empty() {
            writeln!(w)?;
        } else {
            write!(w, " ")?;
            let mut title_buffer = Vec::new();
            let title_processor = processor;
            let mut title_visitor = TerminalVisitor::new(
                &mut title_buffer,
                title_processor,
                self.diagnostics.reborrow(),
            );
            title_visitor.visit_inline_nodes(traversal, &admon.title)?;

            let title_text = String::from_utf8_lossy(&title_buffer);
            let w = self.writer_mut();
            writeln!(w, "{}", title_text.trim())?;
        }

        // Render content blocks with left border
        for block in &admon.blocks {
            let buffer = Vec::new();
            let inner = BufWriter::new(buffer);
            let mut temp_visitor =
                TerminalVisitor::new(inner, processor, self.diagnostics.reborrow());
            traversal.visit_block(&mut temp_visitor, block)?;

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
