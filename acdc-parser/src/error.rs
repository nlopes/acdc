use std::{fmt, path::PathBuf};

use serde::Deserialize;

use crate::model::{Location, Position, SectionLevel};

#[non_exhaustive]
#[derive(thiserror::Error, Debug, Deserialize)]
pub enum Error {
    #[error("Invalid include path: {0}")]
    InvalidIncludePath(PathBuf),

    #[error("Invalid line range: {0}")]
    InvalidLineRange(String),

    #[error("Parsing error: {0}")]
    Parse(String),

    #[error("PEG parsing error at position {0}: {1}")]
    PegParse(Position, String),

    #[error("Parsing error: {0}")]
    #[serde(skip_deserializing)]
    ParseGrammar(#[from] peg::error::ParseError<peg::str::LineCol>),

    #[error("section level mismatch: {1} (expected '{2}'), position: {0}")]
    NestedSectionLevelMismatch(Detail, SectionLevel, SectionLevel),

    #[error("table row has {0} columns, expected {1}")]
    InvalidTableColumnLength(usize, usize),

    #[error("mismatched delimiters: {0}")]
    MismatchedDelimiters(String),

    #[error("Invalid admonition variant: {0}")]
    InvalidAdmonitionVariant(String),

    #[error("Invalid conditional directive")]
    InvalidConditionalDirective,

    #[error("Invalid include directive: {0}")]
    InvalidIncludeDirective(String),

    #[error("Invalid attribute directive")]
    InvalidAttributeDirective,

    #[error("Invalid indent: {0}")]
    InvalidIndent(String),

    #[error("Invalid level offset: {0}")]
    InvalidLevelOffset(String),

    #[error("I/O error: {0}")]
    #[serde(skip_deserializing)]
    Io(#[from] std::io::Error),

    #[error("URL error: {0}")]
    #[serde(skip_deserializing)]
    Url(#[from] url::ParseError),

    #[error("ParseInt error: {0}")]
    #[serde(skip_deserializing)]
    ParseInt(#[from] std::num::ParseIntError),

    #[error("Unexpected block: {0}")]
    UnexpectedBlock(String),

    #[error("Invalid ifeval directive")]
    InvalidIfEvalDirectiveMismatchedTypes,

    #[error("Unknown encoding: {0}")]
    UnknownEncoding(String),

    #[error("Unrecognized encoding in file: {0}")]
    UnrecognizedEncodingInFile(String),

    #[error("Unable to retrieve HTTP response: {0}")]
    #[serde(skip_deserializing)]
    HttpRequest(#[from] reqwest::Error),

    #[error("Could not convert from int: {0}")]
    #[serde(skip_deserializing)]
    TryFromIntError(#[from] std::num::TryFromIntError),
}

#[derive(Debug, PartialEq, Deserialize)]
pub struct Detail {
    pub location: Location,
}

impl fmt::Display for Detail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Location {
            start:
                Position {
                    line: start_line,
                    column: start_column,
                },
            end:
                Position {
                    line: end_line,
                    column: end_column,
                },
            ..
        } = self.location;

        write!(
            f,
            "start(line: {start_line}, column: {start_column}), end(line: {end_line}, column: {end_column})",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_detail_display() {
        let detail = Detail {
            location: Location {
                absolute_start: 2,
                absolute_end: 20,
                start: Position { line: 1, column: 2 },
                end: Position { line: 3, column: 4 },
            },
        };
        assert_eq!(
            format!("{detail}"),
            "start(line: 1, column: 2), end(line: 3, column: 4)"
        );
    }

    #[test]
    fn test_error_nested_section_level_mismatch_display() {
        let error = Error::NestedSectionLevelMismatch(
            Detail {
                location: Location {
                    absolute_start: 2,
                    absolute_end: 20,
                    start: Position { line: 1, column: 2 },
                    end: Position { line: 3, column: 4 },
                },
            },
            1,
            2,
        );
        assert_eq!(
            format!("{error}"),
            "section level mismatch: 1 (expected '2'), position: start(line: 1, column: 2), end(line: 3, column: 4)"
        );
    }
}
