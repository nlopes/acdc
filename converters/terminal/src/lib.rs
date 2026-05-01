use std::{
    cell::{Cell, RefCell},
    io::Write,
    path::{Path, PathBuf},
    rc::Rc,
};

use acdc_converters_core::{
    Converter, Diagnostics, Options, decode_numeric_char_refs,
    section::{AppendixTracker, PartNumberTracker, SectionNumberTracker, last_section_has_style},
    visitor::Visitor,
};
use acdc_parser::{Document, DocumentAttributes, IndexTermKind, InlineMacro, InlineNode, TocEntry};

pub(crate) use appearance::Appearance;

pub(crate) const FALLBACK_TERMINAL_WIDTH: usize = 80;
pub(crate) const MAX_TERMINAL_WIDTH: usize = 120;

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
    ///
    /// Stored as `'static` because entries are collected during visitor
    /// traversal where the `Visitor` trait erases lifetimes, preventing
    /// propagation of `'a` through the call chain.
    pub(crate) index_entries: Rc<RefCell<Vec<IndexTermKind<'static>>>>,
    /// Whether the document has a valid `[index]` section (last section).
    pub(crate) has_valid_index_section: bool,
    /// Current list nesting indentation (shared across clones).
    pub(crate) list_indent: Rc<Cell<usize>>,
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
        let appearance = Appearance::detect();

        let section_number_tracker = SectionNumberTracker::new(&document_attributes);
        let part_number_tracker =
            PartNumberTracker::new(&document_attributes, section_number_tracker.clone());
        let appendix_tracker =
            AppendixTracker::new(&document_attributes, section_number_tracker.clone());

        let terminal_width = crossterm::terminal::size()
            .map_or(FALLBACK_TERMINAL_WIDTH, |(cols, _)| usize::from(cols))
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

    fn document_attributes(&self) -> &DocumentAttributes<'a> {
        &self.document_attributes
    }

    fn derive_output_path(
        &self,
        _input: &Path,
        _doc: &Document<'a>,
    ) -> Result<Option<PathBuf>, Error> {
        // Terminal converter always outputs to stdout by default
        Ok(None)
    }

    fn write_to<W: Write>(
        &self,
        doc: &Document<'a>,
        writer: W,
        _source_file: Option<&Path>,
        _output_path: Option<&Path>,
        diagnostics: &mut Diagnostics<'_>,
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
            has_valid_index_section: last_section_has_style(&doc.blocks, "index"),
            list_indent: Rc::new(Cell::new(0)),
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

/// Extract plain text from inline nodes, recursively handling all formatting variants.
///
/// `line_break` controls how `LineBreak` nodes are represented: `" "` for titles,
/// `"\n"` for literal paragraphs.
pub(crate) fn extract_inline_text(nodes: &[InlineNode], line_break: &str) -> String {
    nodes
        .iter()
        .map(|node| match node {
            InlineNode::PlainText(p) => p.content.to_string(),
            InlineNode::BoldText(b) => extract_inline_text(&b.content, line_break),
            InlineNode::ItalicText(i) => extract_inline_text(&i.content, line_break),
            InlineNode::MonospaceText(m) => extract_inline_text(&m.content, line_break),
            InlineNode::HighlightText(h) => extract_inline_text(&h.content, line_break),
            InlineNode::SuperscriptText(s) => extract_inline_text(&s.content, line_break),
            InlineNode::SubscriptText(s) => extract_inline_text(&s.content, line_break),
            InlineNode::CurvedQuotationText(c) => extract_inline_text(&c.content, line_break),
            InlineNode::CurvedApostropheText(c) => extract_inline_text(&c.content, line_break),
            InlineNode::VerbatimText(v) => v.content.to_string(),
            InlineNode::RawText(r) => decode_numeric_char_refs(r.content).into_owned(),
            InlineNode::StandaloneCurvedApostrophe(_) => "\u{2019}".to_string(),
            InlineNode::LineBreak(_) => line_break.to_string(),
            InlineNode::CalloutRef(c) => format!("<{}>", c.number),
            InlineNode::Macro(m) => extract_macro_text(m, line_break),
            // InlineAnchor is an invisible marker; unknown future variants fall through
            InlineNode::InlineAnchor(_) | _ => String::new(),
        })
        .collect::<String>()
}

pub(crate) fn extract_macro_text(m: &InlineMacro, line_break: &str) -> String {
    match m {
        InlineMacro::Image(img) => img.source.to_string(),
        InlineMacro::Icon(icon) => icon.target.to_string(),
        InlineMacro::Keyboard(kbd) => kbd
            .keys
            .iter()
            .map(std::convert::AsRef::as_ref)
            .collect::<Vec<&str>>()
            .join("+"),
        InlineMacro::Button(b) => b.label.to_string(),
        InlineMacro::Menu(menu) => {
            let mut parts: Vec<String> = vec![menu.target.to_string()];
            parts.extend(menu.items.iter().map(|i| (*i).to_string()));
            parts.join(" > ")
        }
        InlineMacro::Link(l) => {
            let text = extract_inline_text(&l.text, line_break);
            if text.is_empty() {
                l.target.to_string()
            } else {
                text
            }
        }
        InlineMacro::Url(u) => {
            let text = extract_inline_text(&u.text, line_break);
            if text.is_empty() {
                u.target.to_string()
            } else {
                text
            }
        }
        InlineMacro::Mailto(m) => {
            let text = extract_inline_text(&m.text, line_break);
            if text.is_empty() {
                m.target.to_string()
            } else {
                text
            }
        }
        InlineMacro::Autolink(a) => a.url.to_string(),
        InlineMacro::CrossReference(x) => {
            let text = extract_inline_text(&x.text, line_break);
            if text.is_empty() {
                x.target.to_string()
            } else {
                text
            }
        }
        InlineMacro::Footnote(f) => format!("[{}]", f.number),
        InlineMacro::Pass(p) => p.text.map(ToString::to_string).unwrap_or_default(),
        InlineMacro::Stem(s) => s.content.to_string(),
        InlineMacro::IndexTerm(it) => match &it.kind {
            IndexTermKind::Flow(term) => (*term).to_string(),
            IndexTermKind::Concealed { .. } | _ => String::new(),
        },
        _ => String::new(),
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
mod wrap;

pub use appearance::Capabilities;
pub use error::Error;
pub use terminal_visitor::TerminalVisitor;
