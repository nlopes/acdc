//! Formatting boundaries shared by inline parsing and preprocessing.

fn is_word_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn match_constrained_boundary(b: u8) -> bool {
    // Use source punctuation; converter escaping must not change parsing.
    !is_word_char(b) && !matches!(b, b':' | b';' | b'}')
}

/**
Check whether the character before `pos` is a valid constrained opening boundary.

At position 0, falls back to `outer_delimiter` (the byte preceding the current
inline span in the parent context). A word-character outer delimiter means the
boundary is invalid.
*/
pub(super) fn check_constrained_opening_boundary(
    pos: usize,
    input: &[u8],
    outer_delimiter: Option<u8>,
    marker: u8,
) -> bool {
    if pos == 0 {
        return outer_delimiter.is_none_or(|d| !is_word_char(d));
    }
    match input.get(pos - 1) {
        None => true,
        // A hash after an ampersand belongs to character-reference syntax.
        Some(b'&') if marker == b'#' => false,
        Some(&b) if b.is_ascii() => match_constrained_boundary(b),
        // The preceding byte belongs to a multibyte (non-ASCII) character. It is
        // a valid boundary unless that character is a Unicode word character
        // (letter or number) — matching asciidoctor, where Unicode punctuation
        // such as `“` or `«` opens a constrained span but letters like `é`/`日`
        // do not.
        Some(_) => char_ending_at(input, pos).is_none_or(|c| !c.is_alphanumeric()),
    }
}

/// Decode the UTF-8 character whose final byte is at `end - 1` (i.e. the
/// character immediately preceding byte offset `end`). Returns `None` at the
/// start of input or if the bytes are not valid UTF-8.
fn char_ending_at(input: &[u8], end: usize) -> Option<char> {
    if end == 0 || end > input.len() {
        return None;
    }
    // Walk back over UTF-8 continuation bytes (0b10xx_xxxx) to the lead byte.
    let mut start = end - 1;
    while start > 0
        && input
            .get(start)
            .is_some_and(|&b| b & 0b1100_0000 == 0b1000_0000)
    {
        start -= 1;
    }
    std::str::from_utf8(input.get(start..end)?)
        .ok()?
        .chars()
        .next()
}

/**
Check whether a constrained closing delimiter at `end` is valid.

If `end` is at the end of the input, the outer delimiter must not be a word
character (otherwise the markup would be adjacent to a word character in the
parent context).
*/
pub(super) fn check_constrained_closing_at_end(
    end: usize,
    input_len: usize,
    outer_delimiter: Option<u8>,
) -> bool {
    end < input_len || outer_delimiter.is_none_or(|d| !is_word_char(d))
}
