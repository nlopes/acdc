use std::io::Write;

use acdc_converters_common::video::TryUrl;
use acdc_parser::Video;

use crate::{Processor, Render, RenderOptions};

impl Render for Video {
    type Error = crate::Error;

    fn render<W: Write>(
        &self,
        w: &mut W,
        processor: &Processor,
        options: &RenderOptions,
    ) -> Result<(), Self::Error> {
        write!(w, "<div")?;
        if let Some(id) = &self.metadata.id {
            write!(w, " id=\"{}\"", id.id)?;
        }
        writeln!(w, " class=\"videoblock\">")?;

        if !self.title.is_empty() {
            write!(w, "<div class=\"title\">")?;
            crate::inlines::render_inlines(&self.title, w, processor, options)?;
            writeln!(w, "</div>")?;
        }

        writeln!(w, "<div class=\"content\">")?;

        // Video blocks can have multiple sources
        if self.sources.is_empty() {
            writeln!(w, "</div>")?;
            writeln!(w, "</div>")?;
            return Ok(());
        }

        // Check if this is a YouTube or Vimeo video
        // The platform is stored as an attribute with boolean value
        let is_youtube = matches!(
            self.metadata.attributes.get("youtube"),
            Some(acdc_parser::AttributeValue::Bool(true))
        );
        let is_vimeo = matches!(
            self.metadata.attributes.get("vimeo"),
            Some(acdc_parser::AttributeValue::Bool(true))
        );

        if is_youtube || is_vimeo {
            render_iframe_video(self, w)?;
        } else {
            render_local_video(self, w)?;
        }

        writeln!(w, "</div>")?;
        writeln!(w, "</div>")?;

        Ok(())
    }
}

/// Render a video as an iframe, suitable for `YouTube` or `Vimeo` embedding.
fn render_iframe_video<W: Write>(video: &Video, w: &mut W) -> Result<(), crate::Error> {
    let url = video.try_url(true)?;
    let allow_fullscreen = !video.metadata.options.iter().any(|o| o == "nofullscreen");

    write!(w, "<iframe")?;

    if let Some(acdc_parser::AttributeValue::String(width)) = video.metadata.attributes.get("width")
    {
        write!(w, " width=\"{width}\"")?;
    }

    if let Some(acdc_parser::AttributeValue::String(height)) =
        video.metadata.attributes.get("height")
    {
        write!(w, " height=\"{height}\"")?;
    }

    write!(w, " src=\"{url}\" frameborder=\"0\"")?;

    if allow_fullscreen {
        write!(w, " allowfullscreen")?;
    }

    writeln!(w, "></iframe>")?;

    Ok(())
}

/// Render a local video using the `HTML5` `<video>` tag.
fn render_local_video<W: Write>(video: &Video, w: &mut W) -> Result<(), crate::Error> {
    let src = video.try_url(false)?;

    write!(w, "<video src=\"{src}\"")?;

    if let Some(acdc_parser::AttributeValue::String(width)) = video.metadata.attributes.get("width")
    {
        write!(w, " width=\"{width}\"")?;
    }

    if let Some(acdc_parser::AttributeValue::String(height)) =
        video.metadata.attributes.get("height")
    {
        write!(w, " height=\"{height}\"")?;
    }

    if let Some(acdc_parser::AttributeValue::String(poster)) =
        video.metadata.attributes.get("poster")
    {
        write!(w, " poster=\"{poster}\"")?;
    }

    if let Some(acdc_parser::AttributeValue::String(preload)) =
        video.metadata.attributes.get("preload")
    {
        write!(w, " preload=\"{preload}\"")?;
    }

    if video.metadata.options.iter().any(|o| o == "autoplay") {
        write!(w, " autoplay")?;
    }

    if !video.metadata.options.iter().any(|o| o == "muted") {
        write!(w, " muted")?;
    }

    // Add nocontrols option check - if present, don't add controls
    if !video.metadata.options.iter().any(|o| o == "nocontrols") {
        write!(w, " controls")?;
    }

    if video.metadata.options.iter().any(|o| o == "loop") {
        write!(w, " loop")?;
    }

    writeln!(w, ">")?;
    writeln!(w, "Your browser does not support the video tag.")?;
    writeln!(w, "</video>")?;

    Ok(())
}
