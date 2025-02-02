#![deny(clippy::pedantic)]
#![warn(clippy::all)]
#![allow(clippy::module_name_repetitions)]
//! `AsciiDoc` parser.
//!
//! This module provides a parser for the `AsciiDoc` markup language. The parser is
//! implemented using the `pest` parser generator.
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
//! let document = parse(content).unwrap();
//!
//! println!("{:?}", document);
use std::{path::Path, string::ToString};

use pest::Parser as _;
use pest_derive::Parser;
use tracing::instrument;

mod anchor;
mod blocks;
mod document;
mod error;
pub(crate) mod grammar;
mod inlines;
mod model;
mod preprocessor;

pub(crate) use grammar::{inline_preprocessing, ParserState, ProcessedContent, ProcessedKind};
use preprocessor::Preprocessor;

pub use error::{Detail as ErrorDetail, Error};
pub use model::{
    Admonition, AdmonitionVariant, Anchor, AttributeName, AttributeValue, Audio, AudioSource,
    Author, Autolink, Block, BlockMetadata, Bold, Button, DelimitedBlock, DelimitedBlockType,
    DescriptionList, DescriptionListDescription, DescriptionListItem, DiscreteHeader, Document,
    DocumentAttribute, DocumentAttributes, ElementAttributes, Header, Highlight, Icon, Image,
    ImageSource, InlineMacro, InlineNode, Italic, Keyboard, LineBreak, Link, ListItem, Location,
    Menu, Monospace, OrderedList, PageBreak, Paragraph, Pass, PassthroughKind, Plain, Position,
    Raw, Role, Section, Subscript, Substitution, Superscript, Table, TableColumn, TableOfContents,
    TableRow, ThematicBreak, UnorderedList, Url, Video, VideoSource,
};

#[derive(Parser, Debug)]
#[grammar = "../grammar/inlines.pest"]
#[grammar = "../grammar/block.pest"]
#[grammar = "../grammar/core.pest"]
#[grammar = "../grammar/list.pest"]
#[grammar = "../grammar/delimited.pest"]
#[grammar = "../grammar/document.pest"]
#[grammar = "../grammar/asciidoc.pest"]
pub(crate) struct InnerPestParser;

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
/// let file = File::open("fixtures/samples/README.adoc").unwrap();
/// let document = parse_from_reader(file).unwrap();
/// ```
///
/// # Errors
/// This function returns an error if the content cannot be parsed.
#[instrument(skip(reader))]
pub fn parse_from_reader<R: std::io::Read>(reader: R) -> Result<Document, Error> {
    let input = Preprocessor.process_reader(reader)?;
    parse_input(input)
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
/// let content = r#"= Document Title\n\nThis is a paragraph.\n\n== Section Title\n\nThis is a subsection."#;
/// let document = parse(content).unwrap();
/// ```
///
/// # Errors
/// This function returns an error if the content cannot be parsed.
#[instrument]
pub fn parse(input: &str) -> Result<Document, Error> {
    let input = Preprocessor.process(input)?;
    parse_input(input)
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
/// let file_path = Path::new("fixtures/samples/README.adoc");
/// let document = parse_file(file_path).unwrap();
/// ```
///
/// # Errors
/// This function returns an error if the content cannot be parsed.
#[instrument(skip(file_path))]
pub fn parse_file<P: AsRef<Path>>(file_path: P) -> Result<Document, Error> {
    let input = Preprocessor.process_file(file_path)?;
    parse_input(input)
}

#[instrument]
fn parse_input(input: String) -> Result<Document, Error> {
    tracing::trace!(?input, "post preprocessor");
    let pairs = InnerPestParser::parse(Rule::document, &input);
    match pairs {
        Ok(pairs) => Document::parse(pairs),
        Err(e) => {
            tracing::error!("error parsing document content: {e}");
            Err(Error::Parse(e.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[rstest::rstest]
    #[trace]
    fn for_each_file(#[files("fixtures/tests/**/*.adoc")] path: std::path::PathBuf) {
        let test_file_path = path.with_extension("json");

        // We do this check because we have files that won't have a test file, namely ones
        // that are supposed to error out!
        if test_file_path.exists() {
            let test_file_contents = std::fs::read_to_string(test_file_path).unwrap();
            match parse_file(path.clone()) {
                Ok(result) => {
                    let result_str = serde_json::to_string(&result).unwrap();
                    let test: Document = serde_json::from_str(&test_file_contents).unwrap();
                    let test_str = serde_json::to_string(&test).unwrap();
                    assert_eq!(test_str, result_str);
                }
                Err(e) => {
                    let test: Error = serde_json::from_str(&test_file_contents).unwrap();
                    assert_eq!(test.to_string(), e.to_string());
                }
            }
        } else {
            tracing::warn!("no test file found for {:?}", path);
        }
    }

    //     #[test]
    //     #[tracing_test::traced_test]
    //     fn test_something() {
    //         let result = parse("
    // = Test Document
    // :docname: test-doc
    // :version: 2.0
    // :nested1: {version}
    // :nested2: {nested1}
    // :url: https://example.org

    // // Basic paragraph with attribute
    // This is {docname} version {version}.

    // // Paragraph with nested attributes
    // The document is at version {nested2} right now.

    // // Basic passthrough tests
    // Here is some +*escaped bold*+ and ++**more escaped**++.

    // // Pass macro variations
    // Look at pass:q[*quoted*] vs pass:a[{docname}] and pass:q,a[_{docname}_].

    // // Complex pass macro with HTML and attributes
    // The text pass:q,a[<u>My doc *{docname}* v{version}</u>] is underlined.

    // // Test attributes in links
    // Check {url}[the main site] for more.
    // The {url}[site] has more info.

    // // Complex nesting test
    // See pass:a[version *{nested2}*] details at +https://link.to/{docname}+ or pass:q,a[this **{url}[{docname}]**].

    // // Multiple attributes in one line
    // Project {docname} v{version} by pass:a[{author}].",
    //             )
    //             .unwrap();
    //         dbg!(&result);
    //         panic!();
    //     }

    //     #[test]
    //     #[tracing_test::traced_test]
    //     fn test_something() {
    //         let result = parse(
    //             ":norberto: meh
    // :asdf: something + \\
    // or other {norberto}
    // :app-name: pass:q[MyApp^2^]

    // == Section **Title**

    // First: {asdf}

    // :asdf: another thing {asdf}

    // Second: {asdf}

    // {app-name}

    // Click image:pause.png[title=Pause **for** stuff] when you need a break.

    // .Something other meh
    // Ok here we go, a paragraph.

    // .Mint
    // [sidebar]
    // Mint has visions of global conquest.
    // If you don't plant it in a container, it will take over your garden.
    // ",
    //         )
    //         .unwrap();
    //         dbg!(&result);
    //         panic!();
    //     }

    // #[test]
    // #[tracing_test::traced_test]
    // fn test_mdbasics_adoc() {
    //     let result = PestParser
    //         .parse_file("fixtures/samples/mdbasics/mdbasics.adoc")
    //         .unwrap();
    //     dbg!(&result);
    //     panic!()
    // }
}
