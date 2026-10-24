use std::ops::Deref;

use serde::{Serialize, Serializer};

use super::{ContentAttribute, InlineNode};

/// A document, section, or block title, including whether one was supplied.
///
/// `Title::default()` is absent. `Title::new()` is present even when its inline
/// array is empty. Use [`Self::is_present`] to decide whether to render a caption,
/// and slice methods such as `is_empty()` to inspect its text.
///
/// Serialization emits the inline array. A containing block omits an absent title.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct Title<'a>(Option<ContentAttribute<'a>>);

/// A secondary document title.
pub type Subtitle<'a> = Title<'a>;

impl<'a> Title<'a> {
    /// Construct a present title, including an explicitly empty title.
    #[must_use]
    pub fn new(inlines: Vec<InlineNode<'a>>) -> Self {
        Self(Some(ContentAttribute::new(inlines)))
    }

    /// Whether a title was supplied, independently of its text length.
    #[must_use]
    pub fn is_present(&self) -> bool {
        self.0.is_some()
    }

    /// Whether the title contains no inline nodes, including an absent title.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.as_ref().is_empty()
    }

    /// Number of inline nodes in the title.
    #[must_use]
    pub fn len(&self) -> usize {
        self.as_ref().len()
    }

    /// Consume the title, returning its inline nodes or an empty vector if absent.
    #[must_use]
    pub fn into_inlines(self) -> Vec<InlineNode<'a>> {
        self.0.map_or_else(Vec::new, ContentAttribute::into_inlines)
    }

    pub(crate) fn inlines_mut(&mut self) -> &mut [InlineNode<'a>] {
        self.0
            .as_mut()
            .map_or(&mut [], |content| content.inlines_mut().as_mut_slice())
    }
}

impl Serialize for Title<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.as_ref().serialize(serializer)
    }
}

impl<'a> From<Vec<InlineNode<'a>>> for Title<'a> {
    fn from(inlines: Vec<InlineNode<'a>>) -> Self {
        Self::new(inlines)
    }
}

impl<'a> AsRef<[InlineNode<'a>]> for Title<'a> {
    fn as_ref(&self) -> &[InlineNode<'a>] {
        self.0.as_ref().map_or(&[], ContentAttribute::as_ref)
    }
}

impl<'a> Deref for Title<'a> {
    type Target = [InlineNode<'a>];
    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

impl<'a, 'b> IntoIterator for &'b Title<'a> {
    type Item = &'b InlineNode<'a>;
    type IntoIter = std::slice::Iter<'b, InlineNode<'a>>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
