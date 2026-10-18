//! Static media rendering for manpages.

use std::io::Write;

use acdc_converters_core::{
    InlineTextTransform, TraversalContext,
    media::{resolve_target, with_time_range},
    video::watch_urls,
    visitor::WritableVisitor,
};
use acdc_parser::{Audio, Image, InlineNode, Source, Video};

use crate::{
    Error, ManpageVisitor,
    escape::{EscapeMode, escape_rendered_roff_macro_argument, escape_roff_macro_argument, manify},
};

fn block_image_alt(image: &Image<'_>) -> String {
    acdc_converters_core::media::image_alt(image, false).into_owned()
}

fn inline_image_alt(image: &Image<'_>) -> String {
    acdc_converters_core::media::image_alt(image, true).into_owned()
}

fn media_title(title: &[InlineNode<'_>], source: Option<&Source<'_>>, fallback: &str) -> String {
    if title.is_empty() {
        source.map_or_else(
            || fallback.to_string(),
            |source| {
                source
                    .get_filename()
                    .map_or_else(|| source.to_string(), str::to_string)
            },
        )
    } else {
        InlineTextTransform::default().to_string(title)
    }
}

impl<'a, W: Write> ManpageVisitor<'a, '_, W> {
    pub(crate) fn render_image(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        image: &Image<'_>,
    ) -> Result<(), Error> {
        self.write_sp()?;
        self.render_captioned_title(traversal, &image.title, &image.metadata)?;
        let alt = block_image_alt(image);
        let label = format!("[{alt}]");
        let link = image.metadata.attributes.get_string("link");
        self.write_static_link(link.as_deref(), &label, false)
    }

    pub(crate) fn render_inline_image(&mut self, image: &Image<'_>) -> Result<(), Error> {
        let label = format!("[{}]", inline_image_alt(image));
        let link = image.metadata.attributes.get_string("link");
        self.write_static_link(link.as_deref(), &label, true)
    }

    pub(crate) fn render_video(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        video: &Video<'_>,
    ) -> Result<(), Error> {
        self.warn_static_media_fallback();
        self.write_sp()?;
        self.render_captioned_title(traversal, &video.title, &video.metadata)?;
        let title = media_title(&video.title, video.sources.first(), "video");
        let targets = watch_urls(video)?;
        let source_count = targets.len();
        if source_count == 0 {
            self.write_static_link(None, "[Video]", false)?;
        }

        let mut has_link = false;
        for (index, target) in targets.iter().enumerate() {
            if has_link {
                writeln!(self.writer_mut(), ".br")?;
            }
            let target = resolve_target(target, traversal);
            let label = if source_count == 1 {
                format!("[Video: {title}]")
            } else {
                format!("[Video source {}/{source_count}: {title}]", index + 1)
            };
            self.write_static_link(Some(&target), &label, false)?;
            has_link = true;
        }

        if let Some(poster) = video.metadata.attributes.get_string("poster") {
            if has_link || source_count == 0 {
                writeln!(self.writer_mut(), ".br")?;
            }
            let target = resolve_target(&poster, traversal);
            let label = format!("[Poster: {poster}]");
            self.write_static_link(Some(&target), &label, false)?;
        }
        Ok(())
    }

    pub(crate) fn render_audio(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        audio: &Audio<'_>,
    ) -> Result<(), Error> {
        self.warn_static_media_fallback();
        self.write_sp()?;
        self.render_captioned_title(traversal, &audio.title, &audio.metadata)?;
        let title = media_title(&audio.title, Some(&audio.source), "audio");
        let mut target = resolve_target(&audio.source.to_string(), traversal);
        target = with_time_range(&target, &audio.metadata.attributes);
        let label = format!("[Audio: {title}]");
        self.write_static_link(
            (!target.is_empty()).then_some(target.as_str()),
            &label,
            false,
        )
    }

    fn warn_static_media_fallback(&mut self) {
        if !self.processor.static_media_warning.replace(true) {
            self.diagnostics.warn_with_advice(
                "audio and video playback are not available in manpage output; rendering static links",
                "Use an HTML-capable backend when embedded playback controls are required.",
            );
        }
    }

    fn write_static_link(
        &mut self,
        target: Option<&str>,
        label: &str,
        inline: bool,
    ) -> Result<(), Error> {
        let label = manify(label, EscapeMode::Collapse);
        let Some(target) = target.filter(|target| !target.is_empty()) else {
            if inline {
                write!(self.writer_mut(), "{label}")?;
            } else {
                writeln!(self.writer_mut(), "{label}")?;
            }
            return Ok(());
        };

        let target = escape_roff_macro_argument(target);
        let label = escape_rendered_roff_macro_argument(&label);
        if inline {
            return self.write_link_command("URL", &target, &label);
        }
        writeln!(self.writer_mut(), ".URL \"{target}\" \"{label}\" \"\"")?;
        Ok(())
    }
}
