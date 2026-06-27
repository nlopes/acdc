use std::{fmt, path::PathBuf};

use crate::model::{Location, Position, SectionLevel};

#[non_exhaustive]
#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Invalid include path: {1}, position: {0}")]
    InvalidIncludePath(Box<SourceLocation>, PathBuf),

    #[error("Invalid line range: {1}, position: {0}")]
    InvalidLineRange(Box<SourceLocation>, String),

    #[error("Parsing error: {1}, position: {0}")]
    Parse(Box<SourceLocation>, String),

    #[error("PEG parsing error: {1}, position {0}")]
    PegParse(Box<SourceLocation>, String),

    #[error("Parsing error: {0}")]
    ParseGrammar(#[from] peg::error::ParseError<peg::str::LineCol>),

    #[error("section level mismatch: {1} (expected '{2}'), position: {0}")]
    NestedSectionLevelMismatch(Box<SourceLocation>, SectionLevel, SectionLevel),

    #[error("mismatched delimiters: {1}, position: {0}")]
    MismatchedDelimiters(Box<SourceLocation>, String),

    #[error("Invalid admonition variant: {1}, position: {0}")]
    InvalidAdmonitionVariant(Box<SourceLocation>, String),

    #[error("Invalid conditional directive, position: {0}")]
    InvalidConditionalDirective(Box<SourceLocation>),

    #[error("Invalid include directive: {1}, position: {0}")]
    InvalidIncludeDirective(Box<SourceLocation>, String),

    #[error("Invalid indent: {1}, position: {0}")]
    InvalidIndent(Box<SourceLocation>, String),

    #[error("Invalid level offset: {1}, position: {0}")]
    InvalidLevelOffset(Box<SourceLocation>, String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("URL error: {0}")]
    Url(#[from] url::ParseError),

    #[error("ParseInt error: {0}")]
    ParseInt(#[from] std::num::ParseIntError),

    #[error("Invalid ifeval directive, position: {0}")]
    InvalidIfEvalDirectiveMismatchedTypes(Box<SourceLocation>),

    #[error("Unknown encoding: {0}")]
    UnknownEncoding(String),

    #[error("Unrecognized encoding in file: {0}")]
    UnrecognizedEncodingInFile(String),

    #[cfg(feature = "network")]
    #[error("Unable to retrieve HTTP response: {0}")]
    HttpRequest(String),

    #[cfg(not(feature = "network"))]
    #[error(
        "Network support is disabled (compile with 'network' feature to enable remote includes)"
    )]
    NetworkDisabled,

    #[error("Could not convert from int: {0}")]
    TryFromIntError(#[from] std::num::TryFromIntError),

    #[error("Non-conforming manpage title: {1}, position: {0}")]
    NonConformingManpageTitle(Box<SourceLocation>, String),
}

impl Error {
    /// Helper for creating mismatched delimiter errors
    #[must_use]
    pub(crate) fn mismatched_delimiters(detail: SourceLocation, block_type: &str) -> Self {
        Self::MismatchedDelimiters(Box::new(detail), block_type.to_string())
    }

    /// Extract source location information from this error if available.
    /// Returns the `SourceLocation` (either Location or Position) for errors that have positional information.
    #[must_use]
    pub fn source_location(&self) -> Option<&SourceLocation> {
        match self {
            Self::NestedSectionLevelMismatch(detail, ..)
            | Self::MismatchedDelimiters(detail, ..)
            | Self::InvalidAdmonitionVariant(detail, ..)
            | Self::Parse(detail, ..)
            | Self::PegParse(detail, ..)
            | Self::InvalidIncludePath(detail, ..)
            | Self::InvalidLineRange(detail, ..)
            | Self::InvalidConditionalDirective(detail)
            | Self::InvalidIncludeDirective(detail, ..)
            | Self::InvalidIndent(detail, ..)
            | Self::InvalidLevelOffset(detail, ..)
            | Self::InvalidIfEvalDirectiveMismatchedTypes(detail)
            | Self::NonConformingManpageTitle(detail, ..) => Some(detail),
            Self::ParseGrammar(_)
            | Self::Io(_)
            | Self::Url(_)
            | Self::ParseInt(_)
            | Self::UnknownEncoding(_)
            | Self::UnrecognizedEncodingInFile(_)
            | Self::TryFromIntError(_) => None,
            #[cfg(feature = "network")]
            Self::HttpRequest(_) => None,
            #[cfg(not(feature = "network"))]
            Self::NetworkDisabled => None,
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
            Self::InvalidIfEvalDirectiveMismatchedTypes(..) => Some(
                "ifeval expressions must compare values of the same type (both numbers or both strings)",
            ),
            Self::InvalidConditionalDirective(..) => Some(
                "Valid conditional directives are: ifdef, ifndef, ifeval, endif. Check the syntax of your conditional block.",
            ),
            Self::InvalidLineRange(..) => Some(
                "Line ranges must be in the format 'start..end' where start and end are positive integers",
            ),
            Self::InvalidIncludeDirective(..) => Some(
                "Valid include directive attributes are: leveloffset, lines, tag, tags, indent, encoding, opts",
            ),
            Self::InvalidIndent(..) => Some(
                "The indent attribute must be a non-negative integer specifying the number of spaces to indent included content",
            ),
            Self::InvalidLevelOffset(..) => Some(
                "The leveloffset attribute must be a signed integer (e.g., +1, -1, 0) to adjust section levels in included content",
            ),
            Self::InvalidIncludePath(..) => Some(
                "Include paths must have a valid parent directory. Check that the path is not empty or relative to a non-existent location",
            ),
            Self::Parse(..) => Some(
                "Check the AsciiDoc syntax at the indicated location. Common issues: incorrect block delimiters, malformed section headings, or invalid attribute syntax",
            ),
            Self::PegParse(..) => Some(
                "The parser encountered unexpected syntax. Verify that block delimiters match, section levels increment correctly, and all syntax follows AsciiDoc specification",
            ),
            Self::Url(..) => Some(
                "Verify the URL syntax is correct (e.g., https://example.com/file.adoc). Check for typos in the protocol, domain, or path",
            ),
            #[cfg(feature = "network")]
            Self::HttpRequest(..) => Some(
                "Check that the URL is accessible, the server is reachable, and you have network connectivity. For includes, consider using safe mode restrictions",
            ),
            #[cfg(not(feature = "network"))]
            Self::NetworkDisabled => Some(
                "Remote includes require the 'network' feature. Rebuild with `cargo build --features network` or use local file includes instead",
            ),
            Self::UnknownEncoding(..) | Self::UnrecognizedEncodingInFile(..) => Some(
                "We only support UTF-8 or UTF-16 encoded files. Ensure the specified encoding is correct and the file is saved with that encoding",
            ),
            Self::NonConformingManpageTitle(..) => Some(
                "Manpage document titles must be in the format 'name(volume)', e.g., 'git-commit(1)'. Remove --strict flag to use fallback values.",
            ),
            Self::ParseGrammar(_) | Self::Io(_) | Self::ParseInt(_) | Self::TryFromIntError(_) => {
                None
            }
        }
    }
}

/// Source location information combining a file path with a [`Location`] span.
///
/// A point diagnostic (one that only knows a line/column) is a zero-width span with
/// `location.start == location.end` (see [`Location::point`]).
#[derive(Debug, PartialEq)]
#[non_exhaustive]
pub struct SourceLocation {
    pub file: Option<PathBuf>,
    pub location: Location,
}

impl SourceLocation {
    /// A diagnostic at a single `position` (line/column) in `file`.
    #[must_use]
    pub fn at_position(file: Option<PathBuf>, position: Position) -> Self {
        Self {
            file,
            location: Location::point(position),
        }
    }

    /// A diagnostic spanning `location` in `file`.
    #[must_use]
    pub fn at_location(file: Option<PathBuf>, location: Location) -> Self {
        Self { file, location }
    }
}

impl fmt::Display for SourceLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Location { start, end, .. } = &self.location;
        // A zero-width span renders as a single point; a real span shows both ends.
        if start == end {
            write!(f, "line: {}, column: {}", start.line, start.column)
        } else {
            write!(
                f,
                "start(line: {}, column: {}), end(line: {}, column: {})",
                start.line, start.column, end.line, end.column
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_detail_display() {
        let detail = SourceLocation {
            file: None,
            location: Location {
                absolute_start: 2,
                absolute_end: 20,
                start: Position::new(1, 2),
                end: Position::new(3, 4),
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
            Box::new(SourceLocation {
                file: None,
                location: Location {
                    absolute_start: 2,
                    absolute_end: 20,
                    start: Position::new(1, 2),
                    end: Position::new(3, 4),
                },
            }),
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
            Box::new(SourceLocation {
                file: None,
                location: Location {
                    absolute_start: 10,
                    absolute_end: 25,
                    start: Position::new(2, 1),
                    end: Position::new(2, 15),
                },
            }),
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
            Box::new(SourceLocation {
                file: None,
                location: Location {
                    absolute_start: 0,
                    absolute_end: 50,
                    start: Position::new(1, 1),
                    end: Position::new(5, 5),
                },
            }),
            "example".to_string(),
        );
        assert_eq!(
            format!("{error}"),
            "mismatched delimiters: example, position: start(line: 1, column: 1), end(line: 5, column: 5)"
        );
    }

    #[test]
    fn test_error_parse_display() {
        let error = Error::Parse(
            Box::new(SourceLocation {
                file: None,
                location: crate::Location::point(Position::new(1, 6)),
            }),
            "unexpected token".to_string(),
        );
        assert_eq!(
            format!("{error}"),
            "Parsing error: unexpected token, position: line: 1, column: 6"
        );
    }
}
