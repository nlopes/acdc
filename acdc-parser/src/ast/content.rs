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
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContentRef<'doc> {
    pub(super) nodes: &'doc [InlineNode<'doc>],
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

/// An ordered collection of separate inline-content values.
#[derive(Clone, Copy, Debug)]
pub struct ContentListRef<'doc> {
    source: ContentList<'doc>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum ContentList<'doc> {
    Values(&'doc [Vec<InlineNode<'doc>>]),
    IndexTerms(&'doc crate::IndexTerm<'doc>),
}

impl PartialEq for ContentListRef<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.iter().eq(other.iter())
    }
}

impl<'doc> ContentListRef<'doc> {
    pub(super) fn new(items: &'doc [Vec<InlineNode<'doc>>]) -> Self {
        Self {
            source: ContentList::Values(items),
        }
    }

    pub(super) fn index_terms(node: &'doc crate::IndexTerm<'doc>) -> Self {
        Self {
            source: ContentList::IndexTerms(node),
        }
    }

    /// Read each content value in order, including empty values.
    pub fn iter(self) -> impl Iterator<Item = ContentRef<'doc>> + Send + Sync + 'doc {
        let (levels, values) = match self.source {
            ContentList::Values(values) => ([None, None, None], values),
            ContentList::IndexTerms(node) => (
                [Some(node.term()), node.secondary(), node.tertiary()],
                &[][..],
            ),
        };
        levels
            .into_iter()
            .flatten()
            .map(ContentRef::new)
            .chain(values.iter().map(|nodes| ContentRef::new(nodes)))
    }

    /// Whether there are no content values, rather than one empty value.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.iter().next().is_none()
    }
}

impl Title<'_> {
    /// The title content; `None` if no title was supplied.
    #[must_use]
    pub fn content(&self) -> Option<ContentRef<'_>> {
        self.is_present().then(|| ContentRef::new(self.as_ref()))
    }
}
