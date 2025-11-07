use std::fmt::Display;

use serde::ser::{Serialize, SerializeMap, Serializer};

use crate::{Block, BlockMetadata, InlineNode, Location, model::inlines::converter};

/// A `SectionLevel` represents a section depth in a document.
pub type SectionLevel = u8;

/// A `Section` represents a section in a document.
#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    pub metadata: BlockMetadata,
    pub title: Vec<InlineNode>,
    pub level: SectionLevel,
    pub content: Vec<Block>,
    pub location: Location,
}

/// A `SafeId` represents a sanitised ID.
#[derive(Clone, Debug, PartialEq)]
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
    fn id_from_inlines(title: &[InlineNode]) -> String {
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
    /// This function checks if the section has an explicit ID in its metadata. If not, it
    /// generates an ID from the title by converting it to lowercase, replacing spaces and
    /// hyphens with underscores, and removing non-alphanumeric characters.
    #[must_use]
    pub fn generate_id(metadata: &BlockMetadata, title: &[InlineNode]) -> SafeId {
        // Check if section has an explicit ID in metadata
        if let Some(anchor) = &metadata.id {
            return SafeId::Explicit(anchor.id.clone());
        }
        let id = Self::id_from_inlines(title);
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
    fn test_id_from_inlines() {
        let inlines: &[InlineNode] = &[InlineNode::PlainText(Plain {
            content: "This is a title.".to_string(),
            location: Location::default(),
        })];
        assert_eq!(
            Section::id_from_inlines(inlines),
            "this_is_a_title".to_string()
        );
        let inlines: &[InlineNode] = &[InlineNode::PlainText(Plain {
            content: "This is a----title.".to_string(),
            location: Location::default(),
        })];
        assert_eq!(
            Section::id_from_inlines(inlines),
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

        // metadata has a specific id
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
    }
}
