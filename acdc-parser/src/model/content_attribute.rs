use std::ops::Deref;

use serde::Serialize;

use super::InlineNode;

/// Parsed inline content of a displayable attribute.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(transparent)]
#[non_exhaustive]
pub struct ContentAttribute<'a>(Vec<InlineNode<'a>>);

impl<'a> ContentAttribute<'a> {
    /// Construct an attribute from parsed inline content.
    #[must_use]
    pub fn new(inlines: Vec<InlineNode<'a>>) -> Self {
        Self(inlines)
    }

    /// Consume the content, retaining its inline nodes.
    #[must_use]
    pub fn into_inlines(self) -> Vec<InlineNode<'a>> {
        self.0
    }

    pub(crate) fn inlines_mut(&mut self) -> &mut Vec<InlineNode<'a>> {
        &mut self.0
    }
}

impl<'a> From<Vec<InlineNode<'a>>> for ContentAttribute<'a> {
    fn from(inlines: Vec<InlineNode<'a>>) -> Self {
        Self::new(inlines)
    }
}

impl<'a> AsRef<[InlineNode<'a>]> for ContentAttribute<'a> {
    fn as_ref(&self) -> &[InlineNode<'a>] {
        &self.0
    }
}

impl<'a> Deref for ContentAttribute<'a> {
    type Target = [InlineNode<'a>];
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a, 'b> IntoIterator for &'b ContentAttribute<'a> {
    type Item = &'b InlineNode<'a>;
    type IntoIter = std::slice::Iter<'b, InlineNode<'a>>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

/// Deferred element content, consumed only after attribute precedence is settled.
#[derive(Clone, Debug)]
pub(crate) struct PendingContentAttribute<'a> {
    pub(crate) source: &'a str,
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) formatted: bool,
    pub(crate) attributes: Option<Box<crate::DocumentAttributes<'a>>>,
}
