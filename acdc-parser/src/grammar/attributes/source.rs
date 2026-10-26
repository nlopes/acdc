//! Attribute-list text transformations and their authored source ranges.

use std::borrow::Cow;

use crate::{
    CompatibilityMode, DocumentAttributes,
    grammar::inline_preprocessor::{ProcessedKind, SourceMap},
};

pub(in crate::grammar) struct ExpandedAttributeList<'a> {
    pub(in crate::grammar) text: Cow<'a, str>,
    pub(in crate::grammar) source_map: SourceMap,
}

/// Expand authored references once, without scanning text inserted by a value.
pub(in crate::grammar) fn expand_attribute_list<'a>(
    source: &'a str,
    attributes: &DocumentAttributes<'_>,
    mode: CompatibilityMode,
) -> ExpandedAttributeList<'a> {
    let mut output = None;
    let mut source_map = SourceMap::default();
    let mut cursor = 0;
    let mut unwritten = 0;
    while let Some(relative) = source[cursor..].find('{') {
        let open = cursor + relative;
        cursor = open + 1;
        let name_len = source[cursor..]
            .bytes()
            .take_while(|byte| byte.is_ascii_alphanumeric() || matches!(*byte, b'_' | b'-'))
            .count();
        let close = cursor + name_len;
        if name_len == 0 || !source[close..].starts_with('}') {
            continue;
        }
        let name = &source[cursor..close];
        cursor = close + 1;
        let slashes = source[..open]
            .bytes()
            .rev()
            .take_while(|b| *b == b'\\')
            .count();
        let escaped = match mode {
            CompatibilityMode::Asciidoctor => slashes != 0,
            CompatibilityMode::Draft => slashes % 2 != 0,
        };
        let value = (!escaped).then(|| attributes.get(name)).flatten();
        if slashes == 0 && value.is_none() {
            continue;
        }
        let output = output.get_or_insert_with(|| String::with_capacity(source.len()));
        let start = if mode == CompatibilityMode::Draft {
            open - slashes
        } else {
            open - usize::from(escaped)
        };
        output.push_str(&source[unwritten..start]);
        if mode == CompatibilityMode::Draft {
            collapse_backslashes(output, &mut source_map, start, slashes / 2);
        }
        if escaped {
            output.push('{');
            source_map.add_replacement(open - 1, open + 1, 1, ProcessedKind::Escape);
            output.push_str(&source[open + 1..cursor]);
        } else if let Some(value) = value {
            let before = output.len();
            let _ = value.write_text(output);
            source_map.add_replacement(
                open,
                cursor,
                output.len() - before,
                ProcessedKind::Attribute,
            );
        } else {
            output.push_str(&source[open..cursor]);
        }
        unwritten = cursor;
    }
    ExpandedAttributeList {
        text: output.map_or(Cow::Borrowed(source), |mut output| {
            output.push_str(&source[unwritten..]);
            Cow::Owned(output)
        }),
        source_map,
    }
}

fn collapse_backslashes(
    output: &mut String,
    source_map: &mut SourceMap,
    start: usize,
    pairs: usize,
) {
    for pair in 0..pairs {
        let start = start + pair * 2;
        output.push('\\');
        source_map.add_replacement(start, start + 2, 1, ProcessedKind::Escape);
    }
}

/// Decode only escapes belonging to this enclosure.
pub(in crate::grammar) fn decode_attribute_escapes(
    source: &str,
    delimiter: char,
    mode: CompatibilityMode,
    before_closing_delimiter: bool,
) -> (String, SourceMap) {
    let mut output = String::with_capacity(source.len());
    let mut source_map = SourceMap::default();
    let mut cursor = 0;
    while let Some(relative) = source[cursor..].find('\\') {
        let start = cursor + relative;
        output.push_str(&source[cursor..start]);
        let count = source[start..]
            .bytes()
            .take_while(|byte| *byte == b'\\')
            .count();
        let end = start + count;
        let escaped_delimiter = source[end..].starts_with(delimiter);
        if escaped_delimiter || (end == source.len() && before_closing_delimiter) {
            if mode == CompatibilityMode::Draft {
                collapse_backslashes(&mut output, &mut source_map, start, count / 2);
                if count % 2 != 0 && escaped_delimiter {
                    output.push(delimiter);
                    source_map.add_replacement(
                        end - 1,
                        end + delimiter.len_utf8(),
                        delimiter.len_utf8(),
                        ProcessedKind::Escape,
                    );
                    cursor = end + delimiter.len_utf8();
                } else {
                    cursor = end;
                }
            } else if escaped_delimiter {
                output.push_str(&source[start..end - 1]);
                output.push(delimiter);
                source_map.add_replacement(
                    end - 1,
                    end + delimiter.len_utf8(),
                    delimiter.len_utf8(),
                    ProcessedKind::Escape,
                );
                cursor = end + delimiter.len_utf8();
            } else {
                output.push_str(&source[start..end]);
                cursor = end;
            }
        } else {
            output.push_str(&source[start..end]);
            cursor = end;
        }
    }
    output.push_str(&source[cursor..]);
    (output, source_map)
}
