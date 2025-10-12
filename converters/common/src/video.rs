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

#[cfg(test)]
mod tests {
    use super::*;
    use acdc_parser::{AttributeValue, BlockMetadata, ElementAttributes, Location, Source};

    fn create_video(
        sources: Vec<&str>,
        attributes: ElementAttributes,
        options: Vec<String>,
    ) -> Video {
        Video {
            sources: sources
                .into_iter()
                .map(|s| Source::Path(s.to_string()))
                .collect(),
            metadata: BlockMetadata {
                attributes,
                options,
                ..Default::default()
            },
            title: vec![],
            location: Location::default(),
        }
    }

    #[test]
    fn test_youtube_watch_url_basic() -> Result<(), std::fmt::Error> {
        let mut attrs = ElementAttributes::default();
        attrs.insert("youtube".to_string(), AttributeValue::Bool(true));
        let video = create_video(vec!["rPQoq7ThGAU"], attrs, vec![]);

        let url = video.try_url(false)?;
        assert_eq!(url, "https://www.youtube.com/watch?v=rPQoq7ThGAU");
        Ok(())
    }

    #[test]
    fn test_youtube_watch_url_with_start() -> Result<(), std::fmt::Error> {
        let mut attrs = ElementAttributes::default();
        attrs.insert("youtube".to_string(), AttributeValue::Bool(true));
        attrs.insert(
            "start".to_string(),
            AttributeValue::String("60".to_string()),
        );
        let video = create_video(vec!["rPQoq7ThGAU"], attrs, vec![]);

        let url = video.try_url(false)?;
        assert_eq!(url, "https://www.youtube.com/watch?v=rPQoq7ThGAU&t=60");
        Ok(())
    }

    #[test]
    fn test_youtube_watch_url_with_start_and_end() -> Result<(), std::fmt::Error> {
        let mut attrs = ElementAttributes::default();
        attrs.insert("youtube".to_string(), AttributeValue::Bool(true));
        attrs.insert(
            "start".to_string(),
            AttributeValue::String("60".to_string()),
        );
        attrs.insert("end".to_string(), AttributeValue::String("120".to_string()));
        let video = create_video(vec!["rPQoq7ThGAU"], attrs, vec![]);

        let url = video.try_url(false)?;
        assert_eq!(
            url,
            "https://www.youtube.com/watch?v=rPQoq7ThGAU&t=60&end=120"
        );
        Ok(())
    }

    #[test]
    fn test_youtube_embed_url_basic() -> Result<(), std::fmt::Error> {
        let mut attrs = ElementAttributes::default();
        attrs.insert("youtube".to_string(), AttributeValue::Bool(true));
        let video = create_video(vec!["rPQoq7ThGAU"], attrs, vec![]);

        let url = video.try_url(true)?;
        assert_eq!(url, "https://www.youtube.com/embed/rPQoq7ThGAU?rel=0");
        Ok(())
    }

    #[test]
    fn test_youtube_embed_url_with_all_params() -> Result<(), std::fmt::Error> {
        let mut attrs = ElementAttributes::default();
        attrs.insert("youtube".to_string(), AttributeValue::Bool(true));
        attrs.insert(
            "start".to_string(),
            AttributeValue::String("60".to_string()),
        );
        attrs.insert("end".to_string(), AttributeValue::String("120".to_string()));
        attrs.insert(
            "theme".to_string(),
            AttributeValue::String("light".to_string()),
        );
        let video = create_video(
            vec!["rPQoq7ThGAU"],
            attrs,
            vec![
                "autoplay".to_string(),
                "loop".to_string(),
                "muted".to_string(),
                "nocontrols".to_string(),
                "modest".to_string(),
            ],
        );

        let url = video.try_url(true)?;
        assert_eq!(
            url,
            "https://www.youtube.com/embed/rPQoq7ThGAU?rel=0&start=60&end=120&theme=light&autoplay=1&loop=1&playlist=rPQoq7ThGAU&mute=1&controls=0&modestbranding=1"
        );
        Ok(())
    }

    #[test]
    fn test_youtube_embed_url_with_autoplay_only() -> Result<(), std::fmt::Error> {
        let mut attrs = ElementAttributes::default();
        attrs.insert("youtube".to_string(), AttributeValue::Bool(true));
        let video = create_video(vec!["rPQoq7ThGAU"], attrs, vec!["autoplay".to_string()]);

        let url = video.try_url(true)?;
        assert_eq!(
            url,
            "https://www.youtube.com/embed/rPQoq7ThGAU?rel=0&autoplay=1"
        );
        Ok(())
    }

    #[test]
    fn test_youtube_embed_url_with_loop_only() -> Result<(), std::fmt::Error> {
        let mut attrs = ElementAttributes::default();
        attrs.insert("youtube".to_string(), AttributeValue::Bool(true));
        let video = create_video(vec!["rPQoq7ThGAU"], attrs, vec!["loop".to_string()]);

        let url = video.try_url(true)?;
        assert_eq!(
            url,
            "https://www.youtube.com/embed/rPQoq7ThGAU?rel=0&loop=1&playlist=rPQoq7ThGAU"
        );
        Ok(())
    }

    #[test]
    fn test_vimeo_watch_url_basic() -> Result<(), std::fmt::Error> {
        let mut attrs = ElementAttributes::default();
        attrs.insert("vimeo".to_string(), AttributeValue::Bool(true));
        let video = create_video(vec!["67480300"], attrs, vec![]);

        let url = video.try_url(false)?;
        assert_eq!(url, "https://vimeo.com/67480300");
        Ok(())
    }

    #[test]
    fn test_vimeo_watch_url_with_start() -> Result<(), std::fmt::Error> {
        let mut attrs = ElementAttributes::default();
        attrs.insert("vimeo".to_string(), AttributeValue::Bool(true));
        attrs.insert(
            "start".to_string(),
            AttributeValue::String("30".to_string()),
        );
        let video = create_video(vec!["67480300"], attrs, vec![]);

        let url = video.try_url(false)?;
        assert_eq!(url, "https://vimeo.com/67480300#t=30");
        Ok(())
    }

    #[test]
    fn test_vimeo_embed_url_basic() -> Result<(), std::fmt::Error> {
        let mut attrs = ElementAttributes::default();
        attrs.insert("vimeo".to_string(), AttributeValue::Bool(true));
        let video = create_video(vec!["67480300"], attrs, vec![]);

        let url = video.try_url(true)?;
        assert_eq!(url, "https://player.vimeo.com/video/67480300");
        Ok(())
    }

    #[test]
    fn test_vimeo_embed_url_with_all_options() -> Result<(), std::fmt::Error> {
        let mut attrs = ElementAttributes::default();
        attrs.insert("vimeo".to_string(), AttributeValue::Bool(true));
        let video = create_video(
            vec!["67480300"],
            attrs,
            vec![
                "autoplay".to_string(),
                "loop".to_string(),
                "muted".to_string(),
            ],
        );

        let url = video.try_url(true)?;
        assert_eq!(
            url,
            "https://player.vimeo.com/video/67480300?autoplay=1&loop=1&muted=1"
        );
        Ok(())
    }

    #[test]
    fn test_vimeo_embed_url_with_autoplay_only() -> Result<(), std::fmt::Error> {
        let mut attrs = ElementAttributes::default();
        attrs.insert("vimeo".to_string(), AttributeValue::Bool(true));
        let video = create_video(vec!["67480300"], attrs, vec!["autoplay".to_string()]);

        let url = video.try_url(true)?;
        assert_eq!(url, "https://player.vimeo.com/video/67480300?autoplay=1");
        Ok(())
    }

    #[test]
    fn test_vimeo_embed_url_with_loop_only() -> Result<(), std::fmt::Error> {
        let mut attrs = ElementAttributes::default();
        attrs.insert("vimeo".to_string(), AttributeValue::Bool(true));
        let video = create_video(vec!["67480300"], attrs, vec!["loop".to_string()]);

        let url = video.try_url(true)?;
        assert_eq!(url, "https://player.vimeo.com/video/67480300?loop=1");
        Ok(())
    }

    #[test]
    fn test_local_video_url_basic() -> Result<(), std::fmt::Error> {
        let video = create_video(vec!["demo.mp4"], ElementAttributes::default(), vec![]);

        let url = video.try_url(false)?;
        assert_eq!(url, "demo.mp4");
        Ok(())
    }

    #[test]
    fn test_local_video_url_with_start() -> Result<(), std::fmt::Error> {
        let mut attrs = ElementAttributes::default();
        attrs.insert(
            "start".to_string(),
            AttributeValue::String("10".to_string()),
        );
        let video = create_video(vec!["demo.mp4"], attrs, vec![]);

        let url = video.try_url(false)?;
        assert_eq!(url, "demo.mp4#t=10");
        Ok(())
    }

    #[test]
    fn test_local_video_url_with_start_and_end() -> Result<(), std::fmt::Error> {
        let mut attrs = ElementAttributes::default();
        attrs.insert(
            "start".to_string(),
            AttributeValue::String("10".to_string()),
        );
        attrs.insert("end".to_string(), AttributeValue::String("90".to_string()));
        let video = create_video(vec!["demo.mp4"], attrs, vec![]);

        let url = video.try_url(false)?;
        assert_eq!(url, "demo.mp4#t=10,90");
        Ok(())
    }

    #[test]
    fn test_local_video_url_embed_returns_same_as_watch() -> Result<(), std::fmt::Error> {
        let mut attrs = ElementAttributes::default();
        attrs.insert(
            "start".to_string(),
            AttributeValue::String("10".to_string()),
        );
        let video = create_video(vec!["demo.mp4"], attrs, vec![]);

        let watch_url = video.try_url(false)?;
        let embed_url = video.try_url(true)?;
        assert_eq!(watch_url, embed_url);
        assert_eq!(watch_url, "demo.mp4#t=10");
        Ok(())
    }

    #[test]
    fn test_both_youtube_and_vimeo_defaults_to_local() -> Result<(), std::fmt::Error> {
        let mut attrs = ElementAttributes::default();
        attrs.insert("youtube".to_string(), AttributeValue::Bool(true));
        attrs.insert("vimeo".to_string(), AttributeValue::Bool(true));
        let video = create_video(vec!["demo.mp4"], attrs, vec![]);

        let url = video.try_url(false)?;
        assert_eq!(url, "demo.mp4");
        Ok(())
    }

    #[test]
    #[should_panic(expected = "index out of bounds: the len is 0 but the index is 0")]
    fn test_empty_sources_returns_empty_string() {
        let mut attrs = ElementAttributes::default();
        attrs.insert("youtube".to_string(), AttributeValue::Bool(true));
        let video = Video {
            sources: vec![],
            metadata: BlockMetadata {
                attributes: attrs,
                ..Default::default()
            },
            title: vec![],
            location: Location::default(),
        };

        // This panics because we try to access sources[0] when sources is empty
        video.try_url(false).unwrap();
    }
}
