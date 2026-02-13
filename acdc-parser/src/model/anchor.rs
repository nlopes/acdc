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
pub struct Anchor {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub xreflabel: Option<String>,
    pub location: Location,
}

impl Anchor {
    /// Create a new anchor with the given ID and location.
    #[must_use]
    pub fn new(id: String, location: Location) -> Self {
        Self {
            id,
            xreflabel: None,
            location,
        }
    }

    /// Set the cross-reference label.
    #[must_use]
    pub fn with_xreflabel(mut self, xreflabel: Option<String>) -> Self {
        self.xreflabel = xreflabel;
        self
    }
}

/// A `TocEntry` represents a table of contents entry.
///
/// This is collected during parsing from Section.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct TocEntry {
    /// Unique identifier for this section (used for anchor links)
    pub id: String,
    /// Title of the section
    pub title: Title,
    /// Section level (1 for top-level, 2 for subsection, etc.)
    pub level: u8,
    /// Optional cross-reference label (from `[[id,xreflabel]]` syntax)
    pub xreflabel: Option<String>,
    /// Whether this section should be numbered when `sectnums` is enabled.
    ///
    /// False for special section styles like `[bibliography]`, `[glossary]`, etc.
    pub numbered: bool,
    /// Optional style from block metadata (e.g., "appendix", "bibliography").
    pub style: Option<String>,
}

impl Serialize for TocEntry {
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
