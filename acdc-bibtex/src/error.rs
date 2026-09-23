//! Failures raised while reading a database or building a citation.

use std::path::PathBuf;

/// Something went wrong resolving a document's bibliography.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The `.bib` file could not be read.
    #[error("could not read the bibtex file `{path}`: {source}")]
    Read {
        /// The file that could not be read.
        path: PathBuf,
        /// The underlying failure.
        source: std::io::Error,
    },

    /// No `.bib` file was named and none could be found.
    #[error(
        "no bibtex file: set `:bibtex-file:` to one, or put a single `.bib` file beside the document"
    )]
    NoDatabase,

    /// The database could not be parsed.
    #[error("the bibtex database is malformed: {0}")]
    Bibtex(String),

    /// A citation names an entry the database does not hold, and the document
    /// asked for that to be fatal.
    #[error("unknown reference: {0}")]
    UnknownReference(String),
}
