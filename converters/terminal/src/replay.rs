//! Terminal replay frame capture.
//!
//! This module turns already-recorded terminal data into a sequence of visible
//! screen frames. It deliberately does not execute commands or interpret any
//! recording format as instructions to run a shell; it only feeds captured
//! terminal output bytes/events into `libghostty-vt` and snapshots the visible
//! grid.
//!
//! The main concepts are:
//!
//! - [`Event`]: source-neutral replay input. ANSI chunks, asciicast output
//!   events, and future replay formats can all be lowered into this enum.
//! - `libghostty_vt::Terminal`: the terminal state machine that consumes bytes
//!   and tracks screen state.
//! - [`GridCapture`]: reusable render-state machinery that converts a
//!   `Terminal` snapshot into acdc-owned [`CellGrid`] data.
//! - [`Frame`]: one visible screen at one absolute replay timestamp.
//! - [`Timeline`]: ordered frames ready for a renderer/player.
//!
//! Call order:
//!
//! ```text
//! capture_ansi(chunks, options)
//!   |
//!   | lowers each chunk into Event::Write
//!   v
//! capture(events, options)
//!   |
//!   |-- new_terminal(options.size)
//!   |-- GridCapture::new()
//!   |-- GridCapture::capture(...) -> last_grid
//!   |
//!   `-- for each Event
//!        |
//!        |-- next_timestamp(...)
//!        |
//!        |-- Write:          terminal.vt_write(bytes)
//!        |-- Resize:         terminal.resize(cols, rows)
//!        |-- TimingBoundary: no terminal mutation
//!        |
//!        `-- record_frame(...)
//!             |
//!             |-- GridCapture::capture(...) -> grid
//!             |-- if grid changed: push Frame { at, grid }
//!             |-- if timing boundary: optionally push duplicate grid
//!             `-- last_grid = grid
//!
//! Timeline { frames }
//! ```

use std::{borrow::Cow, time::Duration};

use crate::cell_grid::{self, CellGrid, GridCapture, TerminalSize, new_terminal};

const DEFAULT_FRAME_DURATION: Duration = Duration::from_millis(100);
const DEFAULT_CELL_WIDTH_PX: u32 = 1;
const DEFAULT_CELL_HEIGHT_PX: u32 = 1;

/// Options used while capturing replay frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options {
    /// Initial terminal dimensions.
    pub size: TerminalSize,
    /// Time added for each event that does not provide an explicit timestamp.
    pub default_frame_duration: Duration,
}

impl Options {
    /// Create replay capture options for the given terminal size.
    #[must_use]
    pub const fn new(size: TerminalSize) -> Self {
        Self {
            size,
            default_frame_duration: DEFAULT_FRAME_DURATION,
        }
    }

    /// Override the default duration between untimed replay events.
    #[must_use]
    pub const fn with_default_frame_duration(mut self, duration: Duration) -> Self {
        self.default_frame_duration = duration;
        self
    }
}

impl Default for Options {
    fn default() -> Self {
        Self::new(TerminalSize::default())
    }
}

/// A source-neutral terminal replay event.
///
/// The replay core does not know whether an event came from ANSI chunks,
/// asciicast, or another recording format. Format-specific parsers should
/// translate their input into this enum and let the shared replay loop handle
/// terminal mutation, timestamps, snapshots, and deduplication.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event<'a> {
    /// Feed terminal output bytes into the emulator.
    Write {
        /// VT/ANSI encoded terminal output bytes.
        bytes: Cow<'a, [u8]>,
        /// Optional absolute timestamp for this event.
        at: Option<Duration>,
    },
    /// Resize the terminal before capturing the next frame.
    Resize {
        /// New terminal dimensions.
        size: TerminalSize,
        /// Optional absolute timestamp for this event.
        at: Option<Duration>,
    },
    /// Preserve a timing boundary without changing terminal state.
    ///
    /// Most duplicate visible states are dropped, but players still sometimes
    /// need an unchanged frame to preserve a pause. This event exists for that
    /// case: no bytes are written, but `record_frame` may emit a frame if the
    /// timestamp advances.
    TimingBoundary {
        /// Optional absolute timestamp for this boundary.
        at: Option<Duration>,
    },
}

impl<'a> Event<'a> {
    /// Create a terminal write event without explicit timing metadata.
    #[must_use]
    pub fn write(bytes: impl Into<Cow<'a, [u8]>>) -> Self {
        Self::Write {
            bytes: bytes.into(),
            at: None,
        }
    }

    /// Create a terminal write event with an absolute timestamp.
    #[must_use]
    pub fn write_at(bytes: impl Into<Cow<'a, [u8]>>, at: Duration) -> Self {
        Self::Write {
            bytes: bytes.into(),
            at: Some(at),
        }
    }

    /// Create a resize event without explicit timing metadata.
    #[must_use]
    pub const fn resize(size: TerminalSize) -> Self {
        Self::Resize { size, at: None }
    }

    /// Create a resize event with an absolute timestamp.
    #[must_use]
    pub const fn resize_at(size: TerminalSize, at: Duration) -> Self {
        Self::Resize { size, at: Some(at) }
    }

    /// Create a timing boundary without explicit timing metadata.
    #[must_use]
    pub const fn timing_boundary() -> Self {
        Self::TimingBoundary { at: None }
    }

    /// Create a timing boundary with an absolute timestamp.
    #[must_use]
    pub const fn timing_boundary_at(at: Duration) -> Self {
        Self::TimingBoundary { at: Some(at) }
    }

    const fn timestamp(&self) -> Option<Duration> {
        match self {
            Self::Write { at, .. } | Self::Resize { at, .. } | Self::TimingBoundary { at } => *at,
        }
    }
}

/// One visible terminal replay frame.
///
/// A frame is intentionally simple: it contains only the absolute replay time
/// and the acdc-owned visible grid. Renderers should not need access to
/// `libghostty-vt` internals.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    /// Absolute timestamp for the frame.
    pub at: Duration,
    /// Captured terminal screen for this frame.
    pub grid: CellGrid,
}

/// Captured replay frames in playback order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Timeline {
    frames: Vec<Frame>,
}

impl Timeline {
    /// Return the captured frames.
    #[must_use]
    pub fn frames(&self) -> &[Frame] {
        &self.frames
    }

    /// Consume the timeline and return its frames.
    #[must_use]
    pub fn into_frames(self) -> Vec<Frame> {
        self.frames
    }
}

/// Error returned while capturing replay frames.
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum Error {
    /// Terminal render-state capture failed.
    #[error(transparent)]
    RenderState(#[from] cell_grid::Error),
    /// Event timestamps must not move backwards.
    #[error("replay event timestamp moved backwards from {previous:?} to {next:?}")]
    TimestampOutOfOrder {
        /// Timestamp from the preceding event.
        previous: Duration,
        /// Timestamp from the current event.
        next: Duration,
    },
}

/// Capture replay frames from already-recorded ANSI chunks.
///
/// Untimed chunks are assigned timestamps by adding
/// [`Options::default_frame_duration`] for each chunk.
///
/// # Errors
///
/// Returns an error when terminal dimensions are invalid, render-state capture
/// fails, or explicit event timestamps move backwards.
pub fn capture_ansi<I, B>(chunks: I, options: Options) -> Result<Timeline, Error>
where
    I: IntoIterator<Item = B>,
    B: AsRef<[u8]>,
{
    capture(
        chunks
            .into_iter()
            .map(|chunk| Event::write(Cow::Owned(chunk.as_ref().to_vec()))),
        options,
    )
}

/// Capture replay frames from source-neutral terminal events.
///
/// Consecutive events that leave the visible grid unchanged are deduplicated.
/// A [`Event::TimingBoundary`] can intentionally emit a duplicate visible grid
/// at a later timestamp to preserve a pause.
///
/// # Errors
///
/// Returns an error when terminal dimensions are invalid, render-state capture
/// fails, or explicit event timestamps move backwards.
pub fn capture<'a, I>(events: I, options: Options) -> Result<Timeline, Error>
where
    I: IntoIterator<Item = Event<'a>>,
{
    // `current_size` and `current_time` are the replay cursor. They are updated
    // in event order and copied into emitted frames.
    let mut current_size = options.size;
    let mut current_time = Duration::ZERO;

    // The Ghostty terminal is the mutable emulator state. GridCapture is kept
    // separately so callers only see acdc-owned CellGrid snapshots.
    let mut terminal = new_terminal(current_size)?;
    let mut capture = GridCapture::new()?;

    // Seed `last_grid` with the initial blank screen. The blank screen is not
    // emitted as a frame, but it gives deduplication a baseline for the first
    // event. This keeps a no-op first write from producing a visible frame.
    let mut last_grid = capture.capture(&terminal, current_size)?;
    let mut frames = Vec::new();

    for event in events {
        // Timestamps are absolute in the Timeline. Untimed events move forward
        // by a fixed default so callers can pass plain ANSI chunks and still
        // get an ordered animation.
        current_time = next_timestamp(&event, current_time, options.default_frame_duration)?;

        match event {
            Event::Write { bytes, .. } => {
                // Bytes are replay data, not commands to execute. Ghostty
                // interprets them as terminal output and updates screen state.
                terminal.vt_write(bytes.as_ref());
                record_frame(
                    &mut frames,
                    &mut last_grid,
                    &mut capture,
                    &terminal,
                    current_size,
                    current_time,
                    false,
                )?;
            }
            Event::Resize { size, .. } => {
                // Resize changes both the terminal state and the shape of
                // subsequent CellGrid snapshots. Replay only needs cell-grid
                // dimensions today, so the pixel dimensions are stable
                // placeholders for Ghostty APIs that require size metadata.
                let (cols, rows) = size.as_u16()?;
                terminal
                    .resize(cols, rows, DEFAULT_CELL_WIDTH_PX, DEFAULT_CELL_HEIGHT_PX)
                    .map_err(cell_grid::Error::from)?;
                current_size = size;
                record_frame(
                    &mut frames,
                    &mut last_grid,
                    &mut capture,
                    &terminal,
                    current_size,
                    current_time,
                    false,
                )?;
            }
            Event::TimingBoundary { .. } => {
                // A timing boundary does not mutate the terminal. It only gives
                // record_frame permission to preserve a pause in the output.
                record_frame(
                    &mut frames,
                    &mut last_grid,
                    &mut capture,
                    &terminal,
                    current_size,
                    current_time,
                    true,
                )?;
            }
        }
    }

    Ok(Timeline { frames })
}

fn next_timestamp(
    event: &Event<'_>,
    current_time: Duration,
    default_duration: Duration,
) -> Result<Duration, Error> {
    // Event timestamps are absolute, not deltas. If a source format only gives
    // chunks without timing, synthesize steady spacing with the configured
    // default duration.
    let next = event
        .timestamp()
        .unwrap_or_else(|| current_time.saturating_add(default_duration));

    // Backwards timestamps would make playback order ambiguous, so reject them
    // while preserving the exact pair for structured diagnostics.
    if next < current_time {
        return Err(Error::TimestampOutOfOrder {
            previous: current_time,
            next,
        });
    }

    Ok(next)
}

fn record_frame<'alloc>(
    frames: &mut Vec<Frame>,
    last_grid: &mut CellGrid,
    capture: &mut GridCapture<'alloc>,
    terminal: &libghostty_vt::Terminal<'alloc, '_>,
    size: TerminalSize,
    at: Duration,
    timing_boundary: bool,
) -> Result<(), Error> {
    // Snapshot after the event has mutated terminal state. This keeps frame
    // capture independent from how the event was sourced.
    let grid = capture.capture(terminal, size)?;

    // Most events should only emit a frame when the visible terminal changed.
    // This avoids bloating the timeline with repeated states from redundant
    // writes, cursor-only control sequences not represented in CellGrid, or
    // repeated resizes to the same visible contents.
    let changed = grid != *last_grid;

    // TimingBoundary is the explicit exception to normal deduplication: it lets
    // a replay preserve a pause even when the screen contents do not change.
    // It only applies after at least one visible frame exists; a leading timing
    // boundary should not emit the initial blank baseline grid.
    let preserves_timing = timing_boundary && frames.last().is_some_and(|frame| frame.at != at);

    if changed || preserves_timing {
        frames.push(Frame {
            at,
            grid: grid.clone(),
        });
    }
    *last_grid = grid;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn options() -> Options {
        Options::new(TerminalSize::new(8, 3)).with_default_frame_duration(Duration::from_millis(50))
    }

    #[test]
    fn ansi_chunks_produce_ordered_frames_with_default_timing() -> TestResult {
        let timeline = capture_ansi([b"one".as_slice(), b"\r\ntwo".as_slice()], options())?;
        let frames = timeline.frames();
        let [first, second] = frames else {
            return Err(std::io::Error::other("expected two replay frames").into());
        };

        assert_eq!(first.at, Duration::from_millis(50));
        assert_eq!(first.grid.row_text(0), "one");
        assert_eq!(second.at, Duration::from_millis(100));
        assert_eq!(second.grid.row_text(1), "two");

        Ok(())
    }

    #[test]
    fn identical_visible_states_are_deduplicated() -> TestResult {
        let timeline = capture_ansi([b"foo".as_slice(), b"\rfoo".as_slice()], options())?;
        let frames = timeline.frames();
        let [frame] = frames else {
            return Err(std::io::Error::other("expected one replay frame").into());
        };

        assert_eq!(frame.grid.row_text(0), "foo");

        Ok(())
    }

    #[test]
    fn timing_boundary_can_preserve_a_pause() -> TestResult {
        let timeline = capture(
            [
                Event::write_at(b"foo".as_slice(), Duration::from_secs(1)),
                Event::timing_boundary_at(Duration::from_secs(3)),
            ],
            options(),
        )?;
        let frames = timeline.frames();
        let [first, second] = frames else {
            return Err(std::io::Error::other("expected two replay frames").into());
        };

        assert_eq!(first.at, Duration::from_secs(1));
        assert_eq!(second.at, Duration::from_secs(3));
        assert_eq!(first.grid, second.grid);

        Ok(())
    }

    #[test]
    fn leading_timing_boundary_does_not_emit_blank_frame() -> TestResult {
        let timeline = capture(
            [
                Event::timing_boundary_at(Duration::from_secs(1)),
                Event::write_at(b"foo".as_slice(), Duration::from_secs(2)),
            ],
            options(),
        )?;
        let frames = timeline.frames();
        let [frame] = frames else {
            return Err(std::io::Error::other("expected one replay frame").into());
        };

        assert_eq!(frame.at, Duration::from_secs(2));
        assert_eq!(frame.grid.row_text(0), "foo");

        Ok(())
    }

    #[test]
    fn invalid_dimensions_are_structured_errors() -> TestResult {
        let Err(err) = capture_ansi([b"nope".as_slice()], Options::new(TerminalSize::new(0, 3)))
        else {
            return Err(std::io::Error::other("zero-width terminal size should fail").into());
        };

        assert!(matches!(
            err,
            Error::RenderState(cell_grid::Error::InvalidTerminalSize { cols: 0, rows: 3 })
        ));
        Ok(())
    }

    #[test]
    fn explicit_timestamps_must_be_ordered() -> TestResult {
        let Err(err) = capture(
            [
                Event::write_at(b"first".as_slice(), Duration::from_secs(2)),
                Event::write_at(b"second".as_slice(), Duration::from_secs(1)),
            ],
            options(),
        ) else {
            return Err(std::io::Error::other("timestamps should not move backwards").into());
        };

        assert!(matches!(
            err,
            Error::TimestampOutOfOrder {
                previous,
                next
            } if previous == Duration::from_secs(2) && next == Duration::from_secs(1)
        ));
        Ok(())
    }
}
