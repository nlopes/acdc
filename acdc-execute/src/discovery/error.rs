//! Errors reported while discovering executable commands.

use acdc_parser::{SourceLocation, WarningKind};

use crate::command::{BuildError, InvalidCommandId};

/// A document could not provide a complete, valid command graph.
#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    /// The parser could not preserve source content, structure, or requested substitutions.
    #[error("cannot execute recovered source: {source}")]
    RecoveredSource {
        /// The parser's typed recovery diagnostic.
        #[source]
        source: WarningKind,
        /// The original source position, when available.
        location: Option<Box<SourceLocation>>,
    },
    /// A command or dependency has an invalid identifier.
    #[error("command `{command}`: {source}")]
    InvalidId {
        /// The command declaring the invalid identifier or dependency.
        command: String,
        /// The identifier validation error.
        source: InvalidCommandId,
        /// Where the command is declared.
        location: Box<SourceLocation>,
    },
    /// Command dependencies could not form a valid graph.
    #[error(transparent)]
    Build(#[from] BuildError),
    /// A command block has no identifier.
    #[error("command block is missing an id")]
    MissingId {
        /// Where the block is declared.
        location: Box<SourceLocation>,
    },
    /// A marked block is not a listing or source paragraph.
    #[error("command `{id}` is not a listing or source block")]
    NotAScript {
        /// The declared identifier.
        id: String,
        /// Where the block is declared.
        location: Box<SourceLocation>,
    },
    /// A script block has no retained source text.
    #[error("command `{id}` has no retained script source")]
    MissingSource {
        /// The declared identifier.
        id: String,
        /// Where the block is declared.
        location: Box<SourceLocation>,
    },
    /// An interpreter must be a nonempty executable name or path.
    #[error("command `{id}` requires a nonempty interpreter name or path without NUL bytes")]
    InvalidInterpreter {
        /// The declared identifier.
        id: String,
        /// Where the block is declared.
        location: Box<SourceLocation>,
    },
    /// Attribute substitution referenced an unset or unknown document attribute.
    #[error("command `{id}` references missing document attribute `{name}`")]
    MissingAttribute {
        /// The command containing the reference.
        id: String,
        /// The first unresolved attribute name in the script.
        name: String,
        /// Where the command is declared.
        location: Box<SourceLocation>,
    },
}

impl DiscoveryError {
    /// The original file and position associated with this error, when known.
    #[must_use]
    pub fn source_location(&self) -> Option<&SourceLocation> {
        match self {
            Self::RecoveredSource { location, .. } => location.as_deref(),
            Self::Build(error) => Some(error.source_location()),
            Self::InvalidId { location, .. }
            | Self::MissingId { location }
            | Self::NotAScript { location, .. }
            | Self::MissingSource { location, .. }
            | Self::InvalidInterpreter { location, .. }
            | Self::MissingAttribute { location, .. } => Some(location),
        }
    }

    /// A second source position involved in a duplicate identifier or cycle.
    #[must_use]
    pub fn related_location(&self) -> Option<&SourceLocation> {
        match self {
            Self::Build(error) => error.related_location(),
            Self::RecoveredSource { .. }
            | Self::InvalidId { .. }
            | Self::MissingId { .. }
            | Self::NotAScript { .. }
            | Self::MissingSource { .. }
            | Self::InvalidInterpreter { .. }
            | Self::MissingAttribute { .. } => None,
        }
    }
}
