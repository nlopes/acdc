use serde::Serialize;

use super::InlineNode;

/// Parsed inline content of a displayable attribute.
///
/// Read through [`crate::ast::ContentRef`]. Use [`Self::into_inlines`] to transfer
/// prepared nodes, or `ContentAttribute::from(content)` to copy selected content.
///
/// ```
/// use acdc_parser::{ContentAttribute, Options, Title, ast::ContentRef, parse_inline};
/// let parsed = parse_inline("*Bold* text", &Options::default())?;
/// let copied = ContentAttribute::from(parsed.content());
/// let title = Title::from(ContentRef::from(&copied));
/// assert_eq!(title.content(), Some(parsed.content()));
/// # Ok::<(), acdc_parser::Error>(())
/// ```
///
/// ```compile_fail,E0308
/// use acdc_parser::{ContentAttribute, InlineNode};
/// let content = ContentAttribute::default();
/// let _: &[InlineNode<'_>] = &content;
/// ```
///
/// ```compile_fail,E0599
/// use acdc_parser::{ContentAttribute, InlineNode};
/// let content = ContentAttribute::default();
/// let _: &[InlineNode<'_>] = content.as_ref();
/// ```
///
/// ```compile_fail,E0277
/// use acdc_parser::ContentAttribute;
/// let content = ContentAttribute::default();
/// for _ in &content {}
/// ```
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

    /// Whether this content contains no inline nodes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Number of direct inline nodes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub(crate) fn as_inlines(&self) -> &[InlineNode<'a>] {
        &self.0
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

/// Deferred element content, consumed only after attribute precedence is settled.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PendingContentAttribute<'a> {
    pub(crate) source: &'a str,
    pub(crate) input: usize,
    pub(crate) mode: crate::grammar::BlockAttributeMode,
}
