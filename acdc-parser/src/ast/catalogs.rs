use std::{collections::HashMap, iter::FusedIterator};

use crate::{Reference, SectionKind, TocEntry};

use super::{ContentRef, DocumentRef};

node_ref!(
    ReferenceRef<'doc>,
    Reference<'doc>,
    "A registered cross-reference target."
);
node_ref!(
    TocEntryRef<'doc>,
    TocEntry<'doc>,
    "A registered table-of-contents entry."
);

/// Registered cross-reference targets, keyed by ID.
///
/// Reads use the stored catalog. They do not register tree occurrences or repair
/// the catalog after an edit. An absent target differs from a registered target
/// whose title and reference label are both absent.
#[derive(Clone, Copy, Debug)]
pub struct ReferencesRef<'doc> {
    references: &'doc HashMap<&'doc str, Reference<'doc>>,
}

impl<'doc> DocumentRef<'doc> {
    /// Registered cross-reference targets, including aliases and the document target.
    #[must_use]
    pub fn references(self) -> ReferencesRef<'doc> {
        ReferencesRef {
            references: &self.node.references,
        }
    }

    /// Stored TOC entries in catalog order. This does not discover new headings
    /// or refresh entries after an edit; TOC membership is separate from registration.
    pub fn toc_entries(
        self,
    ) -> impl DoubleEndedIterator<Item = TocEntryRef<'doc>>
    + ExactSizeIterator
    + FusedIterator
    + Send
    + Sync
    + 'doc {
        self.node.toc_entries.iter().map(TocEntryRef::from)
    }
}

impl<'doc> ReferencesRef<'doc> {
    /// The registered target, or `None` if the ID is absent.
    #[must_use]
    pub fn get(self, id: &str) -> Option<ReferenceRef<'doc>> {
        self.references.get(id).map(ReferenceRef::from)
    }

    /// Whether the ID is registered, even if it has no display content.
    #[must_use]
    pub fn contains(self, id: &str) -> bool {
        self.references.contains_key(id)
    }

    /// Number of registered IDs, including aliases and the document target.
    #[must_use]
    pub fn len(self) -> usize {
        self.references.len()
    }

    /// Whether no targets are registered.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.references.is_empty()
    }

    /// Each registered ID and target in unspecified order.
    pub fn iter(
        self,
    ) -> impl ExactSizeIterator<Item = (&'doc str, ReferenceRef<'doc>)>
    + FusedIterator
    + Send
    + Sync
    + 'doc {
        self.references
            .iter()
            .map(|(&id, reference)| (id, ReferenceRef::from(reference)))
    }
}

impl<'doc> ReferenceRef<'doc> {
    /// The registered title, including present empty content.
    #[must_use]
    pub fn title(self) -> Option<ContentRef<'doc>> {
        self.node
            .title
            .as_ref()
            .map(|title| ContentRef::new(title.as_ref()))
    }

    /// The registered reference label, including an explicitly empty label.
    #[must_use]
    pub fn xreflabel(self) -> Option<ContentRef<'doc>> {
        self.node
            .xreflabel
            .as_ref()
            .map(|label| ContentRef::new(label.as_ref()))
    }

    /// The section's reference category; absent for other targets.
    #[must_use]
    pub fn section_name(self) -> Option<&'doc str> {
        self.node.section_name()
    }

    /// The assigned reference number without a signifier or trailing punctuation.
    #[must_use]
    pub fn section_number(self) -> Option<&'doc str> {
        self.node.section_number()
    }

    /// Whether this target is a bibliography entry.
    #[must_use]
    pub fn is_bibliography(self) -> bool {
        self.node.is_bibliography()
    }

    /// Whether the catalog records an automatic citation to this target.
    #[must_use]
    pub fn has_automatic_citation(self) -> bool {
        self.node.has_automatic_citation()
    }
}

impl<'doc> TocEntryRef<'doc> {
    /// The registered section ID.
    #[must_use]
    pub fn id(self) -> &'doc str {
        self.node.id
    }

    /// The stored section title, including a present empty title.
    #[must_use]
    pub fn title(self) -> Option<ContentRef<'doc>> {
        self.node.title.content()
    }

    /// The stored reference label, including an explicitly empty label.
    #[must_use]
    pub fn xreflabel(self) -> Option<ContentRef<'doc>> {
        self.node
            .xreflabel
            .as_ref()
            .map(|label| ContentRef::new(label.as_ref()))
    }

    /// The stored section level.
    #[must_use]
    pub fn level(self) -> u8 {
        self.node.level
    }

    /// The section's structural category.
    #[must_use]
    pub fn kind(self) -> SectionKind {
        self.node.kind
    }

    /// The assigned heading number without a signifier or trailing punctuation.
    #[must_use]
    pub fn number(self) -> Option<&'doc str> {
        self.node.number()
    }
}

impl<'doc> From<&'doc Reference<'doc>> for ReferenceRef<'doc> {
    fn from(reference: &'doc Reference<'doc>) -> Self {
        Self::new(reference)
    }
}

impl<'doc> From<&'doc TocEntry<'doc>> for TocEntryRef<'doc> {
    fn from(entry: &'doc TocEntry<'doc>) -> Self {
        Self::new(entry)
    }
}
