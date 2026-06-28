//! Convert asciicast (`.cast`) recordings into source-neutral replay events.
//!
//! Parsing is delegated to the [`asciicast-rs`](https://crates.io/crates/asciicast-rs)
//! crate; this module converts its versioned event model into the shared
//! [`replay`] representation behind an opaque [`Recording`]. A recording is
//! treated as *replay data only*: recorded output bytes are fed to the terminal
//! emulator, while recorded commands, input, markers, and exit status are inert
//! metadata that is never executed.
//!
//! asciicast v2 and v3 are supported (v1 is rejected). Events are pulled lazily
//! from a streaming [`asciicast_rs::Reader`] one line at a time, so a long
//! recording is never fully buffered as a parsed event vector before conversion.
//! The reader's [`absolute_times`](asciicast_rs::Reader::absolute_times) adapter
//! normalises the two versions' timing (v2's absolute timestamps and v3's
//! per-event intervals) into the absolute timestamps the replay core expects.
//! Resize events apply grow-only (a dimension may grow but never shrink), so a
//! replay never loses content that was visible at a larger size.

use std::{io::BufRead, time::Duration};

use acdc_parser::InlineNode;
use asciicast_rs::{Reader, V2, V3, common, v2, v3};

use crate::{
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

/// A parsed asciicast recording, converted into replay events.
///
/// This is the only handle callers hold. Its internals (the initial size and
/// the converted replay events) are private on purpose: a consumer works through
/// [`size`](Self::size), [`duration`](Self::duration), and
/// [`capture`](Self::capture) and never sees the asciicast event model or the
/// `replay` representation it was converted into. That keeps recording sampling
/// and frame capture owned here rather than leaking into call sites.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recording {
    size: TerminalSize,
    events: Vec<Event<'static>>,
    theme: Option<ReplayTheme>,
    title: Option<String>,
}

impl Recording {
    /// Initial terminal size declared by the recording header.
    #[must_use]
    pub const fn size(&self) -> TerminalSize {
        self.size
    }

    /// Total recorded duration: the absolute timestamp of the last event, if any.
    #[must_use]
    pub fn duration(&self) -> Option<Duration> {
        self.events.last().and_then(Event::timestamp)
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

    /// Capture replay frames from the recording.
    ///
    /// Events are first down-sampled to at most `max_frames` terminal writes
    /// (dropped output is folded into the next kept write so nothing is lost),
    /// then fed to the shared replay capture pipeline at `size`.
    /// `default_frame_duration` only spaces untimed events; asciicast events are
    /// always timed, so it has no effect in practice.
    ///
    /// # Errors
    ///
    /// Returns an error when terminal dimensions are invalid, render-state
    /// capture fails, or event timestamps move backwards.
    pub fn capture(
        self,
        size: TerminalSize,
        max_frames: usize,
        default_frame_duration: Duration,
    ) -> Result<replay::Timeline, replay::Error> {
        let events = sample_events(self.events, max_frames);
        let options =
            replay::Options::new(size).with_default_frame_duration(default_frame_duration);
        replay::capture(events, options)
    }
}

/// Error returned when an asciicast recording cannot be parsed.
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum Error {
    /// The recording could not be parsed by `asciicast-rs`.
    #[error(transparent)]
    Parse(#[from] asciicast_rs::Error),
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
        3 => recording_from_v3(v3::stream(input.as_bytes())?, idle_override),
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

/// Build a [`Recording`] from a streaming v2 reader: terminal size, theme, and
/// title come from the header, then each output event becomes a timed terminal
/// write (idle gaps compressed), resizes become grow-only resizes, and input,
/// markers, and exit are ignored. v2 stores absolute timestamps, which
/// `absolute_times` yields directly.
fn recording_from_v2<R: BufRead>(
    reader: Reader<V2, R>,
    idle_override: Option<f64>,
) -> Result<Recording, Error> {
    let header = reader.header();
    let size = TerminalSize::new(header.width.into(), header.height.into());
    let theme = header.theme.as_ref().map(ReplayTheme::from);
    let title = header.title.clone().or_else(|| header.command.clone());
    let mut clock = IdleClock::new(idle_override, header.idle_time_limit);
    let mut current = size;
    let mut events = Vec::new();
    for item in reader.absolute_times() {
        let (time, event) = item?;
        let at = clock.tick(time);
        match event.payload {
            v2::EventPayload::Output(data) => events.push(Event::write_at(data.into_bytes(), at)),
            v2::EventPayload::Resize(resize) => {
                push_resize(&mut events, &mut current, at, resize.cols, resize.rows);
            }
            // Input, markers, and any future payloads are inert replay metadata.
            v2::EventPayload::Input(_) | v2::EventPayload::Marker(_) | _ => {}
        }
    }
    Ok(Recording {
        size,
        events,
        theme,
        title,
    })
}

/// Build a [`Recording`] from a streaming v3 reader, like [`recording_from_v2`]
/// but for v3, whose header nests the terminal info and adds exit events. v3
/// stores per-event intervals, which `absolute_times` accumulates into the
/// absolute timestamps used here.
fn recording_from_v3<R: BufRead>(
    reader: Reader<V3, R>,
    idle_override: Option<f64>,
) -> Result<Recording, Error> {
    let header = reader.header();
    let size = TerminalSize::new(header.term.cols.into(), header.term.rows.into());
    let theme = header.term.theme.as_ref().map(ReplayTheme::from);
    let title = header.title.clone().or_else(|| header.command.clone());
    let mut clock = IdleClock::new(idle_override, header.idle_time_limit);
    let mut current = size;
    let mut events = Vec::new();
    for item in reader.absolute_times() {
        let (time, event) = item?;
        let at = clock.tick(time);
        match event.payload {
            v3::EventPayload::Output(data) => events.push(Event::write_at(data.into_bytes(), at)),
            v3::EventPayload::Resize(resize) => {
                push_resize(&mut events, &mut current, at, resize.cols, resize.rows);
            }
            // Input, markers, exit status, and any future payloads are inert
            // replay metadata.
            v3::EventPayload::Input(_)
            | v3::EventPayload::Marker(_)
            | v3::EventPayload::Exit(_)
            | _ => {}
        }
    }
    Ok(Recording {
        size,
        events,
        theme,
        title,
    })
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

/// Convert a recorded duration in seconds into a [`Duration`], treating
/// non-finite or negative values (which a valid recording never has) as zero.
fn seconds(value: f64) -> Duration {
    Duration::try_from_secs_f64(value).unwrap_or(Duration::ZERO)
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
    idle_limit: f64,
    previous_raw: f64,
    elapsed: f64,
}

impl IdleClock {
    /// Build a clock from the first positive limit among an explicit override
    /// and the header's `idle_time_limit`, falling back to
    /// [`DEFAULT_IDLE_LIMIT_SECS`] when neither is set.
    fn new(idle_override: Option<f64>, header_limit: Option<f64>) -> Self {
        let idle_limit = [idle_override, header_limit]
            .into_iter()
            .flatten()
            .find(|limit| limit.is_finite() && *limit > 0.0)
            .unwrap_or(DEFAULT_IDLE_LIMIT_SECS);
        Self {
            idle_limit,
            previous_raw: 0.0,
            elapsed: 0.0,
        }
    }

    /// Advance to raw absolute time `raw`, returning the compressed timestamp.
    fn tick(&mut self, raw: f64) -> Duration {
        let gap = (raw - self.previous_raw).clamp(0.0, self.idle_limit);
        self.elapsed += gap;
        self.previous_raw = raw;
        seconds(self.elapsed)
    }
}

/// Down-sample timed replay events to at most `max_frames` writes. Output bytes
/// between kept writes are concatenated into the next kept write (so no output is
/// lost) at that write's real timestamp; resize events are always kept and flush
/// any pending bytes before them, preserving ordering and monotonic timing.
fn sample_events(events: Vec<Event<'static>>, max_frames: usize) -> Vec<Event<'static>> {
    let write_count = events
        .iter()
        .filter(|event| matches!(event, Event::Write { .. }))
        .count();
    // Every write fits the budget, so nothing is merged or dropped: hand back the
    // events as-is instead of copying their bytes through the merge buffer below.
    if max_frames >= write_count {
        return events;
    }
    // The kept indices are sorted and unique and write ordinals are visited in
    // order, so a single forward cursor over them replaces a set membership test.
    let mut keep = replay::sampled_indexes(write_count, max_frames)
        .into_iter()
        .peekable();

    let mut result = Vec::new();
    let mut pending: Vec<u8> = Vec::new();
    let mut pending_at = Duration::ZERO;
    let mut write_ordinal = 0;

    for event in events {
        match event {
            Event::Write { bytes, at } => {
                pending.extend_from_slice(&bytes);
                pending_at = at.unwrap_or(pending_at);
                let keep_this = keep.peek() == Some(&write_ordinal);
                if keep_this {
                    keep.next();
                }
                write_ordinal += 1;
                if keep_this && !pending.is_empty() {
                    result.push(Event::write_at(std::mem::take(&mut pending), pending_at));
                }
            }
            Event::Resize { size, at } => {
                if !pending.is_empty() {
                    result.push(Event::write_at(std::mem::take(&mut pending), pending_at));
                }
                result.push(Event::Resize { size, at });
            }
            other @ Event::TimingBoundary { .. } => result.push(other),
        }
    }
    if !pending.is_empty() {
        result.push(Event::write_at(pending, pending_at));
    }
    result
}

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
