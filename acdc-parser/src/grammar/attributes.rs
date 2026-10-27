//! Attribute-list scanning shared by grammars and preprocessing.

mod shorthand;
mod source;

#[cfg(test)]
mod tests;

pub(super) use shorthand::{ShorthandKind, block_shorthand};
use source::decode_attribute_escapes;
pub(super) use source::expand_attribute_list;

use crate::{
    CompatibilityMode, DocumentAttributes, Error, grammar::inline_preprocessor::SourceMap,
};
use std::{borrow::Cow, ops::Range, rc::Rc};

pub(super) fn resolve_block_anchor_reftext<'a>(
    source: &'a str,
    attributes: &DocumentAttributes<'_>,
    mode: CompatibilityMode,
) -> Option<(Cow<'a, str>, SourceMap)> {
    if source.is_empty() {
        return None;
    }
    let expanded = expand_attribute_list(source, attributes, mode);
    if mode == CompatibilityMode::Asciidoctor {
        return Some((expanded.text, expanded.source_map));
    }
    let mut escaped = false;
    for character in expanded.text.chars() {
        if character == ']' && !escaped {
            return None;
        }
        escaped = character == '\\' && !escaped;
    }
    if escaped {
        return None;
    }
    if expanded.text.contains('\\') {
        let (text, source_map) = decode_attribute_escapes(&expanded.text, ']', mode, true);
        Some((
            Cow::Owned(text),
            source_map.with_origin(expanded.source_map, 0),
        ))
    } else {
        Some((expanded.text, expanded.source_map))
    }
}

#[derive(Clone, Copy)]
pub(super) enum BlockAttributeMode {
    Block,
    Macro,
}

// Asciidoctor checks the authored block line before expansion. Draft also
// rejects outer whitespace introduced by an attribute reference.
pub(super) fn block_attribute_list_allowed(
    source: &str,
    attributes: &DocumentAttributes<'_>,
    mode: CompatibilityMode,
    context: BlockAttributeMode,
) -> bool {
    match mode {
        CompatibilityMode::Asciidoctor => {
            matches!(context, BlockAttributeMode::Macro) || !source.starts_with([' ', '\t'])
        }
        CompatibilityMode::Draft => {
            if source.starts_with([' ', '\t']) || source.ends_with([' ', '\t']) {
                return false;
            }
            let expanded = expand_attribute_list(source, attributes, mode);
            !expanded.text.starts_with([' ', '\t']) && !expanded.text.ends_with([' ', '\t'])
        }
    }
}

pub(crate) fn block_attribute_line<'a>(
    line: &'a str,
    attributes: &DocumentAttributes<'_>,
    mode: CompatibilityMode,
) -> Option<&'a str> {
    let source = line.strip_prefix('[')?.strip_suffix(']')?;
    block_attribute_list_allowed(source, attributes, mode, BlockAttributeMode::Block)
        .then_some(source)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AttributeQuote {
    Unquoted,
    Single,
    Double,
}

/// Expanded and dequoted content, captured before selecting the displayed value.
#[derive(Clone, Debug)]
pub(super) struct ResolvedContentAttribute<'a> {
    pub(super) text: &'a str,
    pub(super) quote: AttributeQuote,
    pub(super) source_map: SourceMap,
    pub(super) source_range: Range<usize>,
    pub(super) attributes: Rc<DocumentAttributes<'a>>,
}

/// One slot of a confined attribute list, with byte offsets into its source.
/// Also used by cross-references and links when their text holds attributes.
#[derive(Debug)]
pub(super) struct ScannedAttribute {
    pub(super) name: Option<String>,
    pub(super) value: String,
    pub(super) quote: AttributeQuote,
    pub(super) value_start: usize,
    pub(super) value_end: usize,
    value_map: SourceMap,
}

impl ScannedAttribute {
    pub(super) fn resolve_content<'a>(
        &self,
        text: &'a str,
        list_map: &SourceMap,
        offset: usize,
        attributes: Rc<DocumentAttributes<'a>>,
    ) -> Result<ResolvedContentAttribute<'a>, Error> {
        let range = self.source_range(list_map, 0..text.len())?;
        Ok(ResolvedContentAttribute {
            text,
            quote: self.quote,
            source_map: self.value_map.clone().with_origin(
                list_map.clone().with_origin(SourceMap::default(), offset),
                self.value_start,
            ),
            source_range: range.start + offset..range.end + offset,
            attributes,
        })
    }

    pub(super) fn resolve_content_range<'a>(
        &self,
        text: &'a str,
        range: Range<usize>,
        list_map: &SourceMap,
        offset: usize,
        attributes: Rc<DocumentAttributes<'a>>,
    ) -> Result<ResolvedContentAttribute<'a>, Error> {
        let (_, escapes) = decode_attribute_escapes(
            &self.value[range.clone()],
            ']',
            CompatibilityMode::Draft,
            true,
        );
        let source_range = self.source_range(list_map, range.clone())?;
        Ok(ResolvedContentAttribute {
            text,
            quote: self.quote,
            source_map: escapes.with_origin(
                self.value_map.clone().with_origin(
                    list_map.clone().with_origin(SourceMap::default(), offset),
                    self.value_start,
                ),
                range.start,
            ),
            source_range: source_range.start + offset..source_range.end + offset,
            attributes,
        })
    }

    pub(super) fn canonical_name(&self) -> Option<&str> {
        self.name.as_deref().map(|name| match name {
            "roles" => "role",
            "options" => "opts",
            name => name,
        })
    }

    pub(super) fn is_ignored(&self, mode: CompatibilityMode) -> bool {
        mode == CompatibilityMode::Asciidoctor
            && self.name.is_some()
            && self.quote == AttributeQuote::Unquoted
            && self.value == "None"
    }

    /// Map a decoded value range back through quote escapes and list expansion.
    pub(super) fn source_range(
        &self,
        list_map: &SourceMap,
        range: Range<usize>,
    ) -> Result<Range<usize>, Error> {
        let start =
            list_map.map_position(self.value_start + self.value_map.map_position(range.start)?)?;
        let end = if range.is_empty() {
            start
        } else {
            list_map.map_end_position(
                self.value_start + self.value_map.map_end_position(range.end - 1)?,
            )? + 1
        };
        Ok(start..end)
    }
}

fn is_attribute_name_start(character: char) -> bool {
    character == '_' || character.is_alphanumeric()
}

fn is_attribute_name_continue(character: char) -> bool {
    is_attribute_name_start(character) || matches!(character, '-' | '.')
}

fn named_attribute_parts(value: &str) -> Option<(&str, usize)> {
    let mut characters = value.char_indices();
    let (_, first) = characters.next()?;
    if !is_attribute_name_start(first) {
        return None;
    }

    let mut name_end = first.len_utf8();
    for (index, character) in characters {
        if !is_attribute_name_continue(character) {
            break;
        }
        name_end = index + character.len_utf8();
    }

    let remainder = &value[name_end..];
    let equals_offset = remainder.len() - remainder.trim_start_matches([' ', '\t']).len();
    remainder
        .get(equals_offset..)
        .is_some_and(|remainder| remainder.starts_with('='))
        .then_some((&value[..name_end], name_end + equals_offset + 1))
}

fn closing_quote(
    value: &str,
    quote: char,
    start: usize,
    protected: &[Range<usize>],
    mode: CompatibilityMode,
) -> Option<usize> {
    // Asciidoctor escapes a quote after any backslash run; Draft uses parity.
    let mut escaped = false;
    for (index, character) in value.char_indices().skip(1) {
        if character == quote && !escaped && !inside_attribute_content(start + index, protected) {
            let after = &value[index + character.len_utf8()..];
            return (mode == CompatibilityMode::Asciidoctor
                || after.is_empty()
                || after.starts_with([',', ' ', '\t']))
            .then_some(index);
        }
        escaped = character == '\\' && (mode == CompatibilityMode::Asciidoctor || !escaped);
    }
    None
}

pub(super) fn unescape_attribute_quote(value: &str, quote: char) -> String {
    let escaped_quote = format!("\\{quote}");
    value.replace(&escaped_quote, &quote.to_string())
}

fn inside_attribute_content(position: usize, protected: &[Range<usize>]) -> bool {
    protected
        .get(protected.partition_point(|range| range.end <= position))
        .is_some_and(|range| range.contains(&position))
}

fn attribute_list_comma(source: &str, start: usize, protected: &[Range<usize>]) -> Option<usize> {
    if protected.is_empty() {
        return source.find(',');
    }
    source.char_indices().find_map(|(index, character)| {
        (character == ',' && !inside_attribute_content(start + index, protected)).then_some(index)
    })
}

// SDR-2 governs block lists. Inline macros keep their own boundaries in both modes.
pub(super) fn scan_attribute_list(
    source: &str,
    protected: &[Range<usize>],
) -> Vec<ScannedAttribute> {
    scan_attributes(source, protected, CompatibilityMode::Asciidoctor, false)
}

pub(super) fn scan_block_attribute_list(
    source: &str,
    mode: CompatibilityMode,
    context: BlockAttributeMode,
) -> Vec<ScannedAttribute> {
    scan_attributes(
        source,
        &[],
        mode,
        mode == CompatibilityMode::Draft && matches!(context, BlockAttributeMode::Block),
    )
}

fn scan_attributes(
    source: &str,
    protected: &[Range<usize>],
    mode: CompatibilityMode,
    anchor_shorthand: bool,
) -> Vec<ScannedAttribute> {
    let mut attributes = Vec::new();
    let mut cursor = 0;

    loop {
        let slot_start = cursor;
        let remaining = &source[slot_start..];
        let leading = remaining.len() - remaining.trim_start_matches([' ', '\t']).len();
        let trimmed_start = slot_start + leading;
        let candidate = &source[trimmed_start..];
        let named = named_attribute_parts(candidate);
        let raw_value_start = named.map_or(trimmed_start, |(_, start)| trimmed_start + start);
        let value_leading = source[raw_value_start..].len()
            - source[raw_value_start..]
                .trim_start_matches([' ', '\t'])
                .len();
        let value_start = raw_value_start + value_leading;
        let first = source[value_start..].chars().next();

        if let Some(quote @ ('\'' | '"')) = first {
            let quoted = &source[value_start..];
            if let Some(close) = closing_quote(quoted, quote, value_start, protected, mode) {
                let after_close = value_start + close + quote.len_utf8();
                let (value, value_map) =
                    decode_attribute_escapes(&quoted[quote.len_utf8()..close], quote, mode, true);
                attributes.push(ScannedAttribute {
                    name: named.map(|(name, _)| name.to_string()),
                    value,
                    quote: if quote == '\'' {
                        AttributeQuote::Single
                    } else {
                        AttributeQuote::Double
                    },
                    value_start: value_start + quote.len_utf8(),
                    value_end: value_start + close,
                    value_map,
                });

                let trailing = &source[after_close..];
                let whitespace = trailing.len() - trailing.trim_start_matches([' ', '\t']).len();
                cursor = after_close + whitespace;
                if cursor == source.len() {
                    break;
                }
                if source[cursor..].starts_with(',') {
                    cursor += 1;
                    if cursor == source.len() {
                        attributes.push(ScannedAttribute {
                            name: None,
                            value: String::new(),
                            quote: AttributeQuote::Unquoted,
                            value_start: cursor,
                            value_end: cursor,
                            value_map: SourceMap::default(),
                        });
                        break;
                    }
                }
                continue;
            }
        }

        // Links parse nested macros after selecting the label, so their commas
        // must stay inside that positional value until the child parser runs.
        let tail = if anchor_shorthand && attributes.is_empty() && named.is_none() {
            shorthand::anchor_end(&source[value_start..])
                .map_or(value_start, |end| value_start + end + 1)
        } else {
            value_start
        };
        let comma =
            attribute_list_comma(&source[tail..], tail, protected).map(|index| tail + index);
        let end = comma.unwrap_or(source.len());
        let trimmed = source[value_start..end].trim_end();

        attributes.push(ScannedAttribute {
            name: named.map(|(name, _)| name.to_string()),
            value: trimmed.to_string(),
            quote: AttributeQuote::Unquoted,
            value_start,
            value_end: value_start + trimmed.len(),
            value_map: SourceMap::default(),
        });

        let Some(comma) = comma else {
            break;
        };
        cursor = comma + 1;
        if cursor == source.len() {
            attributes.push(ScannedAttribute {
                name: None,
                value: String::new(),
                quote: AttributeQuote::Unquoted,
                value_start: cursor,
                value_end: cursor,
                value_map: SourceMap::default(),
            });
            break;
        }
    }

    attributes
}

// Preprocessing needs only the style to preserve comments in verbatim
// paragraphs. The caller has already checked the block attribute-line boundary.
pub(crate) fn verbatim_paragraph_style(
    source: &str,
    attributes: &DocumentAttributes<'_>,
    mode: CompatibilityMode,
) -> Option<bool> {
    if source.starts_with('[')
        && (mode == CompatibilityMode::Asciidoctor || source.starts_with("[["))
    {
        return None;
    }
    let substituted = expand_attribute_list(source, attributes, mode);
    let attributes = scan_block_attribute_list(&substituted.text, mode, BlockAttributeMode::Block);
    block_list_style(&attributes, mode)
        .map(|style| matches!(style, "source" | "listing" | "literal" | "verse"))
}

pub(super) fn block_attribute_entries(
    attributes: &[ScannedAttribute],
    mode: CompatibilityMode,
    context: BlockAttributeMode,
) -> impl Iterator<Item = (usize, &ScannedAttribute)> {
    let mut entries = attributes.iter().enumerate();
    // Asciidoctor applies first-slot shorthand after the list's named entries.
    let first = if mode == CompatibilityMode::Asciidoctor
        && matches!(context, BlockAttributeMode::Block)
        && attributes
            .first()
            .is_some_and(|attribute| attribute.name.is_none())
    {
        entries.next()
    } else {
        None
    };
    entries.chain(first)
}

pub(super) fn block_list_style(
    attributes: &[ScannedAttribute],
    mode: CompatibilityMode,
) -> Option<&str> {
    let mut style = None;
    for (slot, attribute) in block_attribute_entries(attributes, mode, BlockAttributeMode::Block) {
        if attribute.is_ignored(mode) {
            continue;
        }
        match attribute.name.as_deref() {
            Some("style") => style = Some(attribute.value.as_str()),
            None if slot == 0 => {
                if let Some(positional) = block_positional_style(attribute, mode) {
                    style = Some(positional);
                }
            }
            _ => {}
        }
    }
    style
}

pub(super) fn block_positional_style(
    attribute: &ScannedAttribute,
    mode: CompatibilityMode,
) -> Option<&str> {
    if attribute.name.is_some() || attribute.is_ignored(mode) {
        return None;
    }
    block_shorthand(&attribute.value, attribute.quote, mode)
        .into_iter()
        .find(|part| part.kind == ShorthandKind::Style)
        .map(|part| &attribute.value[part.range])
}

pub(super) fn decode_anchor_shorthand_reftext(value: &str) -> String {
    decode_attribute_escapes(value, ']', CompatibilityMode::Draft, true).0
}
