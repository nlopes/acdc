use std::borrow::Cow;

use crate::{
    Anchor, AttributeValue, BlockMetadata, Title, grammar::ParserState, model::SectionLevel,
};

#[derive(Debug)]
pub(crate) struct PositionWithOffset {
    pub(crate) offset: usize,
    pub(crate) position: crate::Position,
}

// Used purely in the grammar to break down the block metadata lines into its different
// types.
#[derive(Debug)]
pub(crate) enum BlockMetadataLine<'input> {
    Anchor(Anchor<'input>),
    Attributes((bool, Box<BlockMetadata<'input>>)),
    Title(Title<'input>),
    DocumentAttribute(Cow<'input, str>, AttributeValue<'input>),
}

// Used purely in the grammar to break down header metadata lines (anchors and attributes
// that appear before the document title).
#[derive(Debug)]
pub(crate) enum HeaderMetadataLine<'input> {
    Anchor(Anchor<'input>),
    Attributes((bool, Box<BlockMetadata<'input>>)),
}

// Used purely in the grammar to represent the parsed block details
#[derive(Debug)]
pub(crate) struct BlockParsingMetadata<'input> {
    pub(crate) metadata: BlockMetadata<'input>,
    pub(crate) title: Title<'input>,
    pub(crate) parent_section_level: Option<SectionLevel>,
    pub(crate) macros_enabled: bool,
    pub(crate) attributes_enabled: bool,
}

impl Default for BlockParsingMetadata<'_> {
    fn default() -> Self {
        Self {
            metadata: BlockMetadata::default(),
            title: Title::default(),
            parent_section_level: None,
            macros_enabled: true,
            attributes_enabled: true,
        }
    }
}

/// Attribute shorthand syntax: .role, #id, %option
/// Used for both block-level attributes and inline formatting attributes
#[derive(Debug)]
pub(crate) enum Shorthand<'input> {
    Id(Cow<'input, str>),
    Role(Cow<'input, str>),
    Option(Cow<'input, str>),
}

pub(crate) const RESERVED_NAMED_ATTRIBUTE_ID: &str = "id";
pub(crate) const RESERVED_NAMED_ATTRIBUTE_ROLE: &str = "role";
pub(crate) const RESERVED_NAMED_ATTRIBUTE_OPTIONS: &str = "opts";
pub(crate) const RESERVED_NAMED_ATTRIBUTE_SUBS: &str = "subs";

/// Strip backslash escapes from URL paths.
///
/// In `AsciiDoc`, backslash escapes prevent typography substitutions.
/// For example, `\...` prevents ellipsis conversion. Since URLs are
/// parsed by the `url` crate which normalizes backslashes to forward slashes,
/// we need to strip these escapes before URL parsing.
///
/// This handles:
/// - `\...` → `...` (ellipsis escape)
/// - `\->` → `->` (right arrow escape)
/// - `\<-` → `<-` (left arrow escape)
/// - `\=>` → `=>` (right double arrow escape)
/// - `\<=` → `<=` (left double arrow escape)
/// - `\--` → `--` (em-dash escape)
pub(crate) fn strip_url_backslash_escapes(text: &str) -> Cow<'_, str> {
    if !text.contains('\\') {
        return Cow::Borrowed(text);
    }
    Cow::Owned(
        text.replace("\\...", "...")
            .replace("\\->", "->")
            .replace("\\<-", "<-")
            .replace("\\=>", "=>")
            .replace("\\<=", "<=")
            .replace("\\--", "--"),
    )
}

/// Configuration for attribute list processing
#[derive(Debug, Clone, Copy)]
pub(crate) struct AttributeProcessingMode {
    /// If true, first positional attribute becomes `style` (used by macro attributes)
    /// If false, positional attributes are added to `positional_attributes` list
    pub(crate) first_positional_is_style: bool,
    /// If true, process `subs=` attribute (block attributes only)
    #[allow(dead_code)]
    pub(crate) process_subs: bool,
}

impl AttributeProcessingMode {
    /// Configuration for block-level attributes
    pub(crate) const BLOCK: Self = Self {
        first_positional_is_style: false,
        process_subs: true,
    };

    /// Configuration for macro attributes (image, audio, video, icon)
    pub(crate) const MACRO: Self = Self {
        first_positional_is_style: true,
        process_subs: false,
    };
}

/// Parse a comma-separated list of values, interning each into the state's arena.
///
/// Used for `role=` and `options=` attributes which can be either:
/// - A single value: `role=thumbnail`
/// - A comma-separated list: `role="thumbnail, responsive"` or `role='thumbnail, responsive'`
///
/// Quotes are already stripped by `named_attribute_value()` / `strip_quotes()` upstream,
/// so this function only needs to split on commas.
pub(crate) fn parse_comma_separated_values<'a>(
    state: &ParserState<'a>,
    value: &str,
) -> Vec<&'a str> {
    value
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| state.intern_str(s))
        .collect()
}

/// Process a list of parsed attributes into `BlockMetadata`.
///
/// This is the shared logic between `attributes()` (block-level) and
/// `macro_attributes()` (for image, audio, video, icon macros).
///
/// Returns the title position if a `title=` attribute was found.
pub(crate) fn process_attribute_list<'input>(
    attrs: impl IntoIterator<
        Item = Option<(
            Cow<'input, str>,
            AttributeValue<'input>,
            Option<(usize, usize)>,
        )>,
    >,
    metadata: &mut BlockMetadata<'input>,
    state: &ParserState<'input>,
    fallback_start: usize,
    fallback_end: usize,
    mode: AttributeProcessingMode,
) -> Option<(usize, usize)> {
    let mut title_position = None;
    let mut first_positional = true;

    for (key, value, pos) in attrs.into_iter().flatten() {
        match key.as_ref() {
            k if k == RESERVED_NAMED_ATTRIBUTE_ID && metadata.id.is_none() => {
                let (id_start, id_end) = pos.unwrap_or((fallback_start, fallback_end));
                let id: &'input str = match value {
                    AttributeValue::String(s) => state.intern_cow(s),
                    AttributeValue::Bool(_) | AttributeValue::None => {
                        state.intern_fmt(format_args!("{value}"))
                    }
                };
                metadata.id = Some(Anchor {
                    id,
                    xreflabel: None,
                    location: state.create_location(id_start, id_end),
                });
            }
            k if k == RESERVED_NAMED_ATTRIBUTE_ROLE => {
                if let AttributeValue::String(ref s) = value {
                    // Roles are space-separated (not comma-separated) per asciidoctor behavior.
                    // `role='a b'` → two roles; `role='a,b'` → one role containing a comma.
                    for role in s.split_whitespace() {
                        if !role.is_empty() {
                            metadata.roles.push(state.intern_str(role));
                        }
                    }
                }
            }
            k if k == RESERVED_NAMED_ATTRIBUTE_OPTIONS => {
                if let AttributeValue::String(ref s) = value {
                    metadata
                        .options
                        .extend(parse_comma_separated_values(state, s));
                }
            }
            // Skip subs= attribute - it's handled separately by the caller
            // (block-specific, feature-gated, requires parse_subs_attribute)
            k if k == RESERVED_NAMED_ATTRIBUTE_SUBS => {}
            "title" => {
                if let AttributeValue::String(ref s) = value {
                    if pos.is_some() {
                        title_position = pos;
                    }
                    metadata
                        .attributes
                        .insert(key, AttributeValue::String(s.clone()));
                }
            }
            _ => {
                if let AttributeValue::String(ref s) = value {
                    metadata
                        .attributes
                        .insert(key, AttributeValue::String(s.clone()));
                } else if value == AttributeValue::None {
                    // Positional attribute
                    let key_str: &'input str = state.intern_cow(key);
                    if mode.first_positional_is_style && first_positional {
                        metadata.style = Some(key_str);
                        first_positional = false;
                    } else {
                        metadata.positional_attributes.push(key_str);
                    }
                }
            }
        }
    }

    title_position
}

/// Check if a title line looks like a description list item.
///
/// Description list items have the form `term::`, `term:::`, `term::::`, or `term;;`
/// optionally followed by content. This check prevents these from being matched
/// as setext section titles.
pub(crate) fn title_looks_like_description_list(title: &str) -> bool {
    // Check for :: ;; ::: :::: markers that indicate description list items
    // The marker must appear after some term text, optionally followed by content
    let trimmed = title.trim();
    // Look for description list markers: ::::, :::, ::, ;;
    for marker in &["::::", ":::", "::", ";;"] {
        if let Some(pos) = trimmed.find(marker) &&
            // Marker must not be at the start (there must be a term before it)
            pos > 0 &&
            // After the marker, must be end of string, space, or tab
            let Some(after) = trimmed.get(pos + marker.len()..)
                && (after.is_empty() || after.starts_with(' ') || after.starts_with('\t'))
        {
            return true;
        }
    }
    false
}
