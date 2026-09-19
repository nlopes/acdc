//! Failures raised while generating a diagram.
//!
//! Every variant names something the document author can act on: install a
//! tool, fix an attribute, pick a supported format. `DiagramProcessor` turns
//! each one into either a warning plus a fallback listing block or an abort,
//! depending on the `diagram-on-error` attribute.

use std::path::PathBuf;

/// A diagram could not be generated.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// Reading the diagram source, writing the image, or managing the cache failed.
    #[error("{context}: {source}")]
    Io {
        /// What the process was doing when the failure happened.
        context: String,
        /// The underlying failure.
        source: std::io::Error,
    },

    /// No executable could be found for a diagram tool.
    #[error(
        "could not find the {} executable in PATH; add it to the PATH or point the `{attribute}` \
         document attribute at it",
        format_commands(commands)
    )]
    CommandNotFound {
        /// Executable names that were searched for, in order.
        commands: Vec<String>,
        /// Document attribute that can override the location.
        attribute: String,
    },

    /// A diagram tool ran but exited with a non-zero status.
    #[error("{command} failed: {output}")]
    CommandFailed {
        /// Basename of the tool that failed.
        command: String,
        /// Whatever the tool wrote to stderr, or stdout when stderr was empty.
        output: String,
    },

    /// A diagram tool exited successfully but produced nothing.
    #[error("{command} produced no output")]
    EmptyOutput {
        /// Basename of the tool that produced no output.
        command: String,
    },

    /// The requested output format is not one this diagram type can produce.
    #[error("{diagram} does not support the {format} output format (supported: {supported})")]
    UnsupportedFormat {
        /// Diagram type, for example `graphviz`.
        diagram: String,
        /// Format that was requested.
        format: String,
        /// Comma-separated list of formats this diagram type supports.
        supported: String,
    },

    /// The `format` attribute named something that is not an image format at all.
    #[error("unknown diagram output format `{0}`")]
    UnknownFormat(String),

    /// A generated image could not be decoded well enough to measure it.
    #[error("{0}")]
    Image(String),

    /// A required attribute was missing or held an unusable value.
    #[error("{0}")]
    Config(String),

    /// The diagram source file named by a block macro could not be read.
    #[error("could not read diagram source `{path}`: {source}")]
    SourceFile {
        /// Path that was resolved from the block macro target.
        path: PathBuf,
        /// The underlying failure.
        source: std::io::Error,
    },

    /// Generating this diagram runs arbitrary commands, which the current safe
    /// mode forbids.
    #[error(
        "{diagram} diagrams execute terminal commands and are only generated in unsafe mode; \
         re-run with `--safe-mode unsafe`"
    )]
    SafeMode {
        /// Diagram type that was refused.
        diagram: String,
    },
}

impl Error {
    /// Attach a description of the operation to an [`std::io::Error`].
    pub(crate) fn io(context: impl Into<String>, source: std::io::Error) -> Self {
        Self::Io {
            context: context.into(),
            source,
        }
    }

    /// Build a configuration error from a formatted message.
    pub(crate) fn config(message: impl Into<String>) -> Self {
        Self::Config(message.into())
    }
}

fn format_commands(commands: &[String]) -> String {
    commands
        .iter()
        .map(|c| format!("`{c}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Shorthand for fallible diagram operations.
pub type Result<T> = std::result::Result<T, Error>;
