use std::borrow::Cow;

use crate::{
    Anchor, AttributeValue, BlockMetadata, Title,
    grammar::ParserState,
    model::{PositionalAttribute, SectionLevel, substitution::SubsFlags},
};

#[derive(Debug)]
pub(crate) struct PositionWithOffset {
    pub(crate) offset: usize,
    pub(crate) position: crate::Position,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MacroAttributeContext {
    General,
    Image,
}

// Used purely in the grammar to break down the block metadata lines into its different
// types.
#[derive(Debug)]
pub(crate) enum BlockMetadataLine<'input> {
    Anchor(Anchor<'input>),
    Attributes((bool, Box<BlockMetadata<'input>>)),
    Title(Title<'input>),
    DocumentAttribute(Cow<'input, str>, AttributeValue<'input>, bool),
}

// Used purely in the grammar where only anchors and attribute lists are valid metadata.
#[derive(Debug)]
pub(crate) enum AttributeOrAnchorLine<'input> {
    Anchor(Anchor<'input>),
    Attributes((bool, Box<BlockMetadata<'input>>)),
}

// Used purely in the grammar to represent the parsed block details
#[derive(Debug, Default)]
pub(crate) struct BlockParsingMetadata<'input> {
    pub(crate) metadata: BlockMetadata<'input>,
    pub(crate) title: Title<'input>,
    pub(crate) parent_section_level: Option<SectionLevel>,
    pub(crate) subs_flags: SubsFlags,
    pub(crate) hardbreaks: bool,
    /// Set when the attribute line marks the block as a discrete heading,
    /// either via the `discrete`/`float` block style (`[discrete]`) or as a
    /// bare positional attribute (`[#id,discrete]`).
    pub(crate) discrete: bool,
}

/// Attribute shorthand syntax for inline formatting attributes.
#[derive(Debug)]
pub(crate) enum Shorthand<'input> {
    Id(Cow<'input, str>),
    Role(Cow<'input, str>),
}

pub(crate) const RESERVED_NAMED_ATTRIBUTE_ID: &str = "id";
pub(crate) const RESERVED_NAMED_ATTRIBUTE_ROLE: &str = "role";
pub(crate) const RESERVED_NAMED_ATTRIBUTE_OPTIONS: &str = "opts";
pub(crate) const RESERVED_NAMED_ATTRIBUTE_SUBS: &str = "subs";

pub(crate) fn is_valid_bibliography_id(id: &str) -> bool {
    let mut chars = id.chars();
    chars
        .next()
        .is_some_and(|first| first.is_alphabetic() || matches!(first, '_' | ':'))
        && chars.all(|character| {
            character.is_alphanumeric() || matches!(character, '_' | '-' | ':' | '.')
        })
}

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

/// Store parsed inline macro attributes in `BlockMetadata`.
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
    context: MacroAttributeContext,
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
                    bibliography: false,
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
                    if context == MacroAttributeContext::Image && key == "link" {
                        metadata
                            .attributes
                            .set(key, AttributeValue::String(s.clone()));
                    } else {
                        metadata
                            .attributes
                            .insert(key, AttributeValue::String(s.clone()));
                    }
                } else if value == AttributeValue::None {
                    // Positional attribute
                    let key_str: &'input str = state.intern_cow(key);
                    if first_positional {
                        metadata.style = Some(key_str);
                        first_positional = false;
                    } else {
                        metadata.positional_attributes.push(PositionalAttribute {
                            value: key_str,
                            substitutions: false,
                            location: None,
                        });
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
