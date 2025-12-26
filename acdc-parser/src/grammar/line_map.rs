use crate::Position;

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
