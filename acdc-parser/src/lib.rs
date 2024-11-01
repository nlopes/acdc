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
//! use asciidoc::Parser;
//!
//! let parser = Parser;
//! let content = r#"= Document Title
//!
//! This is a paragraph.
//!
//! == Section Title
//!
//! This is a subsection."#;
//!
//! let document = parser.parse(content).unwrap();
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
    Anchor, AttributeEntry, AttributeName, AttributeValue, AudioSource, Author, Autolink, Block,
    BlockMetadata, BoldText, Button, DelimitedBlock, DelimitedBlockType, DescriptionList,
    DescriptionListDescription, DescriptionListItem, DiscreteHeader, Document, DocumentAttribute,
    Header, HighlightText, Icon, Image, ImageSource, InlineMacro, InlineNode, ItalicText, Keyboard,
    Link, ListItem, Location, Menu, MonospaceText, OrderedList, PageBreak, Paragraph, Pass,
    PlainText, Position, Section, SubscriptText, SuperscriptText, ThematicBreak, Title,
    UnorderedList, Url, VideoSource,
};

#[derive(Parser, Debug)]
#[grammar = "../grammar/block.pest"]
#[grammar = "../grammar/core.pest"]
#[grammar = "../grammar/list.pest"]
#[grammar = "../grammar/delimited.pest"]
#[grammar = "../grammar/document.pest"]
#[grammar = "../grammar/asciidoc.pest"]
pub(crate) struct InnerPestParser;

#[instrument]
pub fn parse(input: &str) -> Result<Document, Error> {
    let input = Preprocessor.process(input)?;
    tracing::trace!(?input, "post preprocessor");
    match InnerPestParser::parse(Rule::document, &input) {
        Ok(pairs) => Document::parse(pairs),
        Err(e) => {
            tracing::error!("error preprocessing document: {e}");
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
            tracing::error!("error preprocessing document: {e}");
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
        let test_file_path = path.with_extension("test");

        // We do this check because we have files that won't have a test file, namely ones
        // that are supposed to error out!
        if test_file_path.exists() {
            let result = parse_file(path).unwrap();
            let test: Document =
                serde_json::from_str(&std::fs::read_to_string(test_file_path).unwrap()).unwrap();
            assert_eq!(test, result);
        } else {
            tracing::warn!("no test file found for {:?}", path);
        }
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_section_with_invalid_subsection() {
        let result = parse_file("fixtures/tests/section_with_invalid_subsection.adoc").unwrap_err();
        if let Error::NestedSectionLevelMismatch(ref detail, 2, 3) = result {
            assert_eq!(
                &ErrorDetail {
                    location: Location {
                        start: Position { line: 3, column: 1 },
                        end: Position { line: 5, column: 1 }
                    }
                },
                detail
            );
        } else {
            panic!("unexpected error: {result:?}");
        }
    }

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
