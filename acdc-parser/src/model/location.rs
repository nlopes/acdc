use std::sync::Arc;

use serde::{
    Serialize,
    ser::{SerializeMap, SerializeSeq, Serializer},
};

/// A range where a specific leveloffset value applies.
///
/// When include directives use `leveloffset=+N`, we track the byte ranges where
/// leveloffsets apply. The parser then queries these ranges to determine the effective
/// leveloffset at any given position.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct LeveloffsetRange {
    /// Byte offset where this leveloffset begins (inclusive).
    pub(crate) start_offset: usize,
    /// Byte offset where this leveloffset ends (exclusive).
    pub(crate) end_offset: usize,
    /// The leveloffset value to apply in this range.
    pub(crate) value: isize,
}

impl LeveloffsetRange {
    /// Create a new leveloffset range.
    #[must_use]
    pub(crate) fn new(start_offset: usize, end_offset: usize, value: isize) -> Self {
        Self {
            start_offset,
            end_offset,
            value,
        }
    }

    /// Check if a byte offset falls within this range.
    #[must_use]
    pub(crate) fn contains(&self, byte_offset: usize) -> bool {
        byte_offset >= self.start_offset && byte_offset < self.end_offset
    }
}

/// Calculate the total leveloffset at a given byte offset.
///
/// Sums all leveloffset values from ranges that contain the given offset.
/// Ranges can nest (include within include), so we sum all applicable values.
#[must_use]
pub(crate) fn calculate_leveloffset_at(ranges: &[LeveloffsetRange], byte_offset: usize) -> isize {
    ranges
        .iter()
        .filter_map(|r| {
            if r.contains(byte_offset) {
                Some(r.value)
            } else {
                None
            }
        })
        .sum()
}

/// Maps a byte range in the preprocessed output back to its source file and starting line.
///
/// When `include::` directives merge content into a single string, we track which byte
/// ranges came from which files. The parser queries these ranges to produce accurate
/// file names and line numbers in warnings and errors.
///
/// The two coordinate spaces and how a `SourceRange` bridges them:
///
/// ```text
/// PREPROCESSED BUFFER (parser coords)     ORIGIN FILE (ASG / diagnostics coords)
///   byte 7        13       19               byte 0      6
///        |        |        |                     |      |
///    ... inc 1\n  inc 2\n ...                inc 1\n inc 2\n
///        `---- this range ----'              `-l1-' `-l2-'
///        [start_offset, end_offset)          start_line = 1, source_start_offset = 0
///
/// preproc abs 13  ->  source_offset = source_start_offset + (abs - start_offset) = 6
///                     source_line   = start_line          + (preproc lines crossed) = 2
/// ```
///
/// So `start_offset`/`end_offset` locate the span in the preprocessed buffer, while
/// `start_line`/`source_start_offset` are its anchor in the origin file. The byte
/// mapping is [`source_offset`](Self::source_offset); the line mapping is
/// `LineMap::source_line` (one shared primitive for the remap and diagnostics).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SourceRange {
    /// Byte offset where this source range begins (inclusive), in the preprocessed buffer.
    pub(crate) start_offset: usize,
    /// Byte offset where this source range ends (exclusive), in the preprocessed buffer.
    pub(crate) end_offset: usize,
    /// The *resolved* path of the file this range came from, for diagnostics (so the
    /// CLI can open it to render a snippet). `None` for the primary input when it has
    /// no path (string/stdin), which still needs same-file line/offset remapping.
    pub(crate) file: Option<std::path::PathBuf>,
    /// The `include::` chain reaching this range — each element an include target *as
    /// written* in its directive, outermost first. Empty for primary-input content
    /// (the ASG omits the primary file). Feeds the per-`Position` ASG `file` array.
    pub(crate) file_chain: Vec<String>,
    /// The line number (1-indexed) of the first line in this range within the source file.
    pub(crate) start_line: usize,
    /// The byte offset (in the *origin file*) of the first byte in this range. Combined
    /// with `start_offset`, this maps a preprocessed offset back to an original-file offset:
    /// `source_offset = source_start_offset + (preproc_offset - start_offset)`.
    pub(crate) source_start_offset: usize,
    /// Columns added (or removed, if negative) per line by an `include::[indent=N]`
    /// re-indent: `N − stripped_common_indent`, uniform across the block. The remap
    /// subtracts it to recover origin columns. `0` for untransformed ranges, whose
    /// preprocessed↔origin mapping is byte-for-byte (1:1); a non-zero shift marks the
    /// range as **not 1:1**, so its `source_offset` no longer recovers origin offsets.
    pub(crate) column_shift: isize,
}

impl SourceRange {
    /// Check if a byte offset falls within this range.
    #[must_use]
    pub(crate) fn contains(&self, byte_offset: usize) -> bool {
        byte_offset >= self.start_offset && byte_offset < self.end_offset
    }

    /// Find the innermost range containing `offset`. Ranges are searched in
    /// reverse so a nested include (recorded after its parent) wins over the
    /// enclosing range.
    #[must_use]
    pub(crate) fn find_containing(ranges: &[SourceRange], offset: usize) -> Option<&SourceRange> {
        ranges.iter().rev().find(|r| r.contains(offset))
    }

    /// Original-file byte offset for a preprocessed `offset` inside this range.
    #[must_use]
    pub(crate) fn source_offset(&self, offset: usize) -> usize {
        self.source_start_offset + offset.saturating_sub(self.start_offset)
    }
}

pub(crate) trait Locateable {
    /// Get a reference to the location.
    fn location(&self) -> &Location;
}

/// A `Location` represents a location in a document.
///
/// After parsing completes, a `Location` is **original-source-relative**: its
/// `absolute_start`/`absolute_end` byte offsets and `start`/`end` positions all refer
/// to the original source the node came from — even across `include::` directives and
/// preprocessor edits (dropped comments, conditionals, attribute continuations). The
/// originating file lives on each [`Position`] (`start.file` / `end.file`), so a span
/// that crosses a file boundary reports each endpoint's own file. For content from the
/// primary input the position `file` is `None`.
///
/// `absolute_start`/`absolute_end` are byte offsets **into each endpoint's own file**.
/// They form a single contiguous byte span only when `start.file == end.file` (the usual
/// case); for a span that crosses a file boundary the two offsets are in different files'
/// coordinate spaces, so byte math like `absolute_end - absolute_start` is meaningless —
/// use the per-boundary `file`/line instead. Prefer [`byte_len`](Self::byte_len) over
/// subtracting the offsets directly: it returns `None` across files. (`absolute_*` is not
/// serialized to the ASG, which carries line/column/file only.)
#[derive(Debug, Default, Clone, Hash, Eq, PartialEq)]
#[non_exhaustive]
pub struct Location {
    /// The absolute start position of the location.
    pub absolute_start: usize,
    /// The absolute end position of the location.
    pub absolute_end: usize,

    /// The start position of the location.
    pub start: Position,
    /// The end position of the location.
    pub end: Position,
}

impl Location {
    /// A zero-width location at a single `position` (`start == end`), used for
    /// point diagnostics that only know a line/column. Byte offsets are `0`.
    #[must_use]
    pub fn point(position: Position) -> Self {
        Self {
            absolute_start: 0,
            absolute_end: 0,
            start: position.clone(),
            end: position,
        }
    }

    /// Byte length of this location (`absolute_end - absolute_start + 1`, end inclusive),
    /// but **only when both boundaries are in the same file** (`start.file == end.file`).
    ///
    /// Returns `None` for a span that crosses a file boundary: its `absolute_start` and
    /// `absolute_end` live in different files' coordinate spaces, so subtracting them is
    /// meaningless. Prefer this over subtracting `absolute_*` directly — it can't silently
    /// produce a garbage length across files. For a cross-file span, locate each endpoint
    /// via its own `file`/line instead.
    #[must_use]
    pub fn byte_len(&self) -> Option<usize> {
        (self.start.file == self.end.file).then(|| {
            self.absolute_end
                .saturating_sub(self.absolute_start)
                .saturating_add(1)
        })
    }

    /// Validates that this location satisfies all invariants.
    ///
    /// Checks:
    /// - `absolute_start <= absolute_end` (valid range)
    /// - `absolute_end <= input.len()` (within bounds)
    /// - Both offsets are on UTF-8 character boundaries
    ///
    /// # Errors
    /// Returned as strings for easier debugging.
    pub fn validate(&self, input: &str) -> Result<(), String> {
        // Check range validity using the canonical byte offsets
        if self.absolute_start > self.absolute_end {
            return Err(format!(
                "Invalid range: start {} > end {}",
                self.absolute_start, self.absolute_end
            ));
        }

        // Check bounds
        if self.absolute_end > input.len() {
            return Err(format!(
                "End offset {} exceeds input length {}",
                self.absolute_end,
                input.len()
            ));
        }

        // Check UTF-8 boundaries on the canonical offsets
        if !input.is_char_boundary(self.absolute_start) {
            return Err(format!(
                "Start offset {} not on UTF-8 boundary",
                self.absolute_start
            ));
        }

        if !input.is_char_boundary(self.absolute_end) {
            return Err(format!(
                "End offset {} not on UTF-8 boundary",
                self.absolute_end
            ));
        }

        Ok(())
    }
}

// We serialize `Location` into the ASG format: a two-element sequence of location
// boundaries, `[start, end]`. Each boundary is a [`Position`] (`{ line, col }` plus an
// optional `file`). See the official schema's `locationBoundary` definition:
// https://gitlab.eclipse.org/eclipse/asciidoc-lang/asciidoc-lang/-/blob/main/asg/schema.json
impl Serialize for Location {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_seq(Some(2))?;
        state.serialize_element(&self.start)?;
        state.serialize_element(&self.end)?;
        state.end()
    }
}

impl std::fmt::Display for Location {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "location.start({}), location.end({})",
            self.start, self.end
        )
    }
}

/// A `Position` represents a human-readable position in a document — one ASG
/// `locationBoundary`. For byte offsets, use `Location.absolute_start` /
/// `Location.absolute_end`.
///
/// Equality and hashing cover **all three** fields, `file` included: two positions at
/// the same line/column but reached through different `include::` chains — or one from
/// the primary input (`file: None`) versus one from an included file — are *not* equal.
/// This is deliberate: a line/column is only a unique location together with its file.
#[derive(Debug, Default, Clone, Hash, Eq, PartialEq)]
#[non_exhaustive]
pub struct Position {
    /// The line number of the position (1-indexed).
    pub line: u32,
    /// The column number of the position (1-indexed, counted as Unicode scalar values).
    pub column: u32,
    /// The `include::` chain this position's content came through, outermost target
    /// first and the file directly containing the content last — each element the
    /// include target *as written* in its directive (e.g. `["a.adoc", "b.adoc"]` for
    /// content in `b.adoc` included by `a.adoc`). `None` for content from the primary
    /// input, which the ASG omits. Shared (`Arc`) across all positions reached through
    /// the same chain, so the field stays a thin 8-byte pointer.
    pub file: Option<Arc<Vec<String>>>,
}

impl Position {
    /// A position at `line`/`column` with no originating file (primary input).
    #[must_use]
    pub fn new(line: u32, column: u32) -> Self {
        Self {
            line,
            column,
            file: None,
        }
    }

    /// A position from `usize` `line`/`column`, saturating at `u32::MAX` (neither can
    /// realistically exceed it). For callers that already hold `u32` — the hot parse
    /// and diagnostic paths — prefer the lossless [`new`](Self::new). The named
    /// arguments here guard against accidentally transposing line and column at the
    /// many `usize`-scanning call sites (preprocessor line counters, byte indices).
    #[must_use]
    pub fn from_line_col(line: usize, column: usize) -> Self {
        Self {
            line: u32::try_from(line).unwrap_or(u32::MAX),
            column: u32::try_from(column).unwrap_or(u32::MAX),
            file: None,
        }
    }
}

impl std::fmt::Display for Position {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line: {}, column: {}", self.line, self.column)
    }
}

// Serialize a `Position` as an ASG `locationBoundary`: `{ line, col }` plus an
// optional `file` — the `include::` chain (array of targets as written) — present
// only for `include::`d content (the primary input has no `file`).
impl Serialize for Position {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let chain = self.file.as_deref().filter(|c| !c.is_empty());
        let len = if chain.is_some() { 3 } else { 2 };
        let mut map = serializer.serialize_map(Some(len))?;
        map.serialize_entry("line", &self.line)?;
        map.serialize_entry("col", &self.column)?;
        if let Some(chain) = chain {
            map.serialize_entry("file", chain)?;
        }
        map.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loc(chain: &[&str]) -> Location {
        let file = (!chain.is_empty())
            .then(|| Arc::new(chain.iter().map(|s| (*s).to_string()).collect::<Vec<_>>()));
        Location {
            absolute_start: 0,
            absolute_end: 1,
            start: Position {
                line: 1,
                column: 2,
                file: file.clone(),
            },
            end: Position {
                line: 3,
                column: 4,
                file,
            },
        }
    }

    #[test]
    fn serializes_two_boundaries_without_file_when_primary() -> Result<(), serde_json::Error> {
        let json = serde_json::to_value(loc(&[]))?;
        assert_eq!(
            json,
            serde_json::json!([{ "line": 1, "col": 2 }, { "line": 3, "col": 4 }])
        );
        Ok(())
    }

    #[test]
    fn serializes_file_as_include_chain_on_each_boundary() -> Result<(), serde_json::Error> {
        let json = serde_json::to_value(loc(&["a.adoc", "chapters/b.adoc"]))?;
        assert_eq!(
            json,
            serde_json::json!([
                { "line": 1, "col": 2, "file": ["a.adoc", "chapters/b.adoc"] },
                { "line": 3, "col": 4, "file": ["a.adoc", "chapters/b.adoc"] }
            ])
        );
        Ok(())
    }

    #[test]
    fn position_equality_is_provenance_aware() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        fn hash(p: &Position) -> u64 {
            let mut h = DefaultHasher::new();
            p.hash(&mut h);
            h.finish()
        }
        // A position at line 1, col 1 reached via `chain` (empty == primary input).
        let pos = |chain: &[&str]| {
            let mut p = Position::new(1, 1);
            if !chain.is_empty() {
                p.file = Some(Arc::new(
                    chain.iter().map(|s| (*s).to_string()).collect::<Vec<_>>(),
                ));
            }
            p
        };

        let primary = pos(&[]);
        let included = pos(&["inc.adoc"]);

        // Same line/col, different (or absent) chain => not equal, and hash differs.
        assert_ne!(primary, included, "primary input vs included differ");
        assert_ne!(
            included,
            pos(&["other.adoc"]),
            "different include chains differ"
        );
        assert_ne!(hash(&primary), hash(&included));

        // Same line/col and same chain => equal (and equal hash, required by Eq + Hash).
        assert_eq!(included, pos(&["inc.adoc"]));
        assert_eq!(hash(&included), hash(&pos(&["inc.adoc"])));
    }

    #[test]
    fn byte_len_only_within_a_single_file() {
        let chain = |c: &[&str]| Arc::new(c.iter().map(|s| (*s).to_string()).collect::<Vec<_>>());

        // Same file (both primary): inclusive length is end - start + 1.
        let same = Location {
            absolute_start: 10,
            absolute_end: 19,
            start: Position::new(1, 1),
            end: Position::new(1, 10),
        };
        assert_eq!(same.byte_len(), Some(10));

        // Cross-file: the offsets are in different files' spaces, so there is no span.
        let mut start = Position::new(1, 1);
        start.file = Some(chain(&["a.adoc"]));
        let mut end = Position::new(5, 1);
        end.file = Some(chain(&["b.adoc"]));
        let cross = Location {
            absolute_start: 10,
            absolute_end: 400,
            start,
            end,
        };
        assert_eq!(cross.byte_len(), None);
    }
}
