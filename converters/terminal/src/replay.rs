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
//! Two capture strategies are available:
//!
//! - [`capture`] (and the [`capture_ansi`] convenience wrapper) snapshots the
//!   visible grid after every event. It handles arbitrary output, including
//!   resizes and redraws of earlier rows.
//! - [`capture_windowed`] is a fast path for append-only output: it writes into
//!   an over-tall terminal that never scrolls, snapshots once at the end, and
//!   derives each frame as a [`window_frame`] of that final screen. It avoids
//!   libghostty's expensive viewport scroll but is only faithful when the
//!   recording never rewrites rows the cursor has already passed.
//!
//! The call order below describes the general [`capture`] path:
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

use crate::cell_grid::{self, Cell, CellGrid, GridCapture, TerminalSize, new_terminal};

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

    pub(crate) const fn timestamp(&self) -> Option<Duration> {
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

/// Capture replay frames from append-only recorded output without paying
/// libghostty's expensive viewport scroll.
///
/// Scrolling a short viewport shifts every visible row, so feeding thousands of
/// lines through a small terminal is pathologically slow. This avoids it:
/// `options.size` must be tall enough to hold the whole recording without
/// scrolling (one grid row per output line, plus a little headroom), so writes
/// never scroll. Each frame is then a `viewport_rows`-tall window of the final
/// screen that follows the cursor, reproducing what a scrolling viewport of
/// that height would have shown.
///
/// This is only faithful for output that never rewrites rows the cursor has
/// already passed (no upward cursor motion, absolute positioning, scroll
/// regions, alternate screen, or erase-in-display). Use [`capture`] for
/// recordings that redraw earlier rows.
///
/// Returns the windowed [`Timeline`], matching what [`capture`] would produce
/// for append-only output.
///
/// # Errors
///
/// Returns an error when terminal dimensions are invalid, render-state capture
/// fails, or explicit event timestamps move backwards.
pub fn capture_windowed<'a, I>(
    events: I,
    options: Options,
    viewport_rows: usize,
) -> Result<Timeline, Error>
where
    I: IntoIterator<Item = Event<'a>>,
{
    let size = options.size;
    let viewport_rows = viewport_rows.max(1);

    let mut terminal = new_terminal(size)?;
    let mut capture = GridCapture::new()?;

    // Record where the cursor lands after each write instead of snapshotting the
    // (potentially huge) screen every time. For append-only output the rows
    // below the cursor are still blank, so a single final capture reproduces
    // every intermediate frame once it is windowed to the recorded cursor row.
    let mut current_time = Duration::ZERO;
    let mut markers = Vec::new();
    for event in events {
        current_time = next_timestamp(&event, current_time, options.default_frame_duration)?;
        let Event::Write { bytes, .. } = event else {
            // Resizes and timing boundaries do not occur in append-only ANSI
            // capture; ignore them rather than complicate the fast path.
            continue;
        };
        terminal.vt_write(bytes.as_ref());
        let cursor_x = terminal.cursor_x().map_err(cell_grid::Error::from)?;
        let cursor_y = usize::from(terminal.cursor_y().map_err(cell_grid::Error::from)?);
        // The viewport's bottom row tracks the cursor, exactly as a scrolling
        // terminal would. After a line break the cursor sits on a fresh blank
        // row, so the last row that actually holds content is the one above it;
        // mid-line, the cursor row itself holds content.
        let content_bottom = cursor_y.saturating_sub(usize::from(cursor_x == 0 && cursor_y > 0));
        markers.push((current_time, cursor_y, content_bottom));
    }

    let screen = capture.capture(&terminal, size)?;

    // Seed deduplication with a blank viewport so a leading all-blank frame is
    // dropped, matching `capture`.
    let mut last_window = CellGrid::new(
        vec![Cell::default(); size.cols.saturating_mul(viewport_rows)],
        TerminalSize::new(size.cols, viewport_rows),
    );
    let mut frames = Vec::new();
    for (at, viewport_bottom, content_bottom) in markers {
        let window = window_frame(&screen, viewport_bottom, content_bottom, viewport_rows);
        if window != last_window {
            frames.push(Frame {
                at,
                grid: window.clone(),
            });
            last_window = window;
        }
    }

    Ok(Timeline { frames })
}

/// Extract a `viewport_rows`-tall window of `screen` whose bottom row aligns
/// with `viewport_bottom` (the cursor row, so the window scrolls with output).
/// Rows at or above `content_bottom` are real screen rows; rows below it have
/// not yet been written at this point in the replay, so they are blanked.
fn window_frame(
    screen: &CellGrid,
    viewport_bottom: usize,
    content_bottom: usize,
    viewport_rows: usize,
) -> CellGrid {
    let first_visible = (viewport_bottom + 1).saturating_sub(viewport_rows);
    let mut cells = Vec::with_capacity(screen.cols().saturating_mul(viewport_rows));
    for offset in 0..viewport_rows {
        let row = first_visible + offset;
        match screen.row(row) {
            Some(row_cells) if row <= content_bottom => cells.extend_from_slice(row_cells),
            _ => cells.extend(std::iter::repeat_with(Cell::default).take(screen.cols())),
        }
    }
    CellGrid::new(cells, TerminalSize::new(screen.cols(), viewport_rows))
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
    // a replay preserve a pause even when the screen contents do not change. It
    // only applies once at least one visible frame exists (a leading timing
    // boundary must not emit the initial blank baseline grid) and only when the
    // timestamp advances past that frame (so it never adds a duplicate frame at
    // an identical time).
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

/// Pick at most `budget` evenly spaced indices from `0..item_count`, always
/// including the last index so a sampled sequence still ends on its final state.
///
/// Replay samplers use this to bound how many frames a long recording produces:
/// they keep the items at these indices and fold the dropped items into them.
/// The returned indices are sorted ascending with no duplicates.
#[must_use]
pub fn sampled_indexes(item_count: usize, budget: usize) -> Vec<usize> {
    if item_count == 0 {
        return Vec::new();
    }
    let budget = budget.min(item_count);
    if item_count <= budget {
        return (0..item_count).collect();
    }
    if budget <= 1 {
        return vec![item_count - 1];
    }

    let last_index = item_count - 1;
    let last_slot = budget - 1;
    let mut indexes = Vec::with_capacity(budget);
    let mut previous_index = None;

    for slot in 0..budget {
        let index = if slot == last_slot {
            last_index
        } else {
            slot.saturating_mul(last_index) / last_slot
        };
        if previous_index != Some(index) {
            indexes.push(index);
            previous_index = Some(index);
        }
    }

    indexes
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn sampled_indexes_bounds_count_and_keeps_endpoints() {
        assert!(sampled_indexes(0, 5).is_empty());
        assert_eq!(sampled_indexes(3, 5), vec![0, 1, 2]);
        assert_eq!(sampled_indexes(10, 1), vec![9]);

        let indexes = sampled_indexes(10, 4);
        assert_eq!(indexes.first(), Some(&0));
        assert_eq!(indexes.last(), Some(&9));
        assert!(indexes.len() <= 4);
        assert!(indexes.is_sorted_by(|a, b| a < b), "strictly increasing");
    }

    fn options() -> Options {
        Options::new(TerminalSize::new(8, 3)).with_default_frame_duration(Duration::from_millis(50))
    }

    #[test]
    fn windowed_capture_matches_scrolling_for_append_only() -> TestResult {
        // Drive both paths with identical per-line write events and assert the
        // tall-terminal windowing reproduces the scrolling viewport exactly.
        let events: Vec<Event<'static>> = (0..50)
            .map(|i| {
                let line = format!("\x1b[3{}mline {i}\x1b[0m\r\n", 1 + i % 7);
                Event::write_at(
                    Cow::Owned(line.into_bytes()),
                    Duration::from_millis(10 * (i + 1)),
                )
            })
            .collect();

        let frame_duration = Duration::from_millis(10);
        let scrolling = capture(
            events.clone(),
            Options::new(TerminalSize::new(20, 6)).with_default_frame_duration(frame_duration),
        )?;
        let windowed = capture_windowed(
            events,
            Options::new(TerminalSize::new(20, 60)).with_default_frame_duration(frame_duration),
            6,
        )?;

        assert_eq!(scrolling.frames(), windowed.frames());
        let Some(last) = windowed.frames().last() else {
            return Err(std::io::Error::other("expected replay frames").into());
        };
        // The final newline leaves the cursor on a blank bottom row, so the
        // last content line sits one row above it.
        assert_eq!(last.grid.row_text(4), "line 49");
        assert_eq!(last.grid.row_text(5), "");
        Ok(())
    }

    #[test]
    fn windowed_capture_handles_unterminated_final_line() -> TestResult {
        let events = vec![
            Event::write_at(b"first\r\n".as_slice(), Duration::from_millis(10)),
            Event::write_at(b"second".as_slice(), Duration::from_millis(20)),
        ];

        let windowed = capture_windowed(
            events,
            Options::new(TerminalSize::new(20, 8))
                .with_default_frame_duration(Duration::from_millis(10)),
            4,
        )?;
        let [first, second] = windowed.frames() else {
            return Err(std::io::Error::other("expected two replay frames").into());
        };

        assert_eq!(first.grid.row_text(0), "first");
        assert_eq!(first.grid.row_text(1), "");
        assert_eq!(second.grid.row_text(0), "first");
        assert_eq!(second.grid.row_text(1), "second");
        Ok(())
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
