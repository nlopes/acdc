//! Discover and run command blocks defined in `AsciiDoc` documents.

pub mod command;
pub mod discovery;

pub use command::{
    BuildError, CommandBlock, CommandGraph, CommandGraphBuilder, CommandId, CommandMetadata,
    CommandOutcome, CommandState, ExecError, ExecutionEvent, ExecutionOptions, ExecutionPlan,
    ExecutionReport, InvalidCommandId, ObservationError, OutcomeState, ProcessError,
    ProcessOptions, RunOutcome, RunReport, SkipReason, Termination, UnknownCommand,
};
pub use discovery::DiscoveryError;
