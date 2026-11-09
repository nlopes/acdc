use std::{
    collections::{HashMap, hash_map::RandomState},
    iter::FusedIterator,
};

use crate::{ReferenceTarget, SectionKind, TocEntry};

use super::{CaptionRef, ContentRef, DocumentRef};

node_ref!(
    ReferenceTargetRef<'doc>,
    ReferenceTarget<'doc>,
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
    references: &'doc HashMap<&'doc str, ReferenceTarget<'doc>>,
}

impl<'doc> From<&'doc HashMap<&'doc str, ReferenceTarget<'doc>>> for ReferencesRef<'doc> {
    fn from(references: &'doc HashMap<&'doc str, ReferenceTarget<'doc>>) -> Self {
        Self { references }
    }
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
    pub fn get(self, id: &str) -> Option<ReferenceTargetRef<'doc>> {
        self.references.get(id).map(ReferenceTargetRef::from)
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
    ) -> impl ExactSizeIterator<Item = (&'doc str, ReferenceTargetRef<'doc>)>
    + FusedIterator
    + Send
    + Sync
    + 'doc {
        self.references
            .iter()
            .map(|(&id, reference)| (id, ReferenceTargetRef::from(reference)))
    }
}

impl<'doc> ReferenceTargetRef<'doc> {
    /// Whether this target is the document top or its explicit header ID.
    #[must_use]
    pub fn is_document(self) -> bool {
        self.node.document_label.is_some()
    }

    /// The registered resolved caption, including empty prefixes and unset ordinals.
    /// Reads keep the catalog snapshot even if the target block has since changed.
    ///
    /// ```
    /// use acdc_parser::{Options, ast::CaptionRef, parse};
    ///
    /// let parsed = parse("[#picture]\n.A bird\nimage::bird.svg[]\n", &Options::default())?;
    /// let reference = parsed.document().references().get("picture").ok_or("picture")?;
    /// assert!(matches!(reference.caption(), Some(CaptionRef::Numbered { label: "Figure", .. })));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[must_use]
    pub fn caption(self) -> Option<CaptionRef<'doc>> {
        self.node.caption.as_ref().map(CaptionRef::from)
    }

    /// The canonical destination ID, independently of the catalog lookup key.
    #[must_use]
    pub fn target(self) -> &'doc str {
        self.node.target
    }

    /// Whether this record identifies the heading snapshot stored in a TOC entry.
    /// Caller edits to the target, title or source span can invalidate this match.
    #[must_use]
    pub fn matches_toc_entry(self, entry: TocEntryRef<'_>) -> bool {
        self.target() == entry.id()
            && match (&self.node.occurrence, &entry.node.occurrence) {
                (Some(reference), Some(entry)) => reference == entry,
                (None, None) => true,
                _ => false,
            }
            && self.title().is_some()
            && self.title() == entry.title()
            && self.location().end() == entry.location().end()
            && self.location().absolute_end() == entry.location().absolute_end()
    }

    /// The registered title, including present empty content.
    #[must_use]
    pub fn title(self) -> Option<ContentRef<'doc>> {
        self.node
            .title
            .as_ref()
            .map(|title| ContentRef::new(title.as_inlines()))
    }

    /// The registered reference label, including an explicitly empty label.
    #[must_use]
    pub fn xreflabel(self) -> Option<ContentRef<'doc>> {
        self.node
            .xreflabel
            .as_ref()
            .map(|label| ContentRef::new(label.as_inlines()))
    }

    /// The section's reference category; absent for other targets.
    #[must_use]
    pub fn section_category(self) -> Option<&'doc str> {
        self.node.section_category()
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
            .map(|label| ContentRef::new(label.as_inlines()))
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

impl<'doc> From<&'doc ReferenceTarget<'doc>> for ReferenceTargetRef<'doc> {
    fn from(reference: &'doc ReferenceTarget<'doc>) -> Self {
        Self::new(reference)
    }
}

impl<'doc> From<&'doc TocEntry<'doc>> for TocEntryRef<'doc> {
    fn from(entry: &'doc TocEntry<'doc>) -> Self {
        Self::new(entry)
    }
}

/// Copies the registered target, including derived values and source locations.
/// Source text remains borrowed. The copy does not refresh the catalog.
impl<'doc> From<ReferenceTargetRef<'doc>> for ReferenceTarget<'doc> {
    fn from(reference: ReferenceTargetRef<'doc>) -> Self {
        reference.node.clone()
    }
}

/// Copies the stored TOC entry, including its assigned number and source location.
/// Source text remains borrowed. The copy does not discover or refresh headings.
impl<'doc> From<TocEntryRef<'doc>> for TocEntry<'doc> {
    fn from(entry: TocEntryRef<'doc>) -> Self {
        entry.node.clone()
    }
}

/// Copies every registered ID and target, including aliases.
/// Source text remains borrowed. The copy does not refresh the catalog.
impl<'doc> From<ReferencesRef<'doc>> for HashMap<&'doc str, ReferenceTarget<'doc>, RandomState> {
    fn from(references: ReferencesRef<'doc>) -> Self {
        references.references.clone()
    }
}
