//! Static media rendering for manpages.

use std::io::Write;

use acdc_converters_core::{
    InlineTextTransform, TraversalContext,
    media::{resolve_target, with_time_range_from_attributes},
    video::TryUrl,
};
use acdc_parser::ast::{AudioRef, ContentRef, ImageRef, InlineImageRef, SourceRef, VideoRef};

use crate::{
    Error, ManpageVisitor,
    escape::{EscapeMode, escape_rendered_roff_macro_argument, escape_roff_macro_argument, manify},
};

fn media_title(
    title: Option<ContentRef<'_>>,
    source: Option<SourceRef<'_>>,
    fallback: &str,
) -> String {
    if let Some(title) = title {
        InlineTextTransform::default().content_to_string(title)
    } else {
        source.map_or_else(
            || fallback.to_string(),
            |source| {
                source
                    .file_name()
                    .map_or_else(|| source.to_string(), str::to_string)
            },
        )
    }
}

impl<'a, W: Write> ManpageVisitor<'a, '_, W> {
    pub(crate) fn render_image(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        image: ImageRef<'_>,
    ) -> Result<(), Error> {
        self.write_sp()?;
        self.render_captioned_title(traversal, image.title(), image.attributes())?;
        let alt = acdc_converters_core::media::image_alt_from_content(image.source(), image.alt());
        self.render_media_label(
            traversal,
            image.alt(),
            &alt,
            super::manpage_visitor::attribute_text(image.attributes(), "link"),
            false,
        )
    }

    pub(crate) fn render_inline_image(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        image: InlineImageRef<'_>,
    ) -> Result<(), Error> {
        let alt = acdc_converters_core::media::image_alt_from_content(image.source(), image.alt());
        self.render_media_label(
            traversal,
            image.alt(),
            &alt,
            super::manpage_visitor::attribute_text(image.attributes(), "link"),
            true,
        )
    }

    pub(crate) fn render_media_label(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        content: Option<ContentRef<'_>>,
        fallback: &str,
        target: Option<&str>,
        inline: bool,
    ) -> Result<(), Error> {
        let target = target.filter(|target| !target.is_empty());
        let Some(content) = content else {
            return self.write_static_link(target, &format!("[{fallback}]"), inline);
        };
        let mut render = |visitor: &mut Self| {
            write!(visitor.writer_mut(), "[")?;
            visitor.write_content(traversal, content)?;
            write!(visitor.writer_mut(), "]")?;
            Ok(())
        };
        if let Some(target) = target {
            self.with_link_label(escape_roff_macro_argument(target), false, render)?;
        } else {
            render(self)?;
        }
        if !inline {
            if target.is_some() {
                writeln!(self.writer_mut(), ".br")?;
            } else {
                writeln!(self.writer_mut())?;
            }
        }
        Ok(())
    }

    pub(crate) fn render_video(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        video: VideoRef<'_>,
    ) -> Result<(), Error> {
        self.warn_static_media_fallback();
        self.write_sp()?;
        self.render_captioned_title(traversal, video.title(), video.attributes())?;
        let title = media_title(video.title(), Some(video.source()), "video");
        let target = resolve_target(&video.try_url(false)?, traversal);
        let label = format!("[Video: {title}]");
        self.write_static_link(Some(&target), &label, false)?;

        if let Some(poster) = super::manpage_visitor::attribute_text(video.attributes(), "poster")
            .filter(|value| !matches!(*value, "youtube" | "vimeo"))
        {
            writeln!(self.writer_mut(), ".br")?;
            let target = resolve_target(poster, traversal);
            let label = format!("[Poster: {poster}]");
            self.write_static_link(Some(&target), &label, false)?;
        }
        Ok(())
    }

    pub(crate) fn render_audio(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        audio: AudioRef<'_>,
    ) -> Result<(), Error> {
        self.warn_static_media_fallback();
        self.write_sp()?;
        self.render_captioned_title(traversal, audio.title(), audio.attributes())?;
        let title = media_title(audio.title(), Some(audio.source()), "audio");
        let mut target = resolve_target(&audio.source().to_string(), traversal);
        target = with_time_range_from_attributes(&target, audio.attributes());
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
