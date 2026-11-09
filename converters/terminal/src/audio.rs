use std::io::Write;

use acdc_converters_core::{
    TraversalContext,
    media::{resolve_target, with_time_range_from_attributes},
};
use acdc_parser::ast::AudioRef;

use crate::{Error, TerminalVisitor, inlines};

impl<'a, W: Write> TerminalVisitor<'a, '_, W> {
    pub(crate) fn render_audio(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        audio: AudioRef<'_>,
    ) -> Result<(), Error> {
        let mut target = resolve_target(&audio.source().to_string(), traversal);
        if target.is_empty() {
            if self.converter.mark_fallback("audio-missing-source") {
                self.diagnostics.warn_with_advice(
                    "audio has no source and cannot be represented in terminal output",
                    "Add a source to the `audio::` macro.",
                );
            }
            return Ok(());
        }
        target = with_time_range_from_attributes(&target, audio.attributes());
        let title = if let Some(title) = audio.title() {
            acdc_converters_core::InlineTextTransform::default()
                .decode_char_refs(true)
                .content_to_string(title)
        } else {
            audio
                .source()
                .file_name()
                .map_or_else(|| audio.source().to_string(), str::to_string)
        };
        let text = format!("[Audio: {title}]");
        let converter = self.converter;
        inlines::maybe_render_osc8_link(&target, &text, self.writer_mut(), converter)?;
        writeln!(self.writer_mut())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use acdc_parser::{Audio, Location, Source};

    use super::*;

    #[test]
    fn missing_audio_source_warns_once() -> Result<(), Error> {
        let audio = Audio::new(Source::Name(""), Location::default());
        let converter = crate::create_test_converter();
        let mut warnings = Vec::new();
        let source = acdc_converters_core::WarningSource::new("terminal");
        let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
        let attribute_header =
            acdc_converters_core::Converter::document_attributes(&converter).clone();
        let mut traversal = TraversalContext::new(&attribute_header);
        let mut visitor = TerminalVisitor::new(Vec::new(), &converter, diagnostics.reborrow());

        visitor.render_audio(&mut traversal, (&audio).into())?;
        visitor.render_audio(&mut traversal, (&audio).into())?;
        let output = visitor.into_writer();

        assert_eq!(output, [] as [u8; 0]);
        assert_eq!(warnings.len(), 1);
        assert!(
            warnings
                .first()
                .is_some_and(|warning| warning.message.contains("audio has no source"))
        );
        Ok(())
    }
}
