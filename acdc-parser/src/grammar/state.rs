use crate::{
    DocumentAttributes, Footnote, InlineNode, Location, Options, TocEntry, grammar::LineMap,
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
    pub(crate) fn register_section(&mut self, title: Vec<InlineNode>, level: u8, id: String) {
        self.entries.push(TocEntry { id, title, level });
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
            current_file: None,
        }
    }

    /// Create a Location from raw byte offsets
    pub(crate) fn create_location(&self, start: usize, end: usize) -> Location {
        let start_pos = self.line_map.offset_to_position(start, &self.input);
        let end_pos = self.line_map.offset_to_position(end, &self.input);

        Location {
            absolute_start: start,
            absolute_end: end,
            start: start_pos,
            end: end_pos,
        }
    }

    /// Helper to create block location with standard offset calculation
    pub(crate) fn create_block_location(
        &self,
        start: usize,
        end: usize,
        offset: usize,
    ) -> Location {
        self.create_location(start + offset, (end + offset).saturating_sub(1))
    }
}
