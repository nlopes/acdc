//! Attribute-list scanning shared by grammars and preprocessing.

use crate::{
    DocumentAttributes,
    model::{Substitution, substitute},
};
use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AttributeQuote {
    Unquoted,
    Single,
    Double,
}

/// One slot of an attribute list, as Asciidoctor's `AttributeList` reads it.
/// Also used by cross-references and links when their text holds attributes.
#[derive(Debug)]
pub(super) struct ScannedAttribute {
    pub(super) name: Option<String>,
    pub(super) value: String,
    pub(super) quote: AttributeQuote,
    pub(super) value_start: usize,
    pub(super) value_end: usize,
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
) -> Option<usize> {
    // AttributeList treats a quote after any backslash run as escaped.
    let mut escaped = false;
    for (index, character) in value.char_indices().skip(1) {
        if character == quote && !escaped && !inside_attribute_content(start + index, protected) {
            return Some(index);
        }
        escaped = character == '\\';
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

pub(super) fn scan_attribute_list(
    source: &str,
    protected: &[Range<usize>],
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
            if let Some(close) = closing_quote(quoted, quote, value_start, protected) {
                let after_close = value_start + close + quote.len_utf8();
                attributes.push(ScannedAttribute {
                    name: named.map(|(name, _)| name.to_string()),
                    value: unescape_attribute_quote(&quoted[quote.len_utf8()..close], quote),
                    quote: if quote == '\'' {
                        AttributeQuote::Single
                    } else {
                        AttributeQuote::Double
                    },
                    value_start: value_start + quote.len_utf8(),
                    value_end: value_start + close,
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
                        });
                        break;
                    }
                }
                continue;
            }
        }

        // Links parse nested macros after selecting the label, so their commas
        // must stay inside that positional value until the child parser runs.
        let comma = attribute_list_comma(&source[value_start..], value_start, protected)
            .map(|index| value_start + index);
        let end = comma.unwrap_or(source.len());
        let trimmed = source[value_start..end].trim_end();

        attributes.push(ScannedAttribute {
            name: named.map(|(name, _)| name.to_string()),
            value: trimmed.to_string(),
            quote: AttributeQuote::Unquoted,
            value_start,
            value_end: value_start + trimmed.len(),
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
            });
            break;
        }
    }

    attributes
}

// Preprocessing needs only the style to preserve comments in verbatim
// paragraphs. Share attribute tokenization with the full metadata parser.
pub(crate) fn verbatim_paragraph_style(
    source: &str,
    attributes: &DocumentAttributes<'_>,
) -> Option<bool> {
    let source = source.strip_prefix('[')?.strip_suffix(']')?;
    if source.starts_with('[') {
        return None;
    }
    let substituted = substitute(source, &[Substitution::Attributes], attributes);
    let mut style = None;
    for (slot, attribute) in scan_attribute_list(&substituted, &[])
        .into_iter()
        .enumerate()
    {
        let value = attribute.value.as_str();
        let name = match attribute.name.as_deref() {
            Some("style") if attribute.quote != AttributeQuote::Unquoted || value != "None" => {
                value
            }
            None if slot == 0 && !value.is_empty() => {
                if value.chars().any(char::is_whitespace) {
                    value
                } else {
                    value.split(['#', '.', '%']).next().unwrap_or_default()
                }
            }
            _ => continue,
        };
        if !name.is_empty() || attribute.name.is_some() {
            style = Some(matches!(name, "source" | "listing" | "literal" | "verse"));
        }
    }
    style
}
