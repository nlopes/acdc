//! Anchor and reference types for `AsciiDoc` documents.

use serde::{
    Serialize,
    ser::{SerializeMap, Serializer},
};

use super::{
    caption::Caption,
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
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[non_exhaustive]
pub struct Anchor<'a> {
    pub id: &'a str,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub xreflabel: Option<&'a str>,
    pub location: Location,
    #[serde(skip)]
    pub(crate) bibliography: bool,
    #[serde(skip)]
    pub(crate) bibliography_label: Option<Box<BibliographyLabel<'a>>>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BibliographyLabel<'a> {
    pub(crate) source: Option<&'a str>,
    pub(crate) content: Vec<InlineNode<'a>>,
}

impl<'a> Anchor<'a> {
    /// Create a new anchor with the given ID and location.
    #[must_use]
    pub fn new(id: &'a str, location: Location) -> Self {
        Self {
            id,
            xreflabel: None,
            location,
            bibliography: false,
            bibliography_label: None,
        }
    }

    /// Set the cross-reference label.
    #[must_use]
    pub fn with_xreflabel(mut self, xreflabel: Option<&'a str>) -> Self {
        self.xreflabel = xreflabel;
        self
    }

    /// Returns whether this anchor identifies a bibliography entry.
    #[must_use]
    pub fn is_bibliography(&self) -> bool {
        self.bibliography
    }

    /// Return the entry label after its source-position inline substitutions.
    ///
    /// Citation text remains separate in [`Self::xreflabel`]. Returns `None`
    /// for ordinary anchors and entries that use their ID as the label.
    #[must_use]
    pub fn bibliography_label(&self) -> Option<&[InlineNode<'a>]> {
        self.bibliography
            .then_some(self.bibliography_label.as_deref())
            .flatten()
            .map(|label| label.content.as_slice())
    }
}

/// A `TocEntry` represents a table of contents entry.
///
/// This is collected during parsing from Section.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct TocEntry<'a> {
    /// Unique identifier for this section (used for anchor links)
    pub id: &'a str,
    /// Title of the section
    pub title: Title<'a>,
    /// Section level (1 for top-level, 2 for subsection, etc.)
    pub level: u8,
    /// Optional cross-reference label from `reftext=` or `[[id,xreflabel]]`.
    pub xreflabel: Option<&'a str>,
    /// The section's structural category (special-section style, or `Normal`).
    /// Converters use it for presentation, such as appendix labels.
    pub kind: SectionKind,
    number: Option<SectionNumber>,
    /// Location of the section heading (the cross-reference target).
    pub location: Location,
}

impl<'a> TocEntry<'a> {
    pub(crate) fn for_section(
        id: &'a str,
        title: Title<'a>,
        level: u8,
        xreflabel: Option<&'a str>,
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
/// [`Document::references`](crate::Document), so a `<<id>>` reference resolves
/// to its target's text in O(1). The id is the map key.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Reference<'a> {
    /// Optional cross-reference label from `reftext=` or `[[id,xreflabel]]`,
    /// parsed as inline content. A label carries formatting, so `*Bold* label`
    /// renders bold. When set, it is the reference text; otherwise converters
    /// use `title` and, for captioned targets, `caption`.
    pub xreflabel: Option<Vec<InlineNode<'a>>>,
    /// The target's title, when it has one. `None` for a referenceable element
    /// with no title, such as an untitled block or an inline link with an `id`
    /// attribute. A reference to such a target renders the literal `[id]`,
    /// unlike an ID that is absent from the catalog and therefore unresolved.
    pub title: Option<Title<'a>>,
    /// Location of the target element (for navigation, e.g. LSP go-to-definition).
    pub location: Location,
    /// The target block's resolved caption, when it has one.
    pub caption: Option<Caption<'a>>,
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
    /// let reference = parsed.document().references.get("intro").ok_or("missing section")?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn section_reference_queries_borrow_shared_numbers() -> Result<(), Box<dyn std::error::Error>> {
        let parsed = crate::parse(
            include_str!("../../fixtures/tests/section_reference_metadata.adoc"),
            &crate::Options::default(),
        )?;
        let document = parsed.document();
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
}
