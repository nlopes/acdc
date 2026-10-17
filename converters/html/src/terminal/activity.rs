use std::{collections::BTreeMap, io::Write};

use acdc_converters_terminal::activity::{Change, Key, Kind, Record, Runbook, Scope, Timeline};
use acdc_runbook::{Capture, Event, Outcome, RunResult, SkipReason, Termination};
use serde::Serialize;
use unicode_properties::{GeneralCategory, UnicodeGeneralCategory};

use super::{Error, escape_html};

const CHECKPOINT_INTERVAL: usize = 256;

#[derive(Clone, Copy)]
pub(super) struct Clock {
    pub record_ms: u128,
    pub total_ms: u128,
}

impl Clock {
    pub fn at(self, millis: u128) -> u128 {
        millis
            .saturating_mul(self.total_ms)
            .checked_div(self.record_ms)
            .unwrap_or(0)
            .min(self.total_ms)
    }
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Player {
    steps: Vec<Step>,
    changes: Vec<TaskChange>,
    checkpoints: Vec<Checkpoint>,
    events: Vec<Navigation>,
    summary: Option<String>,
    notes: Vec<String>,
    raw_duration_ms: Option<u128>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Step {
    scope: String,
    label: String,
    source: String,
    interpreter: String,
    section: Option<String>,
    start: Option<Time>,
    finish: Option<Time>,
    elapsed_ms: Option<u128>,
    state: &'static str,
    detail: String,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
struct Time {
    at: u128,
    raw_ms: u128,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TaskChange {
    at: u128,
    raw_ms: u128,
    key: usize,
    record: Option<Task>,
}

#[derive(Serialize)]
struct Task {
    scope: String,
    id: Option<String>,
    label: String,
    app: Option<String>,
    message: Option<String>,
    state: &'static str,
    kind: Option<&'static str>,
    progress: Option<u8>,
}

#[derive(Serialize)]
struct Checkpoint {
    next: usize,
    indices: Vec<usize>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Navigation {
    at: u128,
    raw_ms: u128,
    category: &'static str,
    label: String,
}

impl Player {
    pub fn new(timeline: &Timeline) -> Self {
        let mut player = Self {
            notes: timeline
                .issues()
                .iter()
                .map(|issue| display_text(&issue.to_string()))
                .collect(),
            raw_duration_ms: timeline.raw_duration().map(|at| at.as_millis()),
            ..Self::default()
        };
        player.tasks(timeline);
        player.runbook(timeline);
        player
            .events
            .extend(timeline.markers().iter().map(|marker| Navigation {
                at: marker.at.as_millis(),
                raw_ms: marker.raw_at.as_millis(),
                category: "marker",
                label: display_text(&marker.label),
            }));
        player.events.sort_by_key(|event| event.at);
        player
    }

    fn tasks(&mut self, timeline: &Timeline) {
        let labels: BTreeMap<_, _> = timeline
            .runbook()
            .into_iter()
            .flat_map(Runbook::steps)
            .map(|step| (step.occurrence, display_text(&step.id)))
            .collect();
        let mut keys = BTreeMap::<&Key, usize>::new();
        let mut current = BTreeMap::new();
        for entry in timeline.changes() {
            let (key, record) = match &entry.change {
                Change::Set(record) => (&record.key, Some(task(record))),
                Change::Remove(key) => (key, None),
            };
            let origin = match key.scope {
                Scope::Session => None,
                Scope::Command(occurrence) => labels.get(&occurrence),
            };
            let next = keys.len();
            let key = *keys.entry(key).or_insert(next);
            if let Some(record) = &record {
                current.insert(key, self.changes.len());
                self.events.push(Navigation {
                    at: entry.at.as_millis(),
                    raw_ms: entry.raw_at.as_millis(),
                    category: task_category(record),
                    label: format!(
                        "{}{}: {}",
                        origin.map_or_else(String::new, |label| format!("{label} · ")),
                        task_state(record),
                        record.label
                    ),
                });
            } else if let Some(previous) = current
                .remove(&key)
                .and_then(|index| self.changes.get(index))
                .and_then(|change| change.record.as_ref())
            {
                self.events.push(Navigation {
                    at: entry.at.as_millis(),
                    raw_ms: entry.raw_at.as_millis(),
                    category: "task",
                    label: format!("Task cleared: {}", previous.label),
                });
            }
            self.changes.push(TaskChange {
                at: entry.at.as_millis(),
                raw_ms: entry.raw_at.as_millis(),
                key,
                record,
            });
            if self.changes.len().is_multiple_of(CHECKPOINT_INTERVAL) {
                self.checkpoints.push(Checkpoint {
                    next: self.changes.len(),
                    indices: current.values().copied().collect(),
                });
            }
        }
    }

    fn runbook(&mut self, timeline: &Timeline) {
        let Some(book) = timeline.runbook() else {
            return;
        };
        let mut positions = BTreeMap::new();
        let labels: BTreeMap<_, _> = book
            .steps()
            .iter()
            .map(|step| (step.occurrence, display_text(&step.id)))
            .collect();
        for source in book.steps() {
            positions.insert(source.occurrence, self.steps.len());
            self.steps.push(Step {
                scope: format!("command-{}", source.occurrence),
                label: display_text(&source.description.as_ref().map_or_else(
                    || source.id.clone(),
                    |description| format!("{} — {description}", source.id),
                )),
                source: display_text(&format!(
                    "{}:{}:{}–{}:{}",
                    source.source.path.as_deref().unwrap_or("Unnamed source"),
                    source.source.start.line,
                    source.source.start.column,
                    source.source.end.line,
                    source.source.end.column
                )),
                interpreter: display_text(&source.interpreter),
                section: source.section.as_deref().map(display_text),
                start: None,
                finish: None,
                elapsed_ms: None,
                state: "pending",
                detail: String::new(),
            });
        }
        for entry in book.events() {
            let time = Time {
                at: entry.at.as_millis(),
                raw_ms: entry.raw_at.as_millis(),
            };
            match &entry.event {
                Event::RunStarted { .. } => {}
                Event::CommandStarted { occurrence } => {
                    if let Some(step) = positions
                        .get(occurrence)
                        .and_then(|index| self.steps.get_mut(*index))
                    {
                        step.start = Some(time);
                        self.events.push(Navigation {
                            at: time.at,
                            raw_ms: time.raw_ms,
                            category: "step",
                            label: format!("Step: {}", step.label),
                        });
                    }
                }
                Event::CommandFinished {
                    occurrence,
                    outcome,
                } => {
                    if let Some(step) = positions
                        .get(occurrence)
                        .and_then(|index| self.steps.get_mut(*index))
                    {
                        step.finish = Some(time);
                        step.elapsed_ms = step
                            .start
                            .map(|start| time.raw_ms.saturating_sub(start.raw_ms));
                        (step.state, step.detail) = outcome_text(outcome, &labels);
                        if matches!(outcome, Outcome::Failed { .. } | Outcome::Error { .. }) {
                            self.events.push(Navigation {
                                at: time.at,
                                raw_ms: time.raw_ms,
                                category: "command-failure",
                                label: format!("Command failed: {} — {}", step.label, step.detail),
                            });
                        }
                    }
                }
                Event::RunFinished { result, capture } => {
                    self.summary = Some(run_summary(*result, capture, book.is_complete()));
                }
            }
        }
        if self.summary.is_none() {
            self.summary = Some("Incomplete recording — no run completion was captured.".into());
        }
    }

    pub fn scale(&mut self, clock: Clock) {
        for change in &mut self.changes {
            change.at = clock.at(change.at);
        }
        for event in &mut self.events {
            event.at = clock.at(event.at);
        }
        for step in &mut self.steps {
            for time in [&mut step.start, &mut step.finish].into_iter().flatten() {
                time.at = clock.at(time.at);
            }
        }
    }

    pub fn write_json(&self, writer: &mut impl Write) -> Result<(), Error> {
        let json = serde_json::to_string(self).map_err(std::io::Error::other)?;
        write!(writer, "{}", json.replace('<', "\\u003c"))?;
        Ok(())
    }

    pub fn controls(&self, writer: &mut impl Write, total_ms: u128) -> Result<(), Error> {
        write!(
            writer,
            "<div class=\"terminal-view__controls\"><button type=\"button\" data-replay-play disabled>Play</button>"
        )?;
        if !self.steps.is_empty() {
            write!(
                writer,
                "<button type=\"button\" data-replay-prev-step disabled>Previous step</button><button type=\"button\" data-replay-next-step disabled>Next step</button>"
            )?;
        }
        write!(
            writer,
            "<label class=\"terminal-view__seek\">Playback position <input type=\"range\" min=\"0\" max=\"{total_ms}\" step=\"1\" value=\"{total_ms}\" disabled></label><output class=\"terminal-view__clock\">{} / {}</output></div>",
            elapsed(total_ms),
            elapsed(total_ms)
        )?;
        write!(
            writer,
            "<span class=\"terminal-view__announcement\" aria-live=\"polite\" aria-atomic=\"true\"></span>"
        )?;
        Ok(())
    }

    pub fn render(&self, writer: &mut impl Write) -> Result<(), Error> {
        if let Some(summary) = &self.summary {
            write!(
                writer,
                "<p class=\"terminal-view__result\">{}</p>",
                escape_html(summary)
            )?;
        }
        if let Some(raw) = self.raw_duration_ms {
            write!(
                writer,
                "<p class=\"terminal-view__elapsed\">Recorded elapsed time: {}</p>",
                elapsed(raw)
            )?;
        }
        for note in &self.notes {
            write!(
                writer,
                "<p class=\"terminal-view__note\">{}</p>",
                escape_html(note)
            )?;
        }
        let records = self.final_records();
        if !self.steps.is_empty() {
            write!(
                writer,
                "<ol class=\"terminal-view__steps\" aria-label=\"Runbook steps\">"
            )?;
            for step in &self.steps {
                render_step(writer, step, &records)?;
            }
            write!(writer, "</ol>")?;
        }
        if self.changes.iter().any(|change| {
            change
                .record
                .as_ref()
                .is_some_and(|record| record.scope == "session")
        }) {
            write!(
                writer,
                "<div class=\"terminal-view__session\"><strong>Program tasks</strong>"
            )?;
            render_tasks(writer, "session", &records)?;
            write!(writer, "</div>")?;
        }
        self.navigation(writer)
    }

    fn final_records(&self) -> BTreeMap<usize, &TaskChange> {
        let mut records = BTreeMap::new();
        for change in &self.changes {
            if change.record.is_some() {
                records.insert(change.key, change);
            } else {
                records.remove(&change.key);
            }
        }
        records
    }

    fn navigation(&self, writer: &mut impl Write) -> Result<(), Error> {
        if self.events.is_empty() {
            return Ok(());
        }
        write!(
            writer,
            "<details class=\"terminal-view__events\"><summary>Timeline events ({})</summary><div class=\"terminal-view__event-controls\"><label>Event type <select disabled>",
            self.events.len()
        )?;
        for (value, label) in [
            ("all", "All events"),
            ("command-failure", "Command failures"),
            ("task-error", "Task errors"),
            ("permission", "Permission requests"),
            ("auth", "Authentication requests"),
            ("question", "Questions"),
            ("blocked", "Other input requests"),
            ("task", "Other task changes"),
            ("step", "Step starts"),
            ("marker", "Markers"),
        ] {
            write!(writer, "<option value=\"{value}\">{label}</option>")?;
        }
        write!(
            writer,
            "</select></label><button type=\"button\" data-replay-prev-event disabled>Previous event</button><button type=\"button\" data-replay-next-event disabled>Next event</button></div><ol>"
        )?;
        for event in &self.events {
            write!(
                writer,
                "<li data-category=\"{}\"><button type=\"button\" data-seek=\"{}\" disabled>{} <span class=\"terminal-view__event-time\">(recorded {})</span></button></li>",
                event.category,
                event.at,
                escape_html(&event.label),
                elapsed(event.raw_ms)
            )?;
        }
        write!(writer, "</ol></details>")?;
        Ok(())
    }
}

fn task(record: &Record) -> Task {
    Task {
        scope: match record.key.scope {
            Scope::Session => "session".into(),
            Scope::Command(occurrence) => format!("command-{occurrence}"),
        },
        id: record.key.id.as_deref().map(display_text),
        label: display_text(
            record
                .title
                .as_deref()
                .filter(|title| !title.trim().is_empty())
                .or(record.key.id.as_deref())
                .unwrap_or("Program"),
        ),
        app: record.effective_app.as_deref().map(display_text),
        message: record.message.as_deref().map(display_text),
        state: record.state.as_str(),
        kind: record.kind.map(Kind::as_str),
        progress: record.progress,
    }
}

fn task_category(task: &Task) -> &'static str {
    match task.state {
        "error" => "task-error",
        "blocked" => task.kind.unwrap_or("blocked"),
        _ => "task",
    }
}

fn task_state(task: &Task) -> &'static str {
    match task.state {
        "blocked" => match task.kind {
            Some("permission") => "Permission requested",
            Some("auth") => "Authentication requested",
            Some("question") => "Question",
            _ => "Input requested",
        },
        "error" => "Reported error",
        "done" => "Done",
        "working" => "Working",
        _ => "Idle",
    }
}

fn outcome_text(outcome: &Outcome, labels: &BTreeMap<u32, String>) -> (&'static str, String) {
    match outcome {
        Outcome::Succeeded => ("succeeded", "Exited successfully".into()),
        Outcome::Failed { termination } => (
            "failed",
            match termination {
                Termination::Exited { code } => format!("Exited with code {code}"),
                Termination::Signaled { signal } => {
                    format!("Stopped by signal {}", display_text(signal))
                }
                Termination::Unknown => "Process failed; termination unavailable".into(),
            },
        ),
        Outcome::Error { message, .. } => ("error", display_text(message)),
        Outcome::Skipped { reason } => (
            "skipped",
            match reason {
                SkipReason::DependencyFailed { occurrence } => {
                    format!(
                        "Prerequisite {} did not succeed",
                        labels.get(occurrence).map_or("command", String::as_str)
                    )
                }
                SkipReason::StoppedAfterFailure { occurrence } => {
                    format!(
                        "Stopped after {} failed",
                        labels.get(occurrence).map_or("command", String::as_str)
                    )
                }
            },
        ),
    }
}

fn run_summary(result: RunResult, capture: &Capture, complete: bool) -> String {
    let result = match result {
        RunResult::Succeeded => "Succeeded",
        RunResult::Failed => "Failed",
        RunResult::Aborted => "Aborted",
    };
    let prefix = if complete {
        "Recorded run"
    } else {
        "Incomplete recording; last reported run result"
    };
    let capture = match capture {
        Capture::Complete => "complete".into(),
        Capture::Incomplete { reason } => format!("incomplete — {}", display_text(reason)),
    };
    format!("{prefix}: {result}. Capture: {capture}.")
}

fn render_step(
    writer: &mut impl Write,
    step: &Step,
    records: &BTreeMap<usize, &TaskChange>,
) -> Result<(), Error> {
    let at = step.start.or(step.finish).map(|time| time.at);
    write!(
        writer,
        "<li class=\"terminal-view__step\" data-scope=\"{}\"><div class=\"terminal-view__step-heading\">",
        step.scope
    )?;
    if let Some(at) = at {
        write!(
            writer,
            "<button type=\"button\" data-seek=\"{at}\" disabled>{}</button>",
            escape_html(&step.label)
        )?;
    } else {
        write!(writer, "<strong>{}</strong>", escape_html(&step.label))?;
    }
    let state = if step.finish.is_some() {
        step.state
    } else if step.start.is_some() {
        "running"
    } else {
        "pending"
    };
    write!(
        writer,
        "<span class=\"terminal-view__badge\" data-state=\"{state}\">{state}</span><span class=\"terminal-view__step-duration\">"
    )?;
    if let Some(millis) = step.elapsed_ms {
        write!(writer, "{} recorded", elapsed(millis))?;
    }
    write!(
        writer,
        "</span></div><div class=\"terminal-view__provenance\">{} · {}",
        escape_html(&step.source),
        escape_html(&step.interpreter)
    )?;
    if let Some(section) = &step.section {
        write!(writer, " · {}", escape_html(section))?;
    }
    write!(
        writer,
        "</div><div class=\"terminal-view__step-detail\">{}</div>",
        escape_html(if step.finish.is_none() && step.start.is_some() {
            "Outcome not captured"
        } else {
            &step.detail
        })
    )?;
    render_tasks(writer, &step.scope, records)?;
    write!(writer, "</li>")?;
    Ok(())
}

fn render_tasks(
    writer: &mut impl Write,
    scope: &str,
    records: &BTreeMap<usize, &TaskChange>,
) -> Result<(), Error> {
    write!(
        writer,
        "<ul class=\"terminal-view__tasks\" data-task-scope=\"{scope}\">"
    )?;
    let mut tasks: Vec<_> = records
        .values()
        .filter_map(|change| {
            change
                .record
                .as_ref()
                .filter(|record| record.scope == scope)
                .map(|record| (*change, record))
        })
        .collect();
    tasks.sort_by(|(_, left), (_, right)| left.id.cmp(&right.id));
    if tasks.is_empty() {
        write!(
            writer,
            "<li class=\"terminal-view__empty\">No reported tasks at this position</li>"
        )?;
    }
    for (change, task) in tasks {
        let depth = task
            .id
            .as_ref()
            .map_or(0, |id| id.bytes().filter(|byte| *byte == b'/').count())
            .min(4);
        write!(
            writer,
            "<li data-depth=\"{depth}\"><button type=\"button\" data-seek=\"{}\" data-task-key=\"{}\" disabled>{}</button><span class=\"terminal-view__badge\" data-state=\"{}\">{}</span>",
            change.at,
            change.key,
            escape_html(&task.label),
            task.state,
            task_state(task)
        )?;
        if let Some(id) = &task.id {
            write!(
                writer,
                " <span class=\"terminal-view__task-id\">{}</span>",
                escape_html(id)
            )?;
        }
        if let Some(app) = &task.app {
            write!(
                writer,
                " <span class=\"terminal-view__task-app\">{}</span>",
                escape_html(app)
            )?;
        }
        if let Some(progress) = task.progress {
            write!(
                writer,
                " <progress aria-label=\"Task progress\" max=\"100\" value=\"{progress}\">{progress}%</progress> {progress}%"
            )?;
        }
        if let Some(message) = &task.message {
            write!(
                writer,
                "<div class=\"terminal-view__task-message\">{}</div>",
                escape_html(message)
            )?;
        }
        write!(writer, "</li>")?;
    }
    write!(writer, "</ul>")?;
    Ok(())
}

fn elapsed(millis: u128) -> String {
    let tenths = millis.saturating_add(50) / 100;
    format!("{}.{:01} s", tenths / 10, tenths % 10)
}

pub(super) fn display_text(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if matches!(
                ch.general_category(),
                GeneralCategory::Control
                    | GeneralCategory::Format
                    | GeneralCategory::LineSeparator
                    | GeneralCategory::ParagraphSeparator
            ) {
                '\u{fffd}'
            } else {
                ch
            }
        })
        .collect()
}
