use std::{cmp::Reverse, collections::BTreeSet};

use super::{FORMATTING_MODIFIERS, RawToken};

/// Resolve nested styles and overlays into single-line, non-overlapping tokens.
pub(super) fn tokens(tokens: &[RawToken]) -> Vec<RawToken> {
    let mut events = Vec::with_capacity(tokens.len() * 2);
    for (index, token) in tokens.iter().enumerate() {
        events.push((token.line, token.start_char, true, index));
        events.push((token.line, token.start_char + token.length, false, index));
    }
    events.sort_unstable();
    let mut active: BTreeSet<(u8, Reverse<u32>, usize)> = BTreeSet::new();
    let mut styles = [0_u32; 32];
    let mut previous = (0, 0);
    let mut result: Vec<RawToken> = Vec::new();
    for (line, column, opening, index) in events {
        if previous.0 == line
            && previous.1 < column
            && let Some(&(_, _, winner)) = active.last()
            && let Some(token) = tokens.get(winner)
        {
            let inherited = styles.iter().enumerate().fold(0, |flags, (bit, count)| {
                if *count > 0 {
                    flags | (1 << bit)
                } else {
                    flags
                }
            });
            let modifiers = token.token_modifiers | if token.priority < 3 { inherited } else { 0 };
            if let Some(last) = result.last_mut()
                && last.line == line
                && last.start_char + last.length == previous.1
                && last.token_type == token.token_type
                && last.token_modifiers == modifiers
            {
                last.length += column - previous.1;
            } else {
                result.push(RawToken {
                    line,
                    start_char: previous.1,
                    length: column - previous.1,
                    token_type: token.token_type,
                    token_modifiers: modifiers,
                    priority: token.priority,
                });
            }
        }
        if let Some(token) = tokens.get(index) {
            let key = (token.priority, Reverse(token.length), index);
            if opening {
                active.insert(key);
            } else {
                active.remove(&key);
            }
            for (bit, count) in styles.iter_mut().enumerate() {
                if token.token_modifiers & FORMATTING_MODIFIERS & (1 << bit) != 0 {
                    if opening {
                        *count += 1;
                    } else {
                        *count -= 1;
                    }
                }
            }
        }
        previous = (line, column);
    }
    result
}
