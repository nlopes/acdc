use std::ops::Deref;

use serde::{Serialize, ser::Serializer};

use super::inlines::InlineNode;

/// A title in a document (section titles, block titles, document title, etc.).
///
/// `Title` is a newtype wrapper around `Vec<InlineNode>` that provides convenient
/// access to inline content. Titles can include formatting, links, and other inline
/// elements.
///
/// # Accessing Content
///
/// `Title` implements `Deref<Target=[InlineNode]>`, so you can use slice methods directly:
///
/// ```
/// # use acdc_parser::{Title, InlineNode};
/// let title = Title::new(vec![/* inline nodes */]);
///
/// // Iterate over inline nodes
/// for node in &title {
///     // ...
/// }
///
/// // Check if empty
/// if title.is_empty() {
///     // ...
/// }
///
/// // Access by index (via deref)
/// if let Some(first) = title.first() {
///     // ...
/// }
/// ```
///
/// # Serialization
///
/// Serializes as a plain JSON array of inline nodes for ASG compatibility.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct Title(Vec<InlineNode>);

impl Serialize for Title {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

pub type Subtitle = Title;

impl Title {
    /// Creates a new `Title` with the given inline content.
    #[must_use]
    pub fn new(inlines: Vec<InlineNode>) -> Self {
        Self(inlines)
    }

    /// Returns `true` if the title has no content.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns the number of inline nodes in the title.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl From<Vec<InlineNode>> for Title {
    fn from(inlines: Vec<InlineNode>) -> Self {
        Self(inlines)
    }
}

impl AsRef<[InlineNode]> for Title {
    fn as_ref(&self) -> &[InlineNode] {
        &self.0
    }
}

impl Deref for Title {
    type Target = [InlineNode];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a> IntoIterator for &'a Title {
    type Item = &'a InlineNode;
    type IntoIter = std::slice::Iter<'a, InlineNode>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}
