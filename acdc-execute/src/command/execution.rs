use std::{convert::Infallible, fmt, io, process::Command};

use petgraph::prelude::Direction;

use super::{
    CommandBlock, CommandOutcome, CommandState, ExecError, ExecutionOptions, ExecutionPlan,
    ExecutionReport, OutcomeState, ProcessError, SkipReason,
};

/// A process result independent of the API used to start or wait for it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Termination {
    /// The process returned an exit code. Zero indicates success.
    Exited(u32),
    /// A signal terminated the process. The runner supplies its name or number.
    Signaled(String),
    /// The process terminated without an available exit code or signal.
    Unknown,
}

impl Termination {
    /// Whether the process returned exit code zero.
    #[must_use]
    pub const fn success(&self) -> bool {
        matches!(self, Self::Exited(0))
    }

    fn stops_plan(&self) -> bool {
        !matches!(self, Self::Exited(_))
    }
}

impl From<std::process::ExitStatus> for Termination {
    fn from(status: std::process::ExitStatus) -> Self {
        if let Some(code) = status.code() {
            // Windows returns unsigned exit codes through a signed i32.
            return Self::Exited(code.cast_unsigned());
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt as _;

            if let Some(signal) = status.signal() {
                return Self::Signaled(signal.to_string());
            }
        }
        Self::Unknown
    }
}

impl fmt::Display for Termination {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exited(code) => write!(formatter, "exit code {code}"),
            Self::Signaled(signal) => write!(formatter, "signal {signal}"),
            Self::Unknown => formatter.write_str("an unknown termination status"),
        }
    }
}

/// A synchronous notification during execution. Skipped commands only finish.
#[derive(Debug)]
pub enum ExecutionEvent<'event, 'graph, Outcome = RunOutcome<'graph>> {
    /// An execution attempt is about to prepare its script and call the runner.
    CommandStarted {
        /// The selected command and its source metadata.
        command: &'graph CommandBlock,
    },
    /// A command ran or was skipped. Its outcome remains in the execution report.
    CommandFinished {
        /// The completed outcome, borrowed for this notification only.
        outcome: &'event Outcome,
    },
}

/// Execution stopped because its observer failed.
#[derive(Debug, thiserror::Error)]
#[error("execution observer failed: {source}")]
pub struct ObservationError<'graph, E> {
    /// Completed outcomes, including an outcome whose finish notification failed.
    /// Commands whose start notification failed have not run and are absent.
    pub report: RunReport<'graph>,
    /// The observer's original error.
    #[source]
    pub source: E,
}

/// A selected command's outcome from a custom process runner.
#[derive(Debug)]
pub struct RunOutcome<'graph> {
    /// The command that ran or was skipped.
    pub command: &'graph CommandBlock,
    /// The runner's result or the reason the command was skipped.
    pub state: OutcomeState<'graph, Termination>,
}

/// Completed outcomes from a custom runner, in plan order.
#[derive(Debug)]
pub struct RunReport<'graph> {
    outcomes: Vec<RunOutcome<'graph>>,
}

impl<'graph> RunReport<'graph> {
    /// Outcomes for completed commands, including skips.
    #[must_use]
    pub fn outcomes(&self) -> &[RunOutcome<'graph>] {
        &self.outcomes
    }

    /// Whether every outcome succeeded. An empty report succeeds.
    /// An observer error's report covers only the completed prefix of its plan.
    #[must_use]
    pub fn is_success(&self) -> bool {
        self.outcomes
            .iter()
            .all(|outcome| matches!(outcome.state, OutcomeState::Succeeded))
    }

    /// Take the outcomes, preserving structured errors for the caller.
    #[must_use]
    pub fn into_outcomes(self) -> Vec<RunOutcome<'graph>> {
        self.outcomes
    }
}

struct Stopped<Outcome, E> {
    outcomes: Vec<Outcome>,
    source: E,
}

impl<'graph> ExecutionPlan<'graph> {
    /// Run commands synchronously and return an outcome for every selected command.
    ///
    /// Commands with unsuccessful prerequisites are skipped. Independent commands continue
    /// after ordinary nonzero exits unless [`ExecutionOptions::exit_on_failure`] is set.
    /// Infrastructure errors and signal termination always stop all remaining commands.
    #[must_use]
    pub fn execute(&self, options: &ExecutionOptions) -> ExecutionReport<'graph> {
        let result = self.run_plan(
            options,
            |command| {
                command.execute_using(
                    &options.process,
                    Command::status,
                    std::process::ExitStatus::success,
                )
            },
            |status| status.code().is_none(),
            |command, state| CommandOutcome {
                command,
                state: match state {
                    OutcomeState::Succeeded => CommandState::Succeeded,
                    OutcomeState::Failed(error) => CommandState::Failed(ExecError::from(error)),
                    OutcomeState::Skipped(reason) => CommandState::Skipped(reason),
                },
            },
            |_| Ok::<(), Infallible>(()),
        );
        match result {
            Ok(outcomes) => ExecutionReport { outcomes },
            Err(error) => match error.source {},
        }
    }

    /// Execute with a process runner and synchronous lifecycle observation.
    ///
    /// The runner receives the selected command and a prepared process containing
    /// its interpreter, one temporary script argument, working directory, and environment overrides.
    /// The script file exists until the runner returns. The runner must wait for and
    /// clean up its process before returning; it must not retain the script path for
    /// later execution. Its I/O errors stop the plan as process failures.
    ///
    /// Dependency and failure policy matches [`Self::execute`]. The observer runs
    /// before preparation and after each outcome, including skips. A failed start
    /// notification prevents that command from running. A failed finish notification
    /// preserves that outcome. Either observer failure stops all further execution.
    ///
    /// # Errors
    /// Returns [`ObservationError`] with the original observer error and completed
    /// outcomes. Script preparation and process failures remain in successful return
    /// values as failed outcomes; inspect [`RunReport::is_success`].
    pub fn execute_with<E>(
        &self,
        options: &ExecutionOptions,
        mut runner: impl FnMut(&CommandBlock, &mut Command) -> io::Result<Termination>,
        observer: impl FnMut(ExecutionEvent<'_, 'graph>) -> Result<(), E>,
    ) -> Result<RunReport<'graph>, ObservationError<'graph, E>> {
        self.run_plan(
            options,
            |command| {
                command.execute_using(
                    &options.process,
                    |process| runner(command, process),
                    Termination::success,
                )
            },
            Termination::stops_plan,
            |command, state| RunOutcome { command, state },
            observer,
        )
        .map(|outcomes| RunReport { outcomes })
        .map_err(|error| ObservationError {
            report: RunReport {
                outcomes: error.outcomes,
            },
            source: error.source,
        })
    }

    fn run_plan<Status, Outcome, E>(
        &self,
        options: &ExecutionOptions,
        mut run: impl FnMut(&CommandBlock) -> Result<(), ProcessError<Status>>,
        stops_plan: fn(&Status) -> bool,
        make_outcome: fn(&'graph CommandBlock, OutcomeState<'graph, Status>) -> Outcome,
        mut observe: impl FnMut(ExecutionEvent<'_, 'graph, Outcome>) -> Result<(), E>,
    ) -> Result<Vec<Outcome>, Stopped<Outcome, E>> {
        let mut outcomes = Vec::with_capacity(self.len());
        let mut succeeded = vec![false; self.graph.graph.node_count()];
        let mut stopped_after = None;

        for &node in self.order.iter() {
            let command = &self.graph.graph[node];
            let state = if let Some(command) = stopped_after {
                OutcomeState::Skipped(SkipReason::StoppedAfterFailure { command })
            } else if let Some(dependency) = self
                .graph
                .graph
                .neighbors_directed(node, Direction::Incoming)
                .filter(|dependency| succeeded.get(dependency.index()) != Some(&true))
                .min()
            {
                OutcomeState::Skipped(SkipReason::DependencyFailed {
                    dependency: &self.graph.graph[dependency].metadata.id,
                })
            } else {
                if let Err(source) = observe(ExecutionEvent::CommandStarted { command }) {
                    return Err(Stopped { outcomes, source });
                }
                match run(command) {
                    Ok(()) => {
                        if let Some(success) = succeeded.get_mut(node.index()) {
                            *success = true;
                        }
                        OutcomeState::Succeeded
                    }
                    Err(error) => {
                        let must_stop = match &error {
                            ProcessError::Failed(status) => {
                                options.exit_on_failure || stops_plan(status)
                            }
                            ProcessError::TempFile(_) | ProcessError::Process(_) => true,
                        };
                        if must_stop {
                            stopped_after = Some(&command.metadata.id);
                        }
                        OutcomeState::Failed(error)
                    }
                }
            };
            let outcome = make_outcome(command, state);
            let observed = observe(ExecutionEvent::CommandFinished { outcome: &outcome });
            outcomes.push(outcome);
            if let Err(source) = observed {
                return Err(Stopped { outcomes, source });
            }
        }

        Ok(outcomes)
    }
}
