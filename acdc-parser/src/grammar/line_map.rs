use std::cell::Cell;

use crate::{Position, model::SourceRange};

/// Pre-calculated line position map for efficient offset-to-position conversion.
///
/// `LineMap` scans the input once to build a sorted list of line start offsets
/// plus a per-line ASCII flag, then provides `O(log n)` line lookups and
/// `O(1)` column computation for the common all-ASCII case. Unicode columns
/// count from the last queried position on the same line, in either direction.
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
    /// Last UTF-8 boundary and its zero-based character column. Nearby lookups
    /// can count forward or backward instead of rescanning a long Unicode line.
    column_offset: usize,
    characters_before: usize,
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
            column_offset: range_start,
            characters_before: 0,
        }));

        (line_idx, range_start)
    }

    /// Convert byte offset to `Position` using binary search — `O(log n)`
    /// line lookup, `O(1)` column for ASCII lines, `O(line_length)` column
    /// for lines with non-ASCII content in the worst case. Nearby Unicode
    /// lookups reuse the last character column. Safe for PEG backtracking.
    #[tracing::instrument(level = "trace", skip_all, fields(offset = offset))]
    pub(super) fn offset_to_position(&self, offset: usize, input: &str) -> Position {
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
        // before this optimization.
        let chars_in_line = if self.line_is_ascii.get(line_idx).copied().unwrap_or(false) {
            adjusted_offset - line_start_byte
        } else {
            self.non_ascii_column(adjusted_offset, line_start_byte, input)
        };

        Position::new(
            u32::try_from(line).unwrap_or(u32::MAX),
            u32::try_from(chars_in_line + 1).unwrap_or(u32::MAX),
        )
    }

    fn non_ascii_column(&self, offset: usize, line_start: usize, input: &str) -> usize {
        let Some(mut cached) = self.last_line.get() else {
            return input
                .get(line_start..offset)
                .map_or(0, |text| text.chars().count());
        };
        let characters = if offset >= cached.column_offset {
            cached.characters_before
                + input
                    .get(cached.column_offset..offset)
                    .map_or(0, |text| text.chars().count())
        } else {
            cached.characters_before
                - input
                    .get(offset..cached.column_offset)
                    .map_or(0, |text| text.chars().count())
        };
        cached.column_offset = offset;
        cached.characters_before = characters;
        self.last_line.set(Some(cached));
        characters
    }

    /// Source line (1-indexed) for a preprocessed `offset` in a span that begins at
    /// origin line `start_line` and preprocessed line `preproc_start_line`: `start_line`
    /// plus the number of lines between the span start and `offset`.
    ///
    /// The remap and the diagnostics both call this (the latter via
    /// [`source_line`](Self::source_line)), so they count lines the same way and can't
    /// disagree for the same byte. Callers with the start line already cached (the remap,
    /// once per range) save a lookup by calling this instead of `source_line`.
    pub(super) fn source_line_from(
        &self,
        start_line: u32,
        preproc_start_line: u32,
        input: &str,
        offset: usize,
    ) -> u32 {
        let offset_line = self.offset_to_position(offset, input).line;
        start_line + offset_line.saturating_sub(preproc_start_line)
    }

    /// Original-source line for a preprocessed `offset` inside `range`, deriving the
    /// range's preprocessed start line on the fly. Convenience over
    /// [`source_line_from`](Self::source_line_from) for callers without a cached start
    /// line (the rare diagnostic paths).
    pub(crate) fn source_line(&self, range: &SourceRange, input: &str, offset: usize) -> u32 {
        let preproc_start_line = self.offset_to_position(range.start_offset, input).line;
        self.source_line_from(
            u32::try_from(range.start_line).unwrap_or(u32::MAX),
            preproc_start_line,
            input,
            offset,
        )
    }
}

#[cfg(test)]
mod tests;
