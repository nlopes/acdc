use std::{fmt, path::Path};

use crate::{Image, InlineImage, Source};

use super::{AttributesRef, ContentRef, ElementIdRef, TokensRef};

node_ref!(ImageRef<'doc>, Image<'doc>, "A block image.");
node_ref!(InlineImageRef<'doc>, InlineImage<'doc>, "An inline image.");

impl<'doc> ImageRef<'doc> {
    /// Read this element's attributes.
    #[must_use]
    pub fn attributes(self) -> AttributesRef<'doc> {
        AttributesRef::image(self.node)
    }

    /// Role tokens in their retained order.
    #[must_use]
    pub fn roles(self) -> TokensRef<'doc> {
        self.attributes().roles()
    }

    /// Option tokens in their retained order.
    #[must_use]
    pub fn options(self) -> TokensRef<'doc> {
        self.attributes().options()
    }

    /// The retained block style, including an explicitly empty style.
    #[must_use]
    pub fn style(self) -> Option<&'doc str> {
        self.attributes().style()
    }

    /// The image target before converter path resolution.
    #[must_use]
    pub fn source(self) -> SourceRef<'doc> {
        SourceRef::from(&self.node.source)
    }

    /// Explicit alternative content; `None` leaves fallback text to the converter.
    #[must_use]
    pub fn alt(self) -> Option<ContentRef<'doc>> {
        self.node.alt.as_ref().map(ContentRef::from)
    }

    /// The caption title, independent of alternative content.
    #[must_use]
    pub fn title(self) -> Option<ContentRef<'doc>> {
        self.node.title.content()
    }

    /// The primary ID, if present.
    #[must_use]
    pub fn id(self) -> Option<ElementIdRef<'doc>> {
        self.attributes().id()
    }
}

impl<'doc> From<&'doc Image<'doc>> for ImageRef<'doc> {
    fn from(image: &'doc Image<'doc>) -> Self {
        Self::new(image)
    }
}

impl<'doc> InlineImageRef<'doc> {
    /// Read this element's attributes.
    #[must_use]
    pub fn attributes(self) -> AttributesRef<'doc> {
        AttributesRef::inline_image(self.node)
    }

    /// Role tokens in their retained order.
    #[must_use]
    pub fn roles(self) -> TokensRef<'doc> {
        self.attributes().roles()
    }

    /// Option tokens in their retained order.
    #[must_use]
    pub fn options(self) -> TokensRef<'doc> {
        self.attributes().options()
    }

    /// The image target before converter path resolution.
    #[must_use]
    pub fn source(self) -> SourceRef<'doc> {
        SourceRef::from(&self.node.source)
    }

    /// Explicit alternative content; `None` leaves fallback text to the converter.
    #[must_use]
    pub fn alt(self) -> Option<ContentRef<'doc>> {
        self.node.alt.as_ref().map(ContentRef::from)
    }

    /// Hover content supplied by `title=`, including an explicitly empty value.
    #[must_use]
    pub fn tooltip(self) -> Option<ContentRef<'doc>> {
        self.node.tooltip.as_ref().map(ContentRef::from)
    }

    /// The element ID, if present.
    #[must_use]
    pub fn id(self) -> Option<ElementIdRef<'doc>> {
        self.attributes().id()
    }
}

impl<'doc> From<&'doc InlineImage<'doc>> for InlineImageRef<'doc> {
    fn from(image: &'doc InlineImage<'doc>) -> Self {
        Self::new(image)
    }
}

/// A media target.
#[derive(Clone, Copy, Debug)]
pub struct SourceRef<'doc> {
    source: &'doc Source<'doc>,
}

/// The form of a media target, before converter path resolution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SourceKind<'doc> {
    /// A filesystem path, which need not be UTF-8.
    Path(&'doc Path),
    /// A URL with its original spelling preserved.
    Url(&'doc str),
    /// A symbolic name.
    Name(&'doc str),
}

impl<'doc> SourceRef<'doc> {
    /// The target's semantic kind and value.
    #[must_use]
    pub fn kind(self) -> SourceKind<'doc> {
        match self.source {
            Source::Path(path) => SourceKind::Path(path.as_path()),
            Source::Url(url) => SourceKind::Url(url.original()),
            Source::Name(name) => SourceKind::Name(name),
        }
    }
}

impl<'doc> From<&'doc Source<'doc>> for SourceRef<'doc> {
    fn from(source: &'doc Source<'doc>) -> Self {
        Self { source }
    }
}

impl fmt::Display for SourceRef<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.source.fmt(formatter)
    }
}
