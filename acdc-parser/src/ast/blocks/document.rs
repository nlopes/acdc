use crate::{Author, DocumentAttributeAssignment, DocumentAttributeValue, DocumentAttributes};

use super::{
    AttributesRef, ContentRef, DocumentAttributeRef, DocumentRef, ElementIdRef, TokensRef,
};

node_ref!(HeaderRef<'doc>, crate::Header<'doc>, "A document header.");

/// An author's retained name parts and optional email address.
#[derive(Clone, Copy, Debug)]
pub struct AuthorRef<'doc> {
    author: &'doc Author<'doc>,
}

/// Document attribute values and explicit assignments at one snapshot.
///
/// Effective values include referenceable defaults. Explicit assignments retain
/// unsets, even when the registry supplies a fallback value. Names use document
/// attribute policy; element aliases such as `roles` and `options` do not apply.
#[derive(Clone, Copy, Debug)]
pub struct DocumentAttributesRef<'doc> {
    attributes: &'doc DocumentAttributes<'doc>,
}

impl<'doc> DocumentRef<'doc> {
    /// The header, when present.
    #[must_use]
    pub fn header(self) -> Option<HeaderRef<'doc>> {
        self.node.header.as_ref().map(HeaderRef::from)
    }

    /// The end-of-header attribute snapshot. Later assignments remain in `blocks()`.
    #[must_use]
    pub fn attributes(self) -> DocumentAttributesRef<'doc> {
        DocumentAttributesRef::from(&self.node.attributes)
    }
}

impl<'doc> HeaderRef<'doc> {
    /// The title, including an explicitly empty title.
    #[must_use]
    pub fn title(self) -> Option<ContentRef<'doc>> {
        self.node.title.content()
    }

    /// The subtitle, including a present empty subtitle.
    #[must_use]
    pub fn subtitle(self) -> Option<ContentRef<'doc>> {
        self.node
            .subtitle
            .as_ref()
            .map(|title| ContentRef::new(title.as_ref()))
    }

    /// Authors in header order, after parser normalization and overrides.
    pub fn authors(self) -> impl Iterator<Item = AuthorRef<'doc>> + Send + Sync + 'doc {
        self.node.authors.iter().map(AuthorRef::from)
    }

    /// Element attributes on the document title, separate from document attributes.
    #[must_use]
    pub fn attributes(self) -> AttributesRef<'doc> {
        AttributesRef::header(self.node)
    }

    /// The primary ID, if present.
    #[must_use]
    pub fn id(self) -> Option<ElementIdRef<'doc>> {
        self.attributes().id()
    }

    /// The retained title style.
    #[must_use]
    pub fn style(self) -> Option<&'doc str> {
        self.attributes().style()
    }

    /// Role tokens in their retained order.
    #[must_use]
    pub fn roles(self) -> TokensRef<'doc> {
        self.attributes().roles()
    }

    /// Option tokens in their retained order.
    #[must_use]
    pub fn options(self) -> TokensRef<'doc> {
        self.attributes().options()
    }
}

impl<'doc> From<&'doc crate::Header<'doc>> for HeaderRef<'doc> {
    fn from(header: &'doc crate::Header<'doc>) -> Self {
        Self::new(header)
    }
}

impl<'doc> AuthorRef<'doc> {
    /// The first name.
    #[must_use]
    pub fn first_name(self) -> &'doc str {
        self.author.first_name
    }

    /// The middle name, when present.
    #[must_use]
    pub fn middle_name(self) -> Option<&'doc str> {
        self.author.middle_name
    }

    /// The last name; empty for a single-name author.
    #[must_use]
    pub fn last_name(self) -> &'doc str {
        self.author.last_name
    }

    /// The stored initials, without recomputing them from the names.
    #[must_use]
    pub fn initials(self) -> &'doc str {
        self.author.initials
    }

    /// The email address, when present.
    #[must_use]
    pub fn email(self) -> Option<&'doc str> {
        self.author.email
    }
}

impl<'doc> From<&'doc Author<'doc>> for AuthorRef<'doc> {
    fn from(author: &'doc Author<'doc>) -> Self {
        Self { author }
    }
}

impl<'doc> DocumentAttributesRef<'doc> {
    /// The effective value, including referenceable defaults.
    #[must_use]
    pub fn get(self, name: &str) -> Option<&'doc DocumentAttributeValue<'doc>> {
        self.attributes.get(name)
    }

    /// Whether an effective value exists.
    #[must_use]
    pub fn contains(self, name: &str) -> bool {
        self.attributes.contains_key(name)
    }

    /// Effective values, including defaults, in unspecified order.
    pub fn iter(
        self,
    ) -> impl Iterator<Item = (&'doc str, &'doc DocumentAttributeValue<'doc>)> + Send + Sync + 'doc
    {
        self.attributes.iter()
    }

    /// Whether there are no effective values.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.attributes.is_empty()
    }

    /// An explicit assignment, including an unset; `None` means no assignment.
    #[must_use]
    pub fn assignment(self, name: &str) -> Option<&'doc DocumentAttributeAssignment<'doc>> {
        self.attributes.assignment(name)
    }

    /// Explicit assignments, including unsets, in unspecified order.
    ///
    /// This snapshot does not retain header assignment order. Body assignments
    /// retain their order and source locations in the block tree.
    pub fn assignments(
        self,
    ) -> impl Iterator<Item = (&'doc str, &'doc DocumentAttributeAssignment<'doc>)> + Send + Sync + 'doc
    {
        self.attributes.assignments()
    }

    /// Whether an explicit assignment exists, including caller input and unsets.
    #[must_use]
    pub fn is_explicit(self, name: &str) -> bool {
        self.attributes.is_explicit(name)
    }
}

impl<'doc> From<&'doc DocumentAttributes<'doc>> for DocumentAttributesRef<'doc> {
    fn from(attributes: &'doc DocumentAttributes<'doc>) -> Self {
        Self { attributes }
    }
}

impl<'doc> DocumentAttributeRef<'doc> {
    /// The assignment's document attribute name.
    #[must_use]
    pub fn name(self) -> &'doc str {
        self.node.name.as_ref()
    }

    /// The accepted set or unset assignment at this point in the block tree.
    #[must_use]
    pub fn assignment(self) -> &'doc DocumentAttributeAssignment<'doc> {
        self.node.assignment()
    }
}

impl<'doc> From<&'doc crate::DocumentAttribute<'doc>> for DocumentAttributeRef<'doc> {
    fn from(attribute: &'doc crate::DocumentAttribute<'doc>) -> Self {
        Self::new(attribute)
    }
}
