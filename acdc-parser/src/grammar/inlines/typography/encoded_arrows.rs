use std::ops::Range;

use crate::SymbolKind;

use super::{Replacement, ReplacementValue};

pub(super) fn at(text: &str) -> Option<(usize, SymbolKind)> {
    if !matches!(text.as_bytes().first(), Some(b'&' | b'-' | b'=')) {
        return None;
    }
    for (pattern, kind) in [
        ("=&gt;", SymbolKind::RightDoubleArrow),
        ("-&gt;", SymbolKind::RightArrow),
        ("&lt;=", SymbolKind::LeftDoubleArrow),
        ("&lt;-", SymbolKind::LeftArrow),
        (r"&lt;\=", SymbolKind::LeftDoubleArrow),
        (r"&lt;\-", SymbolKind::LeftArrow),
    ] {
        if let Some(rest) = text.strip_prefix(pattern) {
            if pattern.starts_with("&lt;") && !pattern.contains('\\') && rest.starts_with("&gt;") {
                continue;
            }
            if pattern.contains('\\') && !rest.starts_with("&gt;") {
                continue;
            }
            return Some((pattern.len(), kind));
        }
    }
    None
}

pub(super) fn append_edits(
    text: &str,
    range: Range<usize>,
    value: ReplacementValue,
    edits: &mut Vec<Replacement>,
    allowed: &impl Fn(Range<usize>, SymbolKind) -> bool,
) {
    let token = &text[range.clone()];
    match value {
        ReplacementValue::Symbol(kind) if token.ends_with("&gt;") => {
            if let Some(prefix) = text[..range.start].strip_suffix("&lt;")
                && let start = prefix.len()
                && edits.last().is_none_or(|edit| edit.range.end <= start)
                && allowed(start..range.end, kind)
            {
                edits.push(Replacement {
                    range: start..range.start,
                    value: ReplacementValue::EncodedLiteral {
                        range: start..range.start,
                        decoded: "<",
                    },
                });
            }
            edits.push(Replacement {
                range,
                value: ReplacementValue::Symbol(kind),
            });
        }
        ReplacementValue::Symbol(kind) if token.starts_with("&lt;\\") => {
            let end = range.end;
            edits.push(Replacement {
                range,
                value: ReplacementValue::Symbol(kind),
            });
            edits.push(Replacement {
                range: end..end + 4,
                value: ReplacementValue::EncodedLiteral {
                    range: end..end + 4,
                    decoded: ">",
                },
            });
        }
        ReplacementValue::Literal(literal) if literal.start > range.start => {
            let decoded = match &text[literal.clone()] {
                "-&gt;" => Some("->"),
                "=&gt;" => Some("=>"),
                "&lt;-" => Some("<-"),
                "&lt;=" => Some("<="),
                _ => None,
            };
            let value = decoded.map_or(ReplacementValue::Literal(literal.clone()), |decoded| {
                ReplacementValue::EncodedLiteral {
                    range: literal,
                    decoded,
                }
            });
            edits.push(Replacement { range, value });
        }
        value @ (ReplacementValue::Symbol(_)
        | ReplacementValue::Literal(_)
        | ReplacementValue::EncodedLiteral { .. }) => edits.push(Replacement { range, value }),
    }
}
