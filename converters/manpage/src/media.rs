//! Static media rendering for manpages.

use std::{borrow::Cow, fmt::Write as _, io::Write, path::Path};

use acdc_converters_core::{
    InlineTextTransform, media::resolve_target, video::TryUrl, visitor::WritableVisitor,
};
use acdc_parser::{AttributeValue, Audio, Image, InlineNode, Source, Video};

use crate::{
    Error, ManpageVisitor,
    escape::{EscapeMode, manify},
};

fn image_filename_alt(source: &Source<'_>) -> String {
    source
        .get_filename()
        .and_then(|filename| Path::new(filename).file_stem())
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or_default()
        .replace(['-', '_'], " ")
}

fn block_image_alt(image: &Image<'_>) -> String {
    image
        .metadata
        .attributes
        .get_string("alt")
        .map_or_else(|| image_filename_alt(&image.source), Cow::into_owned)
}

fn inline_image_alt(image: &Image<'_>) -> String {
    if image.title.is_empty() {
        block_image_alt(image)
    } else {
        InlineTextTransform::default().to_string(image.title.as_ref())
    }
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

impl<W: Write> ManpageVisitor<'_, '_, W> {
    pub(crate) fn render_image(&mut self, image: &Image<'_>) -> Result<(), Error> {
        self.write_sp()?;
        self.render_captioned_title(&image.title, &image.metadata)?;
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

    pub(crate) fn render_video(&mut self, video: &Video<'_>) -> Result<(), Error> {
        self.warn_static_media_fallback();
        self.write_sp()?;
        self.render_captioned_title(&video.title, &video.metadata)?;
        let title = media_title(&video.title, video.sources.first(), "video");
        let source_count = video.sources.len();
        if source_count == 0 {
            self.write_static_link(None, "[Video]", false)?;
        }

        let mut has_link = false;
        for (index, source) in video.sources.iter().enumerate() {
            if has_link {
                writeln!(self.writer_mut(), ".br")?;
            }
            let mut single_source = video.clone();
            single_source.sources = vec![source.clone()];
            let target = single_source.try_url(false)?;
            let target = resolve_target(&target, &self.processor.document_attributes);
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
            let target = resolve_target(&poster, &self.processor.document_attributes);
            let label = format!("[Poster: {poster}]");
            self.write_static_link(Some(&target), &label, false)?;
        }
        Ok(())
    }

    pub(crate) fn render_audio(&mut self, audio: &Audio<'_>) -> Result<(), Error> {
        self.warn_static_media_fallback();
        self.write_sp()?;
        self.render_captioned_title(&audio.title, &audio.metadata)?;
        let title = media_title(&audio.title, Some(&audio.source), "audio");
        let mut target = resolve_target(
            &audio.source.to_string(),
            &self.processor.document_attributes,
        );
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

        if inline {
            writeln!(self.writer_mut(), "\\c")?;
        }
        let target = manify(target, EscapeMode::Collapse);
        let suffix = if inline { "\\c" } else { "" };
        writeln!(
            self.writer_mut(),
            ".URL \"{target}\" \"{label}\" \"{suffix}\""
        )?;
        if inline {
            self.strip_next_leading_space = true;
        }
        Ok(())
    }
}
