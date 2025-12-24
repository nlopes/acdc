//! Anchor and reference types for `AsciiDoc` documents.

use serde::{
    Deserialize, Serialize,
    de::{self, Deserializer, MapAccess, Visitor},
    ser::{SerializeMap, Serializer},
};

use super::inlines::InlineNode;
use super::location::Location;
use super::title::Title;

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
        state.end()
    }
}

impl<'de> Deserialize<'de> for TocEntry {
    fn deserialize<D>(deserializer: D) -> Result<TocEntry, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct TocEntryVisitor;

        impl<'de> Visitor<'de> for TocEntryVisitor {
            type Value = TocEntry;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("struct TocEntry")
            }

            fn visit_map<V>(self, mut map: V) -> Result<TocEntry, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut id = None;
                let mut title: Option<Vec<InlineNode>> = None;
                let mut level = None;
                let mut xreflabel = None;

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "id" => id = Some(map.next_value()?),
                        "title" => title = Some(map.next_value()?),
                        "level" => level = Some(map.next_value()?),
                        "xreflabel" => xreflabel = Some(map.next_value()?),
                        _ => {
                            let _ = map.next_value::<de::IgnoredAny>()?;
                        }
                    }
                }

                Ok(TocEntry {
                    id: id.ok_or_else(|| de::Error::missing_field("id"))?,
                    title: title.unwrap_or_default().into(),
                    level: level.ok_or_else(|| de::Error::missing_field("level"))?,
                    xreflabel,
                })
            }
        }

        deserializer.deserialize_map(TocEntryVisitor)
    }
}
