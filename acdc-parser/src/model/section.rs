use std::fmt::Display;

use bumpalo::Bump;
use serde::ser::{Serialize, SerializeMap, Serializer};

use crate::{Block, BlockMetadata, InlineMacro, InlineNode, Location};

use super::title::Title;

/// A `SectionLevel` represents a section depth in a document.
pub type SectionLevel = u8;

/// The structural category of a section.
///
/// `AsciiDoc` designates certain section styles as *special sections* — built-in
/// styles for specialized front matter and back matter (preface, glossary, …).
/// `SectionKind` captures that category, derived from the section's style;
/// `Normal` is any ordinary (non-special) section.
///
/// This is a purely structural classification: it carries no rendering decision
/// of its own. Converters consult it — for example, to exclude special sections
/// from `:sectnums:` numbering.
///
/// `#[non_exhaustive]` so further kinds (e.g. `partintro`, `acknowledgments`)
/// can be added without breaking downstream matches.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum SectionKind {
    /// An ordinary section (the default).
    #[default]
    Normal,
    Preface,
    Abstract,
    Dedication,
    Colophon,
    Appendix,
    Glossary,
    Bibliography,
    Index,
}

impl SectionKind {
    /// Classify a section from its block style (e.g. `[preface]`). Unknown or
    /// absent styles are `Normal`.
    #[must_use]
    pub fn from_style(style: Option<&str>) -> Self {
        match style {
            Some("preface") => SectionKind::Preface,
            Some("abstract") => SectionKind::Abstract,
            Some("dedication") => SectionKind::Dedication,
            Some("colophon") => SectionKind::Colophon,
            Some("appendix") => SectionKind::Appendix,
            Some("glossary") => SectionKind::Glossary,
            Some("bibliography") => SectionKind::Bibliography,
            Some("index") => SectionKind::Index,
            _ => SectionKind::Normal,
        }
    }

    /// The block style string this kind corresponds to (e.g. `"preface"`), or
    /// `None` for `Normal`. Inverse of [`from_style`](Self::from_style).
    #[must_use]
    pub fn as_style(self) -> Option<&'static str> {
        match self {
            SectionKind::Normal => None,
            SectionKind::Preface => Some("preface"),
            SectionKind::Abstract => Some("abstract"),
            SectionKind::Dedication => Some("dedication"),
            SectionKind::Colophon => Some("colophon"),
            SectionKind::Appendix => Some("appendix"),
            SectionKind::Glossary => Some("glossary"),
            SectionKind::Bibliography => Some("bibliography"),
            SectionKind::Index => Some("index"),
        }
    }

    /// Whether this is a special (front/back-matter) section. True for every
    /// kind except `Normal`. Structural only — implies nothing about rendering.
    #[must_use]
    pub fn is_special(self) -> bool {
        !matches!(self, SectionKind::Normal)
    }
}

/// A `Section` represents a section in a document.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Section<'a> {
    pub metadata: BlockMetadata<'a>,
    pub title: Title<'a>,
    pub level: SectionLevel,
    pub content: Vec<Block<'a>>,
    /// The section's structural category (special-section style, or `Normal`).
    pub kind: SectionKind,
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
            kind: SectionKind::Normal,
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
            // An empty generated slug (a title with no id-able characters)
            // yields an empty id rather than a bare `_`, matching asciidoctor.
            SafeId::Generated("") => Ok(()),
            SafeId::Generated(id) => write!(f, "_{id}"),
            SafeId::Explicit(id) => write!(f, "{id}"),
        }
    }
}

impl<'a> SafeId<'a> {
    /// Return the display-equivalent `&'a str` for this safe id without going
    /// through `format!`/`to_string()`. `Generated` variants are prepended
    /// with `_` into the arena; an empty generated slug stays empty; `Explicit`
    /// is returned unchanged.
    #[must_use]
    pub(crate) fn as_arena_str(&self, arena: &'a Bump) -> &'a str {
        match self {
            SafeId::Generated("") => "",
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
    /// consecutive `_` collapsed, leading and trailing `_` trimmed. Single
    /// pass. The `_` prefix on a generated id is added by `SafeId`, so leading
    /// separators here must be squeezed away to avoid a doubled `__`.
    fn id_from_title(title: &[InlineNode<'a>]) -> String {
        let mut out = String::new();
        // Start as if a `_` was just emitted so leading separators are dropped.
        let mut last_was_underscore = true;
        Self::append_id_from_inlines(title, &mut out, &mut last_was_underscore);
        while out.ends_with('_') {
            out.pop();
        }
        out
    }

    fn append_id_from_inlines(
        inlines: &[InlineNode<'a>],
        out: &mut String,
        last_was_underscore: &mut bool,
    ) {
        for node in inlines {
            match node {
                InlineNode::PlainText(text) => {
                    Self::append_id_text(text.content, out, last_was_underscore);
                }
                InlineNode::RawText(text) => {
                    Self::append_id_text(text.content, out, last_was_underscore);
                }
                InlineNode::VerbatimText(text) => {
                    Self::append_id_text(text.content, out, last_was_underscore);
                }
                InlineNode::BoldText(text) => {
                    Self::append_id_from_inlines(&text.content, out, last_was_underscore);
                }
                InlineNode::ItalicText(text) => {
                    Self::append_id_from_inlines(&text.content, out, last_was_underscore);
                }
                InlineNode::MonospaceText(text) => {
                    Self::append_id_from_inlines(&text.content, out, last_was_underscore);
                }
                InlineNode::HighlightText(text) => {
                    Self::append_id_from_inlines(&text.content, out, last_was_underscore);
                }
                InlineNode::SubscriptText(text) => {
                    Self::append_id_from_inlines(&text.content, out, last_was_underscore);
                }
                InlineNode::SuperscriptText(text) => {
                    Self::append_id_from_inlines(&text.content, out, last_was_underscore);
                }
                InlineNode::CurvedQuotationText(text) => {
                    Self::append_id_from_inlines(&text.content, out, last_was_underscore);
                }
                InlineNode::CurvedApostropheText(text) => {
                    Self::append_id_from_inlines(&text.content, out, last_was_underscore);
                }
                InlineNode::StandaloneCurvedApostrophe(_) => {
                    Self::append_id_text("'", out, last_was_underscore);
                }
                InlineNode::LineBreak(_) => {
                    Self::append_id_text(" ", out, last_was_underscore);
                }
                InlineNode::InlineAnchor(_) => {}
                InlineNode::Macro(macro_node) => {
                    Self::append_id_from_macro(macro_node, out, last_was_underscore);
                }
                InlineNode::CalloutRef(callout) => {
                    Self::append_id_text(
                        &format!("<{}>", callout.number),
                        out,
                        last_was_underscore,
                    );
                }
            }
        }
    }

    fn append_id_from_macro(
        macro_node: &InlineMacro<'a>,
        out: &mut String,
        last_was_underscore: &mut bool,
    ) {
        match macro_node {
            InlineMacro::Link(link) => {
                if link.text.is_empty() {
                    Self::append_id_text(&link.target.to_string(), out, last_was_underscore);
                } else {
                    Self::append_id_from_inlines(&link.text, out, last_was_underscore);
                }
            }
            InlineMacro::Url(url) => {
                if url.text.is_empty() {
                    Self::append_id_text(&url.target.to_string(), out, last_was_underscore);
                } else {
                    Self::append_id_from_inlines(&url.text, out, last_was_underscore);
                }
            }
            InlineMacro::Mailto(mailto) => {
                if mailto.text.is_empty() {
                    Self::append_id_text(&mailto.target.to_string(), out, last_was_underscore);
                } else {
                    Self::append_id_from_inlines(&mailto.text, out, last_was_underscore);
                }
            }
            InlineMacro::Autolink(autolink) => {
                Self::append_id_text(&autolink.url.to_string(), out, last_was_underscore);
            }
            InlineMacro::CrossReference(xref) => {
                if xref.text.is_empty() {
                    Self::append_id_text(xref.target, out, last_was_underscore);
                } else {
                    Self::append_id_from_inlines(&xref.text, out, last_was_underscore);
                }
            }
            InlineMacro::IndexTerm(index_term) if index_term.is_visible() => {
                Self::append_id_text(index_term.term(), out, last_was_underscore);
            }
            InlineMacro::Pass(pass) => {
                Self::append_id_text(pass.text.unwrap_or_default(), out, last_was_underscore);
            }
            InlineMacro::Keyboard(keyboard) => {
                Self::append_id_text(&keyboard.keys.join("+"), out, last_was_underscore);
            }
            InlineMacro::Button(button) => {
                Self::append_id_text(button.label, out, last_was_underscore);
            }
            InlineMacro::Menu(menu) => {
                Self::append_id_text(&menu.items.join(" > "), out, last_was_underscore);
            }
            InlineMacro::Image(_)
            | InlineMacro::Footnote(_)
            | InlineMacro::Stem(_)
            | InlineMacro::Icon(_)
            | InlineMacro::IndexTerm(_) => {}
        }
    }

    fn append_id_text(text: &str, out: &mut String, last_was_underscore: &mut bool) {
        for c in text.chars() {
            for c in c.to_lowercase() {
                let mapped = if c.is_alphanumeric() {
                    Some(c)
                } else if c.is_whitespace() || c == '-' || c == '.' || c == '_' {
                    Some('_')
                } else {
                    None
                };
                let Some(ch) = mapped else { continue };
                if ch == '_' {
                    if !*last_was_underscore {
                        out.push('_');
                    }
                    *last_was_underscore = true;
                } else {
                    out.push(ch);
                    *last_was_underscore = false;
                }
            }
        }
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
    /// Checks in order: explicit `metadata.id` (e.g. `[id=foo]`), then the last entry in
    /// `metadata.anchors` (e.g. `[[foo]]`), otherwise auto-generates one from the title
    /// and interns it into the supplied arena.
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

    /// Generate a section ID based on its title and metadata, returning a `String`
    /// directly.
    ///
    /// Returns the `Display`-formatted form (prefixed with `_` for generated IDs)
    /// matching `safe_id.to_string()`.
    #[must_use]
    pub fn generate_id_string(metadata: &BlockMetadata<'a>, title: &[InlineNode<'a>]) -> String {
        if let Some(id) = Self::explicit_id(metadata) {
            return id.to_string();
        }
        let slug = Self::id_from_title(title);
        if slug.is_empty() {
            slug
        } else {
            format!("_{slug}")
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
    fn test_id_from_title_strips_leading_separators() {
        // Leading non-alphanumerics must be squeezed away so the `_` prefix
        // added by `SafeId::Generated` does not produce a doubled `__`.
        let inlines: &[InlineNode] = &[InlineNode::PlainText(Plain {
            content: "--  Specialized Environments",
            location: Location::default(),
            escaped: false,
        })];
        assert_eq!(
            Section::id_from_title(inlines),
            "specialized_environments".to_string()
        );
        assert_eq!(
            Section::generate_id_string(&BlockMetadata::default(), inlines),
            "_specialized_environments".to_string()
        );
    }

    #[test]
    fn test_id_from_title_all_separators_is_empty() {
        // A title with no id-able characters yields an empty id (not a bare
        // `_`), matching asciidoctor.
        let arena = Bump::new();
        let inlines: &[InlineNode] = &[InlineNode::PlainText(Plain {
            content: "---",
            location: Location::default(),
            escaped: false,
        })];
        assert_eq!(Section::id_from_title(inlines), String::new());
        let metadata = BlockMetadata::default();
        assert_eq!(
            Section::generate_id_string(&metadata, inlines),
            String::new()
        );
        let safe_id = Section::generate_id(&arena, &metadata, inlines);
        assert_eq!(safe_id, SafeId::Generated(""));
        assert_eq!(safe_id.to_string(), String::new());
        assert_eq!(safe_id.as_arena_str(&arena), "");
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
