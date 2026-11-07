use std::ops::Range;

use super::AttributeQuote;
use crate::CompatibilityMode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::grammar) enum ShorthandKind {
    Style,
    Id,
    Role,
    Option,
    Reftext,
}

pub(in crate::grammar) struct ShorthandPart {
    pub(in crate::grammar) kind: ShorthandKind,
    pub(in crate::grammar) range: Range<usize>,
}

pub(super) fn anchor_end(value: &str) -> Option<usize> {
    value.strip_prefix('[')?;
    let mut escaped = false;
    for (index, character) in value.char_indices().skip(1) {
        if character == ']' && !escaped {
            return Some(index);
        }
        escaped = character == '\\' && !escaped;
    }
    None
}

/// Return value ranges for shorthand parts, or the whole literal style.
pub(in crate::grammar) fn block_shorthand(
    value: &str,
    quote: AttributeQuote,
    mode: CompatibilityMode,
) -> Vec<ShorthandPart> {
    let literal = || {
        vec![ShorthandPart {
            kind: ShorthandKind::Style,
            range: 0..value.len(),
        }]
    };
    if value.is_empty() {
        return Vec::new();
    }
    if mode == CompatibilityMode::Draft && quote != AttributeQuote::Unquoted {
        return literal();
    }
    let mut parts = Vec::new();
    let mut start = 0;
    if mode == CompatibilityMode::Draft && value.starts_with('[') {
        let Some(close) = anchor_end(value) else {
            return literal();
        };
        let comma = value[1..close].find(',').map(|index| index + 1);
        let id_end = comma.unwrap_or(close);
        if id_end == 1
            || value[1..id_end]
                .chars()
                .any(|c| c.is_whitespace() || matches!(c, '[' | ']'))
        {
            return literal();
        }
        parts.push(ShorthandPart {
            kind: ShorthandKind::Id,
            range: 1..id_end,
        });
        if let Some(comma) = comma {
            if comma + 1 == close {
                return literal();
            }
            parts.push(ShorthandPart {
                kind: ShorthandKind::Reftext,
                range: comma + 1..close,
            });
        }
        start = close + 1;
    }
    if value[start..].chars().any(char::is_whitespace) {
        return literal();
    }
    let mut kind = ShorthandKind::Style;
    let tail = start;
    for (index, character) in value.char_indices().filter(|(index, _)| *index >= tail) {
        let next = match character {
            '#' => ShorthandKind::Id,
            '.' => ShorthandKind::Role,
            '%' => ShorthandKind::Option,
            _ => continue,
        };
        if index == start && kind != ShorthandKind::Style {
            if mode == CompatibilityMode::Draft {
                return literal();
            }
        } else if index != start {
            parts.push(ShorthandPart {
                kind,
                range: start..index,
            });
        }
        start = index + character.len_utf8();
        kind = next;
    }
    if start == value.len() && kind != ShorthandKind::Style {
        if mode == CompatibilityMode::Draft {
            return literal();
        }
    } else if start != value.len() {
        parts.push(ShorthandPart {
            kind,
            range: start..value.len(),
        });
    }
    parts
}
