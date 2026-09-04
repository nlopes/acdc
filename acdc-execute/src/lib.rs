//! Discover and run command blocks defined in `AsciiDoc` documents.
//!
//! A *command block* is a listing block carrying the `command` role and an
//! explicit id:
//!
//! ```asciidoc
//! [.command, id=build]
//! [source, bash]
//! ----
//! cargo xtask build
//! ----
//! ```
//!
//! Commands may declare prerequisites with a `deps` attribute
//! (`deps="build, gen"`). [`CommandGraph`] validates the declared commands
//! (duplicate ids, unknown dependencies, dependency cycles) and yields
//! execution queues in topological order.
//!
//! # Behavior
//!
//! - Scripts use the parsed verbatim content of the listing body. Listing
//!   content is already verbatim, so this is the script as written; hard line
//!   breaks become newlines and inline nodes that carry no text (anchors,
//!   formatting spans, callout markers) contribute nothing.
//! - The script is written to a temporary file which is passed as the single
//!   argument to the configured interpreter. The interpreter defaults to `sh`; a
//!   `[source, <lang>]` header on the command block selects an interpreter name
//!   or path instead (e.g. `bash`, `python3`). The value is unfiltered and is
//!   not a sandbox boundary.
//! - Commands run in the current working directory of the calling process
//!   and inherit its environment.
//! - A command that exits with a non-zero status is a command failure the
//!   caller decides how to handle. A script that cannot be written or an
//!   interpreter that cannot be spawned is an infrastructure failure,
//!   reported immediately.
//! - Parser safe mode limits which files the *document* may read via
//!   `include::`; it is not a sandbox for executed commands.

pub mod adoc;
pub mod command;

pub use adoc::Error as AdocError;
pub use command::{
    BuildError, CommandBlock, CommandGraph, CommandGraphBuilder, CommandId, CommandMetadata,
    CommandQueue, ExecError, InvalidCommandId, UnknownCommand,
};
