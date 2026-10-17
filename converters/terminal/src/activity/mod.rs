//! Task and runbook history independent of visible terminal screen frames.

mod framing;
mod runbook;

use std::{collections::BTreeMap, fmt, time::Duration};

use osc7501_parser::{Event as OscEvent, parse_sequence};
use osc7501_store::{Record as StoredRecord, Store};

pub use osc7501_parser::{Kind, State};
pub(crate) use runbook::validate;
pub use runbook::{Runbook, TimedLifecycle};

use framing::{Control, Framer};

/// Maximum retained task mutations per recording.
pub const MAX_CHANGES: usize = 16_384;
/// Maximum text bytes in retained task mutations.
pub const MAX_TEXT_BYTES: usize = 16 * 1024 * 1024;
const CHECKPOINT_INTERVAL: usize = 256;

/// A child's task namespace. Root and named records remain distinct in each scope.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Scope {
    /// A recording-wide namespace.
    Session,
    /// The recording-local command occurrence, not its authored ID.
    Command(u32),
}

/// A record identity within one task scope.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Key {
    pub scope: Scope,
    /// `None` identifies the root record.
    pub id: Option<String>,
}

/// An owned record. All strings are untrusted text, not links or markup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub key: Key,
    pub state: State,
    pub app: Option<String>,
    /// The explicit app, or the nearest ancestor's app when inherited.
    pub effective_app: Option<String>,
    pub title: Option<String>,
    pub message: Option<String>,
    pub kind: Option<Kind>,
    pub progress: Option<u8>,
}

/// A complete record replacement or removal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    Set(Record),
    Remove(Key),
}

/// A task mutation with raw elapsed time and idle-compressed playback time.
/// Equal-time mutations retain input order in [`Timeline::changes`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimedChange {
    pub raw_at: Duration,
    pub at: Duration,
    pub change: Change,
}

/// An untrusted navigation label at both recording and playback positions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Marker {
    pub raw_at: Duration,
    pub at: Duration,
    pub label: String,
}

/// A bounded summary of replay data that could not be retained or interpreted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Issue {
    MalformedReports(usize),
    TaskLimit,
    Metadata(String),
    PartialRecording(String),
}

impl fmt::Display for Issue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MalformedReports(count) => {
                write!(f, "ignored {count} malformed or oversized OSC 7501 reports")
            }
            Self::TaskLimit => write!(
                f,
                "task history exceeded {MAX_CHANGES} changes or {MAX_TEXT_BYTES} text bytes; retained the earlier history"
            ),
            Self::Metadata(message) => write!(
                f,
                "runbook metadata ignored: {message}; using ordinary terminal replay"
            ),
            Self::PartialRecording(message) => write!(
                f,
                "recording is incomplete: {message}; retained the readable prefix"
            ),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Checkpoint {
    next: usize,
    records: Vec<usize>,
}

/// Seekable task history independent of screen capture.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Timeline {
    changes: Vec<TimedChange>,
    checkpoints: Vec<Checkpoint>,
    pub(crate) markers: Vec<Marker>,
    pub(crate) runbook: Option<Runbook>,
    pub(crate) issues: Vec<Issue>,
    pub(crate) duration: Option<Duration>,
    pub(crate) raw_duration: Option<Duration>,
}

impl Timeline {
    /// Retained task mutations in recording order.
    #[must_use]
    pub fn changes(&self) -> &[TimedChange] {
        &self.changes
    }
    /// Retained navigation markers in recording order.
    #[must_use]
    pub fn markers(&self) -> &[Marker] {
        &self.markers
    }
    /// Valid lifecycle metadata, including partial histories without a trailer.
    #[must_use]
    pub fn runbook(&self) -> Option<&Runbook> {
        self.runbook.as_ref()
    }
    /// Summaries of invalid or discarded recording data.
    #[must_use]
    pub fn issues(&self) -> &[Issue] {
        &self.issues
    }
    /// Playback duration through the last retained source event.
    #[must_use]
    pub const fn duration(&self) -> Option<Duration> {
        self.duration
    }
    /// Elapsed recording time before idle compression.
    #[must_use]
    pub const fn raw_duration(&self) -> Option<Duration> {
        self.raw_duration
    }

    /// Reconstruct state at a playback position, including all equal-time changes.
    /// Later replacements and removals do not alter earlier results.
    #[must_use]
    pub fn records_at(&self, at: Duration) -> Vec<&Record> {
        let end = self.changes.partition_point(|change| change.at <= at);
        let checkpoint = self
            .checkpoints
            .iter()
            .rev()
            .find(|checkpoint| checkpoint.next <= end);
        let mut records = BTreeMap::new();
        if let Some(checkpoint) = checkpoint {
            for &index in &checkpoint.records {
                if let Some(TimedChange {
                    change: Change::Set(record),
                    ..
                }) = self.changes.get(index)
                {
                    records.insert(&record.key, record);
                }
            }
        }
        for entry in self
            .changes
            .get(checkpoint.map_or(0, |point| point.next)..end)
            .unwrap_or_default()
        {
            match &entry.change {
                Change::Set(record) => {
                    records.insert(&record.key, record);
                }
                Change::Remove(key) => {
                    records.remove(key);
                }
            }
        }
        records.into_values().collect()
    }
}

pub(crate) struct Builder {
    framer: Framer,
    store: Store<'static>,
    scope: Scope,
    current: BTreeMap<Key, usize>,
    text_bytes: usize,
    malformed: usize,
    limited: bool,
    pub timeline: Timeline,
}

impl Builder {
    pub fn new() -> Result<Self, osc7501_store::Error> {
        Ok(Self {
            framer: Framer::default(),
            store: Store::new(256)?,
            scope: Scope::Session,
            current: BTreeMap::new(),
            text_bytes: 0,
            malformed: 0,
            limited: false,
            timeline: Timeline::default(),
        })
    }

    pub fn output(
        &mut self,
        bytes: &[u8],
        raw_at: Duration,
        at: Duration,
    ) -> Result<(), osc7501_store::Error> {
        for &byte in bytes {
            match self.framer.push(byte) {
                Some(Control::Osc(frame)) if frame.starts_with(b"\x1b]7501;") => {
                    match parse_sequence(&frame) {
                        Ok(Some(OscEvent::Report(report))) => {
                            self.store.apply(report, |_| {})?;
                            self.sync(raw_at, at);
                        }
                        Ok(Some(OscEvent::Query | OscEvent::Capabilities(_) | _) | None) => {}
                        Err(_) => {
                            self.malformed = self.malformed.saturating_add(1);
                        }
                    }
                }
                Some(Control::Osc(frame)) => {
                    let body = frame
                        .strip_suffix(b"\x1b\\")
                        .or_else(|| frame.strip_suffix(b"\x07"))
                        .unwrap_or_default();
                    if body == b"\x1b]133;A" || body.starts_with(b"\x1b]133;A;") {
                        self.store.on_prompt(|_| {});
                        self.sync(raw_at, at);
                    }
                }
                Some(Control::Reset) => {
                    self.store.on_full_reset(|_| {});
                    self.sync(raw_at, at);
                }
                Some(Control::Oversized) => {
                    self.malformed = self.malformed.saturating_add(1);
                }
                None => {}
            }
        }
        Ok(())
    }

    pub fn start_command(&mut self, occurrence: u32) {
        self.store.on_full_reset(|_| {});
        self.scope = Scope::Command(occurrence);
        self.framer.boundary();
    }

    pub fn process_exit(&mut self, raw_at: Duration, at: Duration) {
        self.store.on_process_exit(|_| {});
        self.sync(raw_at, at);
        self.framer.boundary();
    }

    pub fn finish(mut self) -> Timeline {
        if self.malformed > 0 {
            self.timeline
                .issues
                .push(Issue::MalformedReports(self.malformed));
        }
        if self.limited {
            self.timeline.issues.push(Issue::TaskLimit);
        }
        self.timeline
    }

    fn sync(&mut self, raw_at: Duration, at: Duration) {
        if self.limited {
            return;
        }
        let records: BTreeMap<_, _> = self
            .store
            .iter()
            .map(|record| {
                let owned = self.own(record);
                (owned.key.clone(), owned)
            })
            .collect();
        let removed: Vec<_> = self
            .current
            .keys()
            .filter(|key| key.scope == self.scope && !records.contains_key(*key))
            .cloned()
            .collect();
        for key in removed {
            self.push(Change::Remove(key), raw_at, at);
        }
        for (key, record) in records {
            let unchanged = self
                .current
                .get(&key)
                .and_then(|index| self.timeline.changes.get(*index))
                .is_some_and(
                    |entry| matches!(&entry.change, Change::Set(previous) if *previous == record),
                );
            if !unchanged {
                self.push(Change::Set(record), raw_at, at);
            }
        }
    }

    fn own(&self, record: StoredRecord<'_>) -> Record {
        Record {
            key: Key {
                scope: self.scope,
                id: record.id().map(|id| id.as_str().to_owned()),
            },
            state: record.state(),
            app: record.app().map(|app| app.as_str().to_owned()),
            effective_app: self
                .store
                .effective_app(record.id())
                .map(|app| app.as_str().to_owned()),
            title: record.title().map(str::to_owned),
            message: record.message().map(str::to_owned),
            kind: record.kind(),
            progress: record.progress().map(osc7501_parser::Progress::get),
        }
    }

    fn push(&mut self, change: Change, raw_at: Duration, at: Duration) {
        let key = match &change {
            Change::Set(record) => &record.key,
            Change::Remove(key) => key,
        };
        let text = key.id.as_ref().map_or(0, String::len)
            + match &change {
                Change::Set(record) => [
                    &record.app,
                    &record.effective_app,
                    &record.title,
                    &record.message,
                ]
                .into_iter()
                .flatten()
                .map(String::len)
                .sum(),
                Change::Remove(_) => 0,
            };
        if self.limited
            || self.timeline.changes.len() == MAX_CHANGES
            || self.text_bytes + text > MAX_TEXT_BYTES
        {
            self.limited = true;
            return;
        }
        match &change {
            Change::Set(record) => {
                self.current
                    .insert(record.key.clone(), self.timeline.changes.len());
            }
            Change::Remove(key) => {
                self.current.remove(key);
            }
        }
        self.text_bytes += text;
        self.timeline
            .changes
            .push(TimedChange { raw_at, at, change });
        if self
            .timeline
            .changes
            .len()
            .is_multiple_of(CHECKPOINT_INTERVAL)
        {
            self.timeline.checkpoints.push(Checkpoint {
                next: self.timeline.changes.len(),
                records: self.current.values().copied().collect(),
            });
        }
    }
}
