#![deny(clippy::pedantic)]
#![warn(clippy::all)]
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
//!
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
use std::{
    path::{Path, PathBuf},
    string::ToString,
};

use tracing::instrument;

mod blocks;
mod constants;
mod error;
pub(crate) mod grammar;
mod model;
mod options;
mod preprocessor;
mod safe_mode;

pub(crate) use grammar::{InlinePreprocessorParserState, ProcessedContent, inline_preprocessing};
use preprocessor::Preprocessor;

pub use error::{Error, Positioning, SourceLocation};
pub use grammar::parse_text_for_quotes;
pub use model::{
    Admonition, AdmonitionVariant, Anchor, AttributeName, AttributeValue, Audio, Author, Autolink,
    Block, BlockMetadata, Bold, Button, CalloutList, CalloutListItem, CalloutRef, CalloutRefKind,
    ColumnFormat, ColumnStyle, ColumnWidth, Comment, CrossReference, CurvedApostrophe,
    CurvedQuotation, DelimitedBlock, DelimitedBlockType, DescriptionList, DescriptionListItem,
    DiscreteHeader, Document, DocumentAttribute, DocumentAttributes, ElementAttributes, Footnote,
    Form, HEADER, Header, Highlight, HorizontalAlignment, ICON_SIZES, Icon, Image, IndexTerm,
    IndexTermKind, InlineMacro, InlineNode, Italic, Keyboard, LineBreak, Link, ListItem,
    ListItemCheckedStatus, Location, MAX_SECTION_LEVELS, MAX_TOC_LEVELS, Mailto, Menu, Monospace,
    NORMAL, OrderedList, PageBreak, Paragraph, Pass, PassthroughKind, Plain, Position, Raw, Role,
    Section, Source, SourceUrl, StandaloneCurvedApostrophe, Stem, StemContent, StemNotation,
    Subscript, Substitution, SubstitutionOp, SubstitutionSpec, Subtitle, Superscript, Table,
    TableColumn, TableOfContents, TableRow, ThematicBreak, Title, TocEntry,
    UNNUMBERED_SECTION_STYLES, UnorderedList, Url, VERBATIM, Verbatim, VerticalAlignment, Video,
    inlines_to_string, substitute,
};
pub use options::{Options, OptionsBuilder, SafeMode};

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
    options: Options,
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
    pub fn with_options(mut self, options: Options) -> Self {
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
    pub fn parse(self) -> Result<Document, Error> {
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
    pub fn parse_inline(self) -> Result<Vec<InlineNode>, Error> {
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
    options: &Options,
) -> Result<Document, Error> {
    let result = Preprocessor.process_reader(reader, options)?;
    parse_input(&result.text, options, None, result.leveloffset_ranges)
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
pub fn parse(input: &str, options: &Options) -> Result<Document, Error> {
    let result = Preprocessor.process(input, options)?;
    parse_input(&result.text, options, None, result.leveloffset_ranges)
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
pub fn parse_file<P: AsRef<Path>>(file_path: P, options: &Options) -> Result<Document, Error> {
    let path = file_path.as_ref().to_path_buf();
    let result = Preprocessor.process_file(file_path, options)?;
    parse_input(&result.text, options, Some(path), result.leveloffset_ranges)
}

/// Helper to convert a PEG parse error to our `SourceLocation` type
fn peg_error_to_source_location(
    error: &peg::error::ParseError<peg::str::LineCol>,
    file: Option<PathBuf>,
) -> SourceLocation {
    SourceLocation {
        file,
        positioning: Positioning::Position(Position {
            line: error.location.line,
            column: error.location.column,
        }),
    }
}

#[instrument]
fn parse_input(
    input: &str,
    options: &Options,
    file_path: Option<PathBuf>,
    leveloffset_ranges: Vec<model::LeveloffsetRange>,
) -> Result<Document, Error> {
    tracing::trace!(?input, "post preprocessor");
    let mut state = grammar::ParserState::new(input);
    state.document_attributes = options.document_attributes.clone();
    state.options = options.clone();
    state.current_file.clone_from(&file_path);
    state.leveloffset_ranges = leveloffset_ranges;
    let result = match grammar::document_parser::document(input, &mut state) {
        Ok(doc) => doc,
        Err(error) => {
            tracing::error!(?error, "error parsing document content");
            let source_location = peg_error_to_source_location(&error, file_path);
            Err(Error::Parse(Box::new(source_location), error.to_string()))
        }
    };
    state.emit_warnings();
    result
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
pub fn parse_inline(input: &str, options: &Options) -> Result<Vec<InlineNode>, Error> {
    tracing::trace!(?input, "post preprocessor");
    let mut state = grammar::ParserState::new(input);
    state.document_attributes = options.document_attributes.clone();
    state.options = options.clone();
    let result = match grammar::document_parser::inlines(
        input,
        &mut state,
        0,
        &grammar::BlockParsingMetadata::default(),
    ) {
        Ok(inlines) => Ok(inlines),
        Err(error) => {
            tracing::error!(?error, "error parsing inline content");
            Err(Error::Parse(
                Box::new(peg_error_to_source_location(&error, None)),
                error.to_string(),
            ))
        }
    };
    state.emit_warnings();
    result
}

#[cfg(test)]
mod proptests;

#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::panic)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn read_file_contents_with_extension(
        path: &std::path::PathBuf,
        ext: &str,
    ) -> Result<String, Error> {
        let test_file_path = path.with_extension(ext);
        let file_contents = std::fs::read_to_string(&test_file_path).inspect_err(
            |e| tracing::warn!(?path, ?test_file_path, error = %e, "test file not found"),
        )?;
        Ok(file_contents)
    }

    #[rstest::rstest]
    #[tracing_test::traced_test]
    fn test_with_fixtures(
        #[files("fixtures/tests/**/*.adoc")] path: std::path::PathBuf,
    ) -> Result<(), Error> {
        let options = Options::builder().with_safe_mode(SafeMode::Unsafe).build();

        match parse_file(&path, &options) {
            Ok(result) => {
                let expected = read_file_contents_with_extension(&path, "json")?;
                let actual =
                    serde_json::to_string_pretty(&result).expect("could not serialize result");
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
                    Ok(doc) => {
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
                let doc =
                    parse(input, &options).unwrap_or_else(|_| panic!("Should parse {input:?}"));

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
                    Ok(doc) => {
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
                        println!("Failed to parse {input:?}: {e} (this might be expected)",);
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
        fn counter_reference_emits_single_warning() {
            // A document with the same counter referenced multiple times should
            // produce exactly one warning after parsing (not one per PEG attempt).
            let input = "= Title\n\n{counter:hits} then {counter:hits} again";
            let options = Options::default();
            let _doc = parse(input, &options).expect("should parse");
            assert!(logs_contain("Counters"));
            logs_assert(|lines: &[&str]| {
                let count = lines
                    .iter()
                    .filter(|l| l.contains("not supported and will be removed"))
                    .count();
                if count == 1 {
                    Ok(())
                } else {
                    Err(format!("expected exactly 1 counter warning, got {count}"))
                }
            });
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

    mod attribute_resolution_tests {
        use crate::{AttributeValue, Options, parse};

        #[test]
        fn test_definition_time_resolution_bar_defined_first() {
            // When bar is defined BEFORE foo, {bar} in foo's value should be expanded
            let input = r":bar: resolved-bar
:foo: {bar}

{foo}
";
            let options = Options::default();
            let doc = parse(input, &options).expect("should parse");

            // foo should have bar's value expanded at definition time
            assert_eq!(
                doc.attributes.get("foo"),
                Some(&AttributeValue::String("resolved-bar".to_string()))
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
            let doc = parse(input, &options).expect("should parse");

            // foo should keep {bar} as literal since bar wasn't defined yet
            assert_eq!(
                doc.attributes.get("foo"),
                Some(&AttributeValue::String("{bar}".to_string()))
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
            let doc = parse(input, &options).expect("should parse");

            // c is defined first, so b gets "final-value", then a gets "final-value"
            assert_eq!(
                doc.attributes.get("c"),
                Some(&AttributeValue::String("final-value".to_string()))
            );
            assert_eq!(
                doc.attributes.get("b"),
                Some(&AttributeValue::String("final-value".to_string()))
            );
            assert_eq!(
                doc.attributes.get("a"),
                Some(&AttributeValue::String("final-value".to_string()))
            );
        }
    }
}
