//! Errors reported while constructing, selecting, and executing commands.

use acdc_parser::SourceLocation;

use super::CommandId;

/// Error returned when a string is not a valid [`CommandId`].
#[derive(Debug, thiserror::Error)]
pub enum InvalidCommandId {
    /// The id was empty.
    #[error("command id must not be empty")]
    Empty,
    /// The id started with `-`, which would collide with command-line flag parsing.
    #[error("command id {id:?} must not start with '-'")]
    LeadingDash {
        /// The rejected id.
        id: String,
    },
    /// The id contained a character outside the allowed set.
    #[error("command id {id:?} contains illegal character {ch:?}")]
    IllegalChar {
        /// The rejected id.
        id: String,
        /// The first offending character.
        ch: char,
    },
}

/// Error returned when a [`CommandBlock`](super::CommandBlock) fails to execute.
#[derive(Debug, thiserror::Error)]
pub enum ExecError {
    /// The temporary script file could not be created or written.
    #[error("could not write temporary script file: {0}")]
    TempFile(#[source] std::io::Error),
    /// Starting or waiting for the interpreter failed.
    #[error("could not run interpreter: {0}")]
    Process(#[source] std::io::Error),
    /// The child exited unsuccessfully, including termination by a signal.
    #[error("exited with {0}")]
    Failed(std::process::ExitStatus),
}

/// Error returned when building a [`CommandGraph`](super::CommandGraph) from a
/// [`CommandGraphBuilder`](super::CommandGraphBuilder) fails.
#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    /// Two commands share the same id.
    #[error("duplicate command id: {id}")]
    DuplicateId {
        /// The repeated id.
        id: CommandId,
        /// The first declaration.
        first_location: Box<SourceLocation>,
        /// The duplicate declaration.
        duplicate_location: Box<SourceLocation>,
    },
    /// A declared dependency does not name any added command.
    #[error("command `{command}` declares unknown dependency: {dep}")]
    UnknownDep {
        /// The command declaring the dependency.
        command: CommandId,
        /// The dependency that could not be resolved.
        dep: CommandId,
        /// The dependent command's declaration.
        location: Box<SourceLocation>,
    },
    /// A dependency edge would introduce a cycle.
    ///
    /// The reported edge runs from `dep` (the prerequisite) to `command` (the dependent) and is
    /// one edge in the cycle. A self-dependency has `dep == command`.
    #[error("dependency cycle includes: {dep} -> {command}")]
    Cycle {
        /// The dependent command.
        command: CommandId,
        /// The prerequisite that closes the cycle.
        dep: CommandId,
        /// The dependent command's declaration.
        command_location: Box<SourceLocation>,
        /// The prerequisite's declaration.
        dep_location: Box<SourceLocation>,
    },
}

impl BuildError {
    /// The declaration that caused graph validation to fail.
    #[must_use]
    pub fn source_location(&self) -> &SourceLocation {
        match self {
            Self::DuplicateId {
                duplicate_location, ..
            } => duplicate_location,
            Self::UnknownDep { location, .. } => location,
            Self::Cycle {
                command_location, ..
            } => command_location,
        }
    }

    /// The other declaration involved in a duplicate id or dependency cycle.
    #[must_use]
    pub fn related_location(&self) -> Option<&SourceLocation> {
        match self {
            Self::DuplicateId { first_location, .. } => Some(first_location),
            Self::Cycle { dep_location, .. } => Some(dep_location),
            Self::UnknownDep { .. } => None,
        }
    }
}

/// Error returned when a command id is not found in a [`CommandGraph`](super::CommandGraph).
#[derive(Debug, thiserror::Error)]
#[error("unknown command: {0}")]
pub struct UnknownCommand(pub CommandId);
