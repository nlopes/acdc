//! Video URL generation for `YouTube`, `Vimeo`, and local videos.
//!
//! This module provides URL generation for video embedding and linking,
//! supporting platform-specific parameters like start time, autoplay, and controls.
//!
//! # Supported Platforms
//!
//! - **`YouTube`** - Embed and watch URLs with start/end time, autoplay, loop, etc.
//! - **`Vimeo`** - Embed and watch URLs with similar options
//! - **Local** - Direct file paths with optional fragment parameters

use std::fmt::Write as _;

use acdc_parser::Video;

/// Errors that can occur while generating a video URL.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum UrlError {
    /// The video has no source to build a URL from.
    #[error("video has no source")]
    MissingSource,

    /// Formatting failed while building the URL string.
    #[error(transparent)]
    Format(#[from] std::fmt::Error),
}

/// Trait to generate a `URL` for a video, either for embedding or direct linking.
///
/// This trait is implemented for `acdc_parser::Video` and provides methods to generate
/// `URL`s for `YouTube` and `Vimeo` videos based on their metadata attributes and
/// options.
///
/// # Errors
///
/// Returns an error if the video has no source or URL construction fails.
pub trait TryUrl {
    /// The error type returned if URL generation fails.
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

impl TryUrl for Video<'_> {
    type Error = UrlError;

    fn try_url(&self, embed: bool) -> Result<String, Self::Error> {
        if self.source.to_string().is_empty() {
            return Err(UrlError::MissingSource);
        }
        let is_youtube =
            self.metadata.attributes.get_string("poster").as_deref() == Some("youtube");
        let is_vimeo = self.metadata.attributes.get_string("poster").as_deref() == Some("vimeo");

        match ((is_youtube, is_vimeo), embed) {
            ((true, false), true) => build_youtube_embed_url(self),
            ((true, false), false) => build_youtube_watch_url(self),
            ((false, true), true) => build_vimeo_embed_url(self),
            ((false, true), false) => build_vimeo_watch_url(self),
            _ => Ok(build_local_url(self)), // Local video
        }
    }
}

struct YoutubeTarget {
    id: String,
    playlist: Option<(&'static str, String)>,
}

/// Resolve `YouTube`'s list and dynamic playlist shorthand before building a URL.
fn youtube_target(video: &Video<'_>) -> Result<YoutubeTarget, UrlError> {
    let source = video.source.to_string();
    if source.is_empty() {
        return Err(UrlError::MissingSource);
    }
    let (target, list) = source
        .split_once('/')
        .map_or((source.as_str(), None), |(target, list)| {
            (target, Some(list))
        });
    let named_list = video.metadata.attributes.get_string("list");
    if let Some(list) = list.or(named_list.as_deref()) {
        return Ok(YoutubeTarget {
            id: target.to_owned(),
            playlist: Some(("list", list.to_owned())),
        });
    }
    let (target, playlist) = target
        .split_once(',')
        .map_or((target, None), |(target, playlist)| {
            (target, Some(playlist))
        });
    let named_playlist = video.metadata.attributes.get_string("playlist");
    let playlist = playlist
        .or(named_playlist.as_deref())
        .map(|playlist| ("playlist", format!("{target},{playlist}")));
    Ok(YoutubeTarget {
        id: target.to_owned(),
        playlist,
    })
}

/// Build a `YouTube` watch URL with parameters for a video.
///
/// Returns a URL like: `https://www.youtube.com/watch?v={id}&t={start}&end={end}`
///
/// This is suitable for terminal output and direct linking.
///
/// # Errors
///
/// Returns an error if the video has no source or if URL formatting fails.
fn build_youtube_watch_url(video: &Video) -> Result<String, UrlError> {
    let YoutubeTarget {
        id: video_id,
        playlist,
    } = youtube_target(video)?;
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

    if let Some((name, value)) = playlist {
        write!(url, "&{name}={value}")?;
    }

    Ok(url)
}

/// Build a `YouTube` embed URL with all parameters for iframe embedding.
///
/// Returns a URL like: `https://www.youtube.com/embed/{id}?rel=0&start={start}&end={end}`
///
/// This is suitable for HTML iframe embedding.
///
/// # Errors
///
/// Returns an error if the video has no source or if URL formatting fails.
fn build_youtube_embed_url(video: &Video) -> Result<String, UrlError> {
    let YoutubeTarget {
        id: video_id,
        playlist,
    } = youtube_target(video)?;
    let related = u8::from(video.metadata.options.contains(&"related"));
    let mut url = format!("https://www.youtube.com/embed/{video_id}?rel={related}");

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
    if video.metadata.options.contains(&"autoplay") {
        write!(url, "&autoplay=1")?;
    }

    // Add loop parameter if present (YouTube requires playlist for looping)
    if video.metadata.options.contains(&"loop") {
        write!(url, "&loop=1")?;
        if playlist.is_none() {
            write!(url, "&playlist={video_id}")?;
        }
    }

    // Add muted parameter if present
    if video.metadata.options.contains(&"muted") {
        write!(url, "&mute=1")?;
    }

    // Add controls parameter if nocontrols is present
    if video.metadata.options.contains(&"nocontrols") {
        write!(url, "&controls=0")?;
    }

    if let Some((name, value)) = playlist {
        write!(url, "&{name}={value}")?;
    }
    if video.metadata.options.contains(&"nofullscreen") {
        write!(url, "&fs=0")?;
    }
    if let Some(lang) = video.metadata.attributes.get_string("lang") {
        write!(url, "&hl={lang}")?;
    }

    // Add modest branding if modest option is present
    if video.metadata.options.contains(&"modest") {
        write!(url, "&modestbranding=1")?;
    }

    Ok(url)
}

/// Build a `Vimeo` watch URL with parameters for a video.
///
/// Returns a URL like: `https://vimeo.com/{id}#t={start}`
///
/// This is suitable for both terminal output and direct linking.
///
/// # Errors
///
/// Returns an error if the video has no source or if URL formatting fails.
fn build_vimeo_watch_url(video: &Video) -> Result<String, UrlError> {
    let video_id = video.source.to_string();
    let mut url = format!("https://vimeo.com/{video_id}");
    if !video_id.contains('/')
        && let Some(hash) = video.metadata.attributes.get_string("hash")
    {
        write!(url, "/{hash}")?;
    }

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
///
/// # Errors
///
/// Returns an error if the video has no source or if URL formatting fails.
fn build_vimeo_embed_url(video: &Video) -> Result<String, UrlError> {
    let video_id = video.source.to_string();
    let (video_id, hash) = video_id
        .split_once('/')
        .map_or((video_id.as_str(), None), |(id, hash)| (id, Some(hash)));
    let named_hash = video.metadata.attributes.get_string("hash");
    let mut url = format!("https://player.vimeo.com/video/{video_id}");
    let mut first_param = true;
    if let Some(hash) = hash.or(named_hash.as_deref()) {
        write!(url, "?h={hash}")?;
        first_param = false;
    }

    // Add autoplay parameter if present
    if video.metadata.options.contains(&"autoplay") {
        write!(url, "{}autoplay=1", if first_param { "?" } else { "&" })?;
        first_param = false;
    }

    // Add loop parameter if present
    if video.metadata.options.contains(&"loop") {
        write!(url, "{}loop=1", if first_param { "?" } else { "&" })?;
        first_param = false;
    }

    // Add muted parameter if present
    if video.metadata.options.contains(&"muted") {
        write!(url, "{}muted=1", if first_param { "?" } else { "&" })?;
    }

    if let Some(start) = video.metadata.attributes.get_string("start") {
        write!(url, "#at={start}")?;
    }

    Ok(url)
}

/// Build a local video URL with optional time parameters.
///
/// Returns a URL like: `video.mp4#t={start},{end}`
fn build_local_url(video: &Video) -> String {
    let source = video.source.to_string();
    crate::media::with_time_range(&source, &video.metadata.attributes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use acdc_parser::{AttributeValue, BlockMetadata, ElementAttributes, Location, Source};

    fn create_video<'a>(
        source: &'a str,
        attributes: ElementAttributes<'a>,
        options: Vec<&'a str>,
    ) -> Video<'a> {
        Video::new(Source::Name(source), Location::default()).with_metadata(
            BlockMetadata::new()
                .with_attributes(attributes)
                .with_options(options),
        )
    }

    #[test]
    fn test_youtube_watch_url_basic() -> Result<(), UrlError> {
        let mut attrs = ElementAttributes::default();
        attrs.insert("poster".into(), "youtube".into());
        let video = create_video("rPQoq7ThGAU", attrs, vec![]);

        let url = video.try_url(false)?;
        assert_eq!(url, "https://www.youtube.com/watch?v=rPQoq7ThGAU");
        Ok(())
    }

    #[test]
    fn test_youtube_watch_url_with_start() -> Result<(), UrlError> {
        let mut attrs = ElementAttributes::default();
        attrs.insert("poster".into(), "youtube".into());
        attrs.insert("start".into(), AttributeValue::String("60".into()));
        let video = create_video("rPQoq7ThGAU", attrs, vec![]);

        let url = video.try_url(false)?;
        assert_eq!(url, "https://www.youtube.com/watch?v=rPQoq7ThGAU&t=60");
        Ok(())
    }

    #[test]
    fn test_youtube_watch_url_with_start_and_end() -> Result<(), UrlError> {
        let mut attrs = ElementAttributes::default();
        attrs.insert("poster".into(), "youtube".into());
        attrs.insert("start".into(), AttributeValue::String("60".into()));
        attrs.insert("end".into(), AttributeValue::String("120".into()));
        let video = create_video("rPQoq7ThGAU", attrs, vec![]);

        let url = video.try_url(false)?;
        assert_eq!(
            url,
            "https://www.youtube.com/watch?v=rPQoq7ThGAU&t=60&end=120"
        );
        Ok(())
    }

    #[test]
    fn test_youtube_embed_url_basic() -> Result<(), UrlError> {
        let mut attrs = ElementAttributes::default();
        attrs.insert("poster".into(), "youtube".into());
        let video = create_video("rPQoq7ThGAU", attrs, vec![]);

        let url = video.try_url(true)?;
        assert_eq!(url, "https://www.youtube.com/embed/rPQoq7ThGAU?rel=0");
        Ok(())
    }

    #[test]
    fn test_youtube_embed_url_with_all_params() -> Result<(), UrlError> {
        let mut attrs = ElementAttributes::default();
        attrs.insert("poster".into(), "youtube".into());
        attrs.insert("start".into(), AttributeValue::String("60".into()));
        attrs.insert("end".into(), AttributeValue::String("120".into()));
        attrs.insert("theme".into(), AttributeValue::String("light".into()));
        let video = create_video(
            "rPQoq7ThGAU",
            attrs,
            vec!["autoplay", "loop", "muted", "nocontrols", "modest"],
        );

        let url = video.try_url(true)?;
        assert_eq!(
            url,
            "https://www.youtube.com/embed/rPQoq7ThGAU?rel=0&start=60&end=120&theme=light&autoplay=1&loop=1&playlist=rPQoq7ThGAU&mute=1&controls=0&modestbranding=1"
        );
        Ok(())
    }

    #[test]
    fn test_youtube_embed_url_with_autoplay_only() -> Result<(), UrlError> {
        let mut attrs = ElementAttributes::default();
        attrs.insert("poster".into(), "youtube".into());
        let video = create_video("rPQoq7ThGAU", attrs, vec!["autoplay"]);

        let url = video.try_url(true)?;
        assert_eq!(
            url,
            "https://www.youtube.com/embed/rPQoq7ThGAU?rel=0&autoplay=1"
        );
        Ok(())
    }

    #[test]
    fn test_youtube_embed_url_with_loop_only() -> Result<(), UrlError> {
        let mut attrs = ElementAttributes::default();
        attrs.insert("poster".into(), "youtube".into());
        let video = create_video("rPQoq7ThGAU", attrs, vec!["loop"]);

        let url = video.try_url(true)?;
        assert_eq!(
            url,
            "https://www.youtube.com/embed/rPQoq7ThGAU?rel=0&loop=1&playlist=rPQoq7ThGAU"
        );
        Ok(())
    }

    #[test]
    fn test_vimeo_watch_url_basic() -> Result<(), UrlError> {
        let mut attrs = ElementAttributes::default();
        attrs.insert("poster".into(), "vimeo".into());
        let video = create_video("67480300", attrs, vec![]);

        let url = video.try_url(false)?;
        assert_eq!(url, "https://vimeo.com/67480300");
        Ok(())
    }

    #[test]
    fn test_vimeo_watch_url_with_start() -> Result<(), UrlError> {
        let mut attrs = ElementAttributes::default();
        attrs.insert("poster".into(), "vimeo".into());
        attrs.insert("start".into(), AttributeValue::String("30".into()));
        let video = create_video("67480300", attrs, vec![]);

        let url = video.try_url(false)?;
        assert_eq!(url, "https://vimeo.com/67480300#t=30");
        Ok(())
    }

    #[test]
    fn test_vimeo_embed_url_basic() -> Result<(), UrlError> {
        let mut attrs = ElementAttributes::default();
        attrs.insert("poster".into(), "vimeo".into());
        let video = create_video("67480300", attrs, vec![]);

        let url = video.try_url(true)?;
        assert_eq!(url, "https://player.vimeo.com/video/67480300");
        Ok(())
    }

    #[test]
    fn test_vimeo_embed_url_with_all_options() -> Result<(), UrlError> {
        let mut attrs = ElementAttributes::default();
        attrs.insert("poster".into(), "vimeo".into());
        let video = create_video("67480300", attrs, vec!["autoplay", "loop", "muted"]);

        let url = video.try_url(true)?;
        assert_eq!(
            url,
            "https://player.vimeo.com/video/67480300?autoplay=1&loop=1&muted=1"
        );
        Ok(())
    }

    #[test]
    fn test_vimeo_embed_url_with_autoplay_only() -> Result<(), UrlError> {
        let mut attrs = ElementAttributes::default();
        attrs.insert("poster".into(), "vimeo".into());
        let video = create_video("67480300", attrs, vec!["autoplay"]);

        let url = video.try_url(true)?;
        assert_eq!(url, "https://player.vimeo.com/video/67480300?autoplay=1");
        Ok(())
    }

    #[test]
    fn test_vimeo_embed_url_with_loop_only() -> Result<(), UrlError> {
        let mut attrs = ElementAttributes::default();
        attrs.insert("poster".into(), "vimeo".into());
        let video = create_video("67480300", attrs, vec!["loop"]);

        let url = video.try_url(true)?;
        assert_eq!(url, "https://player.vimeo.com/video/67480300?loop=1");
        Ok(())
    }

    #[test]
    fn test_local_video_url_basic() -> Result<(), UrlError> {
        let video = create_video("demo.mp4", ElementAttributes::default(), vec![]);

        let url = video.try_url(false)?;
        assert_eq!(url, "demo.mp4");
        Ok(())
    }

    #[test]
    fn test_local_video_url_with_start() -> Result<(), UrlError> {
        let mut attrs = ElementAttributes::default();
        attrs.insert("start".into(), AttributeValue::String("10".into()));
        let video = create_video("demo.mp4", attrs, vec![]);

        let url = video.try_url(false)?;
        assert_eq!(url, "demo.mp4#t=10");
        Ok(())
    }

    #[test]
    fn test_local_video_url_with_start_and_end() -> Result<(), UrlError> {
        let mut attrs = ElementAttributes::default();
        attrs.insert("start".into(), AttributeValue::String("10".into()));
        attrs.insert("end".into(), AttributeValue::String("90".into()));
        let video = create_video("demo.mp4", attrs, vec![]);

        let url = video.try_url(false)?;
        assert_eq!(url, "demo.mp4#t=10,90");
        Ok(())
    }

    #[test]
    fn test_local_video_url_embed_returns_same_as_watch() -> Result<(), UrlError> {
        let mut attrs = ElementAttributes::default();
        attrs.insert("start".into(), AttributeValue::String("10".into()));
        let video = create_video("demo.mp4", attrs, vec![]);

        let watch_url = video.try_url(false)?;
        let embed_url = video.try_url(true)?;
        assert_eq!(watch_url, embed_url);
        assert_eq!(watch_url, "demo.mp4#t=10");
        Ok(())
    }

    #[test]
    fn test_empty_sources_returns_error() {
        let mut attrs = ElementAttributes::default();
        attrs.insert("poster".into(), "youtube".into());
        let video = Video::new(Source::Name(""), Location::default())
            .with_metadata(BlockMetadata::new().with_attributes(attrs));

        // Should return an error instead of panicking
        let result = video.try_url(false);
        assert!(result.is_err(), "Expected error for empty video target");
    }
    #[test]
    fn private_vimeo_hash_syntaxes_produce_the_same_links() -> Result<(), UrlError> {
        let mut attributes = ElementAttributes::default();
        attributes.insert("poster".into(), "vimeo".into());
        attributes.insert("start".into(), "5".into());
        let shorthand = create_video("12345/secret", attributes.clone(), vec![]);
        attributes.insert("hash".into(), "secret".into());
        let named = create_video("12345", attributes, vec![]);
        for embed in [false, true] {
            assert_eq!(shorthand.try_url(embed)?, named.try_url(embed)?);
        }
        assert_eq!(named.try_url(false)?, "https://vimeo.com/12345/secret#t=5");
        Ok(())
    }
}
