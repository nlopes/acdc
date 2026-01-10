//! Media types for `AsciiDoc` documents (images, audio, video).

use std::fmt::Display;
use std::str::FromStr;

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
///         Source::Name(name) => name.clone(),
///     }
/// }
/// ```
///
/// Or use the `Display` implementation for simple string conversion:
///
/// ```
/// # use acdc_parser::Source;
/// # let source = Source::Name("example".to_string());
/// let source_str = source.to_string();
/// ```
///
/// # Variants
///
/// - `Path(PathBuf)` - Local filesystem path (e.g., `images/photo.png`)
/// - `Url(url::Url)` - Remote URL (e.g., `https://example.com/image.png`)
/// - `Name(String)` - Simple identifier (e.g., icon names like `heart`, `github`)
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    /// A filesystem path (relative or absolute).
    Path(std::path::PathBuf),
    /// A URL (http, https, ftp, etc.).
    Url(url::Url),
    /// A simple name (used for icon names, menu targets, etc.).
    Name(String),
}

impl Source {
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
            Source::Name(name) => Some(name.as_str()),
        }
    }
}

impl FromStr for Source {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        // Try to parse as URL first
        if value.starts_with("http://")
            || value.starts_with("https://")
            || value.starts_with("ftp://")
            || value.starts_with("irc://")
            || value.starts_with("mailto:")
        {
            url::Url::parse(value).map(Source::Url).map_err(|e| {
                Error::Parse(
                    Box::new(SourceLocation {
                        file: None,
                        positioning: Positioning::Position(Position::default()),
                    }),
                    format!("invalid URL: {e}"),
                )
            })
        } else if value.contains('/') || value.contains('\\') || value.contains('.') {
            // Contains path separators - treat as filesystem path or contains a dot which
            // might indicate a filename with extension
            Ok(Source::Path(std::path::PathBuf::from(value)))
        } else {
            // Contains special characters or spaces - treat as a name
            Ok(Source::Name(value.to_string()))
        }
    }
}

impl Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Source::Path(path) => write!(f, "{}", path.display()),
            Source::Url(url) => {
                // The url crate normalizes domain-only URLs by adding a trailing slash
                // (e.g., "https://example.com" -> "https://example.com/").
                // Strip it to match asciidoctor's output behavior.
                let url_str = url.as_str();
                if url.path() == "/" && !url_str.ends_with("://") {
                    write!(f, "{}", url_str.trim_end_matches('/'))
                } else {
                    write!(f, "{url}")
                }
            }
            Source::Name(name) => write!(f, "{name}"),
        }
    }
}

impl Serialize for Source {
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
                state.serialize_entry("value", url.as_str())?;
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
pub struct Audio {
    pub title: Title,
    pub source: Source,
    pub metadata: BlockMetadata,
    pub location: Location,
}

impl Audio {
    /// Create a new audio with the given source and location.
    #[must_use]
    pub fn new(source: Source, location: Location) -> Self {
        Self {
            title: Title::default(),
            source,
            metadata: BlockMetadata::default(),
            location,
        }
    }

    /// Set the title.
    #[must_use]
    pub fn with_title(mut self, title: Title) -> Self {
        self.title = title;
        self
    }

    /// Set the metadata.
    #[must_use]
    pub fn with_metadata(mut self, metadata: BlockMetadata) -> Self {
        self.metadata = metadata;
        self
    }
}

/// A `Video` represents a video block in a document.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Video {
    pub title: Title,
    pub sources: Vec<Source>,
    pub metadata: BlockMetadata,
    pub location: Location,
}

impl Video {
    /// Create a new video with the given sources and location.
    #[must_use]
    pub fn new(sources: Vec<Source>, location: Location) -> Self {
        Self {
            title: Title::default(),
            sources,
            metadata: BlockMetadata::default(),
            location,
        }
    }

    /// Set the title.
    #[must_use]
    pub fn with_title(mut self, title: Title) -> Self {
        self.title = title;
        self
    }

    /// Set the metadata.
    #[must_use]
    pub fn with_metadata(mut self, metadata: BlockMetadata) -> Self {
        self.metadata = metadata;
        self
    }
}

/// An `Image` represents an image block in a document.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Image {
    pub title: Title,
    pub source: Source,
    pub metadata: BlockMetadata,
    pub location: Location,
}

impl Image {
    /// Create a new image with the given source and location.
    #[must_use]
    pub fn new(source: Source, location: Location) -> Self {
        Self {
            title: Title::default(),
            source,
            metadata: BlockMetadata::default(),
            location,
        }
    }

    /// Set the title.
    #[must_use]
    pub fn with_title(mut self, title: Title) -> Self {
        self.title = title;
        self
    }

    /// Set the metadata.
    #[must_use]
    pub fn with_metadata(mut self, metadata: BlockMetadata) -> Self {
        self.metadata = metadata;
        self
    }
}

impl Serialize for Audio {
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

impl Serialize for Image {
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

impl Serialize for Video {
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
