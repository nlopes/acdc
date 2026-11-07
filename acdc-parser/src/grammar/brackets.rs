//! Bracket boundaries shared by inline preprocessing and parsing.

/// Find the closing bracket after `start`, preserving nested and escaped brackets.
/// `start` is the first byte after the opening bracket. Missing closes return `None`.
pub(super) fn escaped_bracket_end(input: &str, start: usize) -> Option<usize> {
    let mut bytes = input.get(start..)?.bytes().enumerate().peekable();
    let mut depth = 0usize;
    // Document-controlled bracket depth must not consume call stack space.
    while let Some((offset, byte)) = bytes.next() {
        match byte {
            b'\\'
                if bytes
                    .peek()
                    .is_some_and(|(_, next)| matches!(next, b'[' | b']')) =>
            {
                bytes.next();
            }
            b'[' => depth += 1,
            b']' if depth == 0 => return Some(start + offset),
            b']' => depth -= 1,
            _ => {}
        }
    }
    None
}
