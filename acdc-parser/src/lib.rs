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

pub(crate) use grammar::{InlinePreprocessorParserState, ProcessedContent, inline_preprocessing};
use preprocessor::Preprocessor;

pub use error::{Error, Positioning, SourceLocation};
pub use model::{
    Admonition, AdmonitionVariant, Anchor, AttributeName, AttributeValue, Audio, Author, Autolink,
    Block, BlockMetadata, Bold, Button, CalloutList, CrossReference, CurvedApostrophe,
    CurvedQuotation, DelimitedBlock, DelimitedBlockType, DescriptionList, DescriptionListItem,
    DiscreteHeader, Document, DocumentAttribute, DocumentAttributes, ElementAttributes, Footnote,
    Form, Header, Highlight, Icon, Image, InlineMacro, InlineNode, Italic, Keyboard, LineBreak,
    Link, ListItem, ListItemCheckedStatus, Location, Menu, Monospace, OrderedList, PageBreak,
    Paragraph, Pass, PassthroughKind, Plain, Position, Raw, Role, Section, Source,
    StandaloneCurvedApostrophe, Stem, StemContent, StemNotation, Subscript, Substitution,
    Superscript, Table, TableColumn, TableOfContents, TableRow, ThematicBreak, TocEntry,
    UnorderedList, Url, Verbatim, Video, inlines_to_string,
};
pub use options::{Options, OptionsBuilder};

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
/// use acdc_parser::{Parser, Options};
/// use acdc_core::SafeMode;
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
    /// use acdc_parser::{Parser, Options};
    /// use acdc_core::SafeMode;
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
/// use acdc_parser::parse_from_reader;
/// use std::fs::File;
///
/// let options = acdc_parser::Options {
///     safe_mode: acdc_core::SafeMode::Unsafe,
///     timings: false,
///     document_attributes: acdc_parser::DocumentAttributes::default(),
/// };
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
    let input = Preprocessor.process_reader(reader, options)?;
    parse_input(&input, options, None)
}

/// Parse `AsciiDoc` content from a string.
///
/// This function parses the provided string as `AsciiDoc`.
///
/// # Example
///
/// ```
/// use acdc_parser::parse;
///
/// let options = acdc_parser::Options {
///     safe_mode: acdc_core::SafeMode::Unsafe,
///     timings: false,
///     document_attributes: acdc_parser::DocumentAttributes::default(),
/// };
/// let content = "= Document Title\n\nThis is a paragraph.\n\n== Section Title\n\nThis is a subsection.";
/// let document = parse(content, &options).unwrap();
/// ```
///
/// # Errors
/// This function returns an error if the content cannot be parsed.
#[instrument]
pub fn parse(input: &str, options: &Options) -> Result<Document, Error> {
    let input = Preprocessor.process(input, options)?;
    parse_input(&input, options, None)
}

/// Parse `AsciiDoc` content from a file.
///
/// This function reads the content from the provided file and parses it as `AsciiDoc`.
///
/// # Example
///
/// ```
/// use acdc_parser::parse_file;
/// use std::path::Path;
///
/// let options = acdc_parser::Options {
///     safe_mode: acdc_core::SafeMode::Unsafe,
///     timings: false,
///     document_attributes: acdc_parser::DocumentAttributes::default(),
/// };
/// let file_path = Path::new("fixtures/samples/README.adoc");
/// let document = parse_file(file_path, &options).unwrap();
/// ```
///
/// # Errors
/// This function returns an error if the content cannot be parsed.
#[instrument(skip(file_path))]
pub fn parse_file<P: AsRef<Path>>(file_path: P, options: &Options) -> Result<Document, Error> {
    let path = file_path.as_ref().to_path_buf();
    let input = Preprocessor.process_file(file_path, options)?;
    parse_input(&input, options, Some(path))
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
            offset: error.location.offset,
        }),
    }
}

#[instrument]
fn parse_input(
    input: &str,
    options: &Options,
    file_path: Option<PathBuf>,
) -> Result<Document, Error> {
    tracing::trace!(?input, "post preprocessor");
    let mut state = grammar::ParserState::new(input);
    state.document_attributes = options.document_attributes.clone();
    state.options = options.clone();
    state.current_file.clone_from(&file_path);
    match grammar::document_parser::document(input, &mut state) {
        Ok(doc) => doc,
        Err(error) => {
            tracing::error!(?error, "error parsing document content");
            let source_location = peg_error_to_source_location(&error, file_path);
            Err(Error::Parse(Box::new(source_location), error.to_string()))
        }
    }
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
/// use acdc_parser::parse_inline;
///
/// let options = acdc_parser::Options {
///     safe_mode: acdc_core::SafeMode::Unsafe,
///     timings: false,
///     document_attributes: acdc_parser::DocumentAttributes::default(),
/// };
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
    match grammar::document_parser::inlines(
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
    }
}

#[cfg(test)]
mod proptests;

#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::panic)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use acdc_core::SafeMode;
    use pretty_assertions::assert_eq;

    #[rstest::rstest]
    #[tracing_test::traced_test]
    fn test_with_fixtures(
        #[files("fixtures/tests/**/*.adoc")] path: std::path::PathBuf,
    ) -> Result<(), Error> {
        let test_file_path = path.with_extension("json");
        let options = Options {
            safe_mode: SafeMode::Unsafe,
            timings: false,
            document_attributes: DocumentAttributes::default(),
        };

        // We do this check because we have files that won't have a test file, namely ones
        // that are supposed to error out!
        if test_file_path.exists() {
            let test_file_contents = std::fs::read_to_string(test_file_path)?;
            match parse_file(&path, &options) {
                Ok(result) => {
                    let result_str =
                        serde_json::to_string(&result).expect("could not serialize result");
                    let test: Document = serde_json::from_str(&test_file_contents)
                        .expect("could not deserialize test");
                    let test_str = serde_json::to_string(&test).expect("could not serialize test");
                    assert_eq!(test_str, result_str);
                }
                Err(e) => {
                    let test: Error = serde_json::from_str(&test_file_contents)
                        .expect("could not deserialize test");
                    assert_eq!(test.to_string(), e.to_string());
                }
            }
        } else {
            tracing::warn!(?path, "test file not found");
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
                        // Validate the invariant
                        assert!(
                            doc.location.start.offset <= doc.location.end.offset,
                            "Failed for input {input:?}: start {} > end {}",
                            doc.location.start.offset,
                            doc.location.end.offset
                        );

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
                    doc.location.start.offset <= doc.location.end.offset,
                    "Failed for input {input:?}: start {} > end {}",
                    doc.location.start.offset,
                    doc.location.end.offset
                );

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
                            input.is_char_boundary(doc.location.start.offset),
                            "Start offset {} not on UTF-8 boundary for {input:?}",
                            doc.location.start.offset,
                        );
                        assert!(
                            input.is_char_boundary(doc.location.end.offset),
                            "End offset {} not on UTF-8 boundary for {input:?}",
                            doc.location.end.offset,
                        );
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
}
