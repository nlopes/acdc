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

/// An element ID and the source that supplied it.
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

    /// The source span recorded for this ID.
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
/// Reads preserve duplicates and spelling. They do not split or normalize tokens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TokensRef<'doc> {
    tokens: &'doc [&'doc str],
}

impl<'doc> TokensRef<'doc> {
    pub(super) fn new(tokens: &'doc [&'doc str]) -> Self {
        Self { tokens }
    }

    /// Iterate in token order, including any retained duplicates.
    pub fn iter(self) -> impl Iterator<Item = &'doc str> + Send + Sync + 'doc {
        self.tokens.iter().copied()
    }

    /// Whether this collection contains no tokens.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.tokens.is_empty()
    }

    /// Whether an exact token is present.
    #[must_use]
    pub fn contains(self, token: &str) -> bool {
        self.tokens.contains(&token)
    }
}
