use std::fmt::Display;

use serde::ser::{Serialize, SerializeMap, Serializer};

use crate::{Block, BlockMetadata, InlineNode, Location, model::inlines::converter};

use super::title::Title;

/// A `SectionLevel` represents a section depth in a document.
pub type SectionLevel = u8;

/// A `Section` represents a section in a document.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Section {
    pub metadata: BlockMetadata,
    pub title: Title,
    pub level: SectionLevel,
    pub content: Vec<Block>,
    pub location: Location,
}

impl Section {
    /// Create a new section with the given title, level, content, and location.
    #[must_use]
    pub fn new(title: Title, level: SectionLevel, content: Vec<Block>, location: Location) -> Self {
        Self {
            metadata: BlockMetadata::default(),
            title,
            level,
            content,
            location,
        }
    }

    /// Set the metadata.
    #[must_use]
    pub fn with_metadata(mut self, metadata: BlockMetadata) -> Self {
        self.metadata = metadata;
        self
    }
}

/// A `SafeId` represents a sanitised ID.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum SafeId {
    Generated(String),
    Explicit(String),
}

impl Display for SafeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SafeId::Generated(id) => write!(f, "_{id}"),
            SafeId::Explicit(id) => write!(f, "{id}"),
        }
    }
}

impl Section {
    fn id_from_title(title: &[InlineNode]) -> String {
        // Generate ID from title
        let title_text = converter::inlines_to_string(title);
        let mut id = title_text
            .to_lowercase()
            .chars()
            .filter_map(|c| {
                if c.is_alphanumeric() {
                    Some(c)
                } else if c.is_whitespace() || c == '-' || c == '.' {
                    Some('_')
                } else {
                    None
                }
            })
            .collect::<String>();

        // Trim trailing underscores
        id = id.trim_end_matches('_').to_string();

        // Collapse consecutive underscores into single underscore
        //
        // We'll build a (String, bool) tuple
        // The bool tracks: "was the last char an underscore?"
        let (collapsed, _) = id.chars().fold(
            (String::with_capacity(id.len()), false), // (new_string, last_was_underscore)
            |(mut acc_string, last_was_underscore), current_char| {
                if current_char == '_' {
                    if !last_was_underscore {
                        acc_string.push('_'); // Only add if last char wasn't one
                    }
                    (acc_string, true) // Mark last_was_underscore as true
                } else {
                    acc_string.push(current_char);
                    (acc_string, false) // Mark last_was_underscore as false
                }
            },
        );
        collapsed
    }

    /// Generate a section ID based on its title and metadata.
    ///
    /// This function checks for explicit IDs in the following order:
    /// 1. `metadata.id` - from attribute list syntax like `[id=foo]`
    /// 2. `metadata.anchors` - from anchor syntax like `[[foo]]` or `[#foo]`
    ///
    /// If no explicit ID is found, it generates one from the title by converting
    /// to lowercase, replacing spaces and hyphens with underscores, and removing
    /// non-alphanumeric characters.
    #[must_use]
    pub fn generate_id(metadata: &BlockMetadata, title: &[InlineNode]) -> SafeId {
        // Check explicit ID from attribute list first
        if let Some(anchor) = &metadata.id {
            return SafeId::Explicit(anchor.id.clone());
        }
        // Check last anchor from block metadata lines (e.g., [[id]] or [#id])
        // asciidoctor uses the last anchor when multiple are present
        if let Some(anchor) = metadata.anchors.last() {
            return SafeId::Explicit(anchor.id.clone());
        }
        // Fall back to auto-generated ID from title
        let id = Self::id_from_title(title);
        SafeId::Generated(id)
    }
}

impl Serialize for Section {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(None)?;
        state.serialize_entry("name", "section")?;
        state.serialize_entry("type", "block")?;
        state.serialize_entry("title", &self.title)?;
        state.serialize_entry("level", &self.level)?;
        if !self.metadata.is_default() {
            state.serialize_entry("metadata", &self.metadata)?;
        }
        if !self.content.is_empty() {
            state.serialize_entry("blocks", &self.content)?;
        }
        state.serialize_entry("location", &self.location)?;
        state.end()
    }
}

#[cfg(test)]
mod tests {
    use crate::{Anchor, Plain};

    use super::*;

    #[test]
    fn test_id_from_title() {
        let inlines: &[InlineNode] = &[InlineNode::PlainText(Plain {
            content: "This is a title.".to_string(),
            location: Location::default(),
        })];
        assert_eq!(
            Section::id_from_title(inlines),
            "this_is_a_title".to_string()
        );
        let inlines: &[InlineNode] = &[InlineNode::PlainText(Plain {
            content: "This is a----title.".to_string(),
            location: Location::default(),
        })];
        assert_eq!(
            Section::id_from_title(inlines),
            "this_is_a_title".to_string()
        );
    }

    #[test]
    fn test_section_generate_id() {
        let inlines: &[InlineNode] = &[InlineNode::PlainText(Plain {
            content: "This is a b__i__g title.".to_string(),
            location: Location::default(),
        })];
        // metadata has an empty id
        let metadata = BlockMetadata::default();
        assert_eq!(
            Section::generate_id(&metadata, inlines),
            SafeId::Generated("this_is_a_big_title".to_string())
        );

        // metadata has a specific id in metadata.id
        let metadata = BlockMetadata {
            id: Some(Anchor {
                id: "custom_id".to_string(),
                xreflabel: None,
                location: Location::default(),
            }),
            ..Default::default()
        };
        assert_eq!(
            Section::generate_id(&metadata, inlines),
            SafeId::Explicit("custom_id".to_string())
        );

        // metadata has anchor in metadata.anchors (from [[id]] or [#id] syntax)
        let metadata = BlockMetadata {
            anchors: vec![Anchor {
                id: "anchor_id".to_string(),
                xreflabel: None,
                location: Location::default(),
            }],
            ..Default::default()
        };
        assert_eq!(
            Section::generate_id(&metadata, inlines),
            SafeId::Explicit("anchor_id".to_string())
        );

        // with multiple anchors, the last one is used (matches asciidoctor behavior)
        let metadata = BlockMetadata {
            anchors: vec![
                Anchor {
                    id: "first_anchor".to_string(),
                    xreflabel: None,
                    location: Location::default(),
                },
                Anchor {
                    id: "last_anchor".to_string(),
                    xreflabel: None,
                    location: Location::default(),
                },
            ],
            ..Default::default()
        };
        assert_eq!(
            Section::generate_id(&metadata, inlines),
            SafeId::Explicit("last_anchor".to_string())
        );

        // metadata.id takes precedence over metadata.anchors
        let metadata = BlockMetadata {
            id: Some(Anchor {
                id: "from_id".to_string(),
                xreflabel: None,
                location: Location::default(),
            }),
            anchors: vec![Anchor {
                id: "from_anchors".to_string(),
                xreflabel: None,
                location: Location::default(),
            }],
            ..Default::default()
        };
        assert_eq!(
            Section::generate_id(&metadata, inlines),
            SafeId::Explicit("from_id".to_string())
        );
    }
}
