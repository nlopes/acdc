use std::io::Write;

use acdc_converters_core::{TraversalContext, media::resolve_target, video::TryUrl};
use acdc_parser::ast::{AttributeValueRef, VideoRef};

use crate::{
    Error, HtmlVariant, HtmlVisitor, build_class, inlines::escape_attribute, write_element_id,
};

impl<'a, W: Write> HtmlVisitor<'a, '_, W> {
    pub(crate) fn render_video(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        video: VideoRef<'_>,
    ) -> Result<(), Error> {
        if self.processor.variant() == HtmlVariant::Semantic {
            return visit_video_semantic(traversal, video, self);
        }

        write!(self.writer, "<div")?;
        write_element_id(&mut self.writer, video.id())?;
        let mut class = build_class("videoblock", video.roles().iter());
        if let Some(align) = video
            .attributes()
            .get("align")
            .and_then(AttributeValueRef::as_str)
        {
            class.push_str(" text-");
            class.push_str(align);
        }
        if let Some(float) = video
            .attributes()
            .get("float")
            .and_then(AttributeValueRef::as_str)
        {
            class.push(' ');
            class.push_str(float);
        }
        writeln!(self.writer, " class=\"{class}\">")?;

        if video.title().is_some() {
            write!(self.writer, "<div class=\"title\">")?;
            self.visit_title(traversal, video.title())?;
            writeln!(self.writer, "</div>")?;
        }

        writeln!(self.writer, "<div class=\"content\">")?;

        let is_youtube = video
            .attributes()
            .get("poster")
            .and_then(AttributeValueRef::as_str)
            == Some("youtube");
        let is_vimeo = video
            .attributes()
            .get("poster")
            .and_then(AttributeValueRef::as_str)
            == Some("vimeo");

        if is_youtube || is_vimeo {
            render_iframe_video(video, traversal, &mut self.writer)?;
        } else {
            render_local_video(video, traversal, &mut self.writer)?;
        }

        writeln!(self.writer, "</div>")?;
        writeln!(self.writer, "</div>")?;

        Ok(())
    }
}

/// Render a video as an iframe, suitable for `YouTube` or `Vimeo` embedding.
fn render_iframe_video<W: Write + ?Sized>(
    video: VideoRef<'_>,
    attributes: &TraversalContext<'_>,
    w: &mut W,
) -> Result<(), Error> {
    let mut url = resolve_target(&video.try_url(true)?, attributes);
    if let Some(scheme) = attributes
        .get("asset-uri-scheme")
        .and_then(|value| value.text())
    {
        let prefix = if scheme.is_empty() {
            String::new()
        } else {
            format!("{scheme}:")
        };
        if let Some(target) = url.strip_prefix("https:") {
            url = format!("{prefix}{target}");
        }
    }
    let allow_fullscreen = !video.options().contains("nofullscreen");

    write!(w, "<iframe")?;

    if let Some(AttributeValueRef::Text(width)) = video.attributes().get("width") {
        write!(w, " width=\"{}\"", escape_attribute(width))?;
    }

    if let Some(AttributeValueRef::Text(height)) = video.attributes().get("height") {
        write!(w, " height=\"{}\"", escape_attribute(height))?;
    }

    write!(w, " src=\"{}\" frameborder=\"0\"", escape_attribute(&url))?;

    if allow_fullscreen {
        write!(w, " allowfullscreen")?;
    }

    writeln!(w, "></iframe>")?;

    Ok(())
}

/// Render a local video using the `HTML5` `<video>` tag.
fn render_local_video<W: Write + ?Sized>(
    video: VideoRef<'_>,
    attributes: &TraversalContext<'_>,
    w: &mut W,
) -> Result<(), Error> {
    let src = resolve_target(&video.try_url(false)?, attributes);

    write!(w, "<video src=\"{}\"", escape_attribute(&src))?;

    if let Some(AttributeValueRef::Text(width)) = video.attributes().get("width") {
        write!(w, " width=\"{}\"", escape_attribute(width))?;
    }

    if let Some(AttributeValueRef::Text(height)) = video.attributes().get("height") {
        write!(w, " height=\"{}\"", escape_attribute(height))?;
    }

    if let Some(poster) = video
        .attributes()
        .get("poster")
        .and_then(AttributeValueRef::as_str)
        .filter(|value| !value.is_empty())
    {
        let poster = resolve_target(poster, attributes);
        write!(w, " poster=\"{}\"", escape_attribute(&poster))?;
    }

    if let Some(preload) = video
        .attributes()
        .get("preload")
        .and_then(AttributeValueRef::as_str)
        .filter(|value| !value.is_empty())
    {
        write!(w, " preload=\"{}\"", escape_attribute(preload))?;
    }

    if video.options().contains("autoplay") {
        write!(w, " autoplay")?;
    }

    if video.options().contains("muted") {
        write!(w, " muted")?;
    }

    // Add nocontrols option check - if present, don't add controls
    if !video.options().contains("nocontrols") {
        write!(w, " controls")?;
    }

    if video.options().contains("loop") {
        write!(w, " loop")?;
    }

    writeln!(w, ">")?;
    writeln!(w, "Your browser does not support the video tag.")?;
    writeln!(w, "</video>")?;

    Ok(())
}

fn visit_video_semantic<'a, W: Write>(
    traversal: &mut TraversalContext<'a>,
    video: VideoRef<'_>,
    visitor: &mut HtmlVisitor<'a, '_, W>,
) -> Result<(), Error> {
    let has_title = video.title().is_some();

    let tag = if has_title { "figure" } else { "div" };
    let class = build_class("video-block", video.roles().iter());
    write!(visitor.writer, "<{tag}")?;
    write_element_id(&mut visitor.writer, video.id())?;
    write!(visitor.writer, " class=\"{class}\"")?;
    let mut styles = Vec::new();
    if let Some(align) = video
        .attributes()
        .get("align")
        .and_then(AttributeValueRef::as_str)
    {
        styles.push(format!("text-align: {align}"));
    }
    if let Some(float) = video
        .attributes()
        .get("float")
        .and_then(AttributeValueRef::as_str)
    {
        styles.push(format!("float: {float}"));
    }
    if !styles.is_empty() {
        write!(
            visitor.writer,
            " style=\"{}\"",
            escape_attribute(&styles.join("; "))
        )?;
    }
    writeln!(visitor.writer, ">")?;

    if video.source().to_string().is_empty() {
        writeln!(visitor.writer, "</{tag}>")?;
        return Ok(());
    }

    let is_youtube = video
        .attributes()
        .get("poster")
        .and_then(AttributeValueRef::as_str)
        == Some("youtube");
    let is_vimeo = video
        .attributes()
        .get("poster")
        .and_then(AttributeValueRef::as_str)
        == Some("vimeo");

    if is_youtube || is_vimeo {
        render_iframe_video(video, traversal, &mut visitor.writer)?;
    } else {
        render_local_video(video, traversal, &mut visitor.writer)?;
    }

    if has_title {
        write!(visitor.writer, "<figcaption>")?;
        visitor.visit_title(traversal, video.title())?;
        writeln!(visitor.writer, "</figcaption>")?;
    }

    writeln!(visitor.writer, "</{tag}>")?;
    Ok(())
}
