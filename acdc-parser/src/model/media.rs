//! Media types for `AsciiDoc` documents (images, audio, video).

use std::fmt::Display;

use serde::{
    Serialize,
    ser::{SerializeMap, Serializer},
};

use crate::{ContentAttribute, Error, SourceLocation};

use super::{
    attributes::{AttributeName, AttributeValue, ElementAttributes},
    location::{Location, Position},
    metadata::{
        BlockMetadata, InlineMetadata, InlineMetadataView, MacroAttributes, ReservedAttribute,
    },
    title::Title,
};

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
/// - `Url(SourceUrl)` - Remote URL, retaining its original spelling for display
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

    pub(crate) fn original(&self) -> &str {
        self.original
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
    /// recognized scheme) but fails URL parsing.
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
                        location: crate::Location::point(Position::default()),
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
    pub source: Source<'a>,
    pub metadata: BlockMetadata<'a>,
    pub location: Location,
}

impl<'a> Video<'a> {
    /// Create a new video with the given source and location.
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

/// A block image with an optional caption title.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Image<'a> {
    /// Explicit alternative content. `None` selects the filename fallback.
    pub alt: Option<ContentAttribute<'a>>,
    /// The block caption title, independent of alternative text.
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
            alt: None,
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

/// An inline image with distinct alternative text and hover text.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct InlineImage<'a> {
    /// Explicit alternative text. `None` selects the converter's filename fallback.
    pub alt: Option<ContentAttribute<'a>>,
    /// Hover text supplied by the inline macro's `title=` attribute.
    pub tooltip: Option<ContentAttribute<'a>>,
    pub source: Source<'a>,
    pub metadata: InlineMetadata<'a>,
    pub(crate) macro_attributes: MacroAttributes<'a>,
    pub location: Location,
}

impl<'a> InlineImage<'a> {
    /// Construct an image with no explicit alternative or hover text.
    #[must_use]
    pub fn new(source: Source<'a>, location: Location) -> Self {
        Self {
            alt: None,
            tooltip: None,
            source,
            metadata: InlineMetadata::default(),
            macro_attributes: MacroAttributes::default(),
            location,
        }
    }

    /// Read scalar macro controls, such as width and link.
    ///
    /// This map excludes alt, tooltip, identity, roles, and options. Read those
    /// values through their dedicated fields or methods.
    #[must_use]
    pub fn attributes(&self) -> &ElementAttributes<'a> {
        &self.macro_attributes.attributes
    }

    /// Set an attribute without a dedicated field.
    ///
    /// # Errors
    /// Rejects `alt`, `title`, `id`, `role`/`roles`, and `opts`/`options`.
    /// Update the content, metadata, or options directly instead.
    pub fn set_attribute(
        &mut self,
        name: AttributeName<'a>,
        value: AttributeValue<'a>,
    ) -> Result<(), ReservedAttribute> {
        self.macro_attributes.set(name, value, &["alt", "title"])
    }

    /// Remove an attribute without a dedicated field and return its previous value.
    ///
    /// # Errors
    /// Rejects the dedicated names listed by [`Self::set_attribute`].
    pub fn remove_attribute(
        &mut self,
        name: &str,
    ) -> Result<Option<AttributeValue<'a>>, ReservedAttribute> {
        self.macro_attributes.remove(name, &["alt", "title"])
    }

    /// Read the macro's option names in their stored order.
    #[must_use]
    pub fn options(&self) -> &[&'a str] {
        &self.macro_attributes.options
    }

    /// Edit the macro's option names without creating attribute-map aliases.
    pub fn options_mut(&mut self) -> &mut Vec<&'a str> {
        &mut self.macro_attributes.options
    }

    pub(crate) fn has_metadata(&self) -> bool {
        !self.metadata.is_empty() || !self.macro_attributes.is_empty()
    }

    pub(crate) fn metadata_view(&self) -> InlineMetadataView<'_, 'a> {
        InlineMetadataView {
            attributes: &self.macro_attributes.attributes,
            roles: &self.metadata.roles,
            options: &self.macro_attributes.options,
            id: &self.metadata.id,
        }
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
        if self.title.is_present() {
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
        if self.title.is_present() {
            state.serialize_entry("title", &self.title)?;
        }
        if let Some(alt) = &self.alt {
            state.serialize_entry("alt", alt)?;
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
        if self.title.is_present() {
            state.serialize_entry("title", &self.title)?;
        }
        state.serialize_entry("source", &self.source)?;
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
