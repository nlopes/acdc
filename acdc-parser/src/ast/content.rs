use crate::{ContentAttribute, InlineNode, Title};

use super::InlineRef;

/// Inline content, which may be empty.
///
/// Optional content uses `Option<ContentRef>`: `None` is absent, while an empty
/// `Some` remains present.
///
/// Content cannot outlive its owner:
///
/// ```compile_fail,E0597
/// use acdc_parser::{Options, parse, ast::BlockKind};
///
/// let content = {
///     let parsed = parse("A paragraph.", &Options::default()).unwrap();
///     let BlockKind::Paragraph(paragraph) = parsed.document_ref().blocks().next().unwrap().kind()
///         else { panic!("expected paragraph") };
///     paragraph.content()
/// };
/// println!("{}", content.is_empty());
/// ```
#[derive(Clone, Copy, Debug)]
pub struct ContentRef<'doc> {
    nodes: &'doc [InlineNode<'doc>],
}

impl<'doc> ContentRef<'doc> {
    pub(super) fn new(nodes: &'doc [InlineNode<'doc>]) -> Self {
        Self { nodes }
    }

    /// Read direct inline children in content order.
    pub fn inlines(self) -> impl Iterator<Item = InlineRef<'doc>> + Send + Sync + 'doc {
        self.nodes.iter().map(InlineRef::from)
    }

    /// Whether this content contains no inline nodes.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.nodes.is_empty()
    }
}

impl<'doc> From<&'doc ContentAttribute<'doc>> for ContentRef<'doc> {
    fn from(content: &'doc ContentAttribute<'doc>) -> Self {
        Self::new(content.as_ref())
    }
}

impl Title<'_> {
    /// The title content; `None` if no title was supplied.
    #[must_use]
    pub fn content(&self) -> Option<ContentRef<'_>> {
        self.is_present().then(|| ContentRef::new(self.as_ref()))
    }
}
