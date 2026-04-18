use crate::Position;

/// Pre-calculated line position map for efficient offset-to-position conversion.
///
/// `LineMap` scans the input once to build a sorted list of line start offsets
/// plus a per-line ASCII flag, then provides `O(log n)` line lookups and
/// `O(1)` column computation for the common all-ASCII case.
///
/// # Key Properties
///
/// - **Immutable**: Safe for use in PEG action blocks and backtracking parsers
/// - **Efficient**: `O(n)` construction, `O(log n)` line lookup, `O(1)` column
///   for ASCII lines, `O(line_length)` column for lines with non-ASCII content
/// - **UTF-8 aware**: Handles multi-byte characters correctly by falling back
///   to `chars().count()` on lines that are not pure ASCII
///
/// # Usage
///
/// ```ignore
/// let line_map = LineMap::new(input);
/// let position = line_map.offset_to_position(byte_offset, input);
/// ```
#[derive(Debug, Clone)]
pub(crate) struct LineMap {
    /// Byte offsets where each line starts in the input.
    line_starts: Vec<usize>,
    /// Per-line flag: `true` when every byte of the line is ASCII (< 0x80).
    /// For ASCII lines, column is just `offset - line_start_byte + 1` — no scan.
    /// Indexed the same way as `line_starts`.
    line_is_ascii: Vec<bool>,
}

impl LineMap {
    /// Build line map by scanning input once during initialization.
    /// This is called once before parsing starts.
    pub(crate) fn new(input: &str) -> Self {
        let mut line_starts = Vec::with_capacity(input.len() / 40 + 1);
        let mut line_is_ascii = Vec::with_capacity(input.len() / 40 + 1);
        line_starts.push(0);

        let mut current_line_ascii = true;
        // Iterate raw bytes: `\n` is always a single-byte character in UTF-8,
        // so we can't miss a line break while still tracking ASCII-ness.
        for (i, &b) in input.as_bytes().iter().enumerate() {
            if b >= 0x80 {
                current_line_ascii = false;
            }
            if b == b'\n' {
                line_is_ascii.push(current_line_ascii);
                line_starts.push(i + 1);
                current_line_ascii = true;
            }
        }
        // Trailing line (no terminating newline or final line after last newline).
        line_is_ascii.push(current_line_ascii);

        Self {
            line_starts,
            line_is_ascii,
        }
    }

    /// Convert byte offset to Position using binary search - O(log n) line
    /// lookup, O(1) column for ASCII lines.
    ///
    /// Hot path — called millions of times on large documents. Do NOT add
    /// `#[tracing::instrument]` here; span construction overhead dominates.
    #[inline]
    pub(crate) fn offset_to_position(&self, offset: usize, input: &str) -> Position {
        // Find which line this offset belongs to
        let line_idx = match self.line_starts.binary_search(&offset) {
            Ok(i) => i, // Exact match: start of line i+1
            Err(i) => i.saturating_sub(1),
        };

        let line_start_byte = self.line_starts.get(line_idx).copied().unwrap_or(0);

        // Ensure the offset doesn't land in the middle of a multi-byte UTF-8 character.
        // UTF-8 chars are at most 4 bytes, so we only look back up to 3 bytes.
        let adjusted_offset = if offset > input.len() {
            input.len()
        } else if input.is_char_boundary(offset) {
            offset
        } else {
            (1..=3)
                .map(|i| offset.saturating_sub(i))
                .find(|&i| input.is_char_boundary(i))
                .unwrap_or(0)
        };

        // Fast path: ASCII-only line → column is byte distance from line start.
        let byte_count = adjusted_offset - line_start_byte;
        let chars_in_line = if self.line_is_ascii.get(line_idx).copied().unwrap_or(false) {
            byte_count
        } else {
            input
                .get(line_start_byte..adjusted_offset)
                .map_or(0, |s| s.chars().count())
        };

        Position {
            line: line_idx + 1,
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

    #[test]
    fn test_line_map_utf8_content() {
        // "é" is 2 bytes in UTF-8 (0xC3 0xA9). Line 1 contains non-ASCII.
        let input = "caf\u{00e9}\nbar";
        let line_map = LineMap::new(input);

        // Line 1 is flagged as non-ASCII (fall back to chars().count())
        let pos = line_map.offset_to_position(5, input); // byte 5 = end of "café"
        assert_eq!(pos.line, 1);
        assert_eq!(pos.column, 5); // 4 chars + 1

        // Line 2 is flagged ASCII (fast path)
        let pos = line_map.offset_to_position(8, input); // byte 8 = "r"
        assert_eq!(pos.line, 2);
        assert_eq!(pos.column, 3);
    }
}
