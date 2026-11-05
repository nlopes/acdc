//! Anchor and reference types for `AsciiDoc` documents.

use serde::{
    Serialize,
    ser::{SerializeMap, Serializer},
};

use super::{
    caption::Caption,
    content_attribute::ContentAttribute,
    inlines::InlineNode,
    location::Location,
    section::{SectionKind, SectionNumber},
    title::Title,
};

/// Section styles that should not receive automatic numbering.
///
/// When `sectnums` is enabled, sections with these styles are excluded from
/// the numbering scheme. Appendix uses letter numbering (A, B, C) which is
/// handled separately.
pub const UNNUMBERED_SECTION_STYLES: &[&str] = &[
    "preface",
    "abstract",
    "dedication",
    "colophon",
    "bibliography",
    "glossary",
    "index",
    "appendix",
];

/// An `Anchor` represents an anchor in a document.
///
/// An anchor is a reference point in a document that can be linked to.
///
/// ```compile_fail,E0616
/// use acdc_parser::{Anchor, Location};
/// let mut anchor = Anchor::new("entry", Location::default());
/// anchor.id = "changed";
/// ```
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct Anchor<'a> {
    pub(crate) id: &'a str,
    pub(crate) xreflabel: Option<ContentAttribute<'a>>,
    pub(crate) location: Location,
    // Key into parser-owned input, cleared before returning the AST.
    pub(crate) reftext_source: Option<usize>,
    pub(crate) bibliography: bool,
    pub(crate) bibliography_label: Option<Box<BibliographyLabel<'a>>>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BibliographyLabel<'a> {
    pub(crate) source: Option<&'a str>,
    pub(crate) content: Vec<InlineNode<'a>>,
}

impl Serialize for Anchor<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        self.serialize_fields(&mut map)?;
        map.end()
    }
}

impl<'a> Anchor<'a> {
    pub(super) fn serialize_fields<M: SerializeMap>(&self, map: &mut M) -> Result<(), M::Error> {
        map.serialize_entry("id", &self.id)?;
        if let Some(label) = &self.xreflabel {
            map.serialize_entry("xreflabel", label)?;
        }
        if self.is_bibliography() {
            map.serialize_entry("bibliography", &true)?;
        }
        if let Some(label) = self.bibliography_label() {
            map.serialize_entry("bibliography_label", label)?;
        }
        map.serialize_entry("location", &self.location)
    }

    /// Create a new anchor with the given ID and location.
    #[must_use]
    pub fn new(id: &'a str, location: Location) -> Self {
        Self {
            id,
            xreflabel: None,
            location,
            bibliography: false,
            reftext_source: None,
            bibliography_label: None,
        }
    }

    /// Set the cross-reference label.
    #[must_use]
    pub fn with_xreflabel(mut self, xreflabel: Option<ContentAttribute<'a>>) -> Self {
        self.set_reftext(xreflabel);
        self
    }

    /// Replace the ID without renaming document catalog entries or xref targets.
    pub fn replace_id(&mut self, id: &'a str) -> &'a str {
        std::mem::replace(&mut self.id, id)
    }

    /// Set prepared citation text without parsing it or updating the reference catalog.
    pub fn set_reftext(&mut self, reftext: Option<ContentAttribute<'a>>) {
        self.reftext_source = None;
        self.xreflabel = reftext;
    }

    /// Transfer citation text, leaving it absent.
    pub fn take_reftext(&mut self) -> Option<ContentAttribute<'a>> {
        self.reftext_source = None;
        self.xreflabel.take()
    }

    /// Classify this as a bibliography entry and set its prepared display label.
    /// `None` selects the ID fallback; empty content keeps an explicitly empty label.
    /// Citation reftext and document catalogs stay unchanged.
    pub fn set_bibliography(&mut self, label: Option<ContentAttribute<'a>>) {
        self.bibliography = true;
        self.bibliography_label = label.map(|content| {
            Box::new(BibliographyLabel {
                source: None,
                content: content.into_inlines(),
            })
        });
    }

    /// Remove bibliography classification and its display label, keeping citation text.
    pub fn clear_bibliography(&mut self) {
        self.bibliography = false;
        self.bibliography_label = None;
    }

    /// Transfer the display label, keeping bibliography classification and citation text.
    pub fn take_bibliography_label(&mut self) -> Option<ContentAttribute<'a>> {
        self.bibliography_label
            .take()
            .map(|label| ContentAttribute::new(label.content))
    }

    /// Replace the anchor span, keeping the locations within its prepared labels.
    pub fn set_location(&mut self, location: Location) {
        self.location = location;
    }

    /// Returns whether this anchor identifies a bibliography entry.
    #[must_use]
    pub fn is_bibliography(&self) -> bool {
        self.bibliography
    }

    pub(crate) fn bibliography_label(&self) -> Option<&[InlineNode<'a>]> {
        self.bibliography
            .then_some(self.bibliography_label.as_deref())
            .flatten()
            .map(|label| label.content.as_slice())
    }
}

/// A `TocEntry` represents a table of contents entry.
///
/// This is collected during parsing from Section.
///
/// Use [`Self::set_title`] to edit the stored title.
///
/// ```compile_fail,E0616
/// use acdc_parser::{Location, Title, TocEntry};
/// let mut entry = TocEntry::new("target", Title::default(), 1, Location::default());
/// entry.title = Title::default();
/// ```
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct TocEntry<'a> {
    /// Unique identifier for this section (used for anchor links)
    pub(crate) id: &'a str,
    /// Title of the section
    pub(crate) title: Title<'a>,
    /// Section level (1 for top-level, 2 for subsection, etc.)
    pub(crate) level: u8,
    /// Optional cross-reference label from `reftext=` or `[[id,xreflabel]]`.
    pub(crate) xreflabel: Option<ContentAttribute<'a>>,
    /// The section's structural category (special-section style, or `Normal`).
    /// Converters use it for presentation, such as appendix labels.
    pub(crate) kind: SectionKind,
    number: Option<SectionNumber>,
    /// Location of the section heading (the cross-reference target).
    pub(crate) location: Location,
}

impl<'a> TocEntry<'a> {
    pub(crate) fn for_section(
        id: &'a str,
        title: Title<'a>,
        level: u8,
        xreflabel: Option<ContentAttribute<'a>>,
        kind: SectionKind,
        location: Location,
    ) -> Self {
        Self {
            id,
            title,
            level,
            xreflabel,
            kind,
            number: None,
            location,
        }
    }

    pub(super) fn set_number(&mut self, number: Option<SectionNumber>) {
        self.number = number;
    }

    /// Return the assigned number without presentation punctuation or a signifier.
    ///
    /// Examples are `1`, `1.2`, `IV`, and `A.1`.
    #[must_use]
    pub fn number(&self) -> Option<&str> {
        self.number.as_ref().map(SectionNumber::as_str)
    }
}

impl Serialize for TocEntry<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(None)?;
        state.serialize_entry("id", &self.id)?;
        state.serialize_entry("title", &self.title)?;
        state.serialize_entry("level", &self.level)?;
        if self.xreflabel.is_some() {
            state.serialize_entry("xreflabel", &self.xreflabel)?;
        }
        if let Some(style) = self.kind.as_style() {
            state.serialize_entry("style", style)?;
        }
        state.end()
    }
}

/// Derived category and number for styled section references.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SectionReference {
    pub(crate) name: &'static str,
    pub(super) number: Option<SectionNumber>,
}

/// Reference metadata for a cross-reference target.
///
/// Collected during parsing into the `id → Reference` map on
/// [`DocumentRef::references`](crate::ast::DocumentRef::references), so a `<<id>>` reference resolves
/// to its target's text in O(1). Lookup keys can be aliases of the stored target.
///
/// Use [`Self::set_title`] to edit the stored title.
///
/// ```compile_fail,E0616
/// use acdc_parser::{Location, Reference};
/// let mut reference = Reference::new("target", Location::default());
/// reference.title = None;
/// ```
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Reference<'a> {
    pub(crate) target: &'a str,
    /// Optional cross-reference label from `reftext=` or `[[id,xreflabel]]`,
    /// copied from the target's parsed content without another inline parse.
    /// When set, it is the reference text; otherwise converters
    /// use `title` and, for captioned targets, `caption`.
    pub(crate) xreflabel: Option<ContentAttribute<'a>>,
    /// The target's title, when it has one. `None` for a referenceable element
    /// with no title, such as an untitled block or an inline link with an `id`
    /// attribute. A reference to such a target renders the literal `[id]`,
    /// unlike an ID that is absent from the catalog and therefore unresolved.
    pub(crate) title: Option<ContentAttribute<'a>>,
    /// Location of the target element (for navigation, e.g. LSP go-to-definition).
    pub(crate) location: Location,
    /// The target block's resolved caption, when it has one.
    pub(crate) caption: Option<Caption<'a>>,
    pub(crate) section: Option<SectionReference>,
    pub(crate) bibliography: bool,
    pub(crate) automatic_citation: bool,
}

impl Reference<'_> {
    /// The section's cross-reference category, or `None` for other targets.
    ///
    /// This is `part`, `chapter`, `section`, `appendix`, or a special section's
    /// style such as `preface`. It selects the `<name>-refsig` attribute.
    #[must_use]
    pub fn section_name(&self) -> Option<&str> {
        self.section.as_ref().map(|section| section.name)
    }

    /// The section's reference number, without a signifier or trailing punctuation.
    ///
    /// Returns `None` for unnumbered sections and non-section targets. A section
    /// beyond `sectnumlevels` can have a reference number without a heading number.
    ///
    /// ```
    /// use acdc_parser::{Options, parse};
    ///
    /// let parsed = parse("= Guide\n:sectnums:\n\n[#intro]\n== Introduction\n", &Options::default())?;
    /// let reference = parsed.document().references().get("intro").ok_or("missing section")?;
    /// assert_eq!(reference.section_name(), Some("section"));
    /// assert_eq!(reference.section_number(), Some("1"));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[must_use]
    pub fn section_number(&self) -> Option<&str> {
        self.section
            .as_ref()?
            .number
            .as_ref()
            .map(SectionNumber::as_str)
    }

    /// Returns whether this target is a bibliography entry.
    #[must_use]
    pub fn is_bibliography(&self) -> bool {
        self.bibliography
    }

    /// Returns whether the document contains an automatic citation to this target.
    #[must_use]
    pub fn has_automatic_citation(&self) -> bool {
        self.automatic_citation
    }
}

impl<'a> Reference<'a> {
    /// Construct a registered target without display content or derived classifications.
    #[must_use]
    pub fn new(target: &'a str, location: Location) -> Self {
        Self {
            target,
            xreflabel: None,
            title: None,
            location,
            caption: None,
            section: None,
            bibliography: false,
            automatic_citation: false,
        }
    }

    /// Replace the canonical target without renaming lookup keys or editing its owner.
    pub fn replace_target(&mut self, target: &'a str) -> &'a str {
        std::mem::replace(&mut self.target, target)
    }

    /// Replace prepared title content; `Some` preserves an explicitly empty title.
    pub fn set_title(&mut self, title: Option<ContentAttribute<'a>>) {
        self.title = title;
    }

    /// Remove the retained title, including a present empty title.
    #[must_use]
    pub fn take_title(&mut self) -> Option<ContentAttribute<'a>> {
        self.title.take()
    }

    /// Replace prepared reference text without editing the target element or map key.
    pub fn set_xreflabel(&mut self, label: Option<ContentAttribute<'a>>) {
        self.xreflabel = label;
    }

    /// Remove reference text, retaining its original source lifetime and empty content.
    #[must_use]
    pub fn take_xreflabel(&mut self) -> Option<ContentAttribute<'a>> {
        self.xreflabel.take()
    }

    /// Replace the retained caption without renumbering it or editing the target.
    pub fn set_caption(&mut self, caption: Option<Caption<'a>>) {
        self.caption = caption;
    }

    /// Remove the retained caption without changing other catalog records.
    #[must_use]
    pub fn take_caption(&mut self) -> Option<Caption<'a>> {
        self.caption.take()
    }

    /// Replace the recorded target span without relocating the target element.
    pub fn set_location(&mut self, location: Location) {
        self.location = location;
    }
}

impl<'a> TocEntry<'a> {
    /// Construct a normal prepared TOC entry without a reference label or assigned number.
    #[must_use]
    pub fn new(id: &'a str, title: Title<'a>, level: u8, location: Location) -> Self {
        Self::for_section(id, title, level, None, SectionKind::Normal, location)
    }

    /// Replace the ID without renaming reference-map keys or editing a heading.
    pub fn replace_id(&mut self, id: &'a str) -> &'a str {
        std::mem::replace(&mut self.id, id)
    }

    /// Replace prepared title content without editing its heading or reference target.
    pub fn set_title(&mut self, title: Title<'a>) {
        self.title = title;
    }

    /// Remove the title, retaining its source lifetime and presence information.
    #[must_use]
    pub fn take_title(&mut self) -> Title<'a> {
        std::mem::take(&mut self.title)
    }

    /// Replace prepared reference text without rebuilding registrations.
    pub fn set_xreflabel(&mut self, label: Option<ContentAttribute<'a>>) {
        self.xreflabel = label;
    }

    /// Remove reference text, retaining its original source lifetime and empty content.
    #[must_use]
    pub fn take_xreflabel(&mut self) -> Option<ContentAttribute<'a>> {
        self.xreflabel.take()
    }

    /// Replace the stored level without changing order, membership or assigned numbers.
    pub fn set_level(&mut self, level: u8) {
        self.level = level;
    }

    /// Replace the structural category without renumbering or editing a heading.
    pub fn set_kind(&mut self, kind: SectionKind) {
        self.kind = kind;
    }

    /// Replace the recorded heading span without relocating its content.
    pub fn set_location(&mut self, location: Location) {
        self.location = location;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn section_reference_queries_borrow_shared_numbers() -> Result<(), Box<dyn std::error::Error>> {
        let parsed = crate::parse(
            include_str!("../../fixtures/tests/section_reference_metadata.adoc"),
            &crate::Options::default(),
        )?;
        let document = parsed.document_storage();
        for id in ["part", "chapter", "section", "appendix"] {
            let reference = document.references.get(id).ok_or("missing reference")?;
            let cloned = reference.clone();
            let number = reference.section_number().ok_or("missing number")?;
            let cloned_number = cloned.section_number().ok_or("missing cloned number")?;
            let entry = document
                .toc_entries
                .iter()
                .find(|entry| entry.id == id)
                .ok_or("missing TOC entry")?;

            // JSON omits the reference catalog and cannot check shared storage.
            assert!(std::ptr::eq(number, cloned_number), "{id}");
            if let Some(heading_number) = entry.number() {
                assert!(std::ptr::eq(number, heading_number), "{id}");
            }
        }
        let plain = document.references.get("plain").ok_or("missing section")?;
        assert_eq!(plain.section_name(), Some("section"));
        assert_eq!(plain.section_number(), None);
        let block = document.references.get("block").ok_or("missing block")?;
        assert_eq!(block.section_name(), None);
        assert_eq!(block.section_number(), None);
        Ok(())
    }

    fn toc_entry(kind: SectionKind) -> TocEntry<'static> {
        TocEntry::for_section(
            "_intro",
            Title::default(),
            1,
            None,
            kind,
            Location::default(),
        )
    }

    #[test]
    fn toc_entry_serializes_special_style() -> Result<(), serde_json::Error> {
        let json = serde_json::to_value(toc_entry(SectionKind::Preface))?;
        assert_eq!(json.get("style").and_then(|v| v.as_str()), Some("preface"));
        Ok(())
    }

    #[test]
    fn toc_entry_omits_style_for_normal_section() -> Result<(), serde_json::Error> {
        let json = serde_json::to_value(toc_entry(SectionKind::Normal))?;
        assert!(json.get("style").is_none());
        Ok(())
    }
    #[test]
    fn bibliography_json_distinguishes_empty_and_automatic_labels() -> Result<(), serde_json::Error>
    {
        let mut anchor = Anchor::new("entry", Location::default());
        anchor.bibliography = true;
        let automatic = serde_json::to_value(&anchor)?;
        assert_eq!(automatic.get("bibliography"), Some(&json!(true)));
        assert!(automatic.get("bibliography_label").is_none());
        anchor.bibliography_label = Some(Box::new(BibliographyLabel {
            source: Some("unexported cache"),
            content: Vec::new(),
        }));
        let direct = serde_json::to_value(&anchor)?;
        let node = serde_json::to_value(InlineNode::InlineAnchor(anchor))?;
        assert_eq!(direct.get("bibliography_label"), Some(&json!([])));
        assert_eq!(node.get("bibliography_label"), Some(&json!([])));
        assert!(!direct.to_string().contains("unexported cache"));
        Ok(())
    }
}

#[cfg(test)]
mod prepared_anchor_tests {
    use super::{Anchor, BibliographyLabel};
    use crate::{ContentAttribute, Location};

    #[test]
    fn prepared_reference_edits_discard_previous_source_hints() {
        let mut anchor = Anchor::new("entry", Location::default());
        anchor.reftext_source = Some(7);
        anchor.set_reftext(Some(ContentAttribute::default()));
        assert!(anchor.reftext_source.is_none());
        anchor.reftext_source = Some(9);
        assert_eq!(anchor.take_reftext(), Some(ContentAttribute::default()));
        assert!(anchor.reftext_source.is_none());
        anchor.reftext_source = Some(11);
        let anchor = anchor.with_xreflabel(Some(ContentAttribute::default()));
        assert!(anchor.reftext_source.is_none());
    }

    #[test]
    fn prepared_display_labels_do_not_reuse_authored_source() {
        let mut anchor = Anchor::new("entry", Location::default());
        anchor.bibliography = true;
        anchor.bibliography_label = Some(Box::new(BibliographyLabel {
            source: Some("old authored label"),
            content: Vec::new(),
        }));
        anchor.set_bibliography(Some(ContentAttribute::default()));
        assert!(
            anchor
                .bibliography_label
                .as_ref()
                .is_some_and(|label| label.source.is_none())
        );
        anchor.clear_bibliography();
        assert!(anchor.bibliography_label.is_none());
        assert!(!anchor.is_bibliography());
    }
}
