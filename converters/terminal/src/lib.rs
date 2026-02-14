use std::{
    cell::{Cell, RefCell},
    io::Write,
    path::{Path, PathBuf},
    rc::Rc,
};

use acdc_converters_core::{
    Backend, Converter, Options,
    section::{AppendixTracker, PartNumberTracker, SectionNumberTracker},
    visitor::Visitor,
};
use acdc_parser::{Block, Document, DocumentAttributes, IndexTermKind, TocEntry};

pub(crate) use appearance::Appearance;

pub(crate) const FALLBACK_TERMINAL_WIDTH: usize = 80;
pub(crate) const MAX_TERMINAL_WIDTH: usize = 120;

#[derive(Clone, Debug)]
pub struct Processor {
    pub(crate) options: Options,
    pub(crate) document_attributes: DocumentAttributes,
    pub(crate) toc_entries: Vec<TocEntry>,
    /// Shared counter for auto-numbering example blocks.
    /// Uses Rc<Cell<>> so all clones share the same counter.
    pub(crate) example_counter: Rc<Cell<usize>>,
    /// Terminal appearance (theme, capabilities, colors)
    pub(crate) appearance: Appearance,
    /// Section number tracker for `:sectnums:` support.
    pub(crate) section_number_tracker: SectionNumberTracker,
    /// Part number tracker for `:partnums:` support in book doctype.
    pub(crate) part_number_tracker: PartNumberTracker,
    /// Appendix tracker for `[appendix]` style on level-0 sections.
    pub(crate) appendix_tracker: AppendixTracker,
    /// Terminal width (read once at start, capped at `MAX_TERMINAL_WIDTH`).
    pub(crate) terminal_width: usize,
    /// Collected index term kinds for rendering in the index catalog.
    pub(crate) index_entries: Rc<RefCell<Vec<IndexTermKind>>>,
    /// Whether the document has a valid `[index]` section (last section).
    pub(crate) has_valid_index_section: bool,
    /// Current list nesting indentation (shared across clones).
    pub(crate) list_indent: Rc<Cell<usize>>,
}

impl Converter for Processor {
    type Error = Error;

    fn document_attributes_defaults() -> DocumentAttributes {
        // Terminal converter uses environment detection (Appearance::detect())
        // rather than document attributes for its configuration.
        // No terminal-specific attribute defaults needed.
        DocumentAttributes::default()
    }

    fn new(options: Options, document_attributes: DocumentAttributes) -> Self {
        let mut document_attributes = document_attributes;
        for (name, value) in Self::document_attributes_defaults().iter() {
            document_attributes.insert(name.clone(), value.clone());
        }
        let appearance = Appearance::detect();

        let section_number_tracker = SectionNumberTracker::new(&document_attributes);
        let part_number_tracker =
            PartNumberTracker::new(&document_attributes, section_number_tracker.clone());
        let appendix_tracker =
            AppendixTracker::new(&document_attributes, section_number_tracker.clone());

        let terminal_width = crossterm::terminal::size()
            .map(|(cols, _)| usize::from(cols))
            .unwrap_or(FALLBACK_TERMINAL_WIDTH)
            .min(MAX_TERMINAL_WIDTH);

        Self {
            options,
            document_attributes,
            toc_entries: vec![],
            example_counter: Rc::new(Cell::new(0)),
            appearance,
            section_number_tracker,
            part_number_tracker,
            appendix_tracker,
            terminal_width,
            index_entries: Rc::new(RefCell::new(Vec::new())),
            has_valid_index_section: false,
            list_indent: Rc::new(Cell::new(0)),
        }
    }

    fn options(&self) -> &Options {
        &self.options
    }

    fn document_attributes(&self) -> &DocumentAttributes {
        &self.document_attributes
    }

    fn derive_output_path(&self, _input: &Path, _doc: &Document) -> Result<Option<PathBuf>, Error> {
        // Terminal converter always outputs to stdout by default
        Ok(None)
    }

    fn write_to<W: Write>(
        &self,
        doc: &Document,
        writer: W,
        _source_file: Option<&Path>,
    ) -> Result<(), Self::Error> {
        let section_number_tracker = SectionNumberTracker::new(&doc.attributes);
        let part_number_tracker =
            PartNumberTracker::new(&doc.attributes, section_number_tracker.clone());
        let appendix_tracker =
            AppendixTracker::new(&doc.attributes, section_number_tracker.clone());

        let processor = Processor {
            document_attributes: doc.attributes.clone(),
            toc_entries: doc.toc_entries.clone(),
            options: self.options.clone(),
            example_counter: self.example_counter.clone(),
            appearance: self.appearance.clone(),
            section_number_tracker,
            part_number_tracker,
            appendix_tracker,
            terminal_width: self.terminal_width,
            index_entries: Rc::new(RefCell::new(Vec::new())),
            has_valid_index_section: Self::last_section_is_index(&doc.blocks),
            list_indent: Rc::new(Cell::new(0)),
        };
        let mut visitor = TerminalVisitor::new(writer, processor);
        visitor.visit_document(doc)?;
        Ok(())
    }

    fn backend(&self) -> Backend {
        Backend::Terminal
    }
}

impl Processor {
    /// Override the detected terminal width.
    ///
    /// Useful for tests and fixture generation where a deterministic width is needed.
    #[must_use]
    pub fn with_terminal_width(mut self, width: usize) -> Self {
        self.terminal_width = width.min(MAX_TERMINAL_WIDTH);
        self
    }

    /// Returns the terminal capabilities.
    #[must_use]
    pub fn terminal_capabilities(&self) -> &Capabilities {
        &self.appearance.capabilities
    }

    /// Collect an index term entry for later rendering in the index catalog.
    pub(crate) fn add_index_entry(&self, kind: IndexTermKind) {
        self.index_entries.borrow_mut().push(kind);
    }

    /// Check if the document has a valid index section (last section with `[index]` style).
    #[must_use]
    pub(crate) fn has_valid_index_section(&self) -> bool {
        self.has_valid_index_section
    }

    /// Check if the last section in the document has the `[index]` style.
    fn last_section_is_index(blocks: &[Block]) -> bool {
        let last_section = blocks.iter().rev().find_map(|block| {
            if let Block::Section(section) = block {
                Some(section)
            } else {
                None
            }
        });

        last_section.is_some_and(|section| {
            section
                .metadata
                .style
                .as_ref()
                .is_some_and(|s| s == "index")
        })
    }
}

mod admonition;
mod appearance;
mod audio;
mod delimited;
mod document;
mod error;
mod image;
mod index;
mod inlines;
mod list;
mod paragraph;
mod section;
mod syntax;
mod table;
mod terminal_visitor;
mod toc;
mod video;

pub use appearance::Capabilities;
pub use error::Error;
pub use terminal_visitor::TerminalVisitor;
