//! Media types for `AsciiDoc` documents (images, audio, video).

use std::fmt::Display;
use std::str::FromStr;

use serde::{
    Deserialize, Serialize,
    de::{self, Deserializer, MapAccess, Visitor},
    ser::{SerializeMap, Serializer},
};

use crate::{Error, Positioning, SourceLocation};

use super::inlines::InlineNode;
use super::location::{Location, Position};
use super::metadata::BlockMetadata;

/// A `Source` represents the source of content (images, audio, video, etc.).
///
/// This type distinguishes between filesystem paths, URLs, and simple names (like icon names).
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    /// A filesystem path
    Path(std::path::PathBuf),
    /// A URL
    Url(url::Url),
    /// A simple name (used for example in menu macros or icon names)
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

impl<'de> Deserialize<'de> for Source {
    fn deserialize<D>(deserializer: D) -> Result<Source, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct SourceVisitor;

        impl<'de> Visitor<'de> for SourceVisitor {
            type Value = Source;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a Source object with type and value fields")
            }

            fn visit_map<V>(self, mut map: V) -> Result<Source, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut source_type: Option<String> = None;
                let mut value: Option<String> = None;

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "type" => {
                            if source_type.is_some() {
                                return Err(de::Error::duplicate_field("type"));
                            }
                            source_type = Some(map.next_value()?);
                        }
                        "value" => {
                            if value.is_some() {
                                return Err(de::Error::duplicate_field("value"));
                            }
                            value = Some(map.next_value()?);
                        }
                        _ => {
                            let _ = map.next_value::<de::IgnoredAny>()?;
                        }
                    }
                }

                let source_type = source_type.ok_or_else(|| de::Error::missing_field("type"))?;
                let value = value.ok_or_else(|| de::Error::missing_field("value"))?;

                match source_type.as_str() {
                    "path" => Ok(Source::Path(std::path::PathBuf::from(value))),
                    "url" => url::Url::parse(&value)
                        .map(Source::Url)
                        .map_err(|e| de::Error::custom(format!("invalid URL: {e}"))),
                    "name" => Ok(Source::Name(value)),
                    _ => Err(de::Error::custom(format!(
                        "unexpected source type: {source_type}"
                    ))),
                }
            }
        }

        deserializer.deserialize_map(SourceVisitor)
    }
}

/// An `Audio` represents an audio block in a document.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Audio {
    pub title: Vec<InlineNode>,
    pub source: Source,
    pub metadata: BlockMetadata,
    pub location: Location,
}

impl Audio {
    /// Create a new audio with the given source and location.
    #[must_use]
    pub fn new(source: Source, location: Location) -> Self {
        Self {
            title: Vec::new(),
            source,
            metadata: BlockMetadata::default(),
            location,
        }
    }

    /// Set the title.
    #[must_use]
    pub fn with_title(mut self, title: Vec<InlineNode>) -> Self {
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
    pub title: Vec<InlineNode>,
    pub sources: Vec<Source>,
    pub metadata: BlockMetadata,
    pub location: Location,
}

impl Video {
    /// Create a new video with the given sources and location.
    #[must_use]
    pub fn new(sources: Vec<Source>, location: Location) -> Self {
        Self {
            title: Vec::new(),
            sources,
            metadata: BlockMetadata::default(),
            location,
        }
    }

    /// Set the title.
    #[must_use]
    pub fn with_title(mut self, title: Vec<InlineNode>) -> Self {
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
    pub title: Vec<InlineNode>,
    pub source: Source,
    pub metadata: BlockMetadata,
    pub location: Location,
}

impl Image {
    /// Create a new image with the given source and location.
    #[must_use]
    pub fn new(source: Source, location: Location) -> Self {
        Self {
            title: Vec::new(),
            source,
            metadata: BlockMetadata::default(),
            location,
        }
    }

    /// Set the title.
    #[must_use]
    pub fn with_title(mut self, title: Vec<InlineNode>) -> Self {
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

impl<'de> Deserialize<'de> for Audio {
    fn deserialize<D>(deserializer: D) -> Result<Audio, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "lowercase")]
        enum Field {
            Metadata,
            Title,
            Source,
            Location,
        }

        struct AudioVisitor;

        impl<'de> Visitor<'de> for AudioVisitor {
            type Value = Audio;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("struct Audio")
            }

            fn visit_map<V>(self, mut map: V) -> Result<Audio, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut metadata = None;
                let mut title = None;
                let mut source = None;
                let mut location = None;

                while let Some(key) = map.next_key()? {
                    match key {
                        Field::Metadata => {
                            metadata = Some(map.next_value()?);
                        }
                        Field::Title => {
                            title = Some(map.next_value()?);
                        }
                        Field::Source => {
                            source = Some(map.next_value()?);
                        }
                        Field::Location => {
                            location = Some(map.next_value()?);
                        }
                    }
                }

                Ok(Audio {
                    title: title.unwrap_or_default(),
                    source: source.ok_or_else(|| serde::de::Error::missing_field("source"))?,
                    metadata: metadata.unwrap_or_default(),
                    location: location
                        .ok_or_else(|| serde::de::Error::missing_field("location"))?,
                })
            }
        }

        deserializer.deserialize_struct(
            "Audio",
            &["metadata", "title", "source", "location"],
            AudioVisitor,
        )
    }
}

impl<'de> Deserialize<'de> for Image {
    fn deserialize<D>(deserializer: D) -> Result<Image, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "lowercase")]
        enum Field {
            Metadata,
            Title,
            Source,
            Location,
        }

        struct ImageVisitor;

        impl<'de> Visitor<'de> for ImageVisitor {
            type Value = Image;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("struct Image")
            }

            fn visit_map<V>(self, mut map: V) -> Result<Image, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut metadata = None;
                let mut title = None;
                let mut source = None;
                let mut location = None;

                while let Some(key) = map.next_key()? {
                    match key {
                        Field::Metadata => {
                            metadata = Some(map.next_value()?);
                        }
                        Field::Title => {
                            title = Some(map.next_value()?);
                        }
                        Field::Source => {
                            source = Some(map.next_value()?);
                        }
                        Field::Location => {
                            location = Some(map.next_value()?);
                        }
                    }
                }

                Ok(Image {
                    title: title.unwrap_or_default(),
                    source: source.ok_or_else(|| serde::de::Error::missing_field("source"))?,
                    metadata: metadata.unwrap_or_default(),
                    location: location
                        .ok_or_else(|| serde::de::Error::missing_field("location"))?,
                })
            }
        }

        deserializer.deserialize_struct(
            "Image",
            &["metadata", "title", "source", "location"],
            ImageVisitor,
        )
    }
}

// Video uses "sources" (plural)
impl<'de> Deserialize<'de> for Video {
    fn deserialize<D>(deserializer: D) -> Result<Video, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "lowercase")]
        enum Field {
            Metadata,
            Title,
            Sources,
            Location,
        }

        struct VideoVisitor;

        impl<'de> Visitor<'de> for VideoVisitor {
            type Value = Video;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("struct Video")
            }

            fn visit_map<V>(self, mut map: V) -> Result<Video, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut metadata = None;
                let mut title = None;
                let mut sources = None;
                let mut location = None;

                while let Some(key) = map.next_key()? {
                    match key {
                        Field::Metadata => metadata = Some(map.next_value()?),
                        Field::Title => title = Some(map.next_value()?),
                        Field::Sources => sources = Some(map.next_value()?),
                        Field::Location => location = Some(map.next_value()?),
                    }
                }

                Ok(Video {
                    title: title.unwrap_or_default(),
                    sources: sources.unwrap_or_default(),
                    metadata: metadata.unwrap_or_default(),
                    location: location
                        .ok_or_else(|| serde::de::Error::missing_field("location"))?,
                })
            }
        }

        deserializer.deserialize_struct(
            "Video",
            &["metadata", "title", "sources", "location"],
            VideoVisitor,
        )
    }
}
