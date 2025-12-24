//! Anchor and reference types for `AsciiDoc` documents.

use serde::{Deserialize, Serialize};

use super::inlines::InlineNode;
use super::location::Location;

/// An `Anchor` represents an anchor in a document.
///
/// An anchor is a reference point in a document that can be linked to.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct TocEntry {
    /// Unique identifier for this section (used for anchor links)
    pub id: String,
    /// Title of the section
    pub title: Vec<InlineNode>,
    /// Section level (1 for top-level, 2 for subsection, etc.)
    pub level: u8,
    /// Optional cross-reference label (from `[[id,xreflabel]]` syntax)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub xreflabel: Option<String>,
}
