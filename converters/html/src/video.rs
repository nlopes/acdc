use std::{fmt::Write as _, io::Write};

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

        if is_youtube {
            render_youtube_video(self, w)?;
        } else if is_vimeo {
            render_vimeo_video(self, w)?;
        } else {
            render_local_video(self, w)?;
        }

        writeln!(w, "</div>")?;
        writeln!(w, "</div>")?;

        Ok(())
    }
}

fn render_youtube_video<W: Write>(video: &Video, w: &mut W) -> Result<(), crate::Error> {
    let video_id = &video.sources[0].to_string();
    let mut url = format!("https://www.youtube.com/embed/{video_id}?rel=0");

    if let Some(acdc_parser::AttributeValue::String(start)) = video.metadata.attributes.get("start")
    {
        write!(url, "&start={start}")?;
    }

    if let Some(acdc_parser::AttributeValue::String(end)) = video.metadata.attributes.get("end") {
        write!(url, "&end={end}")?;
    }

    if let Some(acdc_parser::AttributeValue::String(theme)) = video.metadata.attributes.get("theme")
    {
        write!(url, "&theme={theme}")?;
    }

    if video.metadata.options.iter().any(|o| o == "autoplay") {
        write!(url, "&autoplay=1")?;
    }

    if video.metadata.options.iter().any(|o| o == "loop") {
        write!(url, "&loop=1&playlist={video_id}")?;
    }

    if video.metadata.options.iter().any(|o| o == "muted") {
        write!(url, "&mute=1")?;
    }

    // Add controls parameter if nocontrols is present
    if video.metadata.options.iter().any(|o| o == "nocontrols") {
        write!(url, "&controls=0")?;
    }

    // Add modest branding if modest option is present
    if video.metadata.options.iter().any(|o| o == "modest") {
        write!(url, "&modestbranding=1")?;
    }

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

fn render_vimeo_video<W: Write>(video: &Video, w: &mut W) -> Result<(), std::io::Error> {
    let video_id = &video.sources[0].to_string();

    // Build Vimeo embed URL with parameters
    let mut url = format!("https://player.vimeo.com/video/{video_id}");
    let mut first_param = true;

    if video.metadata.options.iter().any(|o| o == "autoplay") {
        url.push_str(if first_param { "?" } else { "&" });
        url.push_str("autoplay=1");
        first_param = false;
    }

    if video.metadata.options.iter().any(|o| o == "loop") {
        url.push_str(if first_param { "?" } else { "&" });
        url.push_str("loop=1");
        first_param = false;
    }

    if video.metadata.options.iter().any(|o| o == "muted") {
        url.push_str(if first_param { "?" } else { "&" });
        url.push_str("muted=1");
    }

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

    writeln!(
        w,
        " src=\"{url}\" frameborder=\"0\" allowfullscreen></iframe>"
    )?;

    Ok(())
}

fn render_local_video<W: Write>(video: &Video, w: &mut W) -> Result<(), crate::Error> {
    // Build the src attribute with optional start and end time
    let mut src = video.sources[0].to_string();
    let start = video.metadata.attributes.get("start");
    let end = video.metadata.attributes.get("end");

    match (start, end) {
        (
            Some(acdc_parser::AttributeValue::String(s)),
            Some(acdc_parser::AttributeValue::String(e)),
        ) => {
            write!(src, "#t={s},{e}")?;
        }
        (Some(acdc_parser::AttributeValue::String(s)), None) => {
            write!(src, "#t={s}")?;
        }
        _ => {}
    }

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
