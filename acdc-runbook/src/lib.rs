#![doc = include_str!("../README.md")]

mod model;
mod reader;
mod writer;

pub use model::{
    Capture, Decoded, Event, FailureStage, Outcome, Position, RunResult, SkipReason, Source, Step,
    Termination, decode,
};
pub use reader::{ReadError, Reader, TimedEvent};
pub use writer::{WriteError, Writer};

/// The asciicast extension event code used for runbook metadata.
pub const EVENT_CODE: &str = "acdc.runbook";

/// The supported metadata schema version, independent of the asciicast version.
pub const SCHEMA_VERSION: u64 = 1;

/// Maximum encoded header or event line, including its newline, in bytes.
/// Callers must split larger terminal output into separate events.
pub const MAX_LINE_BYTES: usize = 1024 * 1024;
