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

pub(in crate::grammar) fn has_stem_substitution_list(source: &str) -> bool {
    source.contains(':')
        && ["stem:", "asciimath:", "latexmath:"]
            .into_iter()
            .any(|prefix| {
                source.split(prefix).skip(1).any(|tail| {
                    tail.split_once('[').is_some_and(|(controls, _)| {
                        !controls.is_empty()
                            && controls.bytes().all(|byte| {
                                byte.is_ascii_alphabetic()
                                    || matches!(byte, b',' | b'+' | b'-' | b'_')
                            })
                    })
                })
            })
}
