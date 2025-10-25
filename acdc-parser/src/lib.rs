#![deny(clippy::pedantic)]
#![warn(clippy::all)]
#![allow(clippy::module_name_repetitions)]
#![allow(dead_code)]
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
use std::{path::Path, string::ToString};

use acdc_core::SafeMode;
use tracing::instrument;

mod blocks;
mod error;
pub(crate) mod grammar;
mod model;
mod preprocessor;

pub(crate) use grammar::{InlinePreprocessorParserState, ProcessedContent, inline_preprocessing};
use preprocessor::Preprocessor;

pub use error::{Detail as ErrorDetail, Error};
pub use model::{
    Admonition, AdmonitionVariant, Anchor, AttributeName, AttributeValue, Audio, Author, Autolink,
    Block, BlockMetadata, Bold, Button, CalloutList, CrossReference, CurvedApostrophe,
    CurvedQuotation, DelimitedBlock, DelimitedBlockType, DescriptionList, DescriptionListItem,
    DiscreteHeader, Document, DocumentAttribute, DocumentAttributes, ElementAttributes, Footnote,
    Form, Header, Highlight, Icon, Image, InlineMacro, InlineNode, Italic, Keyboard, LineBreak,
    Link, ListItem, ListItemCheckedStatus, Location, Menu, Monospace, OrderedList, PageBreak,
    Paragraph, Pass, PassthroughKind, Plain, Position, Raw, Role, Section, Source,
    StandaloneCurvedApostrophe, Subscript, Substitution, Superscript, Table, TableColumn,
    TableOfContents, TableRow, ThematicBreak, TocEntry, UnorderedList, Url, Verbatim, Video,
};

#[derive(Debug, Clone, Default)]
pub struct Options {
    pub safe_mode: SafeMode,
    pub timings: bool,
    pub document_attributes: DocumentAttributes,
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
    parse_input(&input, options)
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
    parse_input(&input, options)
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
    let input = Preprocessor.process_file(file_path, options)?;
    parse_input(&input, options)
}

#[instrument]
fn parse_input(input: &str, options: &Options) -> Result<Document, Error> {
    tracing::trace!(?input, "post preprocessor");
    let mut state = grammar::ParserState::new(input);
    state.document_attributes = options.document_attributes.clone();
    state.options = options.clone();
    match grammar::document_parser::document(input, &mut state) {
        Ok(doc) => doc,
        Err(error) => {
            tracing::error!(?error, "error parsing document content");
            Err(Error::Parse(error.to_string()))
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
            Err(Error::Parse(error.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
