use std::io::Write;

use acdc_converters_core::{
    TraversalContext, media::resolve_target, video::TryUrl, visitor::WritableVisitor,
};
use acdc_parser::{AttributeValue, Video};

use crate::{Error, TerminalVisitor, inlines};

impl<'a, W: Write> TerminalVisitor<'a, '_, W> {
    pub(crate) fn render_video(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        video: &Video,
    ) -> Result<(), Error> {
        if video.source.to_string().is_empty() {
            if self.processor.mark_fallback("video-missing-source") {
                self.diagnostics.warn_with_advice(
                    "video has no source and cannot be represented in terminal output",
                    "Add at least one source to the `video::` macro.",
                );
            }
            return Ok(());
        }

        let title = if video.title.is_present() {
            acdc_converters_core::inlines_to_string(&video.title)
        } else {
            video
                .source
                .get_filename()
                .map_or("video", |name| name)
                .to_string()
        };
        let processor = self.processor;
        let target = resolve_target(&video.try_url(false)?, traversal);
        let label = format!("[Video: {title}]");
        inlines::maybe_render_osc8_link(&target, &label, self.writer_mut(), processor)?;
        writeln!(self.writer_mut())?;
        if let Some(AttributeValue::String(poster)) = video.metadata.attributes.get("poster")
            && !matches!(poster.as_ref(), "youtube" | "vimeo")
        {
            writeln!(self.writer_mut(), "[Poster: {poster}]")?;
        }
        Ok(())
    }
}
