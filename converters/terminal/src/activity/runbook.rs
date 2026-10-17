use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};

use acdc_runbook::{Capture, Event, Outcome, RunResult, SkipReason, Step, Termination};

/// Lifecycle metadata at both its raw and playback positions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimedLifecycle {
    pub raw_at: Duration,
    pub at: Duration,
    pub event: Event,
}

/// Validated command history. An absent completion trailer never implies success.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Runbook {
    events: Vec<TimedLifecycle>,
    complete: bool,
}

impl Runbook {
    /// Lifecycle events in recording order.
    #[must_use]
    pub fn events(&self) -> &[TimedLifecycle] {
        &self.events
    }
    /// The selected commands in execution order, including skipped commands.
    #[must_use]
    pub fn steps(&self) -> &[Step] {
        match self.events.first() {
            Some(TimedLifecycle {
                event: Event::RunStarted { steps },
                ..
            }) => steps,
            _ => &[],
        }
    }
    /// Whether lifecycle completion and a matching final exit were both captured.
    /// Inspect the final lifecycle event for execution success and capture health.
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        self.complete
    }
}

pub(crate) fn validate(
    events: Vec<TimedLifecycle>,
    exit: Option<(Duration, i32)>,
    intact: bool,
) -> Result<Option<Runbook>, String> {
    let Some(first) = events.first() else {
        return Ok(None);
    };
    let Event::RunStarted { steps } = &first.event else {
        return Err("missing initial selected plan".into());
    };
    validate_plan(steps)?;
    let mut next = 0;
    let mut running = None;
    let mut outcomes = BTreeMap::new();
    let mut finished = None;
    for entry in events.iter().skip(1) {
        if finished.is_some() {
            return Err("lifecycle event after run completion".into());
        }
        match &entry.event {
            Event::RunStarted { .. } => return Err("repeated selected plan".into()),
            Event::CommandStarted { occurrence } => {
                if running.is_some()
                    || steps
                        .get(next)
                        .is_none_or(|step| step.occurrence != *occurrence)
                {
                    return Err("command start is out of plan order".into());
                }
                if steps.get(next).is_some_and(|step| {
                    step.dependencies
                        .iter()
                        .any(|key| !matches!(outcomes.get(key), Some(Outcome::Succeeded)))
                }) {
                    return Err("command started with an unsuccessful prerequisite".into());
                }
                running = Some(*occurrence);
            }
            Event::CommandFinished {
                occurrence,
                outcome,
            } => {
                let step = steps
                    .get(next)
                    .filter(|step| step.occurrence == *occurrence)
                    .ok_or("command finish is out of plan order")?;
                match outcome {
                    Outcome::Skipped { reason } => {
                        if running.is_some() {
                            return Err("running command marked skipped".into());
                        }
                        let (key, dependency) = match reason {
                            SkipReason::DependencyFailed { occurrence } => (*occurrence, true),
                            SkipReason::StoppedAfterFailure { occurrence } => (*occurrence, false),
                        };
                        let failed = matches!(
                            outcomes.get(&key),
                            Some(Outcome::Failed { .. } | Outcome::Error { .. })
                        );
                        let skipped_dependency = dependency
                            && matches!(outcomes.get(&key), Some(Outcome::Skipped { .. }));
                        if !(failed || skipped_dependency)
                            || (dependency && !step.dependencies.contains(&key))
                        {
                            return Err(
                                "skip reason does not reference an unsuccessful earlier command"
                                    .into(),
                            );
                        }
                    }
                    Outcome::Failed {
                        termination: Termination::Exited { code: 0 },
                    } => return Err("failed command has exit code zero".into()),
                    Outcome::Succeeded | Outcome::Failed { .. } | Outcome::Error { .. } => {
                        if running != Some(*occurrence) {
                            return Err("command finished without a matching start".into());
                        }
                    }
                }
                running = None;
                outcomes.insert(*occurrence, outcome.clone());
                next += 1;
            }
            Event::RunFinished { result, capture } => {
                if *result != RunResult::Aborted && (running.is_some() || next != steps.len()) {
                    return Err("run completed before selected commands had outcomes".into());
                }
                let success = outcomes
                    .values()
                    .all(|outcome| matches!(outcome, Outcome::Succeeded));
                if (*result == RunResult::Succeeded && !success)
                    || (*result == RunResult::Failed && success)
                {
                    return Err("aggregate result disagrees with command outcomes".into());
                }
                let code =
                    i32::from(*result != RunResult::Succeeded || *capture != Capture::Complete);
                finished = Some((entry.raw_at, code));
            }
        }
    }
    let complete = intact && finished.is_some() && exit == finished;
    Ok(Some(Runbook { events, complete }))
}

fn validate_plan(steps: &[Step]) -> Result<(), String> {
    let mut selected = BTreeMap::new();
    let mut ids = BTreeSet::new();
    for (index, step) in steps.iter().enumerate() {
        if !ids.insert(&step.id) || selected.insert(step.occurrence, index).is_some() {
            return Err("duplicate command identity in selected plan".into());
        }
        if step.dependencies.iter().any(|key| {
            selected
                .get(key)
                .is_none_or(|dependency| *dependency >= index)
        }) {
            return Err("prerequisite is absent or follows its dependent".into());
        }
    }
    Ok(())
}
