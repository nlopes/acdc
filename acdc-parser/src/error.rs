use std::fmt;

use acdc_core::{Location, Position};

use crate::model::SectionLevel;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Parsing error: {0}")]
    Parse(String),

    #[error("section level mismatch: {1} (expected '{2}'), position: {0}")]
    NestedSectionLevelMismatch(Detail, SectionLevel, SectionLevel),

    #[error("Invalid conditional directive")]
    InvalidConditionalDirective,

    #[error("Invalid include directive")]
    InvalidIncludeDirective,

    #[error("Invalid attribute directive")]
    InvalidAttributeDirective,

    #[error("Invalid indent: {0}")]
    InvalidIndent(String),

    #[error("Invalid level offset: {0}")]
    InvalidLevelOffset(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("URL error: {0}")]
    Url(#[from] url::ParseError),

    #[error("ParseInt error: {0}")]
    ParseInt(#[from] std::num::ParseIntError),

    #[error("Unexpected block: {0}")]
    UnexpectedBlock(String),
}

#[derive(Debug, PartialEq)]
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
