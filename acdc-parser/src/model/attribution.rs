use std::ops::Deref;

use serde::{Serialize, ser::Serializer};

use super::inlines::InlineNode;

/// An attribution in a blockquote (the author/source of the quote).
///
/// `Attribution` is a newtype wrapper around `Vec<InlineNode>` that provides
/// convenient access to inline content. Attributions can include formatting,
/// links, and other inline elements.
///
/// # Serialization
///
/// Serializes as a plain JSON array of inline nodes.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct Attribution<'a>(Vec<InlineNode<'a>>);

impl Serialize for Attribution<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'a> Attribution<'a> {
    /// Creates a new `Attribution` with the given inline content.
    #[must_use]
    pub fn new(inlines: Vec<InlineNode<'a>>) -> Self {
        Self(inlines)
    }

    /// Returns `true` if the attribution has no content.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns the number of inline nodes in the attribution.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl<'a> From<Vec<InlineNode<'a>>> for Attribution<'a> {
    fn from(inlines: Vec<InlineNode<'a>>) -> Self {
        Self(inlines)
    }
}

impl<'a> AsRef<[InlineNode<'a>]> for Attribution<'a> {
    fn as_ref(&self) -> &[InlineNode<'a>] {
        &self.0
    }
}

impl<'a> Deref for Attribution<'a> {
    type Target = [InlineNode<'a>];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a, 'b> IntoIterator for &'b Attribution<'a> {
    type Item = &'b InlineNode<'a>;
    type IntoIter = std::slice::Iter<'b, InlineNode<'a>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

/// A citation title in a blockquote (the title of the cited work).
///
/// `CiteTitle` is a newtype wrapper around `Vec<InlineNode>` that provides
/// convenient access to inline content.
///
/// # Serialization
///
/// Serializes as a plain JSON array of inline nodes.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct CiteTitle<'a>(Vec<InlineNode<'a>>);

impl Serialize for CiteTitle<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'a> CiteTitle<'a> {
    /// Creates a new `CiteTitle` with the given inline content.
    #[must_use]
    pub fn new(inlines: Vec<InlineNode<'a>>) -> Self {
        Self(inlines)
    }

    /// Returns `true` if the cite title has no content.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns the number of inline nodes in the cite title.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl<'a> From<Vec<InlineNode<'a>>> for CiteTitle<'a> {
    fn from(inlines: Vec<InlineNode<'a>>) -> Self {
        Self(inlines)
    }
}

impl<'a> AsRef<[InlineNode<'a>]> for CiteTitle<'a> {
    fn as_ref(&self) -> &[InlineNode<'a>] {
        &self.0
    }
}

impl<'a> Deref for CiteTitle<'a> {
    type Target = [InlineNode<'a>];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a, 'b> IntoIterator for &'b CiteTitle<'a> {
    type Item = &'b InlineNode<'a>;
    type IntoIter = std::slice::Iter<'b, InlineNode<'a>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}
