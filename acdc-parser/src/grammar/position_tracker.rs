use crate::{Location, Position};

/// Mutable position tracker for incremental parsing.
///
/// `PositionTracker` maintains position state (line, column, byte offset) as text is consumed.
/// It's designed for linear parsing scenarios where positions are computed incrementally.
///
/// # When to Use
///
/// Use `PositionTracker` when:
/// - Parsing linearly without backtracking (e.g., inline preprocessor)
/// - You need to create `Location` objects as you parse
/// - Position computation happens during text consumption
///
/// Use [`LineMap`] instead when:
/// - You need random access to positions from byte offsets
/// - The input is fully available and you want O(log n) lookups
/// - You're in a backtracking parser (PEG) where mutable state is problematic
///
/// # Note on `LineMap` Migration
///
/// The inline preprocessor currently uses `PositionTracker` because:
/// 1. It parses linearly and computes positions as it goes
/// 2. Migrating to `LineMap` would require threading it through PEG rules
/// 3. Both produce identical results (verified by tests)
///
/// A future optimization could migrate to `LineMap` if the inline preprocessor
/// gains access to the main parser state's `LineMap`.
#[derive(Clone, Debug, PartialEq, Copy)]
pub(crate) struct PositionTracker {
    line: usize,
    column: usize,
    offset: usize,
}

impl Default for PositionTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl PositionTracker {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self {
            line: 1,
            column: 1,
            offset: 0,
        }
    }

    #[tracing::instrument(level = "debug")]
    pub(crate) fn set_initial_position(&mut self, location: &Location, absolute_offset: usize) {
        self.line = location.start.line;
        self.column = location.start.column;
        self.offset = absolute_offset;
    }

    #[tracing::instrument(level = "debug")]
    pub(crate) fn get_position(&self) -> Position {
        Position {
            line: self.line,
            column: self.column,
        }
    }

    pub(crate) fn get_offset(&self) -> usize {
        self.offset
    }

    // TODO(nlopes): check if `#[inline(always)]` will help
    #[tracing::instrument(level = "debug")]
    pub(crate) fn advance(&mut self, s: &str) {
        // TODO(nlopes): we need a better way to handle this due to unicode characters.
        for c in s.chars() {
            if c == '\n' {
                self.line += 1;
                self.column = 1;
            } else {
                self.column += 1;
            }
        }
        self.offset += s.len();
    }

    pub(crate) fn advance_by(&mut self, n: usize) {
        self.column += n;
        self.offset += n;
    }

    pub(crate) fn calculate_location(
        &mut self,
        start: Position,
        content: &str,
        padding: usize,
    ) -> Location {
        let absolute_start = self.get_offset();
        self.advance(content);
        self.advance_by(padding);
        let absolute_end = self.get_offset();
        let end = self.get_position();
        Location {
            absolute_start,
            absolute_end,
            start,
            end,
        }
    }
}

/// Pre-calculated line position map for efficient offset-to-position conversion.
///
/// `LineMap` scans the input once to build a sorted list of line start offsets,
/// then provides O(log n) binary search lookups for any byte offset.
///
/// # Key Properties
///
/// - **Immutable**: Safe for use in PEG action blocks and backtracking parsers
/// - **Efficient**: O(n) construction, O(log n) lookups
/// - **UTF-8 aware**: Handles multi-byte characters correctly
///
/// # Usage
///
/// ```ignore
/// let line_map = LineMap::new(input);
/// let position = line_map.offset_to_position(byte_offset, input);
/// ```
///
/// See [`PositionTracker`] for an alternative that computes positions incrementally.
#[derive(Debug, Clone)]
pub(crate) struct LineMap {
    /// Byte offsets where each line starts in the input
    line_starts: Vec<usize>,
}

impl LineMap {
    /// Build line map by scanning input once during initialization.
    /// This is called once before parsing starts.
    pub(crate) fn new(input: &str) -> Self {
        let mut line_starts = vec![0]; // Line 1 starts at byte offset 0

        for (offset, ch) in input.char_indices() {
            if ch == '\n' {
                line_starts.push(offset + 1); // Next line starts after the newline (byte offset)
            }
        }

        Self { line_starts }
    }

    /// Convert byte offset to Position using binary search - O(log n) lookup.
    /// This is a pure function with no side effects, safe for use in PEG action blocks.
    /// Columns are counted as Unicode scalar values (characters), not bytes.
    #[tracing::instrument(level = "debug")]
    pub(crate) fn offset_to_position(&self, offset: usize, input: &str) -> Position {
        // Find which line this offset belongs to
        let line = match self.line_starts.binary_search(&offset) {
            Ok(line_idx) => line_idx + 1, // Exact match: start of this line
            Err(line_idx) => line_idx,    // Insert position: this line number
        };

        // Get the byte offset at the start of this line
        let line_start_byte = self
            .line_starts
            .get(line.saturating_sub(1))
            .copied()
            .unwrap_or(0);

        // Ensure the offset doesn't land in the middle of a multi-byte UTF-8 character.
        // If it does, round backward to the start of the current character.
        let adjusted_offset = if offset > input.len() {
            input.len()
        } else if input.is_char_boundary(offset) {
            offset
        } else {
            // Find the previous valid character boundary (start of current char)
            (0..=offset)
                .rev()
                .find(|&i| input.is_char_boundary(i))
                .unwrap_or(0)
        };

        // Count characters from line start to current offset
        let chars_in_line = input
            .get(line_start_byte..adjusted_offset)
            .map_or(0, |s| s.chars().count());

        Position {
            line,
            column: chars_in_line + 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_line_map_single_line() {
        let input = "Hello, World!";
        let line_map = LineMap::new(input);

        assert_eq!(line_map.line_starts, vec![0]);

        // Start of input
        let pos = line_map.offset_to_position(0, input);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.column, 1);

        // Middle of line
        let pos = line_map.offset_to_position(7, input);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.column, 8);

        // End of line
        let pos = line_map.offset_to_position(12, input);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.column, 13);
    }

    #[test]
    fn test_line_map_multiple_lines() {
        let input = "Line 1\nLine 2\nLine 3";
        let line_map = LineMap::new(input);

        assert_eq!(line_map.line_starts, vec![0, 7, 14]);

        // Start of first line
        let pos = line_map.offset_to_position(0, input);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.column, 1);

        // End of first line (before newline)
        let pos = line_map.offset_to_position(6, input);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.column, 7);

        // Start of second line
        let pos = line_map.offset_to_position(7, input);
        assert_eq!(pos.line, 2);
        assert_eq!(pos.column, 1);

        // Middle of second line
        let pos = line_map.offset_to_position(10, input);
        assert_eq!(pos.line, 2);
        assert_eq!(pos.column, 4);

        // Start of third line
        let pos = line_map.offset_to_position(14, input);
        assert_eq!(pos.line, 3);
        assert_eq!(pos.column, 1);
    }

    #[test]
    fn test_line_map_empty_lines() {
        let input = "Line 1\n\nLine 3";
        let line_map = LineMap::new(input);

        assert_eq!(line_map.line_starts, vec![0, 7, 8]);

        // Start of empty line
        let pos = line_map.offset_to_position(7, input);
        assert_eq!(pos.line, 2);
        assert_eq!(pos.column, 1);

        // Start of line after empty line
        let pos = line_map.offset_to_position(8, input);
        assert_eq!(pos.line, 3);
        assert_eq!(pos.column, 1);
    }

    #[test]
    fn test_line_map_matches_position_tracker() {
        let input = "= Document Title\nAuthor Name\nv1.0, 2024: Revision";
        let line_map = LineMap::new(input);

        // Test various positions and compare with position tracker
        for i in 0..input.len() {
            let line_map_pos = line_map.offset_to_position(i, input);

            let mut tracker = PositionTracker::new();
            tracker.advance(&input[..i]);
            let tracker_pos = tracker.get_position();

            assert_eq!(
                line_map_pos.line, tracker_pos.line,
                "Line mismatch at offset {i}",
            );
            assert_eq!(
                line_map_pos.column, tracker_pos.column,
                "Column mismatch at offset {i}",
            );
        }
    }

    #[test]
    fn test_line_map_asciidoc_example() {
        let input = "= Document Title\nLorn_Kismet R. Lee <kismet@asciidoctor.org>\nv2.9, 01-09-2024: Fall incarnation\n:description: The document's description.\n:sectanchors:\n:url-repo: https://my-git-repo.com";
        let line_map = LineMap::new(input);

        // Title start (after "= ")
        let pos = line_map.offset_to_position(2, input);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.column, 3);

        // Author line start (17 = length of "= Document Title\n")
        let pos = line_map.offset_to_position(17, input);
        assert_eq!(pos.line, 2);
        assert_eq!(pos.column, 1);

        // Revision line start (61 = 17 + 44, where 44 is length of author line + newline)
        let pos = line_map.offset_to_position(61, input);
        assert_eq!(pos.line, 3);
        assert_eq!(pos.column, 1);
    }

    #[test]
    fn test_line_map_beyond_input() {
        let input = "Hello";
        let line_map = LineMap::new(input);

        // Beyond input: offset is clamped to input.len(), giving position after last character
        let pos = line_map.offset_to_position(100, input);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.column, 6); // After 5 characters, column is 6
    }

    #[test]
    fn test_line_map_empty_input() {
        let input = "";
        let line_map = LineMap::new(input);

        assert_eq!(line_map.line_starts, vec![0]);

        let pos = line_map.offset_to_position(0, input);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.column, 1);
    }
}
