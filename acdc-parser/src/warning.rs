//! Non-fatal parser diagnostics.
//!
//! Warnings are conditions the parser recovers from but that a caller (CLI,
//! LSP, editor) may want to surface to the user. They are carried on
//! [`ParseResult::warnings`](crate::ParseResult::warnings) and also emitted
//! through `tracing::warn!` as a belt-and-suspenders fallback for callers
//! that ignore the returned slice.

use std::{borrow::Cow, fmt};

use crate::SourceLocation;

/// A non-fatal condition detected during parsing.
///
/// Use [`Warning::source_location`] to map to a location for diagnostic
/// rendering, and [`Warning::advice`] for help text mirroring
/// [`Error::advice`](crate::Error::advice).
#[derive(Debug, PartialEq)]
#[non_exhaustive]
pub struct Warning {
    /// The specific non-fatal condition.
    pub kind: WarningKind,
    /// Where the condition was detected, when known. Absent for warnings
    /// raised outside any source context (e.g. preprocessor configuration).
    pub location: Option<SourceLocation>,
}

impl Warning {
    /// Construct a warning tied to a specific source location.
    #[must_use]
    pub(crate) fn new(kind: WarningKind, location: Option<SourceLocation>) -> Self {
        Self { kind, location }
    }

    /// Source location for this warning, when available.
    #[must_use]
    pub fn source_location(&self) -> Option<&SourceLocation> {
        self.location.as_ref()
    }

    /// Advice text mirroring [`Error::advice`](crate::Error::advice).
    /// Returns `None` when there is no canned guidance for this kind.
    #[must_use]
    pub fn advice(&self) -> Option<&'static str> {
        match &self.kind {
            WarningKind::SectionLevelOutOfSequence { .. } => Some(
                "The first section after the document title must be level 1 (==). Renumber the section headings so levels increment by one.",
            ),
            WarningKind::UnterminatedTable { .. } => Some(
                "The opening delimiter was found but no matching closing delimiter was seen before end of document. Add the closing delimiter on its own line, or remove the opening delimiter if not intended.",
            ),
            WarningKind::Other(_) => None,
        }
    }
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(loc) = &self.location else {
            return write!(f, "{}", self.kind);
        };
        if let Some(name) = loc
            .file
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
        {
            write!(f, "{name}: {}: {}", loc.positioning, self.kind)
        } else {
            write!(f, "{}: {}", loc.positioning, self.kind)
        }
    }
}

/// Categorised non-fatal conditions.
///
/// `Other` is an escape hatch for ad-hoc messages the parser has not yet
/// been taught to categorise. New variants should be added as callers need
/// to assert on them programmatically (e.g. LSP mapping `kind` to LSP
/// diagnostic codes, or tests matching on specific conditions without
/// resorting to string comparison).
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum WarningKind {
    /// The document has a title (level 0) but the first section after it
    /// is not level 1. Matches asciidoctor's "section title out of
    /// sequence" check.
    #[error("expected level 1 (==) as first section, got level {got} ({markers})")]
    SectionLevelOutOfSequence {
        /// The observed section level (e.g. 2 for `===`).
        got: u8,
        /// The `=` markers that produced the observed level.
        markers: String,
    },

    /// A table's opening delimiter was matched but no corresponding
    /// closing delimiter was found before end of input. Matches
    /// asciidoctor's "unterminated table block" warning.
    ///
    /// `delimiter` is the literal opening token as it appeared in the
    /// source (e.g. `"|==="`, `"!====="`).
    #[error("unterminated table block (opened by `{delimiter}`)")]
    UnterminatedTable {
        /// The opening delimiter that was left unmatched.
        delimiter: String,
    },

    /// Ad-hoc message not yet categorised into a typed variant.
    #[error("{0}")]
    Other(Cow<'static, str>),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Position, Positioning};

    #[test]
    fn display_without_location() {
        let w = Warning::new(WarningKind::Other("something happened".into()), None);
        assert_eq!(format!("{w}"), "something happened");
    }

    #[test]
    fn display_with_location_no_file() {
        let loc = SourceLocation {
            file: None,
            positioning: Positioning::Position(Position { line: 5, column: 1 }),
        };
        let w = Warning::new(
            WarningKind::SectionLevelOutOfSequence {
                got: 3,
                markers: "====".into(),
            },
            Some(loc),
        );
        assert_eq!(
            format!("{w}"),
            "line: 5, column: 1: expected level 1 (==) as first section, got level 3 (====)",
        );
    }

    #[test]
    fn display_with_location_and_file() {
        let loc = SourceLocation {
            file: Some(std::path::PathBuf::from("/docs/guide.adoc")),
            positioning: Positioning::Position(Position { line: 5, column: 1 }),
        };
        let w = Warning::new(
            WarningKind::SectionLevelOutOfSequence {
                got: 3,
                markers: "====".into(),
            },
            Some(loc),
        );
        assert_eq!(
            format!("{w}"),
            "guide.adoc: line: 5, column: 1: expected level 1 (==) as first section, got level 3 (====)",
        );
    }

    #[test]
    fn equality_holds_on_kind_and_location() {
        let a = Warning::new(WarningKind::Other("x".into()), None);
        let b = Warning::new(WarningKind::Other("x".into()), None);
        let c = Warning::new(WarningKind::Other("y".into()), None);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn unterminated_table_display_renders_original_token() {
        let w = Warning::new(
            WarningKind::UnterminatedTable {
                delimiter: "|===".into(),
            },
            None,
        );
        assert_eq!(
            format!("{w}"),
            "unterminated table block (opened by `|===`)",
        );
    }

    #[test]
    fn unterminated_table_display_preserves_longer_tokens() {
        let w = Warning::new(
            WarningKind::UnterminatedTable {
                delimiter: "!=====".into(),
            },
            None,
        );
        assert_eq!(
            format!("{w}"),
            "unterminated table block (opened by `!=====`)",
        );
    }
}
