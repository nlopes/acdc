use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    io::Write,
    path::{Path, PathBuf},
    rc::Rc,
};

use acdc_converters_core::{
    BackendProfile, Converter as _, Diagnostics, InlineTextTransform, Options, TraversalContext,
    section::contains_index_section, visitor::DocumentVisitor, xref::XrefGuard,
};
use acdc_parser::{
    CaptionKind, DocumentAttributes, Options as ParserOptions, ReferenceTarget,
    ast::{AttributesRef, CaptionRef, ContentRef, DocumentRef},
};

pub(crate) use appearance::Appearance;

const FALLBACK_TERMINAL_WIDTH: usize = 80;
const MAX_TERMINAL_WIDTH: usize = 120;

const TERMINAL_BACKEND: BackendProfile =
    BackendProfile::new("terminal", "terminal", "terminal", ".terminal");

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct IndexTermLabel {
    plain: String,
    rendered: String,
}

#[derive(Clone, Debug)]
struct IndexTermEntry {
    primary: IndexTermLabel,
    secondary: Option<IndexTermLabel>,
    tertiary: Option<IndexTermLabel>,
    relationship: IndexCatalogRelationship,
}

#[derive(Clone, Debug)]
enum IndexCatalogRelationship {
    None,
    See(IndexTermLabel),
    SeeAlso(Vec<IndexTermLabel>),
}

#[derive(Clone, Debug)]
pub struct Converter<'a> {
    options: Options,
    parser_options: ParserOptions<'a>,
    document: Option<DocumentRef<'a>>,
    references: Rc<HashMap<&'a str, ReferenceTarget<'a>>>,
    /// Keeps a cross-reference inside a resolved target's text from recursing.
    xref_guard: XrefGuard,
    /// Fallback counter for example captions the parser did not number.
    example_counter: Rc<Cell<u32>>,
    /// Fallback counter for figure captions the parser did not number.
    figure_counter: Rc<Cell<u32>>,
    /// Fallback counter for listing and source captions the parser did not number.
    listing_counter: Rc<Cell<u32>>,
    /// Fallback counter for table captions the parser did not number.
    table_counter: Rc<Cell<u32>>,
    /// Terminal appearance (theme, capabilities, colors)
    appearance: Appearance,
    /// Terminal width (read once at start, capped at `MAX_TERMINAL_WIDTH`).
    terminal_width: usize,
    /// Collected index terms for rendering in the index catalog.
    index_entries: Rc<RefCell<Vec<IndexTermEntry>>>,
    /// Whether the section hierarchy contains an `[index]` section.
    has_valid_index_section: bool,
    /// Current list nesting indentation (shared across clones).
    list_indent: Rc<Cell<usize>>,
    /// Document-wide fallback warnings already emitted by this converter.
    warned_fallbacks: Rc<RefCell<HashSet<&'static str>>>,
}

/// Build a `Converter` for tests with default document attributes.
#[cfg(test)]
fn create_test_converter() -> Converter<'static> {
    create_test_converter_with(DocumentAttributes::default())
}

/// Build a `Converter` for tests from the given document attributes.
///
/// Every test in the crate goes through here, so a new `Converter` field is
/// filled in once rather than in each test module.
#[cfg(test)]
fn create_test_converter_with(document_attributes: DocumentAttributes<'_>) -> Converter<'_> {
    let appearance = Appearance::detect();
    Converter {
        options: Options::default(),
        parser_options: ParserOptions::default().with_document_attributes(document_attributes),
        document: None,
        references: Rc::new(HashMap::new()),
        xref_guard: XrefGuard::default(),
        example_counter: Rc::new(Cell::new(0)),
        figure_counter: Rc::new(Cell::new(0)),
        listing_counter: Rc::new(Cell::new(0)),
        table_counter: Rc::new(Cell::new(0)),
        appearance,
        terminal_width: FALLBACK_TERMINAL_WIDTH,
        index_entries: Rc::new(RefCell::new(Vec::new())),
        has_valid_index_section: false,
        list_indent: Rc::new(Cell::new(0)),
        warned_fallbacks: Rc::new(RefCell::new(HashSet::new())),
    }
}

impl<'a> acdc_converters_core::Converter<'a> for Converter<'a> {
    type Error = Error;

    fn new(
        options: Options,
        parser_options: acdc_parser::OptionsBuilder<'a>,
    ) -> Result<Self, Self::Error> {
        let mut parser_options = parser_options;
        for (name, value) in Self::document_attributes_defaults() {
            if parser_options.attribute(name).is_none() {
                parser_options = parser_options.with_default_attribute(*name, value.clone());
            }
        }
        parser_options =
            TERMINAL_BACKEND.apply(parser_options, options.doctype(), options.embedded());
        let appearance = Appearance::detect();

        // On Unix, `size()` can launch `tput` when no terminal is available.
        // Query the terminal directly and use our fallback if that fails.
        #[cfg(unix)]
        let terminal_width = crossterm::terminal::window_size().map(|size| size.columns);
        // `window_size()` is not implemented for the Windows console API.
        #[cfg(not(unix))]
        let terminal_width = crossterm::terminal::size().map(|(columns, _)| columns);
        let terminal_width = terminal_width
            .map_or(FALLBACK_TERMINAL_WIDTH, usize::from)
            .min(MAX_TERMINAL_WIDTH);

        let parser_options = parser_options.build()?;
        Ok(Self {
            options,
            parser_options,
            document: None,
            references: Rc::new(HashMap::new()),
            xref_guard: XrefGuard::default(),
            example_counter: Rc::new(Cell::new(0)),
            figure_counter: Rc::new(Cell::new(0)),
            listing_counter: Rc::new(Cell::new(0)),
            table_counter: Rc::new(Cell::new(0)),
            appearance,
            terminal_width,
            index_entries: Rc::new(RefCell::new(Vec::new())),
            has_valid_index_section: false,
            list_indent: Rc::new(Cell::new(0)),
            warned_fallbacks: Rc::new(RefCell::new(HashSet::new())),
        })
    }

    fn options(&self) -> &Options {
        &self.options
    }

    fn parser_options(&self) -> &ParserOptions<'a> {
        &self.parser_options
    }

    fn derive_output_path(
        &self,
        _input: &Path,
        _doc: DocumentRef<'_>,
    ) -> Result<Option<PathBuf>, Error> {
        // Terminal converter always outputs to stdout by default
        Ok(None)
    }

    fn write_to<W: Write>(
        &self,
        doc: DocumentRef<'_>,
        writer: W,
        _source_file: Option<&Path>,
        _output_path: Option<&Path>,
        diagnostics: &mut Diagnostics<'_>,
    ) -> Result<(), Self::Error> {
        // This converter borrows from `doc`; its lifetime is independent of `self`.
        let document = doc;
        let converter = Converter {
            parser_options: ParserOptions::default()
                .with_document_attributes(document.attributes().into()),
            document: Some(document),
            references: Rc::new(document.references().into()),
            xref_guard: XrefGuard::default(),
            options: self.options.clone(),
            example_counter: Rc::new(Cell::new(
                document.highest_caption_number(CaptionKind::Example),
            )),
            figure_counter: Rc::new(Cell::new(
                document.highest_caption_number(CaptionKind::Figure),
            )),
            listing_counter: Rc::new(Cell::new(
                document.highest_caption_number(CaptionKind::Listing),
            )),
            table_counter: Rc::new(Cell::new(
                document.highest_caption_number(CaptionKind::Table),
            )),
            appearance: self.appearance.clone(),
            terminal_width: self.terminal_width,
            index_entries: Rc::new(RefCell::new(Vec::new())),
            has_valid_index_section: contains_index_section(document.blocks()),
            list_indent: Rc::new(Cell::new(0)),
            warned_fallbacks: Rc::new(RefCell::new(HashSet::new())),
        };
        let mut traversal = TraversalContext::new(document.attributes());
        let mut visitor = TerminalVisitor::new(writer, &converter, diagnostics.reborrow());
        visitor.visit_document(&mut traversal, document)
    }

    fn name(&self) -> &'static str {
        "terminal"
    }
}

impl Converter<'_> {
    /// Return the caption prefix for a titled block.
    fn caption_prefix(
        &self,
        attributes: AttributesRef<'_>,
        fallback: Option<CaptionKind>,
    ) -> Option<String> {
        let resolved = CaptionRef::resolve(
            attributes,
            self.parser_options.document_attributes().into(),
            fallback,
        )?;
        match resolved {
            CaptionRef::Numbered {
                label,
                number,
                kind,
            } => {
                let number = number
                    .map_or_else(|| self.next_caption_number(kind), std::num::NonZeroU32::get);
                Some(format!("{label} {number}. "))
            }
            CaptionRef::Custom(prefix) => Some(prefix.to_owned()),
            CaptionRef::CustomContent(content) => Some(
                InlineTextTransform::default()
                    .decode_char_refs(true)
                    .content_to_string(content),
            ),
            CaptionRef::Unnumbered | _ => None,
        }
    }

    fn next_caption_number(&self, kind: CaptionKind) -> u32 {
        let counter = match kind {
            CaptionKind::Figure => &self.figure_counter,
            CaptionKind::Listing => &self.listing_counter,
            CaptionKind::Table => &self.table_counter,
            CaptionKind::Example | _ => &self.example_counter,
        };
        let number = counter.get() + 1;
        counter.set(number);
        number
    }

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

    /// Sets terminal capabilities instead of using the detected values.
    #[must_use]
    pub fn with_terminal_capabilities(mut self, capabilities: Capabilities) -> Self {
        self.appearance.capabilities = capabilities;
        self
    }

    /// Returns the terminal capabilities.
    #[must_use]
    pub fn terminal_capabilities(&self) -> &Capabilities {
        &self.appearance.capabilities
    }

    /// Collect an index term entry for later rendering in the index catalog.
    fn add_index_entry(&self, entry: IndexTermEntry) {
        self.index_entries.borrow_mut().push(entry);
    }

    /// Check whether the document seeds an `[index]` section.
    #[must_use]
    fn has_valid_index_section(&self) -> bool {
        self.has_valid_index_section
    }

    /// Record a document-wide fallback and return whether its warning is new.
    fn mark_fallback(&self, key: &'static str) -> bool {
        self.warned_fallbacks.borrow_mut().insert(key)
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
    doc: DocumentRef<'_>,
    width: usize,
    diagnostics: &mut Diagnostics<'_>,
) -> Result<Vec<u8>, Error> {
    let converter = Converter::new(
        options,
        ParserOptions::builder().with_attributes(
            DocumentAttributes::from(doc.attributes())
                .to_static()
                .into_inputs(),
        ),
    )?
    .with_terminal_width(width);
    let mut output = Vec::new();
    let source = acdc_converters_core::WarningSource::new("terminal").with_variant("preview");
    let mut warnings = Vec::new();
    let mut terminal_diagnostics = Diagnostics::new(&source, &mut warnings);

    let color_guard = ColorOutputGuard::force_enabled();
    converter.write_to(doc, &mut output, None, None, &mut terminal_diagnostics)?;
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
/// Use [`render_listing_to_ansi_with_diagnostics`] to collect fallback warnings.
///
/// # Errors
///
/// Returns an error if syntax highlighting or writing fails.
#[cfg(feature = "emulator")]
pub fn render_listing_to_ansi(
    options: Options,
    document_attributes: DocumentAttributes<'_>,
    content: ContentRef<'_>,
    attributes: AttributesRef<'_>,
    width: usize,
    dark_mode: bool,
) -> Result<Vec<u8>, Error> {
    render_listing_ansi(
        options,
        document_attributes,
        content,
        attributes,
        width,
        dark_mode,
        None,
    )
}

/// Render a listing/source block to ANSI bytes and collect fallback warnings.
///
/// # Errors
///
/// Returns an error if syntax highlighting or writing fails.
#[cfg(feature = "emulator")]
pub fn render_listing_to_ansi_with_diagnostics(
    options: Options,
    document_attributes: DocumentAttributes<'_>,
    content: ContentRef<'_>,
    attributes: AttributesRef<'_>,
    width: usize,
    dark_mode: bool,
    diagnostics: &mut Diagnostics<'_>,
) -> Result<Vec<u8>, Error> {
    render_listing_ansi(
        options,
        document_attributes,
        content,
        attributes,
        width,
        dark_mode,
        Some(diagnostics),
    )
}

#[cfg(feature = "emulator")]
fn render_listing_ansi(
    options: Options,
    document_attributes: DocumentAttributes<'_>,
    content: ContentRef<'_>,
    attributes: AttributesRef<'_>,
    width: usize,
    dark_mode: bool,
    diagnostics: Option<&mut Diagnostics<'_>>,
) -> Result<Vec<u8>, Error> {
    let mut converter = Converter::new(options, ParserOptions::builder())?
        .with_terminal_width(width)
        .with_dark_mode(dark_mode);
    converter.parser_options = converter
        .parser_options
        .with_document_attributes(document_attributes);
    let color_guard = ColorOutputGuard::force_enabled();
    let language =
        acdc_converters_core::code::source_language(attributes).map(preview_highlight_language);
    let output = crate::delimited::render_preformatted_inlines(
        content.inlines(),
        language,
        &converter,
        |source| acdc_converters_core::code::SourceLineOptions::from_attributes(attributes, source),
        diagnostics,
    )?
    .into_bytes();

    drop(color_guard);
    Ok(output)
}

#[cfg(test)]
mod backend_tests {
    use super::*;
    use std::error::Error as StdError;

    #[test]
    fn constructor_applies_terminal_backend_profile() -> Result<(), Box<dyn StdError>> {
        let converter = Converter::new(Options::default(), ParserOptions::builder())?;

        assert_eq!(
            converter
                .document_attributes()
                .get("backend")
                .and_then(|value| value.text()),
            Some("terminal")
        );
        assert_eq!(
            converter
                .document_attributes()
                .get("filetype")
                .and_then(|value| value.text()),
            Some("terminal")
        );
        Ok(())
    }

    #[test]
    fn canonical_body_attributes_apply_in_source_order() -> Result<(), Box<dyn StdError>> {
        let parsed = acdc_parser::parse(
            include_str!("../../../acdc-parser/fixtures/tests/document_attributes_consumers.adoc"),
            &ParserOptions::default(),
        )?;
        let converter = Converter::new(
            Options::default(),
            ParserOptions::builder().with_attributes(
                acdc_parser::DocumentAttributes::from(parsed.document().attributes()).into_inputs(),
            ),
        )?
        .with_terminal_width(120);
        let source = converter.warning_source();
        let mut warnings = Vec::new();
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let mut output = Vec::new();

        converter.write_to(parsed.document(), &mut output, None, None, &mut diagnostics)?;

        let terminal = String::from_utf8(output)?;
        assert!(terminal.contains("images/header/before.png"));
        assert!(terminal.contains("images/body/after.png"));
        Ok(())
    }

    #[test]
    fn body_experimental_applies_inside_buffered_inline_content() -> Result<(), Box<dyn StdError>> {
        let parsed = acdc_parser::parse(
            "= T\n:experimental!:\n\n:experimental:\nhttps://example.com[btn:[Run]]\n",
            &ParserOptions::default(),
        )?;
        let converter = Converter::new(
            Options::default(),
            ParserOptions::builder().with_attributes(
                acdc_parser::DocumentAttributes::from(parsed.document().attributes()).into_inputs(),
            ),
        )?;
        let source = converter.warning_source();
        let mut warnings = Vec::new();
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let mut output = Vec::new();

        converter.write_to(parsed.document(), &mut output, None, None, &mut diagnostics)?;

        let terminal = String::from_utf8(output)?;
        assert!(terminal.contains("[Run]"), "{terminal:?}");
        assert!(!terminal.contains("btn:[Run]"), "{terminal:?}");
        Ok(())
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

fn extract_heading_content(
    nodes: Option<ContentRef<'_>>,
    references: &HashMap<&str, ReferenceTarget<'_>>,
) -> String {
    nodes.map_or_else(String::new, |content| {
        InlineTextTransform::default()
            .decode_char_refs(true)
            .references(references.into())
            .content_to_string(content)
    })
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
use terminal_visitor::TerminalVisitor;
