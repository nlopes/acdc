//! UTF-8 aware utilities for safe string offset manipulation.
//!
//! All functions in this module guarantee that returned offsets are valid UTF-8 character
//! boundaries within the given input string.

/// Direction for rounding or stepping through UTF-8 character boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoundDirection {
    /// Round backward (toward start of string) to find a valid boundary.
    Backward,
    /// Round forward (toward end of string) to find a valid boundary.
    Forward,
}

/// Snap an offset to the nearest valid UTF-8 character boundary.
///
/// If the offset is already on a valid boundary, returns it unchanged.
/// If not, rounds in the specified direction to find the nearest valid boundary.
///
/// # Arguments
/// * `input` - The string to check boundaries against
/// * `offset` - The byte offset to snap
/// * `direction` - Which direction to round if not on a boundary
///
/// # Returns
/// A valid UTF-8 character boundary offset within `[0, input.len()]`.
pub fn snap_to_boundary(input: &str, offset: usize, direction: RoundDirection) -> usize {
    if offset >= input.len() {
        return input.len();
    }
    if input.is_char_boundary(offset) {
        return offset;
    }

    match direction {
        RoundDirection::Backward => {
            // Find the previous valid character boundary
            (0..offset)
                .rev()
                .find(|&i| input.is_char_boundary(i))
                .unwrap_or(0)
        }
        RoundDirection::Forward => {
            // Find the next valid character boundary
            ((offset + 1)..=input.len())
                .find(|&i| input.is_char_boundary(i))
                .unwrap_or(input.len())
        }
    }
}

/// Move one character in the specified direction from the given offset.
///
/// For `Forward`: moves to the start of the next character (or end of string).
/// For `Backward`: moves to the start of the previous character (or start of string).
///
/// The input offset should already be on a valid boundary, but if not, it will be
/// snapped to a boundary first before stepping.
///
/// # Arguments
/// * `input` - The string to navigate
/// * `offset` - The starting byte offset
/// * `direction` - Which direction to step
///
/// # Returns
/// A valid UTF-8 character boundary offset within `[0, input.len()]`.
pub fn step_char(input: &str, offset: usize, direction: RoundDirection) -> usize {
    match direction {
        RoundDirection::Forward => {
            if offset >= input.len() {
                return input.len();
            }
            // Move to next character boundary
            let next = offset + 1;
            if next >= input.len() {
                input.len()
            } else {
                snap_to_boundary(input, next, RoundDirection::Forward)
            }
        }
        RoundDirection::Backward => {
            if offset == 0 {
                return 0;
            }
            let target = offset.saturating_sub(1);
            snap_to_boundary(input, target, RoundDirection::Backward)
        }
    }
}

// --- Backward-compatible function wrappers ---
// These will be removed after all callsites are migrated.

/// Safely increment a byte offset to the next UTF-8 character boundary.
#[inline]
pub fn safe_increment_offset(input: &str, offset: usize) -> usize {
    step_char(input, offset, RoundDirection::Forward)
}

/// Safely decrement a byte offset by one character, ensuring valid UTF-8 boundary.
#[inline]
pub fn safe_decrement_offset(input: &str, offset: usize) -> usize {
    step_char(input, offset, RoundDirection::Backward)
}

/// Ensure an offset is on a valid UTF-8 character boundary (round backward).
#[inline]
pub fn ensure_char_boundary(input: &str, offset: usize) -> usize {
    snap_to_boundary(input, offset, RoundDirection::Backward)
}

/// Ensure an offset is on a valid UTF-8 character boundary (round forward).
#[inline]
pub fn ensure_char_boundary_forward(input: &str, offset: usize) -> usize {
    snap_to_boundary(input, offset, RoundDirection::Forward)
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Tests for new unified API ---

    #[test]
    fn test_snap_backward_on_boundary() {
        let input = "hello";
        assert_eq!(snap_to_boundary(input, 0, RoundDirection::Backward), 0);
        assert_eq!(snap_to_boundary(input, 3, RoundDirection::Backward), 3);
        assert_eq!(snap_to_boundary(input, 5, RoundDirection::Backward), 5);
    }

    #[test]
    fn test_snap_backward_mid_char() {
        let input = "😀"; // 4 bytes
        assert_eq!(snap_to_boundary(input, 1, RoundDirection::Backward), 0);
        assert_eq!(snap_to_boundary(input, 2, RoundDirection::Backward), 0);
        assert_eq!(snap_to_boundary(input, 3, RoundDirection::Backward), 0);
    }

    #[test]
    fn test_snap_forward_on_boundary() {
        let input = "hello";
        assert_eq!(snap_to_boundary(input, 0, RoundDirection::Forward), 0);
        assert_eq!(snap_to_boundary(input, 3, RoundDirection::Forward), 3);
        assert_eq!(snap_to_boundary(input, 5, RoundDirection::Forward), 5);
    }

    #[test]
    fn test_snap_forward_mid_char() {
        let input = "😀"; // 4 bytes
        assert_eq!(snap_to_boundary(input, 1, RoundDirection::Forward), 4);
        assert_eq!(snap_to_boundary(input, 2, RoundDirection::Forward), 4);
        assert_eq!(snap_to_boundary(input, 3, RoundDirection::Forward), 4);
    }

    #[test]
    fn test_snap_beyond_input() {
        let input = "hello";
        assert_eq!(snap_to_boundary(input, 100, RoundDirection::Backward), 5);
        assert_eq!(snap_to_boundary(input, 100, RoundDirection::Forward), 5);
    }

    #[test]
    fn test_step_forward() {
        let input = "a😀b"; // 1 + 4 + 1 = 6 bytes
        assert_eq!(step_char(input, 0, RoundDirection::Forward), 1); // a -> emoji start
        assert_eq!(step_char(input, 1, RoundDirection::Forward), 5); // emoji start -> b
        assert_eq!(step_char(input, 5, RoundDirection::Forward), 6); // b -> end
        assert_eq!(step_char(input, 6, RoundDirection::Forward), 6); // end -> end
    }

    #[test]
    fn test_step_backward() {
        let input = "a😀b"; // 1 + 4 + 1 = 6 bytes
        assert_eq!(step_char(input, 6, RoundDirection::Backward), 5); // end -> b
        assert_eq!(step_char(input, 5, RoundDirection::Backward), 1); // b -> emoji start
        assert_eq!(step_char(input, 1, RoundDirection::Backward), 0); // emoji start -> a
        assert_eq!(step_char(input, 0, RoundDirection::Backward), 0); // start -> start
    }

    // --- Tests for backward-compatible wrappers ---

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
        assert_eq!(safe_decrement_offset(input, 4), 0);
        assert_eq!(safe_decrement_offset(input, 3), 0);
        assert_eq!(safe_decrement_offset(input, 2), 0);
        assert_eq!(safe_decrement_offset(input, 1), 0);
    }

    #[test]
    fn test_safe_decrement_mixed() {
        let input = "a😀b"; // 1 + 4 + 1 = 6 bytes
        assert_eq!(safe_decrement_offset(input, 6), 5);
        assert_eq!(safe_decrement_offset(input, 5), 1);
        assert_eq!(safe_decrement_offset(input, 1), 0);
    }

    #[test]
    fn test_safe_decrement_with_newline() {
        let input = "😀\n"; // 4 + 1 = 5 bytes
        assert_eq!(safe_decrement_offset(input, 5), 4);

        let input2 = "a\n";
        assert_eq!(safe_decrement_offset(input2, 2), 1);
    }

    #[test]
    fn test_ensure_boundary() {
        let input = "😀";
        assert_eq!(ensure_char_boundary(input, 0), 0);
        assert_eq!(ensure_char_boundary(input, 1), 0);
        assert_eq!(ensure_char_boundary(input, 2), 0);
        assert_eq!(ensure_char_boundary(input, 3), 0);
        assert_eq!(ensure_char_boundary(input, 4), 4);
    }

    #[test]
    fn test_ensure_boundary_forward() {
        let input = "😀"; // 4 bytes
        assert_eq!(ensure_char_boundary_forward(input, 0), 0);
        assert_eq!(ensure_char_boundary_forward(input, 1), 4);
        assert_eq!(ensure_char_boundary_forward(input, 2), 4);
        assert_eq!(ensure_char_boundary_forward(input, 3), 4);
        assert_eq!(ensure_char_boundary_forward(input, 4), 4);
    }

    #[test]
    fn test_ensure_boundary_forward_mixed() {
        let input = "a😀b"; // 1 + 4 + 1 = 6 bytes
        assert_eq!(ensure_char_boundary_forward(input, 0), 0);
        assert_eq!(ensure_char_boundary_forward(input, 1), 1);
        assert_eq!(ensure_char_boundary_forward(input, 2), 5);
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
