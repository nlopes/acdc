use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    io::Write,
    path::{Path, PathBuf},
    rc::Rc,
};

#[cfg(feature = "pre-spec-subs")]
use acdc_converters_core::substitutions::SubsFlags;
use acdc_converters_core::{
    BackendTraits, Converter, Diagnostics, InlineTextTransform, Options,
    section::{
        AppendixTracker, PartNumberTracker, SectionNumberTracker, SpecialSectionTracker,
        last_section_has_style,
    },
    visitor::Visitor,
    xref::XrefGuard,
};
#[cfg(feature = "emulator")]
use acdc_parser::BlockMetadata;
use acdc_parser::{Document, DocumentAttributes, IndexTermKind, InlineNode, Reference, TocEntry};

pub(crate) use appearance::Appearance;

pub(crate) const FALLBACK_TERMINAL_WIDTH: usize = 80;
pub(crate) const MAX_TERMINAL_WIDTH: usize = 120;

/// Intrinsic traits for the terminal backend.
const BACKEND_TRAITS: BackendTraits =
    BackendTraits::new("terminal", "terminal", "terminal", ".terminal");

/// Leak a `&str` into a `&'static str`.
///
/// Used when caching index term data beyond the parser's arena lifetime.
/// Leaks are bounded by the number of index entries encountered during a
/// single document conversion; acceptable for a short-lived converter run.
fn leak_str(s: &str) -> &'static str {
    Box::leak(s.to_string().into_boxed_str())
}

#[derive(Clone, Debug)]
pub struct Processor<'a> {
    pub(crate) options: Options,
    pub(crate) document_attributes: DocumentAttributes<'a>,
    pub(crate) toc_entries: Vec<TocEntry<'a>>,
    pub(crate) references: Rc<HashMap<&'a str, Reference<'a>>>,
    /// Keeps a cross-reference inside a resolved target's text from recursing.
    pub(crate) xref_guard: XrefGuard,
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
    /// Tracks special sections so their subsections skip `:sectnums:` numbering.
    pub(crate) special_section_tracker: SpecialSectionTracker,
    /// Terminal width (read once at start, capped at `MAX_TERMINAL_WIDTH`).
    pub(crate) terminal_width: usize,
    /// Collected index term kinds for rendering in the index catalog.
    ///
    /// Stored as `'static` because entries are collected during visitor
    /// traversal where the `Visitor` trait erases lifetimes, preventing
    /// propagation of `'a` through the call chain.
    pub(crate) index_entries: Rc<RefCell<Vec<IndexTermKind<'static>>>>,
    /// Whether the document has a valid `[index]` section (last section).
    pub(crate) has_valid_index_section: bool,
    /// Current list nesting indentation (shared across clones).
    pub(crate) list_indent: Rc<Cell<usize>>,
    /// Substitutions active for the block currently being rendered, resolved
    /// from `[subs="…"]` (or the block-kind baseline when absent). Lives on
    /// `Processor` so freestanding inline helpers can consult it without
    /// threading a slice through every recursive call. Shared across clones
    /// so sub-visitors (e.g. the temp visitors used for styled paragraphs)
    /// inherit the outer block's effective subs. `Cell<SubsFlags>` is a
    /// single-byte load/store with no borrow tracking — chosen over
    /// `RefCell<Vec<…>>` because the hot path runs once per inline leaf.
    ///
    /// Only present when the `pre-spec-subs` feature is enabled; otherwise
    /// the converter applies typography unconditionally (asciidoctor default).
    #[cfg(feature = "pre-spec-subs")]
    pub(crate) current_subs: Rc<Cell<SubsFlags>>,
}

/// Build a `Processor` for tests with default document attributes.
#[cfg(test)]
pub(crate) fn create_test_processor() -> Processor<'static> {
    create_test_processor_with(DocumentAttributes::default())
}

/// Build a `Processor` for tests from the given document attributes.
///
/// Every test in the crate goes through here, so a new `Processor` field is
/// filled in once rather than in each test module.
#[cfg(test)]
pub(crate) fn create_test_processor_with(
    document_attributes: DocumentAttributes<'_>,
) -> Processor<'_> {
    let appearance = Appearance::detect();
    let section_number_tracker = SectionNumberTracker::new(&document_attributes);
    let part_number_tracker = PartNumberTracker::new(&document_attributes);
    let appendix_tracker =
        AppendixTracker::new(&document_attributes, section_number_tracker.clone());
    let special_section_tracker = SpecialSectionTracker::new(&document_attributes);
    Processor {
        options: Options::default(),
        document_attributes,
        toc_entries: vec![],
        references: Rc::new(HashMap::new()),
        xref_guard: XrefGuard::default(),
        example_counter: Rc::new(Cell::new(0)),
        appearance,
        section_number_tracker,
        part_number_tracker,
        appendix_tracker,
        special_section_tracker,
        terminal_width: FALLBACK_TERMINAL_WIDTH,
        index_entries: Rc::new(RefCell::new(Vec::new())),
        has_valid_index_section: false,
        list_indent: Rc::new(Cell::new(0)),
        #[cfg(feature = "pre-spec-subs")]
        current_subs: Rc::new(Cell::new(SubsFlags::all())),
    }
}

impl<'a> Converter<'a> for Processor<'a> {
    type Error = Error;

    fn document_attributes_defaults() -> DocumentAttributes<'static> {
        // Terminal converter uses environment detection (Appearance::detect())
        // rather than document attributes for its configuration.
        // No terminal-specific attribute defaults needed.
        DocumentAttributes::default()
    }

    fn new(options: Options, document_attributes: DocumentAttributes<'a>) -> Self {
        let mut document_attributes = document_attributes;
        for (name, value) in Self::document_attributes_defaults().iter() {
            document_attributes.insert(name.clone(), value.clone());
        }
        BACKEND_TRAITS.apply(&mut document_attributes, options.doctype());
        let appearance = Appearance::detect();

        let section_number_tracker = SectionNumberTracker::new(&document_attributes);
        let part_number_tracker = PartNumberTracker::new(&document_attributes);
        let appendix_tracker =
            AppendixTracker::new(&document_attributes, section_number_tracker.clone());
        let special_section_tracker = SpecialSectionTracker::new(&document_attributes);

        let terminal_width = crossterm::terminal::size()
            .map_or(FALLBACK_TERMINAL_WIDTH, |(cols, _)| usize::from(cols))
            .min(MAX_TERMINAL_WIDTH);

        Self {
            options,
            document_attributes,
            toc_entries: vec![],
            references: Rc::new(HashMap::new()),
            xref_guard: XrefGuard::default(),
            example_counter: Rc::new(Cell::new(0)),
            appearance,
            section_number_tracker,
            part_number_tracker,
            appendix_tracker,
            special_section_tracker,
            terminal_width,
            index_entries: Rc::new(RefCell::new(Vec::new())),
            has_valid_index_section: false,
            list_indent: Rc::new(Cell::new(0)),
            #[cfg(feature = "pre-spec-subs")]
            current_subs: Rc::new(Cell::new(SubsFlags::all())),
        }
    }

    fn options(&self) -> &Options {
        &self.options
    }

    fn document_attributes(&self) -> &DocumentAttributes<'a> {
        &self.document_attributes
    }

    fn derive_output_path(
        &self,
        _input: &Path,
        _doc: &Document<'_>,
    ) -> Result<Option<PathBuf>, Error> {
        // Terminal converter always outputs to stdout by default
        Ok(None)
    }

    fn write_to<W: Write>(
        &self,
        doc: &Document<'_>,
        writer: W,
        _source_file: Option<&Path>,
        _output_path: Option<&Path>,
        diagnostics: &mut Diagnostics<'_>,
    ) -> Result<(), Self::Error> {
        let section_number_tracker = SectionNumberTracker::new(&doc.attributes);
        let part_number_tracker = PartNumberTracker::new(&doc.attributes);
        let appendix_tracker =
            AppendixTracker::new(&doc.attributes, section_number_tracker.clone());
        let special_section_tracker = SpecialSectionTracker::new(&doc.attributes);

        // Per-conversion processor borrows from `doc`; lifetime independent of `self`.
        let processor = Processor {
            document_attributes: doc.attributes.clone(),
            toc_entries: doc.toc_entries.clone(),
            references: Rc::new(doc.references.clone()),
            xref_guard: XrefGuard::default(),
            options: self.options.clone(),
            example_counter: self.example_counter.clone(),
            appearance: self.appearance.clone(),
            section_number_tracker,
            part_number_tracker,
            appendix_tracker,
            special_section_tracker,
            terminal_width: self.terminal_width,
            index_entries: Rc::new(RefCell::new(Vec::new())),
            has_valid_index_section: last_section_has_style(&doc.blocks, "index"),
            list_indent: Rc::new(Cell::new(0)),
            #[cfg(feature = "pre-spec-subs")]
            current_subs: Rc::new(Cell::new(SubsFlags::all())),
        };
        let mut visitor = TerminalVisitor::new(writer, processor, diagnostics.reborrow());
        visitor.visit_document(doc)
    }

    fn name(&self) -> &'static str {
        "terminal"
    }
}

impl Processor<'_> {
    /// Override the detected terminal width.
    ///
    /// Useful for tests and fixture generation where a deterministic width is needed.
    #[must_use]
    pub fn with_terminal_width(mut self, width: usize) -> Self {
        self.terminal_width = width.min(MAX_TERMINAL_WIDTH);
        self
    }

    /// Override the detected terminal appearance from an explicit dark-mode value.
    #[must_use]
    pub fn with_dark_mode(mut self, dark_mode: bool) -> Self {
        self.appearance = Appearance::for_dark_mode(dark_mode);
        self
    }

    /// Returns the terminal capabilities.
    #[must_use]
    pub fn terminal_capabilities(&self) -> &Capabilities {
        &self.appearance.capabilities
    }

    /// Collect an index term entry for later rendering in the index catalog.
    pub(crate) fn add_index_entry(&self, kind: &IndexTermKind<'_>) {
        let owned: IndexTermKind<'static> = match kind {
            IndexTermKind::Flow(t) => IndexTermKind::Flow(leak_str(t)),
            IndexTermKind::Concealed {
                term,
                secondary,
                tertiary,
            } => IndexTermKind::Concealed {
                term: leak_str(term),
                secondary: secondary.map(leak_str),
                tertiary: tertiary.map(leak_str),
            },
            _ => return,
        };
        self.index_entries.borrow_mut().push(owned);
    }

    /// Check if the document has a valid index section (last section with `[index]` style).
    #[must_use]
    pub(crate) fn has_valid_index_section(&self) -> bool {
        self.has_valid_index_section
    }
}

/// Render an `AsciiDoc` document to ANSI terminal bytes at a deterministic width.
///
/// This is intended for downstream converters that need terminal-rendered bytes
/// without depending on terminal implementation details such as color handling.
///
/// # Errors
///
/// Returns an error if terminal conversion or writing fails.
pub fn render_document_to_ansi(
    options: Options,
    doc: &Document<'_>,
    width: usize,
    diagnostics: &mut Diagnostics<'_>,
) -> Result<Vec<u8>, Error> {
    let processor = Processor::new(options, doc.attributes.clone()).with_terminal_width(width);
    let mut output = Vec::new();
    let source = acdc_converters_core::WarningSource::new("terminal").with_variant("preview");
    let mut warnings = Vec::new();
    let mut terminal_diagnostics = Diagnostics::new(&source, &mut warnings);

    let color_guard = ColorOutputGuard::force_enabled();
    processor.write_to(doc, &mut output, None, None, &mut terminal_diagnostics)?;
    drop(color_guard);

    for warning in warnings {
        diagnostics.emit(warning);
    }

    Ok(output)
}

/// Render a single listing/source block to ANSI terminal bytes.
///
/// This keeps terminal syntax highlighting and ANSI generation inside the
/// terminal converter crate while allowing other converters to render the
/// resulting bytes through a terminal emulator.
///
/// # Errors
///
/// Returns an error if syntax highlighting or writing fails.
#[cfg(feature = "emulator")]
pub fn render_listing_to_ansi(
    options: Options,
    document_attributes: DocumentAttributes<'_>,
    inlines: &[InlineNode<'_>],
    metadata: &BlockMetadata<'_>,
    width: usize,
    dark_mode: bool,
) -> Result<Vec<u8>, Error> {
    let processor = Processor::new(options, document_attributes)
        .with_terminal_width(width)
        .with_dark_mode(dark_mode);
    let mut output = Vec::new();
    let color_guard = ColorOutputGuard::force_enabled();

    if let Some(language) = acdc_converters_core::code::detect_language(metadata) {
        crate::syntax::highlight_code(
            &mut output,
            inlines,
            preview_highlight_language(language),
            &processor,
        )?;
    } else {
        write!(output, "{}", extract_inline_text(inlines, "\n"))?;
    }

    drop(color_guard);
    Ok(output)
}

#[cfg(test)]
mod backend_tests {
    use super::*;

    #[test]
    fn constructor_applies_terminal_backend_traits() {
        let processor = Processor::new(Options::default(), DocumentAttributes::default());

        assert_eq!(
            processor
                .document_attributes()
                .get_string("backend")
                .as_deref(),
            Some("terminal")
        );
        assert_eq!(
            processor
                .document_attributes()
                .get_string("filetype")
                .as_deref(),
            Some("terminal")
        );
    }
}

#[cfg(feature = "emulator")]
fn preview_highlight_language(language: &str) -> &str {
    match language {
        "console" | "terminal" | "shell" => "bash",
        other => other,
    }
}

struct ColorOutputGuard {
    previous_disabled: bool,
}

impl ColorOutputGuard {
    fn force_enabled() -> Self {
        let previous_disabled = crossterm::style::Colored::ansi_color_disabled_memoized();
        crossterm::style::force_color_output(true);
        Self { previous_disabled }
    }
}

impl Drop for ColorOutputGuard {
    fn drop(&mut self) {
        crossterm::style::Colored::set_ansi_color_disabled(self.previous_disabled);
    }
}

/// Plain text for output that cannot carry styling, such as literal content.
///
/// `line_break` controls how `LineBreak` nodes are represented: `" "` for
/// titles, `"\n"` for literal paragraphs.
pub(crate) fn extract_inline_text(nodes: &[InlineNode], line_break: &str) -> String {
    InlineTextTransform::default()
        .line_break(line_break)
        .decode_char_refs(true)
        .to_string(nodes)
}

/// Plain text for a heading or caption.
///
/// Every node contributes its text: a link contributes its link text, and a
/// cross-reference contributes its target's reference text, matching
/// `asciidoctor`.
pub(crate) fn extract_heading_text(
    nodes: &[InlineNode],
    references: &HashMap<&str, Reference<'_>>,
) -> String {
    InlineTextTransform::default()
        .decode_char_refs(true)
        .references(references)
        .to_string(nodes)
}

mod admonition;
mod appearance;
#[cfg(feature = "emulator")]
pub mod asciicast;
mod audio;
#[cfg(feature = "emulator")]
pub mod cell_grid;
mod delimited;
mod document;
mod error;
mod image;
mod index;
mod inlines;
mod list;
mod paragraph;
#[cfg(feature = "emulator")]
pub mod replay;
mod section;
mod syntax;
mod table;
mod terminal_visitor;
mod toc;
mod video;
mod wrap;

pub use appearance::Capabilities;
pub use error::Error;
pub use terminal_visitor::TerminalVisitor;
