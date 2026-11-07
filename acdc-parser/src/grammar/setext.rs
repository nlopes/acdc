use crate::CompatibilityMode;

/// Map a legacy heading underline to its section level.
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

/// Asciidoctor allows at most one character of difference.
pub(crate) fn width_ok(title_width: usize, underline_width: usize) -> bool {
    title_width.abs_diff(underline_width) < 2
}

pub(crate) fn is_enabled(mode: CompatibilityMode) -> bool {
    mode == CompatibilityMode::Asciidoctor
}

pub(crate) fn is_title(title: &str) -> bool {
    !title.starts_with('.') && title.chars().any(char::is_alphanumeric)
}
