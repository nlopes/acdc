use crate::{
    IndexTerm, IndexTermRelationship,
    ast::{AttributesRef, ContentListRef, ContentRef, ElementIdRef, TokensRef},
};

use super::IndexTermRef;

/// A selected relationship between index entries.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum IndexTermRelationshipRef<'doc> {
    /// Use the target entry instead of this entry.
    See { target: ContentRef<'doc> },
    /// Also consult these related entries.
    SeeAlso { targets: ContentListRef<'doc> },
}

impl<'doc> From<&'doc IndexTermRelationship<'doc>> for IndexTermRelationshipRef<'doc> {
    fn from(relationship: &'doc IndexTermRelationship<'doc>) -> Self {
        match relationship {
            IndexTermRelationship::See { target } => Self::See {
                target: ContentRef::new(target),
            },
            IndexTermRelationship::SeeAlso { targets } => Self::SeeAlso {
                targets: ContentListRef::new(targets),
            },
        }
    }
}

/// Labels captured for the index catalog, not another authored occurrence.
#[derive(Clone, Copy, Debug)]
pub struct IndexTermLabelsRef<'doc> {
    node: &'doc IndexTerm<'doc>,
}

impl<'doc> IndexTermLabelsRef<'doc> {
    /// The primary label, including an explicitly empty label.
    #[must_use]
    pub fn term(self) -> ContentRef<'doc> {
        ContentRef::new(self.node.term())
    }

    /// The second hierarchy level; `None` if absent.
    #[must_use]
    pub fn secondary(self) -> Option<ContentRef<'doc>> {
        self.node.secondary().map(ContentRef::new)
    }

    /// The third hierarchy level; `None` if absent.
    #[must_use]
    pub fn tertiary(self) -> Option<ContentRef<'doc>> {
        self.node.tertiary().map(ContentRef::new)
    }

    /// The selected see or see-also relationship, if present.
    #[must_use]
    pub fn relationship(self) -> Option<IndexTermRelationshipRef<'doc>> {
        self.node.relationship.as_ref().map(Into::into)
    }
}

impl<'doc> IndexTermRef<'doc> {
    /// Retained occurrence ID and its source location. This does not register an anchor.
    #[must_use]
    pub fn id(self) -> Option<ElementIdRef<'doc>> {
        self.attributes().id()
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

    /// Identity, roles, options, hierarchy, selected relationships, and scalar controls.
    #[must_use]
    pub fn attributes(self) -> AttributesRef<'doc> {
        AttributesRef::index_term(self.node)
    }

    /// The primary label after its configured substitutions.
    #[must_use]
    pub fn term(self) -> ContentRef<'doc> {
        ContentRef::new(self.node.term())
    }

    /// The second hierarchy level; `None` if absent.
    #[must_use]
    pub fn secondary(self) -> Option<ContentRef<'doc>> {
        self.node.secondary().map(ContentRef::new)
    }

    /// The third hierarchy level; `None` if absent.
    #[must_use]
    pub fn tertiary(self) -> Option<ContentRef<'doc>> {
        self.node.tertiary().map(ContentRef::new)
    }

    /// Whether the primary label is visible in the document body.
    #[must_use]
    pub fn is_visible(self) -> bool {
        self.node.is_visible()
    }

    /// The selected see or see-also relationship, if present.
    #[must_use]
    pub fn relationship(self) -> Option<IndexTermRelationshipRef<'doc>> {
        self.node.relationship.as_ref().map(Into::into)
    }

    /// Labels at index registration, before any later substitutions.
    #[must_use]
    pub fn catalog(self) -> IndexTermLabelsRef<'doc> {
        IndexTermLabelsRef {
            node: self.node.catalog_entry(),
        }
    }
}

impl<'doc> From<&'doc IndexTerm<'doc>> for IndexTermRef<'doc> {
    fn from(owner: &'doc IndexTerm<'doc>) -> Self {
        Self::new(owner)
    }
}
