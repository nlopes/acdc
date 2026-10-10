//! Find the boundaries of single-plus passthroughs.

use regex_syntax::is_word_character;

pub(super) fn constrained_passthrough_start(previous: Option<char>) -> bool {
    previous.is_none_or(|character| {
        !is_word_character(character) && !matches!(character, ';' | ':' | '\\')
    })
}

pub(super) fn constrained_passthrough_end(source: &str, start: usize) -> Option<usize> {
    let content_start = start + 1;
    let first = source[content_start..].chars().next()?;
    // These content edges exclude ASCII whitespace, including VT/FF, but accept NBSP.
    if matches!(first, ' ' | '\t'..='\r') {
        return None;
    }

    for (relative_end, character) in source[content_start..].char_indices() {
        if character != '+' || relative_end == 0 {
            continue;
        }
        let end = content_start + relative_end;
        let last = source[content_start..end].chars().next_back()?;
        let next = source[end + 1..].chars().next();
        if !matches!(last, ' ' | '\t'..='\r') && next.is_none_or(|value| !is_word_character(value))
        {
            return Some(end);
        }
    }
    None
}
