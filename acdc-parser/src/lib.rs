//! `AsciiDoc` parser.
//!
//! This module provides a parser for the `AsciiDoc` markup language. The parser is
//! implemented using the `peg` parser generator.
//!
//! # Quick Start
//!
//! The parser is implemented as a struct that implements the `Parser` trait. The
//! trait provides two methods for parsing `AsciiDoc` content:
//!
//! - `parse`: parses a string containing `AsciiDoc` content.
//! - `parse_file`: parses the content of a file containing `AsciiDoc` content.
//!
//! ```rust
//! use acdc_parser::{Document, parse};
//!
//! let content = r#"= Document Title
//!
//! This is a paragraph.
//!
//! == Section Title
//!
//! This is a subsection."#;
//!
//! let options = acdc_parser::Options::default();
//! let document = parse(content, &options).unwrap();
//!
//! println!("{:?}", document);
//! ```
//!
//! # Features
//!
//! - Full support for `AsciiDoc` syntax, including blocks, inline elements, attributes, and more.
//! - Configurable options for parsing behaviour, including safe mode and timing. Just
//!   like `asciidoctor`, you can choose to enable or disable certain features based on your
//!   needs.
//! - Detailed error reporting with source location information.
//! - Support for parsing from strings, files, and readers.
//!

use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
    string::ToString,
};

use tracing::instrument;

mod blocks;
mod constants;
mod error;
pub(crate) mod grammar;
mod model;
mod options;
mod parsed;
mod preprocessor;
mod safe_mode;
mod warning;

pub(crate) use grammar::{InlinePreprocessorParserState, ProcessedContent, inline_preprocessing};
use preprocessor::Preprocessor;

pub use error::{Error, SourceLocation};
pub use grammar::parse_text_for_quotes;
pub use model::{
    Admonition, AdmonitionVariant, Anchor, AttributeName, AttributeValue, Attribution, Audio,
    Author, Autolink, Block, BlockMetadata, Bold, Button, CalloutList, CalloutListItem, CalloutRef,
    CalloutRefKind, CiteTitle, ColumnFormat, ColumnStyle, ColumnWidth, Comment, CommentKind,
    CrossReference, CurvedApostrophe, CurvedQuotation, DelimitedBlock, DelimitedBlockType,
    DescriptionList, DescriptionListItem, DiscreteHeader, Document, DocumentAttribute,
    DocumentAttributes, ElementAttributes, Footnote, Form, HEADER, Header, Highlight,
    HorizontalAlignment, ICON_SIZES, Icon, Image, IndexTerm, IndexTermKind, InlineMacro,
    InlineNode, Italic, Keyboard, LineBreak, Link, ListItem, ListItemCheckedStatus, Location,
    MAX_SECTION_LEVELS, MAX_TOC_LEVELS, Mailto, Menu, Monospace, NORMAL, OrderedList, PageBreak,
    Paragraph, Pass, PassthroughKind, Plain, Position, Raw, Reference, Role, Section, SectionKind,
    Source, SourceUrl, StandaloneCurvedApostrophe, Stem, StemContent, StemNotation, Subscript,
    Substitution, Subtitle, Superscript, Table, TableColumn, TableOfContents, TableRow,
    ThematicBreak, Title, TocEntry, UNNUMBERED_SECTION_STYLES, UnorderedList, Url, VERBATIM,
    Verbatim, VerticalAlignment, Video, strip_quotes, substitute,
};
#[cfg(feature = "pre-spec-subs")]
pub use model::{SubstitutionOp, SubstitutionSpec};
pub use options::{Options, OptionsBuilder, SafeMode};
pub use parsed::{OwnedSource, ParseInlineResult, ParseResult};
pub use warning::{Warning, WarningKind};

/// Type-based parser for `AsciiDoc` content.
///
/// `Parser` provides a more discoverable, fluent API for parsing `AsciiDoc` documents.
///
/// # Examples
///
/// Basic usage:
///
/// ```
/// use acdc_parser::Parser;
///
/// let content = "= Document Title\n\nParagraph text.";
/// let doc = Parser::new(content).parse()?;
/// # Ok::<(), acdc_parser::Error>(())
/// ```
///
/// With options:
///
/// ```
/// use acdc_parser::{Parser, Options, SafeMode};
///
/// let content = "= Document Title\n\nParagraph text.";
/// let options = Options::builder()
///     .with_safe_mode(SafeMode::Safe)
///     .with_timings()
///     .build();
///
/// let doc = Parser::new(content)
///     .with_options(options)
///     .parse()?;
/// # Ok::<(), acdc_parser::Error>(())
/// ```
///
/// For file-based parsing, read the file first:
///
/// ```no_run
/// use acdc_parser::Parser;
/// use std::fs;
///
/// let content = fs::read_to_string("document.adoc")?;
/// let doc = Parser::new(&content).parse()?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug)]
pub struct Parser<'input> {
    input: &'input str,
    options: Options<'input>,
}

impl<'input> Parser<'input> {
    /// Create a new parser for the given input string.
    ///
    /// The parser will use default options. Use `with_options` to customize.
    ///
    /// # Example
    ///
    /// ```
    /// use acdc_parser::Parser;
    ///
    /// let parser = Parser::new("= Title\n\nContent");
    /// let doc = parser.parse()?;
    /// # Ok::<(), acdc_parser::Error>(())
    /// ```
    #[must_use]
    pub fn new(input: &'input str) -> Self {
        Self {
            input,
            options: Options::default(),
        }
    }

    /// Set the options for this parser.
    ///
    /// This consumes the parser and returns a new one with the specified options.
    ///
    /// # Example
    ///
    /// ```
    /// use acdc_parser::{Parser, Options, SafeMode};
    ///
    /// let options = Options::builder()
    ///     .with_safe_mode(SafeMode::Safe)
    ///     .build();
    ///
    /// let parser = Parser::new("= Title")
    ///     .with_options(options);
    /// # Ok::<(), acdc_parser::Error>(())
    /// ```
    #[must_use]
    pub fn with_options(mut self, options: Options<'input>) -> Self {
        self.options = options;
        self
    }

    /// Parse the input into a Document.
    ///
    /// # Example
    ///
    /// ```
    /// use acdc_parser::Parser;
    ///
    /// let doc = Parser::new("= Title\n\nContent").parse()?;
    /// # Ok::<(), acdc_parser::Error>(())
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if the input cannot be parsed as valid `AsciiDoc`.
    pub fn parse(self) -> Result<ParseResult, Error> {
        parse(self.input, &self.options)
    }

    /// Parse only inline elements from the input.
    ///
    /// This is useful for parsing fragments of `AsciiDoc` that contain only
    /// inline markup like bold, italic, links, etc.
    ///
    /// # Example
    ///
    /// ```
    /// use acdc_parser::Parser;
    ///
    /// let inlines = Parser::new("This is *bold* text").parse_inline()?;
    /// # Ok::<(), acdc_parser::Error>(())
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if the input cannot be parsed.
    pub fn parse_inline(self) -> Result<ParseInlineResult, Error> {
        parse_inline(self.input, &self.options)
    }
}

/// Parse `AsciiDoc` content from a reader.
///
/// This function reads the content from the provided reader and parses it as `AsciiDoc`.
///
/// # Example
///
/// ```
/// use acdc_parser::{Options, SafeMode, parse_from_reader};
/// use std::fs::File;
///
/// let options = Options::builder()
///     .with_safe_mode(SafeMode::Unsafe)
///     .build();
/// let file = File::open("fixtures/samples/README.adoc").unwrap();
/// let document = parse_from_reader(file, &options).unwrap();
/// ```
///
/// # Errors
/// This function returns an error if the content cannot be parsed.
#[instrument(skip(reader))]
pub fn parse_from_reader<R: std::io::Read>(
    reader: R,
    options: &Options<'_>,
) -> Result<ParseResult, Error> {
    // Shared across the preprocessor and the grammar state so both layers'
    // warnings land in the same `ParseResult::warnings()` slice.
    let warnings_handle: Rc<RefCell<Vec<Warning>>> = Rc::new(RefCell::new(Vec::new()));
    let result = {
        let _span = tracing::info_span!("preprocess").entered();
        Preprocessor::process_reader(reader, options, Rc::clone(&warnings_handle))?
    };
    let text: Box<str> = result.text.into_owned().into_boxed_str();
    let _span = tracing::info_span!("grammar_parse", input_len = text.len()).entered();
    parse_input(
        text,
        options.clone(),
        None,
        result.leveloffset_ranges,
        result.source_ranges,
        warnings_handle,
    )
}

/// Parse `AsciiDoc` content from a string.
///
/// This function parses the provided string as `AsciiDoc`.
///
/// # Example
///
/// ```
/// use acdc_parser::{Options, SafeMode, parse};
///
/// let options = Options::builder()
///     .with_safe_mode(SafeMode::Unsafe)
///     .build();
/// let content = "= Document Title\n\nThis is a paragraph.\n\n== Section Title\n\nThis is a subsection.";
/// let document = parse(content, &options).unwrap();
/// ```
///
/// # Errors
/// This function returns an error if the content cannot be parsed.
#[instrument]
pub fn parse(input: &str, options: &Options<'_>) -> Result<ParseResult, Error> {
    let warnings_handle: Rc<RefCell<Vec<Warning>>> = Rc::new(RefCell::new(Vec::new()));
    let result = {
        let _span = tracing::info_span!("preprocess").entered();
        Preprocessor::process(input, options, Rc::clone(&warnings_handle))?
    };
    let text: Box<str> = result.text.into_owned().into_boxed_str();
    let _span = tracing::info_span!("grammar_parse", input_len = text.len()).entered();
    parse_input(
        text,
        options.clone(),
        None,
        result.leveloffset_ranges,
        result.source_ranges,
        warnings_handle,
    )
}

/// Parse `AsciiDoc` content from a file.
///
/// This function reads the content from the provided file and parses it as `AsciiDoc`.
///
/// # Example
///
/// ```
/// use std::path::Path;
/// use acdc_parser::{Options, SafeMode, parse_file};
///
/// let options = Options::builder()
///     .with_safe_mode(SafeMode::Unsafe)
///     .build();
/// let file_path = Path::new("fixtures/samples/README.adoc");
/// let document = parse_file(file_path, &options).unwrap();
/// ```
///
/// # Errors
/// This function returns an error if the content cannot be parsed.
#[instrument(skip(file_path))]
pub fn parse_file<P: AsRef<Path>>(
    file_path: P,
    options: &Options<'_>,
) -> Result<ParseResult, Error> {
    let path = file_path.as_ref().to_path_buf();
    let raw = preprocessor::read_and_decode_file(file_path.as_ref(), None)?;
    let warnings_handle: Rc<RefCell<Vec<Warning>>> = Rc::new(RefCell::new(Vec::new()));
    let result = {
        let _span = tracing::info_span!("preprocess").entered();
        Preprocessor::process_with_file(
            &raw,
            file_path.as_ref(),
            options,
            Rc::clone(&warnings_handle),
        )?
    };
    let text: Box<str> = result.text.into_owned().into_boxed_str();
    let _span = tracing::info_span!("grammar_parse", input_len = text.len()).entered();
    parse_input(
        text,
        options.clone(),
        Some(path),
        result.leveloffset_ranges,
        result.source_ranges,
        warnings_handle,
    )
}

/// Helper to convert a PEG parse error to our `SourceLocation` type,
/// resolving the correct file and line for included content.
fn peg_error_to_source_location(
    error: &peg::error::ParseError<peg::str::LineCol>,
    state: &grammar::ParserState,
) -> SourceLocation {
    let offset = error.location.offset;
    if let Some(range) = model::SourceRange::find_containing(&state.source_ranges, offset) {
        SourceLocation {
            file: range.file.clone(),
            location: crate::Location::point(Position::new(
                state.line_map.source_line(range, state.input, offset),
                u32::try_from(error.location.column).unwrap_or(u32::MAX),
            )),
        }
    } else {
        SourceLocation {
            file: state.current_file.as_deref().cloned(),
            location: crate::Location::point(Position::from_line_col(
                error.location.line,
                error.location.column,
            )),
        }
    }
}

#[instrument(skip_all)]
fn parse_input(
    input: Box<str>,
    options: Options<'_>,
    file_path: Option<PathBuf>,
    leveloffset_ranges: Vec<model::LeveloffsetRange>,
    source_ranges: Vec<model::SourceRange>,
    warnings_handle: Rc<RefCell<Vec<Warning>>>,
) -> Result<ParseResult, Error> {
    tracing::trace!(?input, "post preprocessor");
    // Pin the preprocessed source text and a fresh `bumpalo::Bump` arena
    // together. The grammar borrows `&owner.source` and allocates every owned
    // string into `&owner.arena` via `ParserState::intern_str`. The returned
    // `Document<'_>` borrows from both — `ParseResult` keeps them alive
    // for as long as the consumer holds the wrapper. On drop, the arena,
    // source, and warnings free together in one shot.
    let owner = parsed::OwnedInput::new(input);
    let options_owned = options.into_static();
    // `warnings_handle` is shared with the preprocessor stage (which has
    // already appended any preprocessor-side warnings) and with
    // `ParserState` (which will append grammar-side warnings). The state
    // drops its clone when the builder closure returns, so the outer
    // handle is normally unique by the time `ParseResult::try_new`
    // unwraps it.
    let warnings_for_state = Rc::clone(&warnings_handle);

    ParseResult::try_new(owner, warnings_handle, move |owner| {
        let mut state = grammar::ParserState::new(&owner.source, &owner.arena);
        state.document_attributes = Rc::new(options_owned.document_attributes.clone());
        state.options = Rc::new(options_owned);
        state.current_file = file_path.map(std::sync::Arc::new);
        state.leveloffset_ranges = leveloffset_ranges;
        state.source_ranges = source_ranges;
        state.warnings = warnings_for_state;
        let result = match grammar::document_parser::document(&owner.source, &mut state) {
            Ok(Ok(mut doc)) => {
                // Rewrite every node's location from preprocessed coordinates to the
                // original source (file + line + byte offset). No-op when the
                // preprocessor recorded no ranges (no includes/edits).
                grammar::remap_document_to_source(
                    &mut doc,
                    &state.source_ranges,
                    &owner.source,
                    &state.line_map,
                );
                Ok(doc)
            }
            Ok(Err(e)) => Err(e),
            Err(error) => {
                tracing::error!(?error, "error parsing document content");
                let source_location = peg_error_to_source_location(&error, &state);
                Err(Error::Parse(Box::new(source_location), error.to_string()))
            }
        };
        state.emit_warnings();
        result
    })
}

/// Parse inline `AsciiDoc` content from a string.
///
/// This function parses the provided string as inline `AsciiDoc` elements, returning a
/// vector of inline nodes instead of a complete document structure. This is useful for
/// parsing fragments of `AsciiDoc` content that contain inline markup like emphasis,
/// strong text, links, macros, and other inline elements.
///
/// NOTE: This function exists pretty much just for the sake of the TCK tests, which rely
/// on an "inline" type output.
///
/// # Example
///
/// ```
/// use acdc_parser::{Options, SafeMode, parse_inline};
///
/// let options = Options::builder()
///     .with_safe_mode(SafeMode::Unsafe)
///     .build();
/// let content = "This is *strong* text with a https://example.com[link].";
/// let inline_nodes = parse_inline(content, &options).unwrap();
/// ```
///
/// # Errors
/// This function returns an error if the inline content cannot be parsed.
#[instrument]
pub fn parse_inline(input: &str, options: &Options<'_>) -> Result<ParseInlineResult, Error> {
    tracing::trace!(?input, "post preprocessor");
    let owner = parsed::OwnedInput::new(input.into());
    let options_owned = options.clone().into_static();
    let warnings_handle: Rc<RefCell<Vec<Warning>>> = Rc::new(RefCell::new(Vec::new()));
    let warnings_for_state = Rc::clone(&warnings_handle);

    ParseInlineResult::try_new(owner, warnings_handle, move |owner| {
        let mut state = grammar::ParserState::new(&owner.source, &owner.arena);
        state.document_attributes = Rc::new(options_owned.document_attributes.clone());
        state.options = Rc::new(options_owned);
        state.warnings = warnings_for_state;
        let result = match grammar::inline_parser::inlines(&owner.source, &mut state) {
            Ok(mut inlines) => {
                grammar::remap_inlines_to_source(
                    &mut inlines,
                    &state.source_ranges,
                    &owner.source,
                    &state.line_map,
                );
                Ok(inlines)
            }
            Err(error) => {
                tracing::error!(?error, "error parsing inline content");
                Err(Error::Parse(
                    Box::new(peg_error_to_source_location(&error, &state)),
                    error.to_string(),
                ))
            }
        };
        state.emit_warnings();
        result
    })
}

#[cfg(test)]
mod proptests;

#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::panic)]
#[allow(clippy::expect_used)]
mod tests {
    use std::{fs, path::PathBuf};

    use pretty_assertions::assert_eq;

    use super::*;

    fn read_file_contents_with_extension(path: &PathBuf, ext: &str) -> Result<String, Error> {
        let test_file_path = path.with_extension(ext);
        let file_contents = fs::read_to_string(&test_file_path).inspect_err(
            |e| tracing::warn!(?path, ?test_file_path, error = %e, "test file not found"),
        )?;
        Ok(file_contents)
    }

    #[test]
    fn indent_include_remaps_columns_to_origin() {
        // A `----` listing including a one-line file with `indent=6`. The remap must
        // report the included token at its ORIGIN columns (1..10) — stripping back the
        // six inserted spaces — not the preprocessed columns (7..16). For re-indented
        // content `absolute_*` stays in preprocessed coordinates (not serialized to the
        // ASG), so we only assert it stays a valid `start <= end` span.
        let opts = Options::builder().with_safe_mode(SafeMode::Unsafe).build();
        let result = parse_file("fixtures/preprocessor/include_indent_main.adoc", &opts)
            .expect("parse indented include");
        let doc = result.document();
        let inlines = doc
            .blocks
            .iter()
            .find_map(|b| {
                if let Block::DelimitedBlock(d) = b
                    && let DelimitedBlockType::DelimitedListing(inlines) = &d.inner
                {
                    Some(inlines)
                } else {
                    None
                }
            })
            .expect("listing block from the indented include");
        let loc = inlines.first().expect("listing content inline").location();

        assert_eq!(loc.start.line, 1, "origin line");
        assert_eq!(
            loc.start.column, 1,
            "origin column (the 6-space indent stripped off)"
        );
        assert_eq!(loc.end.line, 1);
        assert_eq!(
            loc.end.column, 10,
            "`TARGETLINE` is 10 columns wide in the origin"
        );
        assert_eq!(
            loc.start
                .file
                .as_deref()
                .and_then(|chain| chain.last())
                .map(String::as_str),
            Some("include_indent_target.rb"),
        );
        assert!(loc.absolute_start <= loc.absolute_end);
    }

    #[test]
    fn quote_attribution_and_citetitle_remap_to_included_file() {
        fn file_name(loc: &Location) -> Option<&str> {
            loc.start
                .file
                .as_deref()
                .and_then(|chain| chain.last())
                .map(String::as_str)
        }

        // A quote block lives on line 3 of an included file, spliced in at primary
        // line 5 (so its preprocessed line is 7). The attribution and citetitle inline
        // nodes must remap to the included file at its true line 3 — not stay at the
        // preprocessed line with `file: None` like the rest of the block.
        let opts = Options::builder().with_safe_mode(SafeMode::Unsafe).build();
        let result = parse_file("fixtures/preprocessor/include_quote_main.adoc", &opts)
            .expect("parse included quote block");
        let doc = result.document();
        let metadata = doc
            .blocks
            .iter()
            .find_map(|b| {
                if let Block::DelimitedBlock(d) = b
                    && d.metadata.attribution.is_some()
                {
                    Some(&d.metadata)
                } else {
                    None
                }
            })
            .expect("quote block with an attribution");

        let part = Some("include_quote_part.adoc");

        let attribution = metadata
            .attribution
            .as_ref()
            .and_then(|a| a.first())
            .expect("attribution inline")
            .location();
        assert_eq!(attribution.start.line, 3, "attribution origin line");
        assert_eq!(file_name(attribution), part, "attribution origin file");

        let citetitle = metadata
            .citetitle
            .as_ref()
            .and_then(|c| c.first())
            .expect("citetitle inline")
            .location();
        assert_eq!(citetitle.start.line, 3, "citetitle origin line");
        assert_eq!(file_name(citetitle), part, "citetitle origin file");
    }

    #[test]
    fn inline_preprocessor_warning_reports_included_file_line() {
        // A `{counter:foo}` (an inline-preprocessor warning) sits on line 3 of an
        // included file, spliced in at primary line 5 (preprocessed line 7). The
        // warning must name the included file at its true line 3 — not `file: None`
        // and the post-splice line, matching the error path and the AST nodes.
        let opts = Options::builder().with_safe_mode(SafeMode::Unsafe).build();
        let result = parse_file("fixtures/preprocessor/include_counter_main.adoc", &opts)
            .expect("parse included counter");
        let warning = result
            .warnings()
            .iter()
            .find(|w| w.kind.to_string().contains("Counters"))
            .expect("counter warning");
        let loc = warning
            .source_location()
            .expect("counter warning carries a location");
        assert_eq!(
            loc.file
                .as_deref()
                .and_then(|p| p.file_name())
                .and_then(std::ffi::OsStr::to_str),
            Some("include_counter_part.adoc"),
            "warning should name the included file",
        );
        assert_eq!(
            loc.location.start.line, 3,
            "warning origin line in the included file"
        );
    }

    #[test]
    fn toc_entries_and_references_remap_to_included_file() {
        // A section on line 1 of an included file (spliced in at primary line 6) must
        // surface in `toc_entries` and `references` at the included file's true line 1
        // — not the post-splice line with `file: None`. `references` is the LSP
        // go-to-definition target.
        fn file_name(loc: &Location) -> Option<&str> {
            loc.start
                .file
                .as_deref()
                .and_then(|chain| chain.last())
                .map(String::as_str)
        }

        let opts = Options::builder().with_safe_mode(SafeMode::Unsafe).build();
        let result = parse_file("fixtures/preprocessor/include_refs_main.adoc", &opts)
            .expect("parse included section");
        let doc = result.document();
        let part = Some("include_refs_part.adoc");

        let entry = doc
            .toc_entries
            .iter()
            .find(|e| e.id == "_included_section")
            .expect("toc entry for the included section");
        assert_eq!(entry.location.start.line, 1, "toc entry origin line");
        assert_eq!(file_name(&entry.location), part, "toc entry origin file");

        let reference = doc
            .references
            .get("_included_section")
            .expect("reference for the included section");
        assert_eq!(reference.location.start.line, 1, "reference origin line");
        assert_eq!(
            file_name(&reference.location),
            part,
            "reference origin file"
        );
    }

    #[test]
    fn footnote_location_is_document_absolute_without_include() {
        // `Document.footnotes` locations were captured during inline parsing in
        // paragraph-local coordinates; they must report the footnote's real document
        // line (5 here), not the line a byte-offset-into-the-paragraph would fall on.
        let input = "= Title\n\nPara one.\n\nPara two with a note.footnote:[The note.]\n";
        let result = parse(input, &Options::default()).expect("parse footnote doc");
        let doc = result.document();
        let footnote = doc.footnotes.first().expect("one footnote");
        assert_eq!(
            footnote.location.start.line, 5,
            "footnote's real document line"
        );
        assert!(
            footnote.location.start.file.is_none(),
            "primary-input footnote carries no file",
        );
    }

    #[test]
    fn footnote_location_remaps_to_included_file() {
        // A footnote on line 3 of an included file (spliced at primary line 5) must
        // report that file at its true line 3.
        let opts = Options::builder().with_safe_mode(SafeMode::Unsafe).build();
        let result = parse_file("fixtures/preprocessor/include_footnote_main.adoc", &opts)
            .expect("parse included footnote");
        let doc = result.document();
        let footnote = doc.footnotes.first().expect("one footnote");
        assert_eq!(footnote.location.start.line, 3, "footnote origin line");
        assert_eq!(
            footnote
                .location
                .start
                .file
                .as_deref()
                .and_then(|chain| chain.last())
                .map(String::as_str),
            Some("include_footnote_part.adoc"),
        );
    }

    #[test]
    fn named_footnote_keeps_defining_occurrence() {
        // A named footnote referenced twice shares one `Document.footnotes` entry; the
        // defining (first) occurrence's content and location win — a later bare
        // reference does not overwrite them.
        let input =
            "= Title\n\nFirst ref.footnote:fn[The definition.]\n\nSecond ref.footnote:fn[]\n";
        let result = parse(input, &Options::default()).expect("parse named footnote doc");
        let doc = result.document();
        assert_eq!(doc.footnotes.len(), 1, "one distinct footnote");
        let footnote = doc.footnotes.first().expect("the distinct footnote");
        assert_eq!(footnote.location.start.line, 3, "defining occurrence line");
        assert!(
            !footnote.content.is_empty(),
            "keeps the defining occurrence's content, not the empty reference",
        );
    }

    #[test]
    fn document_root_follows_per_boundary_file_model() {
        // `include_chain`: main.adoc ends with `include::outer.adoc[]`, and outer.adoc
        // ends with `include::inner.adoc[]`. The document's last content thus comes from
        // inner.adoc, so per the ASG's per-`locationBoundary` `file` model the document's
        // END carries the include chain while its START (primary main.adoc) carries none.
        // The document root is NOT special-cased to the primary file.
        let opts = Options::builder().with_safe_mode(SafeMode::Unsafe).build();
        let result =
            parse_file("fixtures/include_chain/main.adoc", &opts).expect("parse include chain");
        let location = &result.document().location;

        assert!(
            location.start.file.is_none(),
            "document start is primary input (no file)",
        );
        let expected = vec!["outer.adoc".to_string(), "inner.adoc".to_string()];
        assert_eq!(
            location.end.file.as_deref(),
            Some(&expected),
            "document end carries the include chain it ends in",
        );
    }

    #[rstest::rstest]
    #[tracing_test::traced_test]
    fn test_with_fixtures(#[files("fixtures/tests/**/*.adoc")] path: PathBuf) -> Result<(), Error> {
        let stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("");

        #[cfg(not(feature = "pre-spec-subs"))]
        if stem.starts_with("subs_") {
            return Ok(());
        }

        // Fixtures whose name contains `with_setext` exercise Setext (two-line
        // underlined) headings, which are opt-in: their expected JSON captures the
        // `--enable-setext-compatibility` behaviour. Skip when the feature is off
        // (the AST would diverge); enable the option when it is on. The `with_`
        // prefix keeps the marker unambiguous (vs a future `without_setext`).
        let setext_fixture = stem.contains("with_setext");
        #[cfg(not(feature = "setext"))]
        if setext_fixture {
            return Ok(());
        }

        let builder = Options::builder().with_safe_mode(SafeMode::Unsafe);
        #[cfg(feature = "setext")]
        let builder = if setext_fixture {
            builder.with_setext()
        } else {
            builder
        };
        let options = builder.build();

        match parse_file(&path, &options) {
            Ok(result) => {
                let expected = read_file_contents_with_extension(&path, "json")?;
                let actual = serde_json::to_string_pretty(result.document())
                    .expect("could not serialize result");
                assert_eq!(expected, actual);
            }
            Err(e) => {
                let file_contents = read_file_contents_with_extension(&path, "error")?;
                // Error fixtures contain expected error message as plain text
                let expected = file_contents.trim();
                assert_eq!(expected, e.to_string());
            }
        }
        Ok(())
    }

    #[test]
    fn node_locations_are_source_relative_after_dropped_comment() {
        use crate::Block;
        // The adjacent comment on line 4 is dropped by the preprocessor; the section
        // and its body must still report their ORIGINAL source lines (7 and 9), not
        // the shifted preprocessed lines.
        let input =
            "= Doc\n\nfirst para\n// dropped comment\nsecond para\n\n== Section\n\nbody text\n";
        let result = parse(input, &Options::builder().build()).expect("parse");
        let section = result
            .document()
            .blocks
            .iter()
            .find_map(|b| {
                if let Block::Section(s) = b {
                    Some(s)
                } else {
                    None
                }
            })
            .expect("section present");
        assert_eq!(
            section.location.start.line, 7,
            "section title at source line 7"
        );
        let body = section
            .content
            .iter()
            .find_map(|b| {
                if let Block::Paragraph(p) = b {
                    Some(p)
                } else {
                    None
                }
            })
            .expect("section body paragraph");
        assert_eq!(
            body.location.start.line, 9,
            "body paragraph at source line 9"
        );
    }

    #[test]
    fn node_locations_carry_origin_file_across_include() {
        use crate::{Block, SafeMode};
        let path = PathBuf::from("fixtures/tests/leveloffset_include.adoc");
        let options = Options::builder().with_safe_mode(SafeMode::Unsafe).build();
        let result = parse_file(&path, &options).expect("parse");
        // The first section comes from the included file, at its own line 1.
        let section = result
            .document()
            .blocks
            .iter()
            .find_map(|b| {
                if let Block::Section(s) = b {
                    Some(s)
                } else {
                    None
                }
            })
            .expect("included section present");
        // The include chain is the single (as-written) target of the include.
        let chain = section.location.start.file.as_deref().map(Vec::as_slice);
        assert_eq!(
            chain,
            Some(["leveloffset_included.adoc".to_string()].as_slice())
        );
        assert_eq!(
            section.location.start.line, 1,
            "included section at its own line 1"
        );
    }

    #[test]
    fn node_locations_carry_full_include_chain() {
        use crate::{Block, SafeMode};
        // main.adoc includes outer.adoc which includes inner.adoc. Each paragraph's
        // `file` is the chain of include targets (as written) reaching it; primary
        // content has none.
        let path = PathBuf::from("fixtures/include_chain/main.adoc");
        let options = Options::builder().with_safe_mode(SafeMode::Unsafe).build();
        let result = parse_file(&path, &options).expect("parse");

        let paragraphs = result
            .document()
            .blocks
            .iter()
            .filter_map(|block| {
                if let Block::Paragraph(para) = block {
                    Some(para)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        let [main, outer, inner] = paragraphs.as_slice() else {
            panic!("expected main, outer, and inner paragraphs");
        };
        assert!(
            matches!(&main.content[..], [InlineNode::PlainText(text)] if text.content == "Main paragraph.")
        );
        assert!(
            matches!(&outer.content[..], [InlineNode::PlainText(text)] if text.content == "Outer paragraph.")
        );
        assert!(
            matches!(&inner.content[..], [InlineNode::PlainText(text)] if text.content == "Inner paragraph.")
        );
        // Primary content has no include chain.
        assert!(main.location.start.file.is_none());
        assert_eq!(
            outer
                .location
                .start
                .file
                .as_ref()
                .map(|chain| chain.as_slice()),
            Some(["outer.adoc".to_string()].as_slice())
        );
        assert_eq!(
            inner
                .location
                .start
                .file
                .as_ref()
                .map(|chain| chain.as_slice()),
            Some(["outer.adoc".to_string(), "inner.adoc".to_string()].as_slice())
        );
    }

    #[cfg(test)]
    mod empty_document_tests {
        use crate::{Options, parse};

        #[test]
        fn test_whitespace_only_documents() {
            let test_cases = vec![
                "\n", "\n\n", "\t", " \n\t\n ", "   ",
                /* The original proptest failing case -> */ "\n\n\t",
            ];

            for input in test_cases {
                let options = Options::default();
                let result = parse(input, &options);

                match result {
                    Ok(parsed) => {
                        let doc = parsed.document();
                        // Validate the invariant using absolute offsets
                        assert!(
                            doc.location.absolute_start <= doc.location.absolute_end,
                            "Failed for input {input:?}: absolute_start {} > absolute_end {}",
                            doc.location.absolute_start,
                            doc.location.absolute_end
                        );

                        // Validate with our helper
                        doc.location.validate(input).unwrap_or_else(|e| {
                            panic!("Location validation failed for {input:?}: {e}")
                        });
                    }
                    Err(e) => {
                        panic!("Failed to parse {input:?}: {e}");
                    }
                }
            }
        }

        #[test]
        fn test_document_with_content_after_whitespace() {
            let test_cases = vec!["\n\nHello", "\t\tWorld", "  \n  = Title"];

            for input in test_cases {
                let options = Options::default();
                let parsed =
                    parse(input, &options).unwrap_or_else(|_| panic!("Should parse {input:?}"));
                let doc = parsed.document();

                assert!(
                    doc.location.absolute_start <= doc.location.absolute_end,
                    "Failed for input {input:?}: absolute_start {} > absolute_end {}",
                    doc.location.absolute_start,
                    doc.location.absolute_end
                );

                // Validate with our helper
                doc.location
                    .validate(input)
                    .unwrap_or_else(|e| panic!("Location validation failed for {input:?}: {e}"));
            }
        }

        #[test]
        fn test_unicode_characters() {
            // Test that UTF-8 safety is maintained
            let test_cases = vec![
                "😀",         // 4-byte emoji
                "א",          // 2-byte Hebrew
                "Hello 世界", // Mixed content
                "\u{200b}",   // Zero-width space
            ];

            for input in test_cases {
                let options = Options::default();
                let result = parse(input, &options);

                match result {
                    Ok(parsed) => {
                        let doc = parsed.document();
                        // All offsets should be on UTF-8 boundaries
                        assert!(
                            input.is_char_boundary(doc.location.absolute_start),
                            "Absolute start {} not on UTF-8 boundary for {input:?}",
                            doc.location.absolute_start,
                        );
                        assert!(
                            input.is_char_boundary(doc.location.absolute_end),
                            "Absolute end {} not on UTF-8 boundary for {input:?}",
                            doc.location.absolute_end,
                        );

                        // Validate with our helper
                        doc.location.validate(input).unwrap_or_else(|e| {
                            panic!("Location validation failed for {input:?}: {e}");
                        });
                    }
                    Err(e) => {
                        // Some of these might fail to parse, which is OK for now
                        // We're just testing that if they parse, the locations are valid
                        println!("Failed to parse {input:?}: {e} (this might be expected)");
                    }
                }
            }
        }
    }

    /// Integration tests for attribute resolution behavior.
    ///
    /// These tests verify that acdc matches asciidoctor's attribute resolution semantics:
    /// - Attributes are resolved at definition time (not reference time)
    /// - If {bar} is undefined when :foo: {bar} is parsed, foo stores literal "{bar}"
    /// - If {bar} IS defined when :foo: {bar} is parsed, foo stores bar's resolved value
    mod warning_deduplication_tests {
        use crate::{Options, parse};

        #[test]
        #[tracing_test::traced_test]
        fn counter_reference_peg_backtracking_does_not_duplicate() {
            // Each distinct counter reference position is its own diagnostic,
            // but PEG backtracking at a single position must not fire the same
            // warning multiple times. Two positions => two warnings.
            let input = "= Title\n\n{counter:hits} then {counter:hits} again";
            let options = Options::default();
            let result = parse(input, &options).expect("should parse");
            let counter_warnings = result
                .warnings()
                .iter()
                .filter(|w| {
                    w.kind
                        .to_string()
                        .contains("not supported and will be removed")
                })
                .count();
            assert_eq!(
                counter_warnings,
                2,
                "expected 2 counter warnings (one per position), got {counter_warnings}: {:?}",
                result.warnings(),
            );
            // Each warning must carry a location.
            assert!(
                result
                    .warnings()
                    .iter()
                    .all(|w| w.source_location().is_some()),
                "counter warnings must carry locations",
            );
        }

        #[test]
        #[tracing_test::traced_test]
        fn distinct_warnings_all_emitted() {
            // Different warnings should each appear once.
            let input = "= Title\n\n{counter:a} and {counter2:b}";
            let options = Options::default();
            let _doc = parse(input, &options).expect("should parse");
            assert!(logs_contain(
                "Counters ({counter:a}) are not supported and will be removed from output"
            ));
            assert!(logs_contain(
                "Counters ({counter2:b}) are not supported and will be removed from output"
            ));
        }
    }

    mod parse_result_tests {
        use crate::{Options, WarningKind, parse, parse_file};

        /// Preprocessor warnings (missing include file, bad line number,
        /// URL restrictions, if/endif mismatch) must reach
        /// `ParseResult::warnings()` alongside grammar warnings. Test with
        /// a missing include: `parse_file` against a doc whose include
        /// target doesn't exist.
        #[test]
        fn missing_include_warning_surfaces_on_parse_result() {
            use std::io::Write;
            // Write a tmp doc that references a non-existent include.
            let tmp = std::env::temp_dir().join("acdc_test_missing_include.adoc");
            let mut f = std::fs::File::create(&tmp).expect("create tmp");
            writeln!(
                f,
                "= Doc Title\n\ninclude::definitely-missing-{}.adoc[]\n",
                std::process::id()
            )
            .expect("write tmp");
            drop(f);

            let options = Options::default();
            let result = parse_file(&tmp, &options).expect("should parse");
            let _ = std::fs::remove_file(&tmp);

            let has_missing_include = result
                .warnings()
                .iter()
                .any(|w| w.kind.to_string().contains("file is missing"));
            assert!(
                has_missing_include,
                "expected missing-include warning, got: {:?}",
                result.warnings(),
            );
        }

        /// When the document has a title but the first section jumps past
        /// level 1, the parser must surface a typed warning on the
        /// returned `ParseResult` — not only via tracing.
        #[test]
        fn section_level_out_of_sequence_surfaces_on_parse_result() {
            let input = "= Doc Title\n\n=== Starts at level 2\n\nContent\n";
            let options = Options::default();
            let result = parse(input, &options).expect("document should parse");

            assert_eq!(
                result.warnings().len(),
                1,
                "expected exactly one warning, got: {:?}",
                result.warnings(),
            );
            let warning = result.warnings().first().expect("asserted non-empty");
            assert!(
                matches!(
                    &warning.kind,
                    WarningKind::SectionLevelOutOfSequence { got: 2, .. },
                ),
                "unexpected warning kind: {:?}",
                warning.kind,
            );
            assert!(
                warning.source_location().is_some(),
                "warning should carry a source location",
            );
        }

        /// Valid documents must have an empty warnings slice (the
        /// common-case contract: silence means clean).
        #[test]
        fn valid_document_has_no_warnings() {
            let input = "= Doc Title\n\n== First\n\nContent\n";
            let options = Options::default();
            let result = parse(input, &options).expect("document should parse");
            assert!(
                result.warnings().is_empty(),
                "expected no warnings, got: {:?}",
                result.warnings(),
            );
        }

        /// `[subs="…"]` should always surface a warning. When the
        /// `pre-spec-subs` feature is on, the warning says the attribute is
        /// experimental. When off, the warning says the attribute is being
        /// silently dropped because this build follows the draft spec. Both
        /// signals make sure users notice the spec-related shift.
        #[test]
        fn subs_attribute_always_surfaces_a_warning() {
            let input = "[subs=\"-quotes\"]\nContent\n";
            let options = Options::default();
            let result = parse(input, &options).expect("document should parse");

            let messages: Vec<String> = result
                .warnings()
                .iter()
                .map(|w| w.kind.to_string())
                .collect();
            assert!(
                messages.iter().any(|m| m.contains("subs=")),
                "expected a `subs=` warning, got: {messages:?}",
            );
            #[cfg(feature = "pre-spec-subs")]
            assert!(
                messages.iter().any(|m| m.contains(
                    "https://gitlab.eclipse.org/eclipse/asciidoc-lang/asciidoc-lang/-/issues/16"
                )),
                "expected warning with feature on, got: {messages:?}",
            );
            #[cfg(not(feature = "pre-spec-subs"))]
            assert!(
                messages.iter().any(|m| m.contains("not honoured")),
                "expected feature-off warning, got: {messages:?}",
            );
        }
    }

    mod attribute_resolution_tests {
        use std::borrow::Cow;

        use crate::{AttributeValue, Options, parse};

        #[test]
        fn test_definition_time_resolution_bar_defined_first() {
            // When bar is defined BEFORE foo, {bar} in foo's value should be expanded
            let input = r":bar: resolved-bar
:foo: {bar}

{foo}
";
            let options = Options::default();
            let parsed = parse(input, &options).expect("should parse");
            let doc = parsed.document();

            // foo should have bar's value expanded at definition time
            assert_eq!(
                doc.attributes.get("foo"),
                Some(&AttributeValue::String(Cow::Borrowed("resolved-bar")))
            );
        }

        #[test]
        fn test_definition_time_resolution_bar_defined_after() {
            // When bar is defined AFTER foo, {bar} should stay literal in foo's value
            let input = r":foo: {bar}
:bar: resolved-bar

{foo}
";
            let options = Options::default();
            let parsed = parse(input, &options).expect("should parse");
            let doc = parsed.document();

            // foo should keep {bar} as literal since bar wasn't defined yet
            assert_eq!(
                doc.attributes.get("foo"),
                Some(&AttributeValue::String(Cow::Borrowed("{bar}")))
            );
        }

        #[test]
        fn test_chained_attribute_resolution() {
            // When attributes form a chain: a -> b -> c, each should resolve
            // based on what's defined at each definition point
            let input = r":c: final-value
:b: {c}
:a: {b}

{a}
";
            let options = Options::default();
            let parsed = parse(input, &options).expect("should parse");
            let doc = parsed.document();

            // c is defined first, so b gets "final-value", then a gets "final-value"
            assert_eq!(
                doc.attributes.get("c"),
                Some(&AttributeValue::String(Cow::Borrowed("final-value")))
            );
            assert_eq!(
                doc.attributes.get("b"),
                Some(&AttributeValue::String(Cow::Borrowed("final-value")))
            );
            assert_eq!(
                doc.attributes.get("a"),
                Some(&AttributeValue::String(Cow::Borrowed("final-value")))
            );
        }
    }
}
