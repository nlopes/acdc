use crate::{
    CalloutRef, DocumentAttributes, Footnote, Location, Options, Title, TocEntry, grammar::LineMap,
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
    pub(crate) current_file: Option<std::path::PathBuf>,
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
    named_footnote_numbers: std::collections::HashMap<String, u32>,
}

impl FootnoteTracker {
    pub(crate) fn new() -> Self {
        Self {
            footnotes: Vec::new(),
            last_footnote_position: 1,
            named_footnote_numbers: std::collections::HashMap::new(),
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
    /// Register a section for inclusion in the TOC
    pub(crate) fn register_section(
        &mut self,
        title: Title,
        level: u8,
        id: String,
        xreflabel: Option<String>,
    ) {
        self.entries.push(TocEntry {
            id,
            title,
            level,
            xreflabel,
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
