use crate::{AttributeValue, ElementId};

use super::LocationRef;

/// A retained scalar attribute value. Absence is represented by the enclosing `Option`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum AttributeValueRef<'doc> {
    /// Exact semantic text, including meaningful quotes and empty strings.
    Text(&'doc str),
    /// A boolean value, including an explicitly false value.
    Bool(bool),
    /// A retained valueless attribute.
    None,
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
