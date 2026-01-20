use std::io::Write;

use acdc_converters_core::{video::TryUrl, visitor::WritableVisitor};
use acdc_parser::{AttributeValue, Video};

use crate::Error;

pub(crate) fn visit_video<V: WritableVisitor<Error = Error>>(
    video: &Video,
    visitor: &mut V,
) -> Result<(), Error> {
    let mut w = visitor.writer_mut();
    write!(w, "<div")?;
    if let Some(id) = &video.metadata.id {
        write!(w, " id=\"{}\"", id.id)?;
    }
    writeln!(w, " class=\"videoblock\">")?;

    if !video.title.is_empty() {
        write!(w, "<div class=\"title\">")?;
        let _ = w;
        visitor.visit_inline_nodes(&video.title)?;
        w = visitor.writer_mut();
        writeln!(w, "</div>")?;
    }

    writeln!(w, "<div class=\"content\">")?;

    // Video blocks can have multiple sources
    if video.sources.is_empty() {
        writeln!(w, "</div>")?;
        writeln!(w, "</div>")?;
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
        render_iframe_video(video, w)?;
    } else {
        render_local_video(video, w)?;
    }

    writeln!(w, "</div>")?;
    writeln!(w, "</div>")?;

    Ok(())
}

/// Render a video as an iframe, suitable for `YouTube` or `Vimeo` embedding.
fn render_iframe_video<W: Write + ?Sized>(video: &Video, w: &mut W) -> Result<(), Error> {
    let url = video.try_url(true)?;
    let allow_fullscreen = !video.metadata.options.iter().any(|o| o == "nofullscreen");

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

    if video.metadata.options.iter().any(|o| o == "autoplay") {
        write!(w, " autoplay")?;
    }

    if video.metadata.options.iter().any(|o| o == "muted") {
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
