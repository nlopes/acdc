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
mod error;
mod inlines;
mod model;
mod preprocessor;

use preprocessor::Preprocessor;

pub use error::{Detail as ErrorDetail, Error};
pub use model::{
    Admonition, AdmonitionVariant, Anchor, AttributeName, AttributeValue, Audio, AudioSource,
    Author, Autolink, Block, BlockMetadata, Bold, Button, DelimitedBlock, DelimitedBlockType,
    DescriptionList, DescriptionListDescription, DescriptionListItem, DiscreteHeader, Document,
    DocumentAttribute, DocumentAttributes, Header, Highlight, Icon, Image, ImageSource,
    InlineMacro, InlineNode, Italic, Keyboard, LineBreak, Link, LinkTarget, ListItem, Location,
    Menu, Monospace, OptionalAttributeValue, OrderedList, PageBreak, Paragraph, Pass, Plain, Role,
    Section, Subscript, Substitution, Superscript, Table, TableColumn, TableOfContents, TableRow,
    ThematicBreak, UnorderedList, Url, Video, VideoSource,
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

#[instrument(skip(reader))]
pub fn parse_from_reader<R: std::io::Read>(reader: R) -> Result<Document, Error> {
    let input = Preprocessor.process_reader(reader)?;
    tracing::trace!(?input, "post preprocessor");
    match InnerPestParser::parse(Rule::document, &input) {
        Ok(pairs) => Document::parse(pairs),
        Err(e) => {
            tracing::error!("error parsing document content: {e}");
            Err(Error::Parse(e.to_string()))
        }
    }
}

#[instrument]
pub fn parse(input: &str) -> Result<Document, Error> {
    let input = Preprocessor.process(input)?;
    tracing::trace!(?input, "post preprocessor");
    match InnerPestParser::parse(Rule::document, &input) {
        Ok(pairs) => Document::parse(pairs),
        Err(e) => {
            tracing::error!("error parsing document content: {e}");
            Err(Error::Parse(e.to_string()))
        }
    }
}

#[instrument(skip(file_path))]
pub fn parse_file<P: AsRef<Path>>(file_path: P) -> Result<Document, Error> {
    let input = Preprocessor.process_file(file_path)?;
    tracing::trace!(?input, "post preprocessor");
    match InnerPestParser::parse(Rule::document, &input) {
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
            let test_file = std::fs::read_to_string(test_file_path).unwrap();
            match parse_file(path.clone()) {
                Ok(result) => {
                    let test: Document = serde_json::from_str(&test_file).unwrap();
                    assert_eq!(test, result);
                }
                Err(e) => {
                    let test: Error = serde_json::from_str(&test_file).unwrap();
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
