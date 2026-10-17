//! Read asciicast v2/v3 recordings as terminal screens and independent task history.
//!
//! Output is replayed through the emulator and observed for OSC 7501 reports.
//! Valid acdc lifecycle metadata scopes tasks to commands. Input and recorded
//! commands are never executed. Markers, command outcomes, and raw times remain
//! available through [`Recording::activity`].
//!
//! Retained source events and task changes have separate limits. Invalid event
//! lines retain a readable prefix with a diagnostic. Unsupported lifecycle
//! metadata falls back to session-scoped task history. Resize events only grow
//! the screen, preserving content recorded at larger sizes.

use std::{io::BufRead, time::Duration};

use acdc_parser::InlineNode;
use acdc_runbook::{Decoded, Event as Lifecycle};
use asciicast_rs::{Reader, V2, common, v2, v3};

use crate::{
    activity::{self, Issue, Marker, TimedLifecycle},
    cell_grid::{Rgb, TerminalSize},
    replay::{self, Event},
};

/// The default foreground/background colours recorded in an asciicast header.
///
/// A premium replay paints its chrome with the terminal's own colours instead
/// of a generic light/dark theme; this is the subset of the recorded theme the
/// renderer needs (the indexed palette resolves per cell during capture).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReplayTheme {
    /// Default foreground (text) colour.
    pub fg: Rgb,
    /// Default background colour.
    pub bg: Rgb,
    /// The 16-colour palette (slots 0–15), each present when the recording
    /// declared it. A renderer can publish these as themeable palette colours.
    pub palette: [Option<Rgb>; 16],
}

impl From<&common::Theme> for ReplayTheme {
    fn from(theme: &common::Theme) -> Self {
        let mut palette = [None; 16];
        for (slot, colour) in palette.iter_mut().zip(theme.palette.iter()) {
            *slot = Some(rgb_from(*colour));
        }
        Self {
            fg: rgb_from(theme.fg),
            bg: rgb_from(theme.bg),
            palette,
        }
    }
}

/// Convert an `asciicast-rs` colour into acdc's [`Rgb`].
fn rgb_from(colour: common::Rgb) -> Rgb {
    Rgb {
        r: colour.r(),
        g: colour.g(),
        b: colour.b(),
    }
}

/// A parsed recording with screen events, task history, and recording diagnostics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recording {
    size: TerminalSize,
    events: Vec<Event<'static>>,
    theme: Option<ReplayTheme>,
    title: Option<String>,
    activity: activity::Timeline,
}

impl Recording {
    /// Initial terminal size declared by the recording header.
    #[must_use]
    pub const fn size(&self) -> TerminalSize {
        self.size
    }

    /// Playback duration through the last retained event, including metadata and exit.
    /// Use [`activity`](Self::activity) for the raw duration before idle compression.
    #[must_use]
    pub fn duration(&self) -> Option<Duration> {
        self.activity.duration()
    }

    /// Task changes, command provenance, markers, and raw elapsed times.
    #[must_use]
    pub const fn activity(&self) -> &activity::Timeline {
        &self.activity
    }

    /// The default colours recorded in the header, if the recording carried a
    /// theme. Used to paint a replay's chrome in the terminal's own colours.
    #[must_use]
    pub const fn theme(&self) -> Option<ReplayTheme> {
        self.theme
    }

    /// A human label for the recording: its `title`, falling back to the
    /// recorded `command`. Shown in a replay's title bar.
    #[must_use]
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    /// Capture replay frames from the recording at `size`.
    ///
    /// Every distinct visible screen the session produced is captured (with
    /// consecutive duplicates deduplicated by the shared replay pipeline).
    /// Status-only recordings receive one blank screen; task history is retained
    /// independently of screen changes.
    /// asciicast events are always timed, so no synthetic frame spacing is
    /// applied.
    ///
    /// # Errors
    ///
    /// Returns an error when terminal dimensions are invalid or screen capture fails.
    pub fn capture(self, size: TerminalSize) -> Result<replay::Timeline, replay::Error> {
        let mut timeline = replay::capture(self.events, replay::Options::new(size))?;
        timeline.attach_activity(self.activity, size);
        Ok(timeline)
    }
}

/// Error returned when an asciicast recording cannot be parsed.
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum Error {
    /// The version field or v2 header could not be parsed.
    #[error(transparent)]
    Parse(#[from] asciicast_rs::Error),
    #[error(transparent)]
    /// The v3 header could not be read.
    Recording(#[from] acdc_runbook::ReadError),
    #[error(transparent)]
    /// Task storage allocation or field preparation failed.
    Store(#[from] osc7501_store::Error),
    /// The recording used a version the converter does not replay.
    #[error("unsupported asciicast version {version}; only versions 2 and 3 are supported")]
    UnsupportedVersion {
        /// The version found in the recording.
        version: u8,
    },
}

/// Parse asciicast text into a [`Recording`].
///
/// The header's `version` field is peeked first, then the matching streaming
/// reader converts the events lazily. v1 and any other unsupported version are
/// rejected with [`Error::UnsupportedVersion`].
fn parse(input: &str) -> Result<Recording, Error> {
    parse_with(input, None)
}

/// Parse like [`parse`], but override the idle-gap cap (seconds) used to compress
/// dead air. `None` falls back to the header's `idle_time_limit`, then to
/// [`DEFAULT_IDLE_LIMIT_SECS`].
fn parse_with(input: &str, idle_override: Option<f64>) -> Result<Recording, Error> {
    match detect_version(input)? {
        2 => recording_from_v2(v2::stream(input.as_bytes())?, idle_override),
        3 => recording_from_v3(acdc_runbook::Reader::new(input.as_bytes())?, idle_override),
        version => Err(Error::UnsupportedVersion { version }),
    }
}

/// The one header field [`detect_version`] needs; other fields are ignored, so
/// only the `version` integer is materialised rather than the whole header.
#[derive(serde::Deserialize)]
struct VersionProbe {
    version: u8,
}

/// Peek the `version` field from the recording's header line without parsing the
/// whole document, so [`parse`] can pick the right typed streaming reader.
fn detect_version(input: &str) -> Result<u8, Error> {
    let header = input
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .ok_or(asciicast_rs::Error::MissingHeader)?;
    let probe: VersionProbe = serde_json::from_str(header).map_err(asciicast_rs::Error::from)?;
    Ok(probe.version)
}

/// Parse an asciicast recording from a verbatim block's inline nodes.
///
/// The block content is reconstructed as plain text (joined on newlines) and
/// parsed with [`parse`].
///
/// # Errors
///
/// Returns the same errors as [`parse`].
pub fn parse_inlines(inlines: &[InlineNode<'_>]) -> Result<Recording, Error> {
    parse(&crate::extract_inline_text(inlines, "\n"))
}

/// Parse an asciicast recording from inline nodes, overriding the idle-gap cap.
///
/// `idle_limit` caps the gap between consecutive events, compressing dead air
/// (e.g. a build before a test run) so a replay dwells on real output. `None`
/// falls back to the recording's `idle_time_limit` header, then to a default.
///
/// # Errors
///
/// Returns the same errors as [`parse_inlines`].
pub fn parse_inlines_with(
    inlines: &[InlineNode<'_>],
    idle_limit: Option<Duration>,
) -> Result<Recording, Error> {
    parse_with(
        &crate::extract_inline_text(inlines, "\n"),
        idle_limit.map(|limit| limit.as_secs_f64()),
    )
}

/// Maximum source events retained before returning a partial replay.
pub const MAX_RECORDING_EVENTS: usize = 100_000;
const MAX_RECORDING_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug)]
enum Payload {
    Output(Vec<u8>),
    Resize(u16, u16),
    Marker(String),
    Metadata(Lifecycle),
    Exit(i32),
    Other,
}

struct SourceEvent {
    raw_at: Duration,
    at: Duration,
    payload: Payload,
}

fn recording_from_v2<R: BufRead>(
    reader: Reader<V2, R>,
    idle_override: Option<f64>,
) -> Result<Recording, Error> {
    let header = reader.header();
    let size = TerminalSize::new(header.width.into(), header.height.into());
    let theme = header.theme.as_ref().map(ReplayTheme::from);
    let title = header.title.clone().or_else(|| header.command.clone());
    let clock = IdleClock::new(idle_override, header.idle_time_limit);
    let mut source = Source::new(clock);
    for item in reader.absolute_times() {
        let (time, event) = match item {
            Ok(event) => event,
            Err(error) => {
                source.partial(&error.to_string());
                break;
            }
        };
        let payload = match event.payload {
            v2::EventPayload::Output(data) => Payload::Output(data.into_bytes()),
            v2::EventPayload::Resize(resize) => Payload::Resize(resize.cols, resize.rows),
            v2::EventPayload::Marker(label) => Payload::Marker(label),
            v2::EventPayload::Unknown { code, data } if code == acdc_runbook::EVENT_CODE => {
                match data.as_str() {
                    Some(data) => source.metadata(data),
                    None => source.invalid_metadata("extension data is not a JSON string"),
                }
            }
            v2::EventPayload::Input(_) | v2::EventPayload::Unknown { .. } | _ => Payload::Other,
        };
        let Ok(time) = Duration::try_from_secs_f64(time) else {
            source.partial("invalid event timestamp");
            break;
        };
        if !source.push(time, payload) {
            break;
        }
    }
    source.recording(size, theme, title)
}

fn recording_from_v3<R: BufRead>(
    reader: acdc_runbook::Reader<R>,
    idle_override: Option<f64>,
) -> Result<Recording, Error> {
    let header = reader.header();
    let size = TerminalSize::new(header.term.cols.into(), header.term.rows.into());
    let theme = header.term.theme.as_ref().map(ReplayTheme::from);
    let title = header.title.clone().or_else(|| header.command.clone());
    let clock = IdleClock::new(idle_override, header.idle_time_limit);
    let mut source = Source::new(clock);
    for item in reader {
        let event = match item {
            Ok(event) => event,
            Err(error) => {
                source.partial(&error.to_string());
                break;
            }
        };
        let payload = match event.payload {
            v3::EventPayload::Output(data) => Payload::Output(data.into_bytes()),
            v3::EventPayload::Resize(resize) => Payload::Resize(resize.cols, resize.rows),
            v3::EventPayload::Marker(label) => Payload::Marker(label),
            v3::EventPayload::Exit(status) => Payload::Exit(status.code()),
            v3::EventPayload::Unknown { code, data } if code == acdc_runbook::EVENT_CODE => {
                source.metadata(&data)
            }
            v3::EventPayload::Input(_) | v3::EventPayload::Unknown { .. } | _ => Payload::Other,
        };
        if !source.push(event.at, payload) {
            break;
        }
    }
    source.recording(size, theme, title)
}

struct Source {
    clock: IdleClock,
    events: Vec<SourceEvent>,
    issues: Vec<Issue>,
    bytes: usize,
    metadata_valid: bool,
}

impl Source {
    fn new(clock: IdleClock) -> Self {
        Self {
            clock,
            events: Vec::new(),
            issues: Vec::new(),
            bytes: 0,
            metadata_valid: true,
        }
    }

    fn partial(&mut self, message: &str) {
        self.issues
            .push(Issue::PartialRecording(diagnostic_text(message)));
    }

    fn metadata(&mut self, data: &str) -> Payload {
        if !self.metadata_valid {
            return Payload::Other;
        }
        if data.len() > acdc_runbook::MAX_LINE_BYTES {
            return self.invalid_metadata("extension exceeds the metadata line limit");
        }
        self.bytes = self.bytes.saturating_add(data.len());
        match acdc_runbook::decode(data) {
            Ok(Decoded::Event(event)) => Payload::Metadata(event),
            Ok(Decoded::UnsupportedVersion(version)) => {
                self.invalid_metadata(&format!("unsupported schema version {version}"))
            }
            Ok(Decoded::UnsupportedEvent(event)) => {
                self.invalid_metadata(&format!("unsupported event {event}"))
            }
            Err(error) => self.invalid_metadata(&error.to_string()),
        }
    }

    fn invalid_metadata(&mut self, message: &str) -> Payload {
        self.metadata_valid = false;
        self.issues.push(Issue::Metadata(diagnostic_text(message)));
        Payload::Other
    }

    fn push(&mut self, time: Duration, payload: Payload) -> bool {
        let bytes = match &payload {
            Payload::Output(bytes) => bytes.len(),
            Payload::Marker(label) => label.len(),
            Payload::Resize(_, _) | Payload::Metadata(_) | Payload::Exit(_) | Payload::Other => 0,
        };
        self.bytes = self.bytes.saturating_add(bytes);
        if self.events.len() == MAX_RECORDING_EVENTS || self.bytes > MAX_RECORDING_BYTES {
            self.partial(&format!("exceeded {MAX_RECORDING_EVENTS} events or {MAX_RECORDING_BYTES} retained source bytes"));
            return false;
        }
        if time < self.clock.previous_raw {
            self.partial("invalid or backward event timestamp");
            return false;
        }
        let at = self.clock.tick(time);
        self.events.push(SourceEvent {
            raw_at: time,
            at,
            payload,
        });
        true
    }

    fn recording(
        mut self,
        size: TerminalSize,
        theme: Option<ReplayTheme>,
        title: Option<String>,
    ) -> Result<Recording, Error> {
        let lifecycle = self
            .events
            .iter()
            .filter_map(|entry| match &entry.payload {
                Payload::Metadata(event) => Some(TimedLifecycle {
                    raw_at: entry.raw_at,
                    at: entry.at,
                    event: event.clone(),
                }),
                Payload::Output(_)
                | Payload::Resize(_, _)
                | Payload::Marker(_)
                | Payload::Exit(_)
                | Payload::Other => None,
            })
            .collect();
        let exit = self.events.last().and_then(|entry| match entry.payload {
            Payload::Exit(code) => Some((entry.raw_at, code)),
            Payload::Output(_)
            | Payload::Resize(_, _)
            | Payload::Marker(_)
            | Payload::Metadata(_)
            | Payload::Other => None,
        });
        let intact = !self
            .issues
            .iter()
            .any(|issue| matches!(issue, Issue::PartialRecording(_)));
        let book = if self.metadata_valid {
            match activity::validate(lifecycle, exit, intact) {
                Ok(book) => book,
                Err(message) => {
                    self.issues.push(Issue::Metadata(message));
                    None
                }
            }
        } else {
            None
        };
        let mut observer = activity::Builder::new()?;
        for entry in &self.events {
            match &entry.payload {
                Payload::Output(bytes) => observer.output(bytes, entry.raw_at, entry.at)?,
                Payload::Exit(_) => observer.process_exit(entry.raw_at, entry.at),
                Payload::Metadata(Lifecycle::CommandStarted { occurrence }) if book.is_some() => {
                    observer.start_command(*occurrence);
                }
                Payload::Metadata(Lifecycle::CommandFinished { outcome, .. })
                    if book.is_some()
                        && !matches!(outcome, acdc_runbook::Outcome::Skipped { .. }) =>
                {
                    observer.process_exit(entry.raw_at, entry.at);
                }
                Payload::Marker(label) => observer.timeline.markers.push(Marker {
                    raw_at: entry.raw_at,
                    at: entry.at,
                    label: label.clone(),
                }),
                Payload::Metadata(_) | Payload::Resize(_, _) | Payload::Other => {}
            }
        }
        let mut activity = observer.finish();
        activity.runbook = book;
        activity.issues.extend(self.issues);
        activity.duration = self.events.last().map(|entry| entry.at);
        activity.raw_duration = self.events.last().map(|entry| entry.raw_at);
        let mut events = Vec::new();
        let mut current = size;
        for entry in self.events {
            match entry.payload {
                Payload::Output(bytes) => events.push(Event::write_at(bytes, entry.at)),
                Payload::Resize(cols, rows) => {
                    push_resize(&mut events, &mut current, entry.at, cols, rows);
                }
                Payload::Metadata(_) | Payload::Marker(_) | Payload::Exit(_) | Payload::Other => {}
            }
        }
        Ok(Recording {
            size,
            events,
            theme,
            title,
            activity,
        })
    }
}

fn diagnostic_text(message: &str) -> String {
    message
        .chars()
        .take(512)
        .map(|ch| if ch.is_control() { '\u{fffd}' } else { ch })
        .collect()
}

/// Push a grow-only resize event: a dimension may grow but never shrink, so a
/// replay never loses content that was visible at a larger size. A resize that
/// would not grow the terminal is dropped.
fn push_resize(
    events: &mut Vec<Event<'static>>,
    current: &mut TerminalSize,
    at: Duration,
    cols: u16,
    rows: u16,
) {
    let grown = TerminalSize::new(current.cols.max(cols.into()), current.rows.max(rows.into()));
    if grown != *current {
        *current = grown;
        events.push(Event::resize_at(grown, at));
    }
}

/// Default cap (seconds) on the idle gap between two events when the header does
/// not set `idle_time_limit`. Recordings often front-load dead air (e.g. a
/// `cargo build` before a test run), so without a cap a replay spends most of
/// its time on a frozen screen. Compressing long gaps keeps playback on the
/// parts that actually change. Gaps shorter than this are left untouched.
const DEFAULT_IDLE_LIMIT_SECS: f64 = 2.0;

/// Turns raw absolute event times into idle-compressed ones: any gap longer than
/// `idle_limit` is shortened to it, so dead air never dominates a replay while
/// the relative timing of active output is preserved.
struct IdleClock {
    idle_limit: Duration,
    previous_raw: Duration,
    elapsed: Duration,
}

impl IdleClock {
    /// Build a clock from the first positive limit among an explicit override
    /// and the header's `idle_time_limit`, falling back to
    /// [`DEFAULT_IDLE_LIMIT_SECS`] when neither is set.
    fn new(idle_override: Option<f64>, header_limit: Option<f64>) -> Self {
        let idle_limit = [idle_override, header_limit]
            .into_iter()
            .flatten()
            .filter(|limit| limit.is_finite() && *limit > 0.0)
            .find_map(|limit| Duration::try_from_secs_f64(limit).ok())
            .unwrap_or(Duration::from_secs_f64(DEFAULT_IDLE_LIMIT_SECS));
        Self {
            idle_limit,
            previous_raw: Duration::ZERO,
            elapsed: Duration::ZERO,
        }
    }

    /// Advance to raw absolute time `raw`, returning the compressed timestamp.
    fn tick(&mut self, raw: Duration) -> Duration {
        let gap = raw.saturating_sub(self.previous_raw).min(self.idle_limit);
        self.elapsed = self.elapsed.saturating_add(gap);
        self.previous_raw = raw;
        self.elapsed
    }
}

#[cfg(test)]
#[path = "asciicast_activity_tests.rs"]
mod activity_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_v2_output_with_absolute_timestamps() -> Result<(), Error> {
        let cast = concat!(
            "{\"version\": 2, \"width\": 80, \"height\": 24}\n",
            "[0.5, \"o\", \"hello\"]\n",
            "[1.5, \"o\", \" world\\r\\n\"]\n",
        );
        let recording = parse(cast)?;

        assert_eq!(recording.size(), TerminalSize::new(80, 24));
        assert_eq!(
            recording.events,
            vec![
                Event::write_at(b"hello".to_vec(), Duration::from_millis(500)),
                Event::write_at(b" world\r\n".to_vec(), Duration::from_millis(1500)),
            ]
        );
        Ok(())
    }

    #[test]
    fn long_idle_gaps_are_compressed_to_the_default_limit() -> Result<(), Error> {
        // A 60s gap before the second event (dead air, e.g. compilation) is
        // capped to the 2s default; the short 0.3s gap after it is untouched.
        let cast = concat!(
            "{\"version\": 2, \"width\": 80, \"height\": 24}\n",
            "[0.0, \"o\", \"build\\r\\n\"]\n",
            "[60.0, \"o\", \"first\\r\\n\"]\n",
            "[60.3, \"o\", \"second\\r\\n\"]\n",
        );
        let recording = parse(cast)?;

        assert_eq!(
            recording.events,
            vec![
                Event::write_at(b"build\r\n".to_vec(), Duration::ZERO),
                Event::write_at(b"first\r\n".to_vec(), Duration::from_secs(2)),
                Event::write_at(b"second\r\n".to_vec(), Duration::from_millis(2300)),
            ]
        );
        Ok(())
    }

    #[test]
    fn header_idle_time_limit_overrides_the_default() -> Result<(), Error> {
        // The header sets a 0.5s idle limit, so the 60s gap collapses to 0.5s.
        let cast = concat!(
            "{\"version\": 2, \"width\": 80, \"height\": 24, \"idle_time_limit\": 0.5}\n",
            "[0.0, \"o\", \"a\"]\n",
            "[60.0, \"o\", \"b\"]\n",
        );
        let recording = parse(cast)?;

        assert_eq!(
            recording.events,
            vec![
                Event::write_at(b"a".to_vec(), Duration::ZERO),
                Event::write_at(b"b".to_vec(), Duration::from_millis(500)),
            ]
        );
        Ok(())
    }

    #[test]
    fn header_theme_and_command_are_captured() -> Result<(), Error> {
        let cast = concat!(
            "{\"version\": 2, \"width\": 80, \"height\": 24, \"command\": \"cargo test\", ",
            "\"theme\": {\"fg\": \"#c0caf5\", \"bg\": \"#1a1b26\", ",
            "\"palette\": \"#000000:#ff0000:#00ff00:#ffff00:#0000ff:#ff00ff:#00ffff:#ffffff\"}}\n",
            "[0.0, \"o\", \"hi\"]\n",
        );
        let recording = parse(cast)?;

        assert_eq!(recording.title(), Some("cargo test"));
        assert_eq!(
            recording.theme().map(|t| t.fg),
            Some(Rgb {
                r: 0xc0,
                g: 0xca,
                b: 0xf5
            })
        );
        assert_eq!(
            recording.theme().map(|t| t.bg),
            Some(Rgb {
                r: 0x1a,
                g: 0x1b,
                b: 0x26
            })
        );
        // The eight declared palette colours fill slots 0–7; the rest stay unset.
        assert_eq!(
            recording.theme().and_then(|t| t.palette[0]),
            Some(Rgb { r: 0, g: 0, b: 0 })
        );
        assert_eq!(
            recording.theme().and_then(|t| t.palette[7]),
            Some(Rgb {
                r: 0xff,
                g: 0xff,
                b: 0xff
            })
        );
        assert_eq!(recording.theme().and_then(|t| t.palette[8]), None);
        Ok(())
    }

    #[test]
    fn parses_v3_term_object_and_accumulates_relative_intervals() -> Result<(), Error> {
        // v3 times are intervals since the previous event, so absolute
        // timestamps are the running sum: 0.5, then 0.5 + 0.25 = 0.75.
        let cast = concat!(
            "{\"version\": 3, \"term\": {\"cols\": 100, \"rows\": 30}}\n",
            "[0.5, \"o\", \"a\"]\n",
            "[0.25, \"o\", \"b\"]\n",
        );
        let recording = parse(cast)?;

        assert_eq!(recording.size(), TerminalSize::new(100, 30));
        assert_eq!(
            recording.events,
            vec![
                Event::write_at(b"a".to_vec(), Duration::from_millis(500)),
                Event::write_at(b"b".to_vec(), Duration::from_millis(750)),
            ]
        );
        Ok(())
    }

    #[test]
    fn resize_grows_but_never_shrinks() -> Result<(), Error> {
        let cast = concat!(
            "{\"version\": 2, \"width\": 80, \"height\": 24}\n",
            "[0.1, \"r\", \"100x30\"]\n",
            "[0.2, \"r\", \"40x10\"]\n",
            "[0.3, \"r\", \"100x40\"]\n",
        );
        let recording = parse(cast)?;

        // First resize grows to 100x30; the shrink to 40x10 is dropped; the
        // final resize grows rows to 40 while keeping the wider 100 columns.
        assert_eq!(
            recording.events,
            vec![
                Event::resize_at(TerminalSize::new(100, 30), Duration::from_millis(100)),
                Event::resize_at(TerminalSize::new(100, 40), Duration::from_millis(300)),
            ]
        );
        Ok(())
    }

    #[test]
    fn ignores_input_marker_and_exit_events() -> Result<(), Error> {
        let cast = concat!(
            "{\"version\": 3, \"term\": {\"cols\": 80, \"rows\": 24}}\n",
            "[0.1, \"i\", \"ls\"]\n",
            "[0.2, \"m\", \"chapter 1\"]\n",
            "[0.3, \"o\", \"out\"]\n",
            "[0.4, \"x\", \"0\"]\n",
        );
        let recording = parse(cast)?;

        // Only the output event survives; input, marker, and exit are inert.
        // Intervals still accumulate across the ignored events: 0.1+0.2+0.3.
        assert_eq!(
            recording.events,
            vec![Event::write_at(b"out".to_vec(), Duration::from_millis(600))]
        );
        Ok(())
    }

    #[test]
    fn unsupported_v1_is_an_error() {
        let cast =
            "{\"version\": 1, \"width\": 80, \"height\": 24, \"duration\": 1.0, \"stdout\": []}";
        assert!(matches!(
            parse(cast),
            Err(Error::UnsupportedVersion { version: 1 })
        ));
    }

    #[test]
    fn malformed_input_is_a_parse_error() {
        assert!(matches!(parse("not a cast"), Err(Error::Parse(_))));
    }
}
