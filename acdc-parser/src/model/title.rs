use serde::{Serialize, Serializer};

use super::{ContentAttribute, InlineNode};

/// A document, section, or block title, including whether one was supplied.
///
/// `Title::default()` is absent. `Title::new()` is present even when its inline
/// array is empty. Use [`Self::is_present`] to decide whether to render a caption,
/// and [`Self::is_empty`] to inspect its text. Read its nodes through [`Self::content`].
///
/// Serialization emits the inline array. A containing block omits an absent title.
///
/// ```compile_fail,E0308
/// use acdc_parser::{InlineNode, Title};
/// let title = Title::default();
/// let _: &[InlineNode<'_>] = &title;
/// ```
///
/// ```compile_fail,E0599
/// use acdc_parser::{InlineNode, Title};
/// let title = Title::default();
/// let _: &[InlineNode<'_>] = title.as_ref();
/// ```
///
/// ```compile_fail,E0277
/// use acdc_parser::Title;
/// let title = Title::default();
/// for _ in &title {}
/// ```
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
        self.as_inlines().is_empty()
    }

    /// Number of inline nodes in the title.
    #[must_use]
    pub fn len(&self) -> usize {
        self.as_inlines().len()
    }

    /// Consume the title, returning its inline nodes or an empty vector if absent.
    #[must_use]
    pub fn into_inlines(self) -> Vec<InlineNode<'a>> {
        self.0.map_or_else(Vec::new, ContentAttribute::into_inlines)
    }

    pub(crate) fn as_inlines(&self) -> &[InlineNode<'a>] {
        self.0.as_ref().map_or(&[], ContentAttribute::as_inlines)
    }

    pub(crate) fn inlines_mut(&mut self) -> &mut [InlineNode<'a>] {
        self.0
            .as_mut()
            .map_or(&mut [], |content| content.inlines_mut().as_mut_slice())
    }
}

impl Serialize for Title<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.as_inlines().serialize(serializer)
    }
}

impl<'a> From<Vec<InlineNode<'a>>> for Title<'a> {
    fn from(inlines: Vec<InlineNode<'a>>) -> Self {
        Self::new(inlines)
    }
}
