use std::fmt::Write as _;

use acdc_parser::Video;

/// Trait to generate a `URL` for a video, either for embedding or direct linking.
///
/// This trait is implemented for `acdc_parser::Video` and provides methods to generate
/// `URL`s for `YouTube` and `Vimeo` videos based on their metadata attributes and
/// options.
///
/// # Errors
///
/// Returns a `std::fmt::Error` if there is an error constructing the URL string.
pub trait TryUrl {
    type Error;

    /// Generate a `URL` for the video.
    ///
    /// If `embed` is `true`, generates an embed `URL` suitable for iframes (e.g.,
    /// `https://www.youtube.com/embed/{id}` or `https://player.vimeo.com/video/{id}`).
    ///
    /// If `embed` is `false`, generates a watch `URL` suitable for direct linking
    /// (e.g., `https://www.youtube.com/watch?v={id}` or `https://vimeo.com/{id}`).
    ///
    /// The generated `URL` will include any relevant parameters such as start time, end
    /// time, autoplay, loop, muted, and controls based on the video's metadata.
    ///
    /// # Errors
    ///
    /// Returns an error if we could not build the `URL` string.
    fn try_url(&self, embed: bool) -> Result<String, Self::Error>;
}

impl TryUrl for Video {
    type Error = std::fmt::Error;

    fn try_url(&self, embed: bool) -> Result<String, Self::Error> {
        let is_youtube = matches!(
            self.metadata.attributes.get("youtube"),
            Some(acdc_parser::AttributeValue::Bool(true))
        );
        let is_vimeo = matches!(
            self.metadata.attributes.get("vimeo"),
            Some(acdc_parser::AttributeValue::Bool(true))
        );

        match ((is_youtube, is_vimeo), embed) {
            ((true, false), true) => build_youtube_embed_url(self),
            ((true, false), false) => build_youtube_watch_url(self),
            ((false, true), true) => build_vimeo_embed_url(self),
            ((false, true), false) => build_vimeo_watch_url(self),
            _ => build_local_url(self), // Local video
        }
    }
}

/// Build a `YouTube` watch URL with parameters for a video.
///
/// Returns a URL like: `https://www.youtube.com/watch?v={id}&t={start}&end={end}`
///
/// This is suitable for terminal output and direct linking.
fn build_youtube_watch_url(video: &Video) -> Result<String, std::fmt::Error> {
    let video_id = &video.sources[0].to_string();
    let mut url = format!("https://www.youtube.com/watch?v={video_id}");

    // Add start parameter if present (using &t= for watch URLs)
    if let Some(acdc_parser::AttributeValue::String(start)) = video.metadata.attributes.get("start")
    {
        write!(url, "&t={start}")?;
    }

    // Add end parameter if present
    if let Some(acdc_parser::AttributeValue::String(end)) = video.metadata.attributes.get("end") {
        write!(url, "&end={end}")?;
    }

    Ok(url)
}

/// Build a `YouTube` embed URL with all parameters for iframe embedding.
///
/// Returns a URL like: `https://www.youtube.com/embed/{id}?rel=0&start={start}&end={end}`
///
/// This is suitable for HTML iframe embedding.
fn build_youtube_embed_url(video: &Video) -> Result<String, std::fmt::Error> {
    let video_id = &video.sources[0].to_string();
    let mut url = format!("https://www.youtube.com/embed/{video_id}?rel=0");

    // Add start parameter if present (using &start= for embed URLs)
    if let Some(acdc_parser::AttributeValue::String(start)) = video.metadata.attributes.get("start")
    {
        write!(url, "&start={start}")?;
    }

    // Add end parameter if present
    if let Some(acdc_parser::AttributeValue::String(end)) = video.metadata.attributes.get("end") {
        write!(url, "&end={end}")?;
    }

    // Add theme parameter if present
    if let Some(acdc_parser::AttributeValue::String(theme)) = video.metadata.attributes.get("theme")
    {
        write!(url, "&theme={theme}")?;
    }

    // Add autoplay parameter if present
    if video.metadata.options.iter().any(|o| o == "autoplay") {
        write!(url, "&autoplay=1")?;
    }

    // Add loop parameter if present (YouTube requires playlist for looping)
    if video.metadata.options.iter().any(|o| o == "loop") {
        write!(url, "&loop=1&playlist={video_id}")?;
    }

    // Add muted parameter if present
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

    Ok(url)
}

/// Build a `Vimeo` watch URL with parameters for a video.
///
/// Returns a URL like: `https://vimeo.com/{id}#t={start}`
///
/// This is suitable for both terminal output and direct linking.
fn build_vimeo_watch_url(video: &Video) -> Result<String, std::fmt::Error> {
    let video_id = &video.sources[0].to_string();
    let mut url = format!("https://vimeo.com/{video_id}");

    // Add start parameter if present
    if let Some(acdc_parser::AttributeValue::String(start)) = video.metadata.attributes.get("start")
    {
        write!(url, "#t={start}")?;
    }

    Ok(url)
}

/// Build a `Vimeo` embed URL with all parameters for iframe embedding.
///
/// Returns a URL like: `https://player.vimeo.com/video/{id}?autoplay=1&loop=1&muted=1`
///
/// This is suitable for HTML iframe embedding.
fn build_vimeo_embed_url(video: &Video) -> Result<String, std::fmt::Error> {
    let video_id = &video.sources[0].to_string();
    let mut url = format!("https://player.vimeo.com/video/{video_id}");
    let mut first_param = true;

    // Add autoplay parameter if present
    if video.metadata.options.iter().any(|o| o == "autoplay") {
        write!(url, "{}autoplay=1", if first_param { "?" } else { "&" })?;
        first_param = false;
    }

    // Add loop parameter if present
    if video.metadata.options.iter().any(|o| o == "loop") {
        write!(url, "{}loop=1", if first_param { "?" } else { "&" })?;
        first_param = false;
    }

    // Add muted parameter if present
    if video.metadata.options.iter().any(|o| o == "muted") {
        write!(url, "{}muted=1", if first_param { "?" } else { "&" })?;
    }

    Ok(url)
}

fn build_local_url(video: &Video) -> Result<String, std::fmt::Error> {
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

    Ok(src)
}
