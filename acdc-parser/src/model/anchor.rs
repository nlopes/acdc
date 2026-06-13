//! Anchor and reference types for `AsciiDoc` documents.

use serde::{
    Serialize,
    ser::{SerializeMap, Serializer},
};

use super::location::Location;
use super::title::Title;

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
}

impl<'a> Anchor<'a> {
    /// Create a new anchor with the given ID and location.
    #[must_use]
    pub fn new(id: &'a str, location: Location) -> Self {
        Self {
            id,
            xreflabel: None,
            location,
        }
    }

    /// Set the cross-reference label.
    #[must_use]
    pub fn with_xreflabel(mut self, xreflabel: Option<&'a str>) -> Self {
        self.xreflabel = xreflabel;
        self
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
    /// Optional cross-reference label (from `[[id,xreflabel]]` syntax)
    pub xreflabel: Option<&'a str>,
    /// Whether this section should be numbered when `sectnums` is enabled.
    ///
    /// False for special section styles like `[bibliography]`, `[glossary]`, etc.
    pub numbered: bool,
    /// Optional style from block metadata (e.g., "appendix", "bibliography").
    pub style: Option<&'a str>,
    /// Location of the section heading (the cross-reference target).
    pub location: Location,
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
        if self.style.is_some() {
            state.serialize_entry("style", &self.style)?;
        }
        state.end()
    }
}

/// The resolved text of a cross-reference target (a section or a titled block).
///
/// Collected during parsing into the `id → Reference` map on
/// [`Document::references`](crate::Document), so a `<<id>>` reference resolves
/// to its target's text in O(1). The id is the map key.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Reference<'a> {
    /// Optional cross-reference label (from `[[id,xreflabel]]` syntax). When
    /// set, it is the reference text; otherwise `title` is used.
    pub xreflabel: Option<&'a str>,
    /// The target's title (section or block title), when it has one. `None` for
    /// a referenceable element with no title (e.g. an untitled block with an
    /// `[[id]]`): such a reference exists but has no reference text, so an
    /// `<<id>>` to it renders the literal `[id]` — distinct from an id that is
    /// absent from the catalog entirely (an unresolved/broken reference).
    pub title: Option<Title<'a>>,
    /// Location of the target element (for navigation, e.g. LSP go-to-definition).
    pub location: Location,
}
