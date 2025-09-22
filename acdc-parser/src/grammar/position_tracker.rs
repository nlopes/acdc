use crate::{Location, Position};

/// A `PositionTracker` is used to track the position of a parser.
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
/// This provides O(log n) position lookups and is backtrack-safe since it's immutable.
#[derive(Debug, Clone)]
pub(crate) struct LineMap {
    /// Byte offsets where each line starts in the input
    line_starts: Vec<usize>,
}

impl LineMap {
    /// Build line map by scanning input once during initialization.
    /// This is called once before parsing starts.
    pub(crate) fn new(input: &str) -> Self {
        let mut line_starts = vec![0]; // Line 1 starts at offset 0

        for (offset, ch) in input.char_indices() {
            if ch == '\n' {
                line_starts.push(offset + 1); // Next line starts after the newline
            }
        }

        Self { line_starts }
    }

    /// Convert byte offset to Position using binary search - O(log n) lookup.
    /// This is a pure function with no side effects, safe for use in PEG action blocks.
    pub(crate) fn offset_to_position(&self, offset: usize) -> Position {
        // Find which line this offset belongs to
        let line = match self.line_starts.binary_search(&offset) {
            Ok(line_idx) => line_idx + 1, // Exact match: start of this line
            Err(line_idx) => line_idx,    // Insert position: this line number
        };

        // Get the start of this line
        let line_start = self
            .line_starts
            .get(line.saturating_sub(1))
            .copied()
            .unwrap_or(0);

        Position {
            line,
            column: offset - line_start + 1,
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
        let pos = line_map.offset_to_position(0);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.column, 1);

        // Middle of line
        let pos = line_map.offset_to_position(7);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.column, 8);

        // End of line
        let pos = line_map.offset_to_position(12);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.column, 13);
    }

    #[test]
    fn test_line_map_multiple_lines() {
        let input = "Line 1\nLine 2\nLine 3";
        let line_map = LineMap::new(input);

        assert_eq!(line_map.line_starts, vec![0, 7, 14]);

        // Start of first line
        let pos = line_map.offset_to_position(0);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.column, 1);

        // End of first line (before newline)
        let pos = line_map.offset_to_position(6);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.column, 7);

        // Start of second line
        let pos = line_map.offset_to_position(7);
        assert_eq!(pos.line, 2);
        assert_eq!(pos.column, 1);

        // Middle of second line
        let pos = line_map.offset_to_position(10);
        assert_eq!(pos.line, 2);
        assert_eq!(pos.column, 4);

        // Start of third line
        let pos = line_map.offset_to_position(14);
        assert_eq!(pos.line, 3);
        assert_eq!(pos.column, 1);
    }

    #[test]
    fn test_line_map_empty_lines() {
        let input = "Line 1\n\nLine 3";
        let line_map = LineMap::new(input);

        assert_eq!(line_map.line_starts, vec![0, 7, 8]);

        // Start of empty line
        let pos = line_map.offset_to_position(7);
        assert_eq!(pos.line, 2);
        assert_eq!(pos.column, 1);

        // Start of line after empty line
        let pos = line_map.offset_to_position(8);
        assert_eq!(pos.line, 3);
        assert_eq!(pos.column, 1);
    }

    #[test]
    fn test_line_map_matches_position_tracker() {
        let input = "= Document Title\nAuthor Name\nv1.0, 2024: Revision";
        let line_map = LineMap::new(input);

        // Test various positions and compare with position tracker
        for i in 0..input.len() {
            let line_map_pos = line_map.offset_to_position(i);

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
        let pos = line_map.offset_to_position(2);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.column, 3);

        // Author line start (17 = length of "= Document Title\n")
        let pos = line_map.offset_to_position(17);
        assert_eq!(pos.line, 2);
        assert_eq!(pos.column, 1);

        // Revision line start (61 = 17 + 44, where 44 is length of author line + newline)
        let pos = line_map.offset_to_position(61);
        assert_eq!(pos.line, 3);
        assert_eq!(pos.column, 1);
    }

    #[test]
    fn test_line_map_beyond_input() {
        let input = "Hello";
        let line_map = LineMap::new(input);

        // Beyond input should still work and return reasonable position
        let pos = line_map.offset_to_position(100);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.column, 101); // Beyond end of input
    }

    #[test]
    fn test_line_map_empty_input() {
        let input = "";
        let line_map = LineMap::new(input);

        assert_eq!(line_map.line_starts, vec![0]);

        let pos = line_map.offset_to_position(0);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.column, 1);
    }
}
