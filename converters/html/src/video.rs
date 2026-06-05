use std::io::Write;

use acdc_converters_core::{video::TryUrl, visitor::Visitor};
use acdc_parser::{AttributeValue, Video};

use crate::{Error, HtmlVariant, HtmlVisitor};

impl<W: Write> HtmlVisitor<'_, '_, W> {
    pub(crate) fn render_video(&mut self, video: &Video) -> Result<(), Error> {
        if self.processor.variant() == HtmlVariant::Semantic {
            return visit_video_semantic(video, self);
        }

        write!(self.writer, "<div")?;
        if let Some(id) = &video.metadata.id {
            write!(self.writer, " id=\"{}\"", id.id)?;
        }
        writeln!(self.writer, " class=\"videoblock\">")?;

        if !video.title.is_empty() {
            write!(self.writer, "<div class=\"title\">")?;
            self.visit_inline_nodes(&video.title)?;
            writeln!(self.writer, "</div>")?;
        }

        writeln!(self.writer, "<div class=\"content\">")?;

        // Video blocks can have multiple sources
        if video.sources.is_empty() {
            writeln!(self.writer, "</div>")?;
            writeln!(self.writer, "</div>")?;
            return Ok(());
        }

        // Check if this is a YouTube or Vimeo video
        // The platform is stored as an attribute with boolean value
        let is_youtube = matches!(
            video.metadata.attributes.get("youtube"),
            Some(AttributeValue::Bool(true))
        );
        let is_vimeo = matches!(
            video.metadata.attributes.get("vimeo"),
            Some(AttributeValue::Bool(true))
        );

        if is_youtube || is_vimeo {
            render_iframe_video(video, &mut self.writer)?;
        } else {
            render_local_video(video, &mut self.writer)?;
        }

        writeln!(self.writer, "</div>")?;
        writeln!(self.writer, "</div>")?;

        Ok(())
    }
}

/// Render a video as an iframe, suitable for `YouTube` or `Vimeo` embedding.
fn render_iframe_video<W: Write + ?Sized>(video: &Video, w: &mut W) -> Result<(), Error> {
    let url = video.try_url(true)?;
    let allow_fullscreen = !video.metadata.options.contains(&"nofullscreen");

    write!(w, "<iframe")?;

    if let Some(AttributeValue::String(width)) = video.metadata.attributes.get("width") {
        write!(w, " width=\"{width}\"")?;
    }

    if let Some(AttributeValue::String(height)) = video.metadata.attributes.get("height") {
        write!(w, " height=\"{height}\"")?;
    }

    write!(w, " src=\"{url}\"")?;

    if allow_fullscreen {
        write!(w, " allowfullscreen")?;
    }

    writeln!(w, "></iframe>")?;

    Ok(())
}

/// Render a local video using the `HTML5` `<video>` tag.
fn render_local_video<W: Write + ?Sized>(video: &Video, w: &mut W) -> Result<(), Error> {
    let src = video.try_url(false)?;

    write!(w, "<video src=\"{src}\"")?;

    if let Some(AttributeValue::String(width)) = video.metadata.attributes.get("width") {
        write!(w, " width=\"{width}\"")?;
    }

    if let Some(AttributeValue::String(height)) = video.metadata.attributes.get("height") {
        write!(w, " height=\"{height}\"")?;
    }

    if let Some(AttributeValue::String(poster)) = video.metadata.attributes.get("poster") {
        write!(w, " poster=\"{poster}\"")?;
    }

    if let Some(AttributeValue::String(preload)) = video.metadata.attributes.get("preload") {
        write!(w, " preload=\"{preload}\"")?;
    }

    if video.metadata.options.contains(&"autoplay") {
        write!(w, " autoplay")?;
    }

    if video.metadata.options.contains(&"muted") {
        write!(w, " muted")?;
    }

    // Add nocontrols option check - if present, don't add controls
    if !video.metadata.options.contains(&"nocontrols") {
        write!(w, " controls")?;
    }

    if video.metadata.options.contains(&"loop") {
        write!(w, " loop")?;
    }

    writeln!(w, ">")?;
    writeln!(w, "Your browser does not support the video tag.")?;
    writeln!(w, "</video>")?;

    Ok(())
}

fn visit_video_semantic<W: Write>(
    video: &Video,
    visitor: &mut HtmlVisitor<'_, '_, W>,
) -> Result<(), Error> {
    let has_title = !video.title.is_empty();

    let tag = if has_title { "figure" } else { "div" };
    write!(visitor.writer, "<{tag} class=\"video-block\"")?;
    if let Some(id) = &video.metadata.id {
        write!(visitor.writer, " id=\"{}\"", id.id)?;
    }
    writeln!(visitor.writer, ">")?;

    if video.sources.is_empty() {
        writeln!(visitor.writer, "</{tag}>")?;
        return Ok(());
    }

    let is_youtube = matches!(
        video.metadata.attributes.get("youtube"),
        Some(AttributeValue::Bool(true))
    );
    let is_vimeo = matches!(
        video.metadata.attributes.get("vimeo"),
        Some(AttributeValue::Bool(true))
    );

    if is_youtube || is_vimeo {
        render_iframe_video(video, &mut visitor.writer)?;
    } else {
        render_local_video(video, &mut visitor.writer)?;
    }

    if has_title {
        write!(visitor.writer, "<figcaption>")?;
        visitor.visit_inline_nodes(&video.title)?;
        writeln!(visitor.writer, "</figcaption>")?;
    }

    writeln!(visitor.writer, "</{tag}>")?;
    Ok(())
}
