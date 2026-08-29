use std::io::Write;

use acdc_converters_core::{video::TryUrl, visitor::WritableVisitor};
use acdc_parser::{AttributeValue, Video};

use crate::{Error, TerminalVisitor, inlines};

impl<W: Write> TerminalVisitor<'_, '_, W> {
    pub(crate) fn render_video(&mut self, video: &Video) -> Result<(), Error> {
        if video.sources.is_empty() {
            if self.processor.mark_fallback("video-missing-source") {
                self.diagnostics.warn_with_advice(
                    "video has no source and cannot be represented in terminal output",
                    "Add at least one source to the `video::` macro.",
                );
            }
            return Ok(());
        }

        let title = if video.title.is_empty() {
            video
                .sources
                .first()
                .and_then(acdc_parser::Source::get_filename)
                .map_or("video", |name| name)
                .to_string()
        } else {
            acdc_converters_core::inlines_to_string(&video.title)
        };
        let processor = self.processor.clone();
        for (index, source) in video.sources.iter().enumerate() {
            let mut single_source = video.clone();
            single_source.sources = vec![source.clone()];
            let target = single_source.try_url(false)?;
            let label = if video.sources.len() == 1 {
                format!("[Video: {title}]")
            } else {
                format!(
                    "[Video source {}/{}: {title}]",
                    index + 1,
                    video.sources.len()
                )
            };
            inlines::maybe_render_osc8_link(&target, &label, self.writer_mut(), &processor)?;
            writeln!(self.writer_mut())?;
        }
        if let Some(AttributeValue::String(poster)) = video.metadata.attributes.get("poster") {
            writeln!(self.writer_mut(), "[Poster: {poster}]")?;
        }
        Ok(())
    }
}
