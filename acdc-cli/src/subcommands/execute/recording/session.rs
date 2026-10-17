use std::{
    cell::RefCell,
    collections::BTreeMap,
    fs::OpenOptions,
    io::{self, Write},
    os::unix::fs::OpenOptionsExt,
    path::Path,
    sync::atomic::Ordering,
    time::Instant,
};

use acdc_execute::{
    CommandGraph, ExecutionEvent, ExecutionOptions, ExecutionPlan, OutcomeState, ProcessError,
    RunReport, SkipReason, Termination,
};
use acdc_runbook::{
    Capture, Event, FailureStage, Outcome, Position, RunResult, Source, Step, Writer,
};
use miette::IntoDiagnostic as _;

use super::terminal::{self, Terminal};
use crate::{
    error::{LocatedError, SourceCache},
    subcommands::execute::{CommandIssue, ExecutionFailed},
};

pub(super) struct Session<W> {
    pub writer: Writer<W>,
    start: Instant,
    pending_utf8: Vec<u8>,
    pub problem: Option<String>,
    pub size: (u16, u16),
}

impl<W: Write> Session<W> {
    pub fn new(writer: Writer<W>, size: (u16, u16)) -> Self {
        Self {
            writer,
            start: Instant::now(),
            pending_utf8: Vec::new(),
            problem: None,
            size,
        }
    }

    pub fn elapsed(&self) -> std::time::Duration {
        self.start.elapsed()
    }

    pub fn output(&mut self, bytes: &[u8], end: bool) -> io::Result<()> {
        self.pending_utf8.extend_from_slice(bytes);
        let mut remaining = self.pending_utf8.as_slice();
        let mut text = String::new();
        loop {
            match std::str::from_utf8(remaining) {
                Ok(valid) => {
                    text.push_str(valid);
                    remaining = &[];
                    break;
                }
                Err(error) => {
                    let (valid, tail) = remaining.split_at(error.valid_up_to());
                    text.push_str(std::str::from_utf8(valid).map_err(io::Error::other)?);
                    if let Some(length) = error.error_len() {
                        text.push('\u{fffd}');
                        self.problem
                            .get_or_insert_with(|| "non-UTF-8 terminal output was replaced".into());
                        remaining = tail
                            .get(length..)
                            .ok_or_else(|| io::Error::other("invalid UTF-8 boundary"))?;
                    } else if end {
                        text.push('\u{fffd}');
                        self.problem.get_or_insert_with(|| {
                            "terminal output ended inside a UTF-8 character".into()
                        });
                        remaining = &[];
                        break;
                    } else {
                        remaining = tail;
                        break;
                    }
                }
            }
        }
        let retained = remaining.to_vec();
        self.pending_utf8 = retained;
        if !text.is_empty() {
            self.writer
                .output(self.elapsed(), &text)
                .map_err(io::Error::other)?;
        }
        Ok(())
    }

    fn event(&mut self, event: &Event) -> io::Result<()> {
        self.writer
            .metadata(self.elapsed(), event)
            .map_err(io::Error::other)?;
        self.writer.flush().map_err(io::Error::other)
    }
}

pub(super) fn run(
    graph: &CommandGraph,
    plan: &ExecutionPlan<'_>,
    options: &ExecutionOptions,
    path: &Path,
    document: &Path,
) -> miette::Result<()> {
    let size = terminal::preflight().into_diagnostic()?;
    let steps = steps(graph, plan, document).into_diagnostic()?;
    let occurrences: BTreeMap<_, _> = steps
        .iter()
        .map(|step| (step.id.clone(), step.occurrence))
        .collect();
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|error| miette::miette!("cannot create recording {}: {error}", path.display()))?;
    let writer =
        Writer::new(file, size.0, size.1, Some("acdc executable runbook")).into_diagnostic()?;
    let mut session = Session::new(writer, size);
    session
        .event(&Event::RunStarted { steps })
        .into_diagnostic()?;
    let mut terminal = Terminal::enter().into_diagnostic()?;
    let cancellation = terminal.cancellation.clone();
    let session = RefCell::new(session);
    let result = plan.execute_with(
        options,
        |_, process| {
            let mut session = session.borrow_mut();
            terminal::run_process(process, &mut session, &mut terminal)
        },
        |event| {
            let (event, marker) = match event {
                ExecutionEvent::CommandStarted { command } => {
                    if cancellation.load(Ordering::SeqCst) != 0 {
                        return Err(io::Error::new(
                            io::ErrorKind::Interrupted,
                            "recording cancelled",
                        ));
                    }
                    (
                        Event::CommandStarted {
                            occurrence: occurrence(&occurrences, command.metadata.id.as_str())?,
                        },
                        Some(command.metadata.id.as_str()),
                    )
                }
                ExecutionEvent::CommandFinished { outcome } => (
                    Event::CommandFinished {
                        occurrence: occurrence(&occurrences, outcome.command.metadata.id.as_str())?,
                        outcome: recorded_outcome(&outcome.state, &occurrences)?,
                    },
                    None,
                ),
            };
            let mut session = session.borrow_mut();
            session.event(&event)?;
            if let Some(label) = marker {
                let at = session.elapsed();
                session.writer.marker(at, label).map_err(io::Error::other)?;
            }
            if let Some(problem) = &session.problem {
                return Err(io::Error::other(format!("capture incomplete: {problem}")));
            }
            Ok(())
        },
    );
    let restore = terminal.restore();
    finish(
        session.into_inner(),
        result,
        restore,
        cancellation.load(Ordering::SeqCst) != 0,
        path,
    )
}

fn finish(
    session: Session<std::fs::File>,
    result: std::result::Result<RunReport<'_>, acdc_execute::ObservationError<'_, io::Error>>,
    restore: io::Result<()>,
    cancelled: bool,
    path: &Path,
) -> miette::Result<()> {
    let mut session = session;
    if let Err(error) = &restore {
        session
            .problem
            .get_or_insert_with(|| format!("could not restore terminal: {error}"));
    }
    let aborted = result.is_err()
        || cancelled
        || result.as_ref().is_ok_and(|report| {
            report.outcomes().iter().any(|outcome| {
                matches!(
                    &outcome.state,
                    OutcomeState::Failed(ProcessError::Failed(Termination::Signaled(_)))
                )
            })
        });
    let run_result = if aborted {
        RunResult::Aborted
    } else if result.as_ref().is_ok_and(RunReport::is_success) {
        RunResult::Succeeded
    } else {
        RunResult::Failed
    };
    let capture = session
        .problem
        .as_ref()
        .map_or(Capture::Complete, |reason| Capture::Incomplete {
            reason: reason.clone(),
        });
    let at = session.elapsed();
    let finished = session.writer.finish(at, run_result, capture);
    if let Err(error) = result.as_ref() {
        let cause = session.problem.unwrap_or_else(|| error.source.to_string());
        return Err(miette::miette!(
            "recording stopped: {cause}. Partial recording: {}",
            path.display()
        ));
    }
    finished.map_err(|error| {
        miette::miette!(
            "recording write failed: {error}. Partial recording: {}",
            path.display()
        )
    })?;
    restore.into_diagnostic()?;
    if let Some(reason) = session.problem {
        return Err(miette::miette!(
            "recording is incomplete: {reason}. Partial recording: {}",
            path.display()
        ));
    }
    report_execution(result.map_err(|error| miette::miette!(error.source))?)
}

fn occurrence(map: &BTreeMap<String, u32>, id: &str) -> io::Result<u32> {
    map.get(id)
        .copied()
        .ok_or_else(|| io::Error::other(format!("command {id} is absent from the selected plan")))
}

fn steps(graph: &CommandGraph, plan: &ExecutionPlan<'_>, document: &Path) -> io::Result<Vec<Step>> {
    let document = document.canonicalize()?;
    let base = document.parent().unwrap_or(Path::new("."));
    let occurrences = plan
        .commands()
        .enumerate()
        .map(|(index, block)| {
            Ok((
                block.metadata.id.to_string(),
                u32::try_from(index).map_err(io::Error::other)?,
            ))
        })
        .collect::<io::Result<BTreeMap<_, _>>>()?;
    plan.commands()
        .map(|block| {
            let source = &block.location;
            Ok(Step {
                occurrence: occurrence(&occurrences, block.metadata.id.as_str())?,
                id: block.metadata.id.to_string(),
                description: block.metadata.description.clone(),
                section: block.metadata.section_title.clone(),
                interpreter: block.metadata.interpreter.clone(),
                dependencies: graph
                    .dependencies(block.metadata.id.as_str())
                    .into_iter()
                    .flatten()
                    .map(|id| occurrence(&occurrences, id.as_str()))
                    .collect::<io::Result<_>>()?,
                source: Source {
                    path: source.file.as_deref().map(|path| {
                        let path = path.canonicalize().unwrap_or_else(|_| path.to_owned());
                        path.strip_prefix(base)
                            .unwrap_or(&path)
                            .to_string_lossy()
                            .into_owned()
                    }),
                    start: Position {
                        line: source.location.start.line.max(1) as usize,
                        column: source.location.start.column.max(1) as usize,
                    },
                    end: Position {
                        line: source.location.end.line.max(1) as usize,
                        column: source.location.end.column.max(1) as usize,
                    },
                },
            })
        })
        .collect()
}

// Custom I/O messages can contain environment values; record only the error kind.
fn recorded_outcome(
    state: &OutcomeState<'_, Termination>,
    occurrences: &BTreeMap<String, u32>,
) -> io::Result<Outcome> {
    Ok(match state {
        OutcomeState::Succeeded => Outcome::Succeeded,
        OutcomeState::Failed(ProcessError::Failed(termination)) => Outcome::Failed {
            termination: match termination {
                Termination::Exited(code) => acdc_runbook::Termination::Exited { code: *code },
                Termination::Signaled(signal) => acdc_runbook::Termination::Signaled {
                    signal: signal.clone(),
                },
                Termination::Unknown | _ => acdc_runbook::Termination::Unknown,
            },
        },
        OutcomeState::Failed(ProcessError::TempFile(error)) => Outcome::Error {
            stage: FailureStage::Preparation,
            message: format!("could not prepare command script: {}", error.kind()),
        },
        OutcomeState::Failed(ProcessError::Process(error)) => Outcome::Error {
            stage: FailureStage::Process,
            message: format!("could not run interpreter: {}", error.kind()),
        },
        OutcomeState::Skipped(SkipReason::DependencyFailed { dependency }) => Outcome::Skipped {
            reason: acdc_runbook::SkipReason::DependencyFailed {
                occurrence: occurrence(occurrences, dependency.as_str())?,
            },
        },
        OutcomeState::Skipped(SkipReason::StoppedAfterFailure { command }) => Outcome::Skipped {
            reason: acdc_runbook::SkipReason::StoppedAfterFailure {
                occurrence: occurrence(occurrences, command.as_str())?,
            },
        },
    })
}

fn report_execution(report: RunReport<'_>) -> miette::Result<()> {
    if report.is_success() {
        return Ok(());
    }
    let mut failed = 0;
    let mut skipped = 0;
    let mut sources = SourceCache::default();
    let mut issues = Vec::new();
    for outcome in report.into_outcomes() {
        let id = outcome.command.metadata.id.clone();
        let diagnostic = match outcome.state {
            OutcomeState::Succeeded => continue,
            OutcomeState::Failed(error) => {
                failed += 1;
                LocatedError::with_cached_source(
                    CommandIssue::Recorded { id, error },
                    &outcome.command.location,
                    &mut sources,
                )
            }
            OutcomeState::Skipped(reason) => {
                skipped += 1;
                let issue = match reason {
                    SkipReason::DependencyFailed { dependency } => CommandIssue::Dependency {
                        id,
                        dependency: dependency.clone(),
                    },
                    SkipReason::StoppedAfterFailure { command } => CommandIssue::Stopped {
                        id,
                        command: command.clone(),
                    },
                };
                LocatedError::without_source(issue, &outcome.command.location)
            }
        };
        issues.push(diagnostic);
    }
    Err(miette::Report::new(ExecutionFailed {
        failed,
        skipped,
        issues,
    }))
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        io::{self, Write},
    };

    use super::{
        Capture, Outcome, OutcomeState, ProcessError, RunResult, Session, Termination, Writer,
        recorded_outcome,
    };

    type Result = std::result::Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn recorded_errors_omit_custom_io_details() -> Result {
        for error in [
            ProcessError::<Termination>::TempFile(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "private-preparation-environment-value",
            )),
            ProcessError::Process(io::Error::other("private-process-environment-value")),
        ] {
            let state = OutcomeState::Failed(error);
            let outcome = recorded_outcome(&state, &BTreeMap::new())?;
            assert!(matches!(&outcome, Outcome::Error { message, .. } if !message.is_empty()));
            assert!(!serde_json::to_string(&outcome)?.contains("private-"));
        }
        Ok(())
    }

    fn output(bytes: &[u8]) -> std::result::Result<String, Box<dyn std::error::Error>> {
        let mut text = String::new();
        for line in std::str::from_utf8(bytes)?.lines().skip(1) {
            let value: serde_json::Value = serde_json::from_str(line)?;
            if value.get(1).and_then(serde_json::Value::as_str) == Some("o") {
                text.push_str(
                    value
                        .get(2)
                        .and_then(serde_json::Value::as_str)
                        .ok_or("missing output")?,
                );
            }
        }
        Ok(text)
    }

    #[test]
    fn split_utf8_survives_every_byte_boundary_without_a_capture_warning() -> Result {
        let expected = "hello 🙂 世界\x1b]7501;?\x1b\\";
        for split in 0..=expected.len() {
            let writer = Writer::new(Vec::new(), 80, 24, None)?;
            let mut session = Session::new(writer, (80, 24));
            let (left, right) = expected.as_bytes().split_at(split);
            session.output(left, false)?;
            session.output(right, true)?;
            assert!(session.problem.is_none());
            let at = session.elapsed();
            let bytes = session
                .writer
                .finish(at, RunResult::Succeeded, Capture::Complete)?;
            assert_eq!(output(&bytes)?, expected);
        }
        Ok(())
    }

    #[test]
    fn invalid_or_truncated_utf8_retains_valid_output_and_marks_capture_incomplete() -> Result {
        for (bytes, expected) in [
            (&b"a\xffb"[..], "a\u{fffd}b"),
            (&b"prefix\xf0\x9f"[..], "prefix\u{fffd}"),
        ] {
            let writer = Writer::new(Vec::new(), 80, 24, None)?;
            let mut session = Session::new(writer, (80, 24));
            session.output(bytes, true)?;
            assert!(session.problem.is_some());
            let at = session.elapsed();
            let bytes = session.writer.finish(
                at,
                RunResult::Succeeded,
                Capture::Incomplete {
                    reason: "invalid UTF-8".into(),
                },
            )?;
            assert_eq!(output(&bytes)?, expected);
        }
        Ok(())
    }

    #[test]
    fn output_write_errors_prevent_completion_from_claiming_success() -> Result {
        struct Failing {
            remaining: usize,
        }
        impl Write for Failing {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                if self.remaining == 0 {
                    return Err(std::io::Error::other("disk full"));
                }
                let count = self.remaining.min(bytes.len());
                self.remaining -= count;
                Ok(count)
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let writer = Writer::new(Failing { remaining: 100 }, 80, 24, None)?;
        let mut session = Session::new(writer, (80, 24));
        assert!(session.output(&[b'x'; 512], true).is_err());
        let at = session.elapsed();
        assert!(
            session
                .writer
                .finish(at, RunResult::Succeeded, Capture::Complete)
                .is_err()
        );
        Ok(())
    }
}
