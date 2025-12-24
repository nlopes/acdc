use std::ops::Deref;

use serde::{
    Deserialize, Serialize,
    de::{Deserializer, SeqAccess, Visitor},
    ser::Serializer,
};

use super::inlines::InlineNode;

/// A `Title` represents a title in a document (section titles, block titles, etc.).
///
/// Titles contain inline content and can include formatting, links, and other
/// inline elements.
///
/// Serializes as a plain array for backwards compatibility with existing fixtures.
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

impl<'de> Deserialize<'de> for Title {
    fn deserialize<D>(deserializer: D) -> Result<Title, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct TitleVisitor;

        impl<'de> Visitor<'de> for TitleVisitor {
            type Value = Title;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a sequence of inline nodes")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Title, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut inlines = Vec::new();
                while let Some(node) = seq.next_element()? {
                    inlines.push(node);
                }
                Ok(Title(inlines))
            }
        }

        deserializer.deserialize_seq(TitleVisitor)
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
