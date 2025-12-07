//! UTF-8 aware utilities for safe string offset manipulation

/// Safely increment a byte offset to the next UTF-8 character boundary.
///
/// This is used when we want to expand a location to include the next character
/// but need to ensure we land on a valid UTF-8 boundary.
pub fn safe_increment_offset(input: &str, offset: usize) -> usize {
    if offset >= input.len() {
        return input.len();
    }

    // Find the next character boundary after offset
    let mut next_boundary = offset + 1;
    while next_boundary < input.len() && !input.is_char_boundary(next_boundary) {
        next_boundary += 1;
    }
    next_boundary
}

/// Safely decrement a byte offset by 1 byte, ensuring the result
/// lands on a valid UTF-8 character boundary.
///
/// This is used when we want to exclude a trailing byte (like a newline)
/// but need to ensure we land on a valid UTF-8 boundary.
pub fn safe_decrement_offset(input: &str, offset: usize) -> usize {
    if offset == 0 {
        return 0;
    }

    // We want to go back by just 1 byte, but ensure it's a valid boundary
    let target = offset.saturating_sub(1);

    // If the target is already a valid boundary, use it
    if input.is_char_boundary(target) {
        target
    } else {
        // Otherwise, find the start of the character we're in the middle of
        let mut boundary = target;
        while boundary > 0 && !input.is_char_boundary(boundary) {
            boundary -= 1;
        }
        boundary
    }
}

/// Ensure an offset is on a valid UTF-8 character boundary.
/// If not, round backward to the nearest valid boundary.
///
/// This is the "safe" way to adjust an offset that might be mid-character.
/// Use this for start positions where you want to include the current character.
pub fn ensure_char_boundary(input: &str, offset: usize) -> usize {
    if offset >= input.len() {
        input.len()
    } else if input.is_char_boundary(offset) {
        offset
    } else {
        // Find the previous valid character boundary
        (0..=offset)
            .rev()
            .find(|&i| input.is_char_boundary(i))
            .unwrap_or(0)
    }
}

/// Ensure an offset is on a valid UTF-8 character boundary.
/// If not, round forward to the nearest valid boundary.
///
/// This is the "safe" way to adjust an offset that might be mid-character.
/// Use this for end positions where you want to include the current character.
pub fn ensure_char_boundary_forward(input: &str, offset: usize) -> usize {
    if offset >= input.len() {
        input.len()
    } else if input.is_char_boundary(offset) {
        offset
    } else {
        // Find the next valid character boundary
        (offset..=input.len())
            .find(|&i| input.is_char_boundary(i))
            .unwrap_or(input.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_decrement_ascii() {
        let input = "hello";
        assert_eq!(safe_decrement_offset(input, 5), 4);
        assert_eq!(safe_decrement_offset(input, 1), 0);
        assert_eq!(safe_decrement_offset(input, 0), 0);
    }

    #[test]
    fn test_safe_decrement_emoji() {
        let input = "😀"; // 4 bytes, single character
        // When input is just emoji with no newline, offset 4 - 1 = 3 (middle of emoji)
        // Should round down to start of emoji at 0
        assert_eq!(safe_decrement_offset(input, 4), 0);
        assert_eq!(safe_decrement_offset(input, 3), 0);
        assert_eq!(safe_decrement_offset(input, 2), 0);
        assert_eq!(safe_decrement_offset(input, 1), 0);
    }

    #[test]
    fn test_safe_decrement_mixed() {
        let input = "a😀b"; // 1 + 4 + 1 = 6 bytes
        assert_eq!(safe_decrement_offset(input, 6), 5); // From after 'b' to start of 'b'
        assert_eq!(safe_decrement_offset(input, 5), 1); // From start of 'b' back 1, hits emoji, go to after 'a'
        assert_eq!(safe_decrement_offset(input, 1), 0); // From after 'a' to start
    }

    #[test]
    fn test_safe_decrement_with_newline() {
        let input = "😀\n"; // 4 + 1 = 5 bytes
        assert_eq!(safe_decrement_offset(input, 5), 4); // From after \n to start of \n

        let input2 = "a\n";
        assert_eq!(safe_decrement_offset(input2, 2), 1); // From after \n to start of \n
    }

    #[test]
    fn test_ensure_boundary() {
        let input = "😀";
        assert_eq!(ensure_char_boundary(input, 0), 0);
        assert_eq!(ensure_char_boundary(input, 1), 0); // Round back
        assert_eq!(ensure_char_boundary(input, 2), 0); // Round back
        assert_eq!(ensure_char_boundary(input, 3), 0); // Round back
        assert_eq!(ensure_char_boundary(input, 4), 4);
    }

    #[test]
    fn test_ensure_boundary_forward() {
        let input = "😀"; // 4 bytes
        assert_eq!(ensure_char_boundary_forward(input, 0), 0);
        assert_eq!(ensure_char_boundary_forward(input, 1), 4); // Round forward
        assert_eq!(ensure_char_boundary_forward(input, 2), 4); // Round forward
        assert_eq!(ensure_char_boundary_forward(input, 3), 4); // Round forward
        assert_eq!(ensure_char_boundary_forward(input, 4), 4);
    }

    #[test]
    fn test_ensure_boundary_forward_mixed() {
        let input = "a😀b"; // 1 + 4 + 1 = 6 bytes
        assert_eq!(ensure_char_boundary_forward(input, 0), 0);
        assert_eq!(ensure_char_boundary_forward(input, 1), 1); // Already valid
        assert_eq!(ensure_char_boundary_forward(input, 2), 5); // Mid-emoji, round forward to 'b'
        assert_eq!(ensure_char_boundary_forward(input, 3), 5);
        assert_eq!(ensure_char_boundary_forward(input, 4), 5);
        assert_eq!(ensure_char_boundary_forward(input, 5), 5);
        assert_eq!(ensure_char_boundary_forward(input, 6), 6);
    }

    #[test]
    fn test_ensure_boundary_forward_beyond_input() {
        let input = "hello";
        assert_eq!(ensure_char_boundary_forward(input, 100), 5);
    }
}
