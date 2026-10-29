use crate::{AttributeValue, ElementId};

use super::{ContentRef, LocationRef};

/// An element attribute value. Absence is represented by the enclosing `Option`.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum AttributeValueRef<'doc> {
    /// Exact semantic text, including meaningful quotes and empty strings.
    Text(&'doc str),
    /// A boolean value, including an explicitly false value.
    Bool(bool),
    /// A retained valueless attribute.
    None,
    /// Parsed inline content, including present-empty content.
    Content(ContentRef<'doc>),
    /// Role or option tokens in their retained order.
    Tokens(TokensRef<'doc>),
}

impl<'doc> From<&'doc AttributeValue<'doc>> for AttributeValueRef<'doc> {
    fn from(value: &'doc AttributeValue<'doc>) -> Self {
        match value {
            AttributeValue::String(text) => Self::Text(text.as_ref()),
            AttributeValue::Bool(value) => Self::Bool(*value),
            AttributeValue::None => Self::None,
        }
    }
}

/// An element ID and its recorded source location.
#[derive(Clone, Copy, Debug)]
pub struct ElementIdRef<'doc> {
    text: &'doc str,
    location: LocationRef<'doc>,
}

impl<'doc> ElementIdRef<'doc> {
    pub(super) fn new(text: &'doc str, location: &'doc crate::Location) -> Self {
        Self {
            text,
            location: LocationRef::from(location),
        }
    }

    /// The ID text.
    #[must_use]
    pub fn as_str(self) -> &'doc str {
        self.text
    }

    /// The recorded source range; formatting spans use their node's range.
    #[must_use]
    pub fn location(self) -> LocationRef<'doc> {
        self.location
    }
}

impl<'doc> From<&'doc ElementId<'doc>> for ElementIdRef<'doc> {
    fn from(id: &'doc ElementId<'doc>) -> Self {
        Self::new(id.id, &id.location)
    }
}

/// Role or option tokens in retained semantic order.
///
/// Tokens preserve order, duplicates, and spelling. Span role text is split on
/// whitespace; entries already stored as tokens are read as-is.
#[derive(Clone, Copy, Debug)]
pub struct TokensRef<'doc> {
    source: Tokens<'doc>,
}

#[derive(Clone, Copy, Debug)]
enum Tokens<'doc> {
    List(&'doc [&'doc str]),
    Text(&'doc str),
}

impl<'doc> TokensRef<'doc> {
    pub(super) fn new(tokens: &'doc [&'doc str]) -> Self {
        Self {
            source: Tokens::List(tokens),
        }
    }

    pub(super) fn from_text(text: &'doc str) -> Self {
        Self {
            source: Tokens::Text(text),
        }
    }

    /// Iterate in token order, including any retained duplicates.
    pub fn iter(self) -> impl Iterator<Item = &'doc str> + Send + Sync + 'doc {
        let (list, text) = match self.source {
            Tokens::List(tokens) => (tokens, ""),
            Tokens::Text(text) => (&[][..], text),
        };
        list.iter().copied().chain(text.split_whitespace())
    }

    /// Whether this collection contains no tokens.
    #[must_use]
    pub fn is_empty(self) -> bool {
        match self.source {
            Tokens::List(tokens) => tokens.is_empty(),
            Tokens::Text(text) => text.split_whitespace().next().is_none(),
        }
    }

    /// Whether an exact token is present.
    #[must_use]
    pub fn contains(self, token: &str) -> bool {
        self.iter().any(|value| value == token)
    }
}

impl PartialEq for TokensRef<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.iter().eq(other.iter())
    }
}

impl Eq for TokensRef<'_> {}
