//! Post-parse pass that rewrites every AST node's [`Location`] from
//! preprocessed-text coordinates to original-source coordinates.
//!
//! # Why this exists: two coordinate spaces
//!
//! Before parsing, the preprocessor resolves every `include::` (and strips comments,
//! evaluates `ifdef`/`ifndef`/`ifeval`, joins multi-line attribute continuations, ...)
//! into a single flat string: the *preprocessed buffer*. The grammar parses only that
//! buffer, so every node's location is a position **in the merged buffer**, which
//! exists nowhere on disk:
//!
//! ```text
//!  main.adoc           part.adoc      PREPROCESSED BUFFER (the only thing the parser sees)
//!  ─────────           ─────────      ──────────────────────────────────────────────────
//!  = My Doc            inc 1           = My Doc
//!  include::part[]     inc 2           inc 1      <- really part.adoc line 1
//!  After.                              inc 2      <- really part.adoc line 2
//!                                      After.     <- really main.adoc line 3 (directive gone)
//! ```
//!
//! A consumer (e.g: LSP "go to definition", a CLI diagnostic snippet, the ASG's per-node
//! `file`/line) needs the *real* file + line, not the buffer position (`inc 1` is
//! "line 2" of the buffer but line 1 of `part.adoc`). While merging, the preprocessor
//! leaves a receipt for each spliced chunk: a [`SourceRange`] mapping a buffer span back
//! to its origin file + line + byte offset. This pass replays those receipts to
//! translate every node's location from buffer coordinates to origin coordinates.
//!
//! # Why after parsing, not during
//!
//! The preprocessed buffer is the grammar's *only* coordinate system: PEG rules capture
//! positions in the string they scan, and the parser's own offset arithmetic indexes
//! that same string. Locations can't be made origin-relative mid-parse without breaking
//! the parser. PEG also backtracks, so many speculatively-captured locations never reach
//! the final tree. Doing one pass over the finished [`Document`] keeps the translation in
//! a single traversal ([`location_walk`](super::location_walk)) — one place to get right
//! and to test, rather than threaded through every grammar rule — and only touches nodes
//! that actually survived.
//!
//! # How
//!
//! For every node, this pass rewrites its `absolute_start`/`absolute_end` byte offsets
//! and `start`/`end` line numbers to the original source, and stamps the originating
//! file on each boundary independently (so a span crossing a file boundary reports each
//! endpoint's own file). Columns are normally left untouched — same-file line edits and
//! include splicing only add or remove whole lines, never shift within-line columns. The
//! one exception is `include::[indent=N]`, which re-indents every line by a uniform
//! [`SourceRange::column_shift`]; the remap subtracts it to recover origin columns.
//!
//! Byte offsets are origin-relative only for **1:1** ranges (`column_shift == 0`).
//! An indented include inserts/removes bytes per line, so a single linear map can't
//! recover origin offsets; for such ranges the offsets are left in preprocessed
//! coordinates (line/column are still origin-correct) rather than overrun the shorter
//! origin file. Note that `absolute_*` is not serialized to the ASG — the
//! ASG `locationBoundary` is line/column/file only.
//!
//! When the preprocessor recorded no ranges (the fast path: no includes/edits, so
//! preprocessed == source), the pass is skipped entirely and node locations — whose
//! positions carry no file, matching the primary input — are final.

use std::{collections::HashMap, sync::Arc};

use crate::{
    grammar::{
        LineMap,
        location_walk::{walk_document_locations_mut, walk_inline_locations_mut},
    },
    model::{Document, InlineNode, Location, SourceRange},
};

/// Rewrite every location in `doc` to original-source coordinates using the
/// preprocessor's `source_ranges`. No-op when there are no ranges.
pub(crate) fn remap_document_to_source(
    doc: &mut Document,
    ranges: &[SourceRange],
    input: &str,
    line_map: &LineMap,
) {
    if ranges.is_empty() {
        return;
    }
    let remapper = Remapper::new(ranges, input, line_map);
    walk_document_locations_mut(doc, &mut |loc| remapper.remap_one(loc));
}

/// Rewrite every location in a standalone inline slice (the inline-only parse
/// entry point) to original-source coordinates. No-op when there are no ranges.
pub(crate) fn remap_inlines_to_source(
    inlines: &mut [InlineNode<'_>],
    ranges: &[SourceRange],
    input: &str,
    line_map: &LineMap,
) {
    if ranges.is_empty() {
        return;
    }
    let remapper = Remapper::new(ranges, input, line_map);
    for node in inlines {
        walk_inline_locations_mut(node, &mut |loc| remapper.remap_one(loc));
    }
}

struct Remapper<'a> {
    input: &'a str,
    line_map: &'a LineMap,
    ranges: &'a [SourceRange],
    /// Preprocessed (1-indexed) start line of each range, precomputed so `map_offset`
    /// resolves a source line with a single `LineMap` lookup (for the queried offset)
    /// instead of also looking up the range start every time. Parallel to `ranges`.
    preproc_start_lines: Vec<u32>,
    /// One shared `Arc<Vec<String>>` per distinct `include::` chain, keyed by the
    /// range's `file_chain`, so stamping a node's file is a refcount bump rather than
    /// cloning the chain per node. Primary-input ranges (empty chain) are not interned
    /// — their content keeps `file: None`.
    chains: HashMap<&'a [String], Arc<Vec<String>>>,
    /// Sorted, de-duplicated range-boundary offsets, so a point's containing range is a
    /// binary search instead of an O(R) reverse scan. Segment `i` spans
    /// `[boundaries[i], boundaries[i + 1])`.
    boundaries: Vec<usize>,
    /// Innermost range covering each segment, as an index into `ranges`, or `None`
    /// where no range covers it. Length is `boundaries.len().saturating_sub(1)`.
    /// "Innermost" = last-recorded = highest index, matching the old `iter().rev()`.
    segment_cover: Vec<Option<usize>>,
}

impl<'a> Remapper<'a> {
    fn new(ranges: &'a [SourceRange], input: &'a str, line_map: &'a LineMap) -> Self {
        let mut chains: HashMap<&'a [String], Arc<Vec<String>>> = HashMap::new();
        for range in ranges {
            if !range.file_chain.is_empty() {
                chains
                    .entry(range.file_chain.as_slice())
                    .or_insert_with(|| Arc::new(range.file_chain.clone()));
            }
        }
        // "Which range contains offset X?" is a hot per-node lookup, and ranges can
        // nest/overlap: a nested include records a sub-range inside its parent, and
        // *after* it, so by "last-recorded (highest index) wins" the inner range is the
        // answer. Precompute a coordinate-compressed segment cover so the lookup is a
        // binary search (`map_offset` -> `covering_range_index`) instead of an O(R) scan.
        //
        // ranges (recorded order; r3 is a nested include inside r1):
        //   r0 [0,7)   r1 [7,19)   r2 [19,26)   r3 [10,15)      <- r3 recorded last
        //
        // boundaries = sort+dedup of every start & end:  [0, 7, 10, 15, 19, 26]
        // segments   = consecutive boundary pairs:        [0,7) [7,10) [10,15) [15,19) [19,26)
        // segment_cover (innermost range per segment):     r0    r1     r3      r1      r2
        //
        // Lookup: binary-search the offset to its segment, read that segment's range —
        // e.g. offset 12 -> segment [10,15) -> r3 (the nested include); offset 8 -> r1.
        let mut boundaries = Vec::with_capacity(ranges.len() * 2);
        for range in ranges {
            boundaries.push(range.start_offset);
            boundaries.push(range.end_offset);
        }
        boundaries.sort_unstable();
        boundaries.dedup();
        let segment_cover = Self::build_segment_cover(ranges, &boundaries);

        let preproc_start_lines = ranges
            .iter()
            .map(|range| line_map.offset_to_position(range.start_offset, input).line)
            .collect();

        Self {
            input,
            line_map,
            ranges,
            preproc_start_lines,
            chains,
            boundaries,
            segment_cover,
        }
    }

    /// For each segment `[boundaries[i], boundaries[i + 1])`, the index of the innermost
    /// range covering it, or `None` if uncovered.
    ///
    /// Each range covers a contiguous run of segments (its start and end are themselves
    /// boundaries). Stamping the ranges in recorded order lets a later (inner) range
    /// overwrite an earlier (outer) one, so "innermost = last-recorded" falls out for
    /// free — no priority tracking needed. For the example in [`Remapper::new`] the cover
    /// ends up as `[r0, r1, r3, r1, r2]`. One-time cost is the total covered length
    /// (near-linear for the shallow nesting real includes produce).
    fn build_segment_cover(ranges: &[SourceRange], boundaries: &[usize]) -> Vec<Option<usize>> {
        let mut cover = vec![None; boundaries.len().saturating_sub(1)];
        for (index, range) in ranges.iter().enumerate() {
            // The covered segments are those from the range's start boundary up to (but
            // not including) its end boundary.
            let first = boundaries.partition_point(|&b| b < range.start_offset);
            let end = boundaries.partition_point(|&b| b < range.end_offset);
            if let Some(slots) = cover.get_mut(first..end) {
                for slot in slots {
                    *slot = Some(index);
                }
            }
        }
        cover
    }

    /// Map one preprocessed byte offset to `(source_offset, source_line)` within its
    /// innermost containing range, plus that range — via a binary search over the
    /// precomputed segment cover (O(log R)). The source line goes through the shared
    /// [`LineMap::source_line`] so it can't drift from the diagnostic paths.
    fn map_offset(&self, abs: usize) -> Option<(usize, u32, &'a SourceRange)> {
        let range_index = covering_range_index(&self.boundaries, &self.segment_cover, abs)?;
        let range = self.ranges.get(range_index)?;
        let preproc_start_line = *self.preproc_start_lines.get(range_index)?;
        let source_line = self.line_map.source_line_from(
            u32::try_from(range.start_line).unwrap_or(u32::MAX),
            preproc_start_line,
            self.input,
            abs,
        );
        Some((range.source_offset(abs), source_line, range))
    }

    /// The interned `include::` chain for a range, or `None` for primary-input ranges
    /// (empty chain) so their content keeps `file: None`.
    fn range_file(&self, range: &SourceRange) -> Option<Arc<Vec<String>>> {
        self.chains.get(range.file_chain.as_slice()).cloned()
    }

    /// Rewrite a single location in place. Start and end are each mapped through
    /// their own containing range — including the `file` — so a node that spans a
    /// file boundary (e.g. the document, or a section containing an `include::`)
    /// reports each endpoint's true file and line.
    fn remap_one(&self, loc: &mut Location) {
        let Some((src_start, start_line, start_range)) = self.map_offset(loc.absolute_start) else {
            return;
        };
        // `absolute_end` is inclusive; probe the last byte (clamped) for its range.
        let end_probe = loc
            .absolute_end
            .min(self.input.len().saturating_sub(1))
            .max(loc.absolute_start);
        let (src_end, end_line, end_range) =
            self.map_offset(end_probe)
                .unwrap_or((src_start, start_line, start_range));

        // Line + column are always origin-relative. Columns shift by a per-range
        // constant for re-indented includes; untransformed ranges (shift 0) leave
        // them unchanged.
        loc.start.line = start_line;
        loc.end.line = end_line.max(start_line);
        loc.start.column = shift_column(loc.start.column, start_range.column_shift);
        loc.end.column = shift_column(loc.end.column, end_range.column_shift);
        // `range_file` hashes the include chain, so resolve once and reuse when start and
        // end fall in the same range — the case for nearly every node (single-file spans),
        // including the end-probe fallback, which reuses `start_range`.
        let start_file = self.range_file(start_range);
        let end_file = if std::ptr::eq(start_range, end_range) {
            start_file.clone()
        } else {
            self.range_file(end_range)
        };
        loc.start.file = start_file;
        loc.end.file = end_file;

        // Byte offsets are origin-relative only for 1:1 ranges. A re-indented range
        // inserts/removes bytes per line, so `source_offset` can't recover origin
        // offsets — keep the preprocessed offset (in bounds, never overruns the
        // shorter origin file) rather than claim a wrong one. `absolute_*` is not
        // serialized to the ASG, so this divergence is internal only.
        //
        // When start and end fall in different files, the two offsets are in different
        // coordinate spaces and aren't a meaningful byte span (consumers must use the
        // per-boundary `file`/line — see `Location`). The `.max(start)` only upholds the
        // `start <= end` invariant for that degenerate cross-file case; it isn't a real
        // length.
        if start_range.column_shift == 0 {
            loc.absolute_start = src_start;
        }
        if end_range.column_shift == 0 {
            loc.absolute_end = src_end.max(loc.absolute_start);
        } else {
            loc.absolute_end = loc.absolute_end.max(loc.absolute_start);
        }
    }
}

/// Index of the innermost range covering `abs` via the segment cover, or `None` when
/// `abs` lies outside every range. Segment `i` spans `[boundaries[i], boundaries[i+1])`.
fn covering_range_index(
    boundaries: &[usize],
    segment_cover: &[Option<usize>],
    abs: usize,
) -> Option<usize> {
    let &first = boundaries.first()?;
    let &last = boundaries.last()?;
    if abs < first || abs >= last {
        return None;
    }
    let segment = boundaries.partition_point(|&c| c <= abs).checked_sub(1)?;
    *segment_cover.get(segment)?
}

/// Subtract a re-indent's `column_shift` from a preprocessed `column` to recover the
/// origin column, clamped to a minimum of 1. A `0` shift returns `column` unchanged.
fn shift_column(column: u32, column_shift: isize) -> u32 {
    if column_shift == 0 {
        return column;
    }
    let shifted = isize::try_from(column).unwrap_or(isize::MAX) - column_shift;
    u32::try_from(shifted.max(1)).unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn range(start: usize, end: usize) -> SourceRange {
        SourceRange {
            start_offset: start,
            end_offset: end,
            file: None,
            file_chain: Vec::new(),
            start_line: 1,
            source_start_offset: 0,
            column_shift: 0,
        }
    }

    /// The segment-cover lookup must resolve the same innermost (last-recorded)
    /// containing range as the original `iter().rev().find(contains)` scan, for nested,
    /// partially overlapping, and disjoint ranges alike.
    #[test]
    fn segment_cover_matches_linear_scan() {
        let raw = [
            range(0, 100),   // 0: outer
            range(10, 30),   // 1: nested in 0
            range(20, 25),   // 2: nested in 1
            range(50, 70),   // 3: sibling
            range(60, 110),  // 4: partial overlap with 3, extends past 0
            range(200, 210), // 5: disjoint
            range(40, 40),   // 6: zero-width (never contains)
        ];
        let mut boundaries: Vec<usize> = raw
            .iter()
            .flat_map(|r| [r.start_offset, r.end_offset])
            .collect();
        boundaries.sort_unstable();
        boundaries.dedup();
        let segment_cover = Remapper::build_segment_cover(&raw, &boundaries);

        for abs in 0..220 {
            let got = covering_range_index(&boundaries, &segment_cover, abs);
            let expected = raw
                .iter()
                .enumerate()
                .rev()
                .find(|(_, r)| r.contains(abs))
                .map(|(index, _)| index);
            assert_eq!(got, expected, "offset {abs}");
        }
    }
}
