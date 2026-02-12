use std::{collections::HashMap, path::PathBuf};

use crate::{
    CalloutRef, DocumentAttributes, Footnote, Location, Options, Positioning, SourceLocation,
    Title, TocEntry,
    grammar::LineMap,
    model::{LeveloffsetRange, SourceRange},
};

#[derive(Debug)]
pub(crate) struct ParserState {
    pub(crate) document_attributes: DocumentAttributes,
    pub(crate) line_map: LineMap,
    pub(crate) options: Options,
    pub(crate) input: String,
    pub(crate) footnote_tracker: FootnoteTracker,
    pub(crate) toc_tracker: TocTracker,
    pub(crate) last_block_was_verbatim: bool,
    /// Callout references found in the last verbatim block (for validation with callout lists)
    pub(crate) last_verbatim_callouts: Vec<CalloutRef>,
    /// The current file being parsed (None for inline/string parsing)
    pub(crate) current_file: Option<PathBuf>,
    /// Byte ranges where specific leveloffset values apply.
    /// Set by the preprocessor when processing includes with `leveloffset=` attributes.
    /// Used by the parser to adjust section levels.
    pub(crate) leveloffset_ranges: Vec<LeveloffsetRange>,
    /// Byte ranges mapping preprocessed output back to source files.
    /// Set by the preprocessor when processing `include::` directives.
    /// Used to produce accurate file/line info in warnings.
    pub(crate) source_ranges: Vec<SourceRange>,
    /// Warnings collected during PEG parsing for post-parse emission.
    /// PEG backtracking can cause the same warning to fire multiple times;
    /// storing them here with deduplication and emitting after parsing avoids duplicates.
    pub(crate) warnings: Vec<String>,
    /// When true, inline parsing uses a reduced rule set that only matches
    /// formatting markup (bold, italic, monospace, highlight, superscript,
    /// subscript, curved quotes) and plain text. Used by `parse_text_for_quotes`
    /// to apply "quotes" substitution without matching macros, xrefs, etc.
    pub(crate) quotes_only: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct FootnoteTracker {
    /// All registered footnotes in the order they were encountered.
    pub(crate) footnotes: Vec<Footnote>,
    /// The last assigned footnote number (starts at 1)
    last_footnote_position: u32,
    /// Map of named footnote IDs to their assigned numbers
    ///
    /// This helps ensure that named footnotes are only assigned a number once and reused.
    /// If it's an anonymous footnote (no ID), it always gets a new number.
    named_footnote_numbers: HashMap<String, u32>,
}

impl FootnoteTracker {
    pub(crate) fn new() -> Self {
        Self {
            footnotes: Vec::new(),
            last_footnote_position: 1,
            named_footnote_numbers: HashMap::new(),
        }
    }

    /// Register a footnote and assign it a number, but only if not already processed
    #[tracing::instrument(skip_all, fields(?footnote))]
    pub(crate) fn push(&mut self, footnote: &mut Footnote) {
        if let Some(id) = &footnote.id {
            if let Some(&existing_number) = self.named_footnote_numbers.get(id) {
                footnote.number = existing_number;
            } else {
                let number = self.last_footnote_position;
                self.named_footnote_numbers.insert(id.clone(), number);
                footnote.number = number;
                self.footnotes.push(footnote.clone());
                self.last_footnote_position += 1;
            }
        } else {
            // Anonymous footnote
            let number = self.last_footnote_position;
            footnote.number = number;
            self.footnotes.push(footnote.clone());
            self.last_footnote_position += 1;
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct TocTracker {
    /// All TOC entries collected during parsing, in document order
    pub(crate) entries: Vec<TocEntry>,
}

impl TocTracker {
    /// Register a section for inclusion in the TOC.
    ///
    /// The `numbered` parameter indicates whether this section should receive
    /// automatic section numbering when `sectnums` is enabled. This should be
    /// `false` for special section styles like `[bibliography]`, `[glossary]`, etc.
    pub(crate) fn register_section(
        &mut self,
        title: Title,
        level: u8,
        id: String,
        xreflabel: Option<String>,
        numbered: bool,
    ) {
        self.entries.push(TocEntry {
            id,
            title,
            level,
            xreflabel,
            numbered,
        });
    }
}

impl ParserState {
    pub(crate) fn new(input: &str) -> Self {
        Self {
            options: Options::default(),
            document_attributes: DocumentAttributes::default(),
            line_map: LineMap::new(input),
            input: input.to_string(),
            footnote_tracker: FootnoteTracker::new(),
            toc_tracker: TocTracker::default(),
            last_block_was_verbatim: false,
            last_verbatim_callouts: Vec::new(),
            current_file: None,
            leveloffset_ranges: Vec::new(),
            source_ranges: Vec::new(),
            warnings: Vec::new(),
            quotes_only: false,
        }
    }

    /// Resolve a byte offset in the combined preprocessed text to the correct
    /// source file name and line number. For offsets within included content,
    /// returns the included file's name and line; otherwise falls back to the
    /// entry-point file.
    pub(crate) fn resolve_source_location(&self, offset: usize) -> (String, usize) {
        // Find the most specific (innermost/last) SourceRange containing this offset
        if let Some(range) = self.source_ranges.iter().rev().find(|r| r.contains(offset)) {
            let file_name = range
                .file
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string();
            // Count newlines from range start to offset to get line within the included file
            let line_in_file =
                if offset >= range.start_offset && range.start_offset <= self.input.len() {
                    let end = offset.min(self.input.len());
                    let bytes_before = &self.input[range.start_offset..end];
                    range.start_line + bytes_before.matches('\n').count()
                } else {
                    range.start_line
                };
            (file_name, line_in_file)
        } else {
            // Not from an include — use the entry-point file
            let file_name = self
                .current_file
                .as_ref()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or("input")
                .to_string();
            let pos = self.line_map.offset_to_position(offset, &self.input);
            (file_name, pos.line)
        }
    }

    /// Warn about unexpected trailing content after a block macro's attribute list.
    pub(crate) fn warn_trailing_macro_content(
        &mut self,
        macro_name: &str,
        trailing: &str,
        end: usize,
        offset: usize,
    ) {
        if !trailing.trim().is_empty() {
            let (file_name, line) = self.resolve_source_location(end + offset);
            self.add_warning(format!(
                "{file_name}: line {line}: unexpected content after {macro_name} macro: '{trailing}'"
            ));
        }
    }

    /// Collect a warning for post-parse emission. Deduplicates by message.
    pub(crate) fn add_warning(&mut self, message: String) {
        if !self.warnings.contains(&message) {
            self.warnings.push(message);
        }
    }

    /// Emit all collected warnings via tracing. Call after parsing completes.
    pub(crate) fn emit_warnings(&self) {
        for warning in &self.warnings {
            tracing::warn!("{warning}");
        }
    }

    /// Create a `SourceLocation` for an error/warning, resolving the correct
    /// file and adjusting line numbers for included content.
    pub(crate) fn create_error_source_location(&self, location: Location) -> SourceLocation {
        if let Some(range) = self
            .source_ranges
            .iter()
            .rev()
            .find(|r| r.contains(location.absolute_start))
        {
            let file = Some(range.file.clone());
            let start_newlines = self
                .input
                .get(range.start_offset..location.absolute_start)
                .map_or(0, |s| s.matches('\n').count());
            let end_newlines = self
                .input
                .get(range.start_offset..location.absolute_end.min(self.input.len()))
                .map_or(0, |s| s.matches('\n').count());
            let adjusted_start_line = range.start_line + start_newlines;
            let adjusted_end_line = range.start_line + end_newlines;
            SourceLocation {
                file,
                positioning: Positioning::Location(Location {
                    absolute_start: location.absolute_start,
                    absolute_end: location.absolute_end,
                    start: crate::Position {
                        line: adjusted_start_line,
                        column: location.start.column,
                    },
                    end: crate::Position {
                        line: adjusted_end_line,
                        column: location.end.column,
                    },
                }),
            }
        } else {
            SourceLocation {
                file: self.current_file.clone(),
                positioning: Positioning::Location(location),
            }
        }
    }

    /// Create a Location from raw byte offsets.
    ///
    /// This method enforces UTF-8 character boundaries:
    /// - Clamps offsets to input bounds
    /// - Rounds both start and end backward to nearest char boundary
    /// - Ensures start <= end
    ///
    /// We round end backward (not forward) because `absolute_end` has inclusive
    /// semantics - it represents the last byte of the range, not one past it.
    /// When a byte lands mid-character, rounding backward includes that character.
    pub(crate) fn create_location(&self, start: usize, end: usize) -> Location {
        use crate::grammar::utf8_utils::ensure_char_boundary;

        // Clamp to input bounds first
        let clamped_start = start.min(self.input.len());
        let clamped_end = end.min(self.input.len());

        // Ensure UTF-8 boundaries (both round backward for inclusive semantics)
        let safe_start = ensure_char_boundary(&self.input, clamped_start);
        let safe_end = ensure_char_boundary(&self.input, clamped_end);

        // Ensure start <= end
        let safe_end = safe_end.max(safe_start);

        let start_pos = self.line_map.offset_to_position(safe_start, &self.input);
        let end_pos = self.line_map.offset_to_position(safe_end, &self.input);

        Location {
            absolute_start: safe_start,
            absolute_end: safe_end,
            start: start_pos,
            end: end_pos,
        }
    }

    /// Helper to create block location with standard offset calculation.
    ///
    /// Adds `offset` to both start and end, then decrements end by one character
    /// (to exclude trailing delimiter/newline). UTF-8 safety is handled by `create_location`.
    pub(crate) fn create_block_location(
        &self,
        start: usize,
        end: usize,
        offset: usize,
    ) -> Location {
        use crate::grammar::utf8_utils::safe_decrement_offset;

        let adjusted_start = start + offset;
        let adjusted_end = end + offset;

        // Decrement end by one character (safely handling UTF-8)
        let final_end = if adjusted_end == 0 {
            0
        } else {
            safe_decrement_offset(&self.input, adjusted_end)
        };

        // create_location handles all UTF-8 boundary enforcement
        self.create_location(adjusted_start, final_end)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::indexing_slicing)]
#[allow(clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn add_warning_deduplicates_identical_messages() {
        let mut state = ParserState::new("test");
        state.add_warning("duplicate warning".to_string());
        state.add_warning("duplicate warning".to_string());
        state.add_warning("duplicate warning".to_string());
        assert_eq!(state.warnings.len(), 1);
        assert_eq!(state.warnings[0], "duplicate warning");
    }

    #[test]
    fn add_warning_preserves_distinct_messages() {
        let mut state = ParserState::new("test");
        state.add_warning("first".to_string());
        state.add_warning("second".to_string());
        state.add_warning("third".to_string());
        assert_eq!(state.warnings.len(), 3);
    }

    #[test]
    fn add_warning_preserves_insertion_order() {
        let mut state = ParserState::new("test");
        state.add_warning("beta".to_string());
        state.add_warning("alpha".to_string());
        state.add_warning("beta".to_string());
        state.add_warning("gamma".to_string());
        assert_eq!(state.warnings, vec!["beta", "alpha", "gamma"]);
    }

    #[test]
    #[tracing_test::traced_test]
    fn emit_warnings_outputs_via_tracing() {
        let mut state = ParserState::new("test");
        state.add_warning("warning one".to_string());
        state.add_warning("warning two".to_string());
        state.emit_warnings();
        assert!(logs_contain("warning one"));
        assert!(logs_contain("warning two"));
    }

    #[test]
    fn create_error_source_location_resolves_included_file() {
        use crate::model::SourceRange;

        // Simulate: main file "line1\n", included file "inc_line1\ninc_line2\n"
        let input = "line1\ninc_line1\ninc_line2\n";
        let mut state = ParserState::new(input);
        state.current_file = Some(PathBuf::from("main.adoc"));
        state.source_ranges.push(SourceRange {
            start_offset: 6, // "inc_line1\n..." starts at byte 6
            end_offset: 25,
            file: PathBuf::from("/tmp/included.adoc"),
            start_line: 1,
        });

        // Location inside the included range (second line of include: "inc_line2")
        let loc = state.create_location(16, 24); // "inc_line2"
        let src_loc = state.create_error_source_location(loc);

        assert_eq!(src_loc.file, Some(PathBuf::from("/tmp/included.adoc")));
        match &src_loc.positioning {
            Positioning::Location(l) => {
                // From start_offset=6 to absolute_start=16, there's 1 newline ("inc_line1\n")
                // So start_line = 1 + 1 = 2
                assert_eq!(l.start.line, 2);
            }
            Positioning::Position(_) => panic!("expected Positioning::Location"),
        }
    }

    #[test]
    fn create_error_source_location_falls_back_to_current_file() {
        let input = "main content\n";
        let mut state = ParserState::new(input);
        state.current_file = Some(PathBuf::from("main.adoc"));
        // No source ranges

        let loc = state.create_location(0, 11);
        let src_loc = state.create_error_source_location(loc.clone());

        assert_eq!(src_loc.file, Some(PathBuf::from("main.adoc")));
        match &src_loc.positioning {
            Positioning::Location(l) => {
                assert_eq!(l.start.line, loc.start.line);
                assert_eq!(l.end.line, loc.end.line);
            }
            Positioning::Position(_) => panic!("expected Positioning::Location"),
        }
    }
}
