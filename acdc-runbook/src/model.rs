use serde::{Deserialize, Serialize};

use crate::SCHEMA_VERSION;

/// A position in an authored source file, using one-based line and column numbers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Position {
    pub line: usize,
    pub column: usize,
}

/// Source provenance. Paths are display text and must never be opened by replay.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    /// Prefer a path relative to the runbook; absent for an unnamed source.
    pub path: Option<String>,
    pub start: Position,
    pub end: Position,
}

/// A selected command occurrence in plan order, including commands later skipped.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Step {
    /// A recording-local key, separate from the author's command ID and OSC IDs.
    pub occurrence: u32,
    pub id: String,
    pub description: Option<String>,
    pub section: Option<String>,
    pub interpreter: String,
    pub source: Source,
    /// Occurrence keys of the command's direct prerequisites.
    pub dependencies: Vec<u32>,
}

/// The child process result, distinct from child-reported OSC task states.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Termination {
    Exited { code: u32 },
    Signaled { signal: String },
    Unknown,
}

/// The operation that failed before a process result was available.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureStage {
    Preparation,
    Process,
}

/// Why a selected command did not start.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SkipReason {
    DependencyFailed { occurrence: u32 },
    StoppedAfterFailure { occurrence: u32 },
}

/// A command outcome. Capture failures are recorded separately.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Outcome {
    Succeeded,
    Failed {
        termination: Termination,
    },
    Error {
        stage: FailureStage,
        message: String,
    },
    Skipped {
        reason: SkipReason,
    },
}

/// The aggregate execution result, independent of capture health.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunResult {
    Succeeded,
    Failed,
    Aborted,
}

/// Whether terminal capture reached a known end without losing output.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Capture {
    Complete,
    Incomplete { reason: String },
}

/// Version 1 lifecycle metadata carried by an `acdc.runbook` event.
/// Scripts, environment overrides, input, and OSC reports are not metadata fields.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    RunStarted {
        steps: Vec<Step>,
    },
    CommandStarted {
        occurrence: u32,
    },
    /// Skipped commands have a finish event and no start event.
    CommandFinished {
        occurrence: u32,
        outcome: Outcome,
    },
    RunFinished {
        result: RunResult,
        capture: Capture,
    },
}

/// The result of decoding an extension payload. Unsupported metadata is inert.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decoded {
    Event(Event),
    UnsupportedVersion(u64),
    UnsupportedEvent(String),
}

#[derive(Serialize, Deserialize)]
pub(crate) struct Envelope<T> {
    pub version: u64,
    #[serde(flatten)]
    pub event: T,
}

/// Decode the string data of an `acdc.runbook` event.
///
/// Unknown fields are ignored. Unknown schema versions and event names return
/// explicit unsupported results so a reader can fall back to terminal replay.
/// This checks the wire shape; the consumer validates lifecycle ordering and
/// occurrence references before presenting a run as complete.
///
/// # Errors
/// Returns a JSON error for malformed metadata or missing version 1 fields.
pub fn decode(data: &str) -> Result<Decoded, serde_json::Error> {
    #[derive(Deserialize)]
    struct Probe {
        version: u64,
    }
    #[derive(Deserialize)]
    struct Kind {
        event: String,
    }
    let probe: Probe = serde_json::from_str(data)?;
    if probe.version != SCHEMA_VERSION {
        return Ok(Decoded::UnsupportedVersion(probe.version));
    }
    let kind: Kind = serde_json::from_str(data)?;
    match kind.event.as_str() {
        "run_started" | "command_started" | "command_finished" | "run_finished" => {
            let envelope: Envelope<Event> = serde_json::from_str(data)?;
            Ok(Decoded::Event(envelope.event))
        }
        _ => Ok(Decoded::UnsupportedEvent(kind.event)),
    }
}
