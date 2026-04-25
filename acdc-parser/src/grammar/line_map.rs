use std::cell::Cell;

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
    /// Byte offsets where each line starts in the input
    line_starts: Vec<usize>,
    /// Per-line flag: `true` when every byte of the line is ASCII (< 0x80).
    /// For ASCII lines, column is just `offset - line_start_byte` (no scan).
    /// Indexed by zero-based line number, same length as `line_starts`.
    line_is_ascii: Vec<bool>,
    /// Monotonic-access cache: the last resolved line index and the byte
    /// range `[start, end)` of that line. PEG parsing advances mostly
    /// forward, so consecutive lookups usually land on the same line or
    /// the next one — letting us skip the `O(log n)` binary search.
    /// Set to `None` before the first lookup.
    last_line: Cell<Option<CachedLine>>,
}

#[derive(Debug, Clone, Copy)]
struct CachedLine {
    line_idx: usize,
    range_start: usize,
    range_end: usize,
}

impl LineMap {
    /// Build line map by scanning input once during initialization.
    /// This is called once before parsing starts.
    pub(crate) fn new(input: &str) -> Self {
        let mut line_starts = vec![0]; // Line 1 starts at byte offset 0
        let mut line_is_ascii = Vec::new();
        let mut current_line_ascii = true;

        // Iterate raw bytes rather than char_indices: `\n` is always a
        // single-byte character in UTF-8, so we can't miss a line break,
        // and we can incrementally track ASCII-ness per line without a
        // second pass.
        for (i, &b) in input.as_bytes().iter().enumerate() {
            if b >= 0x80 {
                current_line_ascii = false;
            }
            if b == b'\n' {
                line_is_ascii.push(current_line_ascii);
                line_starts.push(i + 1); // Next line starts after the newline
                current_line_ascii = true;
            }
        }
        // Flush the trailing line (or the only line, if there is no newline).
        line_is_ascii.push(current_line_ascii);

        Self {
            line_starts,
            line_is_ascii,
            last_line: Cell::new(None),
        }
    }

    /// Resolve `offset` to a 1-based line number.
    ///
    /// Fast path: consecutive lookups on the same line hit the cache and skip
    /// the binary search. When the cache misses, falls back to
    /// `binary_search` and refreshes the cache so the next call on the same
    /// (or adjacent) line is O(1).
    fn line_for_offset(&self, offset: usize) -> (usize, usize) {
        if let Some(cached) = self.last_line.get()
            && offset >= cached.range_start
            && offset < cached.range_end
        {
            return (cached.line_idx, cached.range_start);
        }

        let line = match self.line_starts.binary_search(&offset) {
            Ok(line_idx) => line_idx + 1, // Exact match: start of this line
            Err(line_idx) => line_idx,    // Insert position: this line number
        };

        let line_idx = line.saturating_sub(1);
        let range_start = self.line_starts.get(line_idx).copied().unwrap_or(0);
        let range_end = self
            .line_starts
            .get(line_idx + 1)
            .copied()
            .unwrap_or(usize::MAX);

        self.last_line.set(Some(CachedLine {
            line_idx,
            range_start,
            range_end,
        }));

        (line_idx, range_start)
    }

    /// Convert byte offset to `Position` using binary search — `O(log n)`
    /// line lookup, `O(1)` column for ASCII lines, `O(line_length)` column
    /// for lines with non-ASCII content. Pure function, safe for use in PEG
    /// action blocks.
    #[tracing::instrument(level = "debug")]
    pub(crate) fn offset_to_position(&self, offset: usize, input: &str) -> Position {
        let (line_idx, line_start_byte) = self.line_for_offset(offset);
        let line = line_idx + 1;

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

        // Fast path: if the containing line is pure ASCII, char count equals
        // byte count, so column is a subtraction. This avoids the per-call
        // `is_ascii()` scan (which itself is O(line_length)) plus the
        // `chars().count()` fallback scan — together the dominant hot spot
        // in large-document parses, accounting for ~24% of parser self-time
        // before this optimisation.
        let chars_in_line = if self.line_is_ascii.get(line_idx).copied().unwrap_or(false) {
            adjusted_offset - line_start_byte
        } else {
            input
                .get(line_start_byte..adjusted_offset)
                .map_or(0, |s| s.chars().count())
        };

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

    #[test]
    fn test_line_map_utf8_content() {
        // "é" is 2 bytes in UTF-8 (0xC3 0xA9). Line 1 is non-ASCII, line 2 is ASCII.
        // Bytes: c(0) a(1) f(2) é(3,4) \n(5) b(6) a(7) r(8)
        let input = "caf\u{00e9}\nbar";
        let line_map = LineMap::new(input);

        // Slow path: non-ASCII line, column via `chars().count()`.
        // Offset 5 = byte just past "café" (the '\n'); binary_search Err(1).
        let pos = line_map.offset_to_position(5, input);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.column, 5); // 4 chars in "café" + 1

        // Fast path on the line *after* a non-ASCII line: line_is_ascii[1] is true.
        let pos = line_map.offset_to_position(8, input);
        assert_eq!(pos.line, 2);
        assert_eq!(pos.column, 3);

        // Mid-char offset: offset 4 lands inside the 2-byte 'é' and must
        // round back to byte 3 before counting chars.
        let pos = line_map.offset_to_position(4, input);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.column, 4); // "caf" = 3 chars + 1

        // Exact match on a line start that follows a non-ASCII line
        // (binary_search Ok arm, guarding line-index arithmetic).
        let pos = line_map.offset_to_position(6, input);
        assert_eq!(pos.line, 2);
        assert_eq!(pos.column, 1);
    }
}
