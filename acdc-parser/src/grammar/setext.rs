use super::ParserState;

/// Map a Setext underline character to its corresponding section level.
///
/// | Character | Level                    |
/// |-----------|--------------------------|
/// | `=`       | 0 (document title)       |
/// | `-`       | 1                        |
/// | `~`       | 2                        |
/// | `^`       | 3                        |
/// | `+`       | 4                        |
///
/// Returns `None` when the setext feature is not compiled in.
#[cfg(feature = "setext")]
pub(crate) fn char_to_level(ch: char) -> Option<u8> {
    match ch {
        '=' => Some(0),
        '-' => Some(1),
        '~' => Some(2),
        '^' => Some(3),
        '+' => Some(4),
        _ => None,
    }
}

#[cfg(not(feature = "setext"))]
pub(crate) fn char_to_level(_ch: char) -> Option<u8> {
    None
}

/// Check if underline width is within tolerance of title width (±2 characters).
///
/// Returns `false` when the setext feature is not compiled in.
#[cfg(feature = "setext")]
pub(crate) fn width_ok(title_width: usize, underline_width: usize) -> bool {
    title_width.abs_diff(underline_width) <= 2
}

#[cfg(not(feature = "setext"))]
pub(crate) fn width_ok(_title_width: usize, _underline_width: usize) -> bool {
    false
}

/// Check if setext mode is enabled in the parser state.
///
/// When the `setext` feature is not compiled in, this always returns false.
/// When the `setext` feature is compiled in, it checks the runtime option.
#[cfg(feature = "setext")]
pub(crate) fn is_enabled(state: &ParserState) -> bool {
    state.options.setext
}

#[cfg(not(feature = "setext"))]
pub(crate) fn is_enabled(_state: &ParserState) -> bool {
    false
}
