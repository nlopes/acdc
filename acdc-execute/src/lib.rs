//! Discover and run command blocks defined in `AsciiDoc` documents.

pub mod adoc;
pub mod command;

pub use adoc::Error as AdocError;
pub use command::{
    BuildError, CommandBlock, CommandGraph, CommandGraphBuilder, CommandId, CommandMetadata,
    CommandQueue, ExecError, InvalidCommandId, UnknownCommand,
};
