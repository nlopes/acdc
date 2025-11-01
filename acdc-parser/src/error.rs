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

    #[error("PEG parsing error: {1}, position {0}")]
    PegParse(Position, String),

    #[error("Parsing error: {0}")]
    #[serde(skip_deserializing)]
    ParseGrammar(#[from] peg::error::ParseError<peg::str::LineCol>),

    #[error("section level mismatch: {1} (expected '{2}'), position: {0}")]
    NestedSectionLevelMismatch(Detail, SectionLevel, SectionLevel),

    #[error("mismatched delimiters: {1}, position: {0}")]
    MismatchedDelimiters(Detail, String),

    #[error("Invalid admonition variant: {1}, position: {0}")]
    InvalidAdmonitionVariant(Detail, String),

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
    HttpRequest(String),

    #[error("Could not convert from int: {0}")]
    #[serde(skip_deserializing)]
    TryFromIntError(#[from] std::num::TryFromIntError),
}

impl Error {
    /// Helper for creating mismatched delimiter errors
    #[must_use]
    pub(crate) fn mismatched_delimiters(detail: Detail, block_type: &str) -> Self {
        Self::MismatchedDelimiters(detail, block_type.to_string())
    }

    /// Extract location information from this error if available.
    /// Returns the Location for errors that have position information.
    #[must_use]
    pub fn location(&self) -> Option<&Location> {
        match self {
            Self::NestedSectionLevelMismatch(detail, ..)
            | Self::MismatchedDelimiters(detail, ..)
            | Self::InvalidAdmonitionVariant(detail, ..) => Some(&detail.location),
            _ => None,
        }
    }

    /// Get advice for this error if available.
    /// Returns helpful information for resolving the error.
    #[must_use]
    pub fn advice(&self) -> Option<&'static str> {
        match self {
            Self::NestedSectionLevelMismatch(..) => Some(
                "Section levels must increment by at most 1. For example, level 2 (==) cannot be followed directly by level 4 (====)",
            ),
            Self::MismatchedDelimiters(..) => Some(
                "Delimited blocks must use the same delimiter to open and close (e.g., '====' to open, '====' to close)",
            ),
            Self::InvalidAdmonitionVariant(..) => {
                Some("Valid admonition types are: NOTE, TIP, IMPORTANT, WARNING, CAUTION")
            }
            Self::InvalidIfEvalDirectiveMismatchedTypes => Some(
                "ifeval expressions must compare values of the same type (both numbers or both strings)",
            ),
            _ => None,
        }
    }
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

    #[test]
    fn test_error_invalid_admonition_variant_display() {
        let error = Error::InvalidAdmonitionVariant(
            Detail {
                location: Location {
                    absolute_start: 10,
                    absolute_end: 25,
                    start: Position { line: 2, column: 1 },
                    end: Position {
                        line: 2,
                        column: 15,
                    },
                },
            },
            "INVALID".to_string(),
        );
        assert_eq!(
            format!("{error}"),
            "Invalid admonition variant: INVALID, position: start(line: 2, column: 1), end(line: 2, column: 15)"
        );
    }

    #[test]
    fn test_error_mismatched_delimiters_display() {
        let error = Error::MismatchedDelimiters(
            Detail {
                location: Location {
                    absolute_start: 0,
                    absolute_end: 50,
                    start: Position { line: 1, column: 1 },
                    end: Position { line: 5, column: 5 },
                },
            },
            "example".to_string(),
        );
        assert_eq!(
            format!("{error}"),
            "mismatched delimiters: example, position: start(line: 1, column: 1), end(line: 5, column: 5)"
        );
    }
}
