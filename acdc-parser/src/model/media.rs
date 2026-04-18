//! Media types for `AsciiDoc` documents (images, audio, video).

use std::fmt::Display;

use serde::{
    Serialize,
    ser::{SerializeMap, Serializer},
};

use crate::{Error, Positioning, SourceLocation};

use super::location::{Location, Position};
use super::metadata::BlockMetadata;
use super::title::Title;

/// The source location for media content (images, audio, video).
///
/// `Source` is an **enum**, not a struct with a `path` field. Use pattern matching
/// to extract the underlying value:
///
/// # Accessing the Source
///
/// ```
/// # use acdc_parser::Source;
/// # use std::path::PathBuf;
/// fn get_path_string(source: &Source) -> String {
///     match source {
///         Source::Path(path) => path.display().to_string(),
///         Source::Url(url) => url.to_string(),
///         Source::Name(name) => (*name).to_string(),
///     }
/// }
/// ```
///
/// Or use the `Display` implementation for simple string conversion:
///
/// ```
/// # use acdc_parser::Source;
/// # let source = Source::Name("example");
/// let source_str = source.to_string();
/// ```
///
/// # Variants
///
/// - `Path(PathBuf)` - Local filesystem path (e.g., `images/photo.png`)
/// - `Url(url::Url)` - Remote URL (e.g., `https://example.com/image.png`)
/// - `Name(&str)` - Simple identifier (e.g., icon names like `heart`, `github`)
#[derive(Clone, Debug, PartialEq)]
pub enum Source<'a> {
    /// A filesystem path (relative or absolute).
    Path(std::path::PathBuf),
    /// A URL (http, https, ftp, etc.).
    Url(SourceUrl<'a>),
    /// A simple name (used for icon names, menu targets, etc.).
    Name(&'a str),
}

/// A parsed URL that preserves the author's original input for display.
///
/// The `url` crate normalizes URLs (e.g., `http://example.com` becomes
/// `http://example.com/`). This wrapper stores the original string so URLs
/// are displayed exactly as written.
///
/// See [issue #335](https://github.com/nlopes/acdc/issues/335).
#[derive(Clone, Debug)]
pub struct SourceUrl<'a> {
    url: url::Url,
    original: &'a str,
}

impl<'a> SourceUrl<'a> {
    /// Create a new `SourceUrl` from a string, preserving the original for display.
    ///
    /// # Errors
    ///
    /// Returns an error if the input is not a valid URL.
    pub fn new(original: &'a str) -> Result<Self, url::ParseError> {
        let url = url::Url::parse(original)?;
        Ok(Self { url, original })
    }

    /// Get the underlying `url::Url`.
    #[must_use]
    pub fn url(&self) -> &url::Url {
        &self.url
    }
}

impl std::ops::Deref for SourceUrl<'_> {
    type Target = url::Url;
    fn deref(&self) -> &Self::Target {
        &self.url
    }
}

impl PartialEq for SourceUrl<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.url == other.url
    }
}

impl Display for SourceUrl<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.original)
    }
}

impl Source<'_> {
    /// Get the filename from the source.
    ///
    /// For paths, this returns the file name component. For URLs, it returns the last path
    /// segment. For names, it returns the name itself.
    #[must_use]
    pub fn get_filename(&self) -> Option<&str> {
        match self {
            Source::Path(path) => path.file_name().and_then(|os_str| os_str.to_str()),
            Source::Url(url) => url
                .path_segments()
                .and_then(std::iter::Iterator::last)
                .filter(|s| !s.is_empty()),
            Source::Name(name) => Some(name),
        }
    }
}

impl<'a> From<SourceUrl<'a>> for Source<'a> {
    fn from(url: SourceUrl<'a>) -> Self {
        Source::Url(url)
    }
}

impl<'a> Source<'a> {
    /// Construct a `Source` from a borrowed string, classifying it as either a
    /// URL or a filesystem path based on its scheme.
    ///
    /// # Errors
    /// Returns [`Error::Parse`] when `value` looks like a URL (starts with a
    /// recognised scheme) but fails URL parsing.
    pub fn from_str_borrowed(value: &'a str) -> Result<Self, Error> {
        // Try to parse as URL first
        if value.starts_with("http://")
            || value.starts_with("https://")
            || value.starts_with("ftp://")
            || value.starts_with("irc://")
            || value.starts_with("mailto:")
        {
            SourceUrl::new(value).map(Source::Url).map_err(|e| {
                Error::Parse(
                    Box::new(SourceLocation {
                        file: None,
                        positioning: Positioning::Position(Position::default()),
                    }),
                    format!("invalid URL: {e}"),
                )
            })
        } else if value.contains('/') || value.contains('\\') || value.contains('.') {
            // Contains path separators or dot (filename with extension)
            Ok(Source::Path(std::path::PathBuf::from(value)))
        } else {
            // Contains special characters or spaces - treat as a name
            Ok(Source::Name(value))
        }
    }
}

impl Display for Source<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Source::Path(path) => write!(f, "{}", path.display()),
            Source::Url(url) => write!(f, "{url}"),
            Source::Name(name) => write!(f, "{name}"),
        }
    }
}

impl Serialize for Source<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(None)?;
        match self {
            Source::Path(path) => {
                state.serialize_entry("type", "path")?;
                state.serialize_entry("value", &path.display().to_string())?;
            }
            Source::Url(url) => {
                state.serialize_entry("type", "url")?;
                state.serialize_entry("value", &url.to_string())?;
            }
            Source::Name(name) => {
                state.serialize_entry("type", "name")?;
                state.serialize_entry("value", name)?;
            }
        }
        state.end()
    }
}

/// An `Audio` represents an audio block in a document.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Audio<'a> {
    pub title: Title<'a>,
    pub source: Source<'a>,
    pub metadata: BlockMetadata<'a>,
    pub location: Location,
}

impl<'a> Audio<'a> {
    /// Create a new audio with the given source and location.
    #[must_use]
    pub fn new(source: Source<'a>, location: Location) -> Self {
        Self {
            title: Title::default(),
            source,
            metadata: BlockMetadata::default(),
            location,
        }
    }

    /// Set the title.
    #[must_use]
    pub fn with_title(mut self, title: Title<'a>) -> Self {
        self.title = title;
        self
    }

    /// Set the metadata.
    #[must_use]
    pub fn with_metadata(mut self, metadata: BlockMetadata<'a>) -> Self {
        self.metadata = metadata;
        self
    }
}

/// A `Video` represents a video block in a document.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Video<'a> {
    pub title: Title<'a>,
    pub sources: Vec<Source<'a>>,
    pub metadata: BlockMetadata<'a>,
    pub location: Location,
}

impl<'a> Video<'a> {
    /// Create a new video with the given sources and location.
    #[must_use]
    pub fn new(sources: Vec<Source<'a>>, location: Location) -> Self {
        Self {
            title: Title::default(),
            sources,
            metadata: BlockMetadata::default(),
            location,
        }
    }

    /// Set the title.
    #[must_use]
    pub fn with_title(mut self, title: Title<'a>) -> Self {
        self.title = title;
        self
    }

    /// Set the metadata.
    #[must_use]
    pub fn with_metadata(mut self, metadata: BlockMetadata<'a>) -> Self {
        self.metadata = metadata;
        self
    }
}

/// An `Image` represents an image block in a document.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Image<'a> {
    pub title: Title<'a>,
    pub source: Source<'a>,
    pub metadata: BlockMetadata<'a>,
    pub location: Location,
}

impl<'a> Image<'a> {
    /// Create a new image with the given source and location.
    #[must_use]
    pub fn new(source: Source<'a>, location: Location) -> Self {
        Self {
            title: Title::default(),
            source,
            metadata: BlockMetadata::default(),
            location,
        }
    }

    /// Set the title.
    #[must_use]
    pub fn with_title(mut self, title: Title<'a>) -> Self {
        self.title = title;
        self
    }

    /// Set the metadata.
    #[must_use]
    pub fn with_metadata(mut self, metadata: BlockMetadata<'a>) -> Self {
        self.metadata = metadata;
        self
    }
}

impl Serialize for Audio<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(None)?;
        state.serialize_entry("name", "audio")?;
        state.serialize_entry("type", "block")?;
        state.serialize_entry("form", "macro")?;
        if !self.metadata.is_default() {
            state.serialize_entry("metadata", &self.metadata)?;
        }
        if !self.title.is_empty() {
            state.serialize_entry("title", &self.title)?;
        }
        state.serialize_entry("source", &self.source)?;
        state.serialize_entry("location", &self.location)?;
        state.end()
    }
}

impl Serialize for Image<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(None)?;
        state.serialize_entry("name", "image")?;
        state.serialize_entry("type", "block")?;
        state.serialize_entry("form", "macro")?;
        if !self.metadata.is_default() {
            state.serialize_entry("metadata", &self.metadata)?;
        }
        if !self.title.is_empty() {
            state.serialize_entry("title", &self.title)?;
        }
        state.serialize_entry("source", &self.source)?;
        state.serialize_entry("location", &self.location)?;
        state.end()
    }
}

impl Serialize for Video<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(None)?;
        state.serialize_entry("name", "video")?;
        state.serialize_entry("type", "block")?;
        state.serialize_entry("form", "macro")?;
        if !self.metadata.is_default() {
            state.serialize_entry("metadata", &self.metadata)?;
        }
        if !self.title.is_empty() {
            state.serialize_entry("title", &self.title)?;
        }
        if !self.sources.is_empty() {
            state.serialize_entry("sources", &self.sources)?;
        }
        state.serialize_entry("location", &self.location)?;
        state.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_display_preserves_trailing_slash() -> Result<(), Error> {
        // Issue #335: URLs with trailing slashes should preserve them
        let source = Source::from_str_borrowed("http://example.com/")?;
        assert_eq!(source.to_string(), "http://example.com/");
        Ok(())
    }

    #[test]
    fn source_display_no_trailing_slash_when_absent() -> Result<(), Error> {
        // Domain-only URL without trailing slash should not gain one
        let source = Source::from_str_borrowed("http://example.com")?;
        assert_eq!(source.to_string(), "http://example.com");
        Ok(())
    }

    #[test]
    fn source_display_preserves_path_trailing_slash() -> Result<(), Error> {
        let source = Source::from_str_borrowed("http://example.com/foo/")?;
        assert_eq!(source.to_string(), "http://example.com/foo/");
        Ok(())
    }

    #[test]
    fn source_display_preserves_path_without_trailing_slash() -> Result<(), Error> {
        let source = Source::from_str_borrowed("http://example.com/foo")?;
        assert_eq!(source.to_string(), "http://example.com/foo");
        Ok(())
    }

    #[test]
    fn source_display_preserves_query_without_path() -> Result<(), Error> {
        // Original URL preserved exactly, even without explicit path before query
        let source = Source::from_str_borrowed("https://example.com?a=1&b=2")?;
        assert_eq!(source.to_string(), "https://example.com?a=1&b=2");
        Ok(())
    }

    #[test]
    fn source_display_preserves_trailing_slash_with_query() -> Result<(), Error> {
        let source = Source::from_str_borrowed("https://example.com/?a=1&b=2")?;
        assert_eq!(source.to_string(), "https://example.com/?a=1&b=2");
        Ok(())
    }
}
