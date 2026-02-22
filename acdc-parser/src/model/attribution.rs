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
pub struct Attribution(Vec<InlineNode>);

impl Serialize for Attribution {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl Attribution {
    /// Creates a new `Attribution` with the given inline content.
    #[must_use]
    pub fn new(inlines: Vec<InlineNode>) -> Self {
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

impl From<Vec<InlineNode>> for Attribution {
    fn from(inlines: Vec<InlineNode>) -> Self {
        Self(inlines)
    }
}

impl AsRef<[InlineNode]> for Attribution {
    fn as_ref(&self) -> &[InlineNode] {
        &self.0
    }
}

impl Deref for Attribution {
    type Target = [InlineNode];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a> IntoIterator for &'a Attribution {
    type Item = &'a InlineNode;
    type IntoIter = std::slice::Iter<'a, InlineNode>;

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
pub struct CiteTitle(Vec<InlineNode>);

impl Serialize for CiteTitle {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl CiteTitle {
    /// Creates a new `CiteTitle` with the given inline content.
    #[must_use]
    pub fn new(inlines: Vec<InlineNode>) -> Self {
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

impl From<Vec<InlineNode>> for CiteTitle {
    fn from(inlines: Vec<InlineNode>) -> Self {
        Self(inlines)
    }
}

impl AsRef<[InlineNode]> for CiteTitle {
    fn as_ref(&self) -> &[InlineNode] {
        &self.0
    }
}

impl Deref for CiteTitle {
    type Target = [InlineNode];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a> IntoIterator for &'a CiteTitle {
    type Item = &'a InlineNode;
    type IntoIter = std::slice::Iter<'a, InlineNode>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}
