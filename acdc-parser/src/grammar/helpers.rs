use std::borrow::Cow;

use crate::{
    BlockMetadata, ProcessedContent, Title,
    grammar::{ParserState, passthrough_processing::replace_passthrough_placeholders},
    model::{SectionLevel, substitution::SubstitutionPlan},
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

// Used purely in the grammar to represent the parsed block details
#[derive(Debug, Default)]
pub(crate) struct BlockParsingMetadata<'input> {
    pub(crate) metadata: BlockMetadata<'input>,
    pub(crate) title: Title<'input>,
    pub(crate) parent_section_level: Option<SectionLevel>,
    pub(crate) substitutions: SubstitutionPlan,
    pub(crate) hardbreaks: bool,
    /// Set when the attribute line marks the block as a discrete heading,
    /// either via the `discrete`/`float` block style (`[discrete]`) or as a
    /// bare positional attribute (`[#id,discrete]`).
    pub(crate) discrete: bool,
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

/// Restore URL passthroughs and remove escapes before URL parsing can normalize them.
pub(crate) fn restore_url_path(processed: ProcessedContent<'_>) -> String {
    let text = if processed.passthroughs.is_empty() {
        processed.text
    } else {
        replace_passthrough_placeholders(&processed.text, &processed).into()
    };
    strip_url_backslash_escapes(&text).into_owned()
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
