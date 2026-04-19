use std::fmt::Display;

use bumpalo::Bump;
use serde::ser::{Serialize, SerializeMap, Serializer};

use crate::{Block, BlockMetadata, InlineNode, Location, model::inlines::converter};

use super::title::Title;

/// A `SectionLevel` represents a section depth in a document.
pub type SectionLevel = u8;

/// A `Section` represents a section in a document.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Section<'a> {
    pub metadata: BlockMetadata<'a>,
    pub title: Title<'a>,
    pub level: SectionLevel,
    pub content: Vec<Block<'a>>,
    pub location: Location,
}

impl<'a> Section<'a> {
    /// Create a new section with the given title, level, content, and location.
    #[must_use]
    pub fn new(
        title: Title<'a>,
        level: SectionLevel,
        content: Vec<Block<'a>>,
        location: Location,
    ) -> Self {
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
    pub fn with_metadata(mut self, metadata: BlockMetadata<'a>) -> Self {
        self.metadata = metadata;
        self
    }
}

/// A `SafeId` represents a sanitised ID.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum SafeId<'a> {
    Generated(&'a str),
    Explicit(&'a str),
}

impl Display for SafeId<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SafeId::Generated(id) => write!(f, "_{id}"),
            SafeId::Explicit(id) => write!(f, "{id}"),
        }
    }
}

impl<'a> SafeId<'a> {
    /// Return the display-equivalent `&'a str` for this safe id without going
    /// through `format!`/`to_string()`. `Generated` variants are prepended
    /// with `_` into the arena; `Explicit` is returned unchanged.
    #[must_use]
    pub(crate) fn as_arena_str(&self, arena: &'a Bump) -> &'a str {
        match self {
            SafeId::Generated(id) => {
                let mut s = bumpalo::collections::String::new_in(arena);
                s.push('_');
                s.push_str(id);
                s.into_bump_str()
            }
            SafeId::Explicit(id) => id,
        }
    }
}

impl<'a> Section<'a> {
    /// Build a section id from title text: lowercase, non-alphanumerics
    /// (except whitespace, `-`, `.`, `_`) dropped, survivors joined with `_`,
    /// consecutive `_` collapsed, trailing `_` trimmed. Single pass.
    fn id_from_title(title: &[InlineNode<'a>]) -> String {
        let mut title_text = String::new();
        // `write_inlines` on a `String` is infallible.
        let _ = converter::write_inlines(&mut title_text, title);
        let mut out = String::with_capacity(title_text.len());
        let mut last_was_underscore = false;
        for c in title_text.to_lowercase().chars() {
            let mapped = if c.is_alphanumeric() {
                Some(c)
            } else if c.is_whitespace() || c == '-' || c == '.' || c == '_' {
                Some('_')
            } else {
                None
            };
            let Some(ch) = mapped else { continue };
            if ch == '_' {
                if !last_was_underscore {
                    out.push('_');
                }
                last_was_underscore = true;
            } else {
                out.push(ch);
                last_was_underscore = false;
            }
        }
        while out.ends_with('_') {
            out.pop();
        }
        out
    }

    /// Pick the explicit id if metadata provides one, else None. Shared by
    /// the arena-returning and `String`-returning variants below.
    fn explicit_id(metadata: &BlockMetadata<'a>) -> Option<&'a str> {
        if let Some(anchor) = &metadata.id {
            return Some(anchor.id);
        }
        metadata.anchors.last().map(|a| a.id)
    }

    /// Generate a section ID based on its title and metadata.
    ///
    /// Checks in order: explicit `metadata.id` (e.g. `[id=foo]`), then the last
    /// entry in `metadata.anchors` (e.g. `[[foo]]`), otherwise auto-generates
    /// one from the title and interns it into the supplied arena.
    #[must_use]
    pub(crate) fn generate_id(
        arena: &'a Bump,
        metadata: &BlockMetadata<'a>,
        title: &[InlineNode<'a>],
    ) -> SafeId<'a> {
        match Self::explicit_id(metadata) {
            Some(id) => SafeId::Explicit(id),
            None => SafeId::Generated(arena.alloc_str(&Self::id_from_title(title))),
        }
    }

    /// Generate a section ID based on its title and metadata, returning a
    /// `String` directly.
    ///
    /// Unlike [`generate_id`](Self::generate_id), this variant does not require
    /// a parser arena — useful for consumers (converters, LSP) that only need
    /// the textual ID and do not have access to the parser's internal arena.
    ///
    /// Returns the `Display`-formatted form (prefixed with `_` for generated
    /// IDs) matching `safe_id.to_string()`.
    #[must_use]
    pub fn generate_id_string(metadata: &BlockMetadata<'a>, title: &[InlineNode<'a>]) -> String {
        match Self::explicit_id(metadata) {
            Some(id) => id.to_string(),
            None => format!("_{}", Self::id_from_title(title)),
        }
    }
}

impl Serialize for Section<'_> {
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
            content: "This is a title.",
            location: Location::default(),
            escaped: false,
        })];
        assert_eq!(
            Section::id_from_title(inlines),
            "this_is_a_title".to_string()
        );
        let inlines: &[InlineNode] = &[InlineNode::PlainText(Plain {
            content: "This is a----title.",
            location: Location::default(),
            escaped: false,
        })];
        assert_eq!(
            Section::id_from_title(inlines),
            "this_is_a_title".to_string()
        );
    }

    #[test]
    fn test_id_from_title_preserves_underscores() {
        let inlines: &[InlineNode] = &[InlineNode::PlainText(Plain {
            content: "CHART_BOT",
            location: Location::default(),
            escaped: false,
        })];
        assert_eq!(Section::id_from_title(inlines), "chart_bot".to_string());
        let inlines: &[InlineNode] = &[InlineNode::PlainText(Plain {
            content: "haiku_robot",
            location: Location::default(),
            escaped: false,
        })];
        assert_eq!(Section::id_from_title(inlines), "haiku_robot".to_string());
        let inlines: &[InlineNode] = &[InlineNode::PlainText(Plain {
            content: "meme_transcriber",
            location: Location::default(),
            escaped: false,
        })];
        assert_eq!(
            Section::id_from_title(inlines),
            "meme_transcriber".to_string()
        );
    }

    #[test]
    fn test_section_generate_id() {
        let arena = Bump::new();
        let inlines: &[InlineNode] = &[InlineNode::PlainText(Plain {
            content: "This is a b__i__g title.",
            location: Location::default(),
            escaped: false,
        })];
        // metadata has an empty id
        let metadata = BlockMetadata::default();
        assert_eq!(
            Section::generate_id(&arena, &metadata, inlines),
            SafeId::Generated("this_is_a_b_i_g_title")
        );

        // metadata has a specific id in metadata.id
        let metadata = BlockMetadata {
            id: Some(Anchor {
                id: "custom_id",
                xreflabel: None,
                location: Location::default(),
            }),
            ..Default::default()
        };
        assert_eq!(
            Section::generate_id(&arena, &metadata, inlines),
            SafeId::Explicit("custom_id")
        );

        // metadata has anchor in metadata.anchors (from [[id]] or [#id] syntax)
        let metadata = BlockMetadata {
            anchors: vec![Anchor {
                id: "anchor_id",
                xreflabel: None,
                location: Location::default(),
            }],
            ..Default::default()
        };
        assert_eq!(
            Section::generate_id(&arena, &metadata, inlines),
            SafeId::Explicit("anchor_id")
        );

        // with multiple anchors, the last one is used (matches asciidoctor behavior)
        let metadata = BlockMetadata {
            anchors: vec![
                Anchor {
                    id: "first_anchor",
                    xreflabel: None,
                    location: Location::default(),
                },
                Anchor {
                    id: "last_anchor",
                    xreflabel: None,
                    location: Location::default(),
                },
            ],
            ..Default::default()
        };
        assert_eq!(
            Section::generate_id(&arena, &metadata, inlines),
            SafeId::Explicit("last_anchor")
        );

        // metadata.id takes precedence over metadata.anchors
        let metadata = BlockMetadata {
            id: Some(Anchor {
                id: "from_id",
                xreflabel: None,
                location: Location::default(),
            }),
            anchors: vec![Anchor {
                id: "from_anchors",
                xreflabel: None,
                location: Location::default(),
            }],
            ..Default::default()
        };
        assert_eq!(
            Section::generate_id(&arena, &metadata, inlines),
            SafeId::Explicit("from_id")
        );
    }
}
