//! Discover and run command blocks defined in `AsciiDoc` documents.

pub mod command;
pub mod discovery;

pub use command::{
    BuildError, CommandBlock, CommandGraph, CommandGraphBuilder, CommandId, CommandMetadata,
    CommandOutcome, CommandState, ExecError, ExecutionOptions, ExecutionPlan, ExecutionReport,
    InvalidCommandId, ProcessOptions, SkipReason, UnknownCommand,
};
pub use discovery::DiscoveryError;
