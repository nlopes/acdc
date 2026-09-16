use std::{fmt::Write as _, io::Write};

use acdc_converters_core::{TraversalContext, media::resolve_target, visitor::WritableVisitor};
use acdc_parser::{AttributeValue, Audio};

use crate::{Error, TerminalVisitor, inlines};

impl<'a, W: Write> TerminalVisitor<'a, '_, W> {
    pub(crate) fn render_audio(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        audio: &Audio,
    ) -> Result<(), Error> {
        let mut target = resolve_target(&audio.source.to_string(), traversal);
        if target.is_empty() {
            if self.processor.mark_fallback("audio-missing-source") {
                self.diagnostics.warn_with_advice(
                    "audio has no source and cannot be represented in terminal output",
                    "Add a source to the `audio::` macro.",
                );
            }
            return Ok(());
        }
        match (
            audio.metadata.attributes.get("start"),
            audio.metadata.attributes.get("end"),
        ) {
            (Some(AttributeValue::String(start)), Some(AttributeValue::String(end))) => {
                let _ = write!(target, "#t={start},{end}");
            }
            (Some(AttributeValue::String(start)), _) => {
                let _ = write!(target, "#t={start}");
            }
            _ => {}
        }
        let title = if audio.title.is_empty() {
            audio
                .source
                .get_filename()
                .map_or_else(|| audio.source.to_string(), str::to_string)
        } else {
            acdc_converters_core::inlines_to_string(&audio.title)
        };
        let text = format!("[Audio: {title}]");
        let processor = self.processor;
        inlines::maybe_render_osc8_link(&target, &text, self.writer_mut(), processor)?;
        writeln!(self.writer_mut())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use acdc_parser::{Location, Source};

    use super::*;

    #[test]
    fn missing_audio_source_warns_once() -> Result<(), Error> {
        let audio = Audio::new(Source::Name(""), Location::default());
        let processor = crate::create_test_processor();
        let mut warnings = Vec::new();
        let source = acdc_converters_core::WarningSource::new("terminal");
        let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
        let attribute_header =
            acdc_converters_core::Converter::document_attributes(&processor).clone();
        let mut traversal = TraversalContext::new(&attribute_header);
        let mut visitor = TerminalVisitor::new(Vec::new(), &processor, diagnostics.reborrow());

        visitor.render_audio(&mut traversal, &audio)?;
        visitor.render_audio(&mut traversal, &audio)?;
        let output = visitor.into_writer();

        assert!(output.is_empty());
        assert_eq!(warnings.len(), 1);
        assert!(
            warnings
                .first()
                .is_some_and(|warning| warning.message.contains("audio has no source"))
        );
        Ok(())
    }
}
