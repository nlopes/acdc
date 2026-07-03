//! HTML rendering for terminal blocks.
//!
//! One "terminal screen" renderer (a `libghostty-vt` cell grid captured into
//! acdc-owned rows, then written as styled HTML) drives two modes:
//!
//! - **static** — a single snapshot: `[terminal]` session/literal blocks and
//!   `:acdc-terminal:` source blocks. Rendered by [`render_static`].
//! - **replay** — animated playback of recorded output (raw ANSI or asciicast)
//!   via a small inline JS player ([`render_replay_player`]); the final frame is
//!   server-rendered so no-JS / reduced-motion readers still see it.
//!
//! Both modes share the base CSS class `.terminal-view` (the box, the
//! `.terminal-view--light`/`--dark` theme colours, and the static `.terminal-view__screen`
//! `<pre>`). A replay additionally carries the `.terminal-view--replay` marker (which
//! is also the player's JS hook) and its own inner structure —
//! `.terminal-view__viewport` > `.terminal-view__stream` > `.terminal-view__row`. A recording
//! that carried its own theme is painted with inline colours; otherwise the
//! `.terminal-view--{light,dark}` class colours it. The authoring surface keeps the
//! `acdc-terminal`/`terminal`/`replay` names; only the rendered classes use
//! the `terminal-view` hierarchy.

use std::{borrow::Cow, io::Write};

use acdc_converters_core::{Diagnostics, Options, code::detect_language};
use acdc_converters_terminal::{
    asciicast,
    cell_grid::{Cell, CellDecorations, CellGrid, Rgb, TerminalSize, capture_ansi},
    replay::{self, Options as ReplayOptions},
};
use acdc_parser::{AttributeValue, BlockMetadata, DocumentAttributes, InlineNode};

use crate::Error;

const DEFAULT_COLS: usize = 80;
const AUTO_ROW_PADDING: usize = 1;
const MAX_AUTO_ROWS: usize = 200;
const REPLAY_OPTION: &str = "replay";
const REPLAY_FORMAT_ATTR: &str = "format";
const REPLAY_FRAME_DURATION_MS: u64 = 500;
const REPLAY_FRAME_DURATION: std::time::Duration =
    std::time::Duration::from_millis(REPLAY_FRAME_DURATION_MS);
const REPLAY_DURATION_MS_ATTR: &str = "replay-duration-ms";
const REPLAY_IDLE_LIMIT_MS_ATTR: &str = "replay-idle-limit-ms";
const REPLAY_RENDER_FPS: u128 = 30;
const MAX_REPLAY_RENDER_FRAMES: usize = 120;
const REPLAY_NO_FRAMES_MESSAGE: &str =
    "terminal replay produced no visible frames; rendering a static terminal preview instead";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Theme {
    Dark,
    Light,
}

impl Theme {
    fn from_document_attributes(attrs: &DocumentAttributes<'_>) -> Self {
        if attrs
            .get("dark-mode")
            .is_some_and(|v| !matches!(v, AttributeValue::Bool(false) | AttributeValue::None))
        {
            Self::Dark
        } else {
            Self::Light
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PreviewOptions {
    cols: usize,
    rows: Option<usize>,
    theme: Theme,
}

impl PreviewOptions {
    fn resolve(attrs: &DocumentAttributes<'_>, metadata: Option<&BlockMetadata<'_>>) -> Self {
        let document_cols = attr_usize(attrs.get("acdc-terminal-cols"));
        let document_rows = attr_usize(attrs.get("acdc-terminal-rows"));
        let document_theme = Theme::from_document_attributes(attrs);

        Self {
            cols: metadata
                .and_then(|metadata| {
                    attr_usize(metadata.attributes.get("cols"))
                        .or_else(|| attr_usize(metadata.attributes.get("acdc-terminal-cols")))
                })
                .or(document_cols)
                .unwrap_or(DEFAULT_COLS),
            rows: metadata
                .and_then(|metadata| {
                    attr_usize(metadata.attributes.get("rows"))
                        .or_else(|| attr_usize(metadata.attributes.get("acdc-terminal-rows")))
                })
                .or(document_rows),
            theme: document_theme,
        }
    }

    fn size_for_output(self, ansi: &[u8]) -> TerminalSize {
        TerminalSize::new(
            self.cols,
            self.rows
                .unwrap_or_else(|| estimate_rows(ansi, self.cols).min(MAX_AUTO_ROWS)),
        )
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SpanStyle {
    fg: Option<Rgb>,
    bg: Option<Rgb>,
    decorations: CellDecorations,
}

pub(crate) fn is_enabled(attrs: &DocumentAttributes<'_>) -> bool {
    attrs
        .get("acdc-terminal")
        .is_some_and(|value| !matches!(value, AttributeValue::Bool(false) | AttributeValue::None))
}

pub(crate) fn is_terminal_listing(
    attrs: &DocumentAttributes<'_>,
    metadata: &BlockMetadata<'_>,
) -> bool {
    is_enabled(attrs) && detect_language(metadata).is_some_and(is_terminal_language)
}

pub(crate) fn is_terminal_session(metadata: &BlockMetadata<'_>) -> bool {
    metadata.style == Some("terminal")
}

fn is_terminal_replay(metadata: &BlockMetadata<'_>) -> bool {
    metadata.options.contains(&REPLAY_OPTION) || metadata.attributes.contains_key(REPLAY_OPTION)
}

pub(crate) fn render_listing<W: Write>(
    writer: W,
    inlines: &[InlineNode<'_>],
    metadata: &BlockMetadata<'_>,
    options: Options,
    attrs: &DocumentAttributes<'_>,
) -> Result<(), Error> {
    let preview_options = PreviewOptions::resolve(attrs, None);
    render_with_options(writer, inlines, metadata, options, attrs, preview_options)
}

pub(crate) fn render_session<W: Write>(
    writer: W,
    inlines: &[InlineNode<'_>],
    metadata: &BlockMetadata<'_>,
    options: Options,
    attrs: &DocumentAttributes<'_>,
    diagnostics: &mut Diagnostics<'_>,
) -> Result<(), Error> {
    let preview_options = PreviewOptions::resolve(attrs, Some(metadata));
    if is_terminal_replay(metadata) {
        return render_replay(
            writer,
            inlines,
            metadata,
            options,
            attrs,
            preview_options,
            diagnostics,
        );
    }
    render_with_options(writer, inlines, metadata, options, attrs, preview_options)
}

fn render_with_options<W: Write>(
    mut writer: W,
    inlines: &[InlineNode<'_>],
    metadata: &BlockMetadata<'_>,
    options: Options,
    attrs: &DocumentAttributes<'_>,
    preview_options: PreviewOptions,
) -> Result<(), Error> {
    let ansi = normalize_terminal_newlines(&acdc_converters_terminal::render_listing_to_ansi(
        options,
        attrs.clone(),
        inlines,
        metadata,
        preview_options.cols,
        preview_options.theme == Theme::Dark,
    )?);
    let size = preview_options.size_for_output(&ansi);
    let grid = capture_ansi(&ansi, size)?;

    render_static(&mut writer, &grid, preview_options.theme)?;
    Ok(())
}

/// The recording format a `[terminal%replay]` block carries. Selected by the
/// `format` block attribute; absent defaults to the original raw-ANSI format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReplayFormat {
    Ansi,
    Asciicast,
}

fn replay_format(metadata: &BlockMetadata<'_>, diagnostics: &mut Diagnostics<'_>) -> ReplayFormat {
    let Some(value) = metadata.attributes.get(REPLAY_FORMAT_ATTR) else {
        return ReplayFormat::Ansi;
    };
    match value.to_string().trim().to_ascii_lowercase().as_str() {
        "" | "ansi" => ReplayFormat::Ansi,
        "asciicast" | "cast" => ReplayFormat::Asciicast,
        other => {
            diagnostics.warn(format!(
                "unknown terminal replay `format` value `{other}`; expected `ansi` or `asciicast`. Replaying as raw ANSI."
            ));
            ReplayFormat::Ansi
        }
    }
}

fn render_replay<W: Write>(
    writer: W,
    inlines: &[InlineNode<'_>],
    metadata: &BlockMetadata<'_>,
    options: Options,
    attrs: &DocumentAttributes<'_>,
    preview_options: PreviewOptions,
    diagnostics: &mut Diagnostics<'_>,
) -> Result<(), Error> {
    match replay_format(metadata, diagnostics) {
        ReplayFormat::Ansi => render_replay_ansi(
            writer,
            inlines,
            metadata,
            options,
            attrs,
            preview_options,
            diagnostics,
        ),
        ReplayFormat::Asciicast => render_replay_asciicast(
            writer,
            inlines,
            metadata,
            options,
            attrs,
            preview_options,
            diagnostics,
        ),
    }
}

fn render_replay_ansi<W: Write>(
    mut writer: W,
    inlines: &[InlineNode<'_>],
    metadata: &BlockMetadata<'_>,
    options: Options,
    attrs: &DocumentAttributes<'_>,
    preview_options: PreviewOptions,
    diagnostics: &mut Diagnostics<'_>,
) -> Result<(), Error> {
    let Some(size) = replay_size(attrs, metadata, diagnostics) else {
        return render_with_options(writer, inlines, metadata, options, attrs, preview_options);
    };
    let playback_duration = replay_playback_override(metadata, diagnostics);

    let ansi = normalize_terminal_newlines(&acdc_converters_terminal::render_listing_to_ansi(
        options,
        attrs.clone(),
        inlines,
        metadata,
        size.cols,
        preview_options.theme == Theme::Dark,
    )?);
    let boundaries = chunk_boundaries(&ansi);
    let estimated_playback_ms = REPLAY_FRAME_DURATION
        .as_millis()
        .saturating_mul(boundaries.len() as u128)
        .max(1);
    let playback_ms = playback_duration.map_or(estimated_playback_ms, |duration| {
        duration.as_millis().max(1)
    });
    let events = sampled_events(
        &ansi,
        &boundaries,
        replay_frame_budget(playback_ms),
        REPLAY_FRAME_DURATION,
    );

    // Append-only recordings (logs, build/test output) can be captured into a
    // tall, non-scrolling terminal and windowed per frame, which avoids
    // libghostty's costly viewport scroll. Recordings that redraw earlier rows
    // (progress bars, full-screen TUIs) fall back to the scrolling emulator,
    // which stays fast because such recordings are short.
    let frames = if is_append_only(&ansi) {
        // The capture terminal must be tall enough to hold the whole recording
        // without scrolling. `estimate_rows` accounts for lines that wrap at
        // `cols` into several grid rows (a raw newline count would undercount
        // them and let the "tall" terminal scroll, corrupting windowed frames);
        // counting bytes over-estimates display width, so it never
        // under-allocates.
        let tall = TerminalSize::new(size.cols, estimate_rows(&ansi, size.cols).max(size.rows));
        let replay_options =
            ReplayOptions::new(tall).with_default_frame_duration(REPLAY_FRAME_DURATION);
        replay::capture_windowed(events, replay_options, size.rows)?.into_frames()
    } else {
        let replay_options =
            ReplayOptions::new(size).with_default_frame_duration(REPLAY_FRAME_DURATION);
        replay::capture(events, replay_options)?.into_frames()
    };

    if frames.is_empty() {
        // No visible frames captured: warn and fall back to a static preview of
        // the final screen.
        diagnostics.warn(REPLAY_NO_FRAMES_MESSAGE);
        let grid = capture_ansi(&ansi, size)?;
        return render_static(&mut writer, &grid, preview_options.theme);
    }
    // Raw ANSI carries no recorded theme or title, so the player uses the
    // generic light/dark colours and emits no `data-title`.
    render_replay_player(
        writer,
        &frames,
        preview_options.theme,
        None,
        None,
        playback_duration,
    )
}

fn render_replay_asciicast<W: Write>(
    mut writer: W,
    inlines: &[InlineNode<'_>],
    metadata: &BlockMetadata<'_>,
    options: Options,
    attrs: &DocumentAttributes<'_>,
    preview_options: PreviewOptions,
    diagnostics: &mut Diagnostics<'_>,
) -> Result<(), Error> {
    // `replay-idle-limit-ms` caps dead air (e.g. a build before a test run) so
    // playback dwells on real output; absent, the header or a default applies.
    let idle_limit = positive_attr(
        REPLAY_IDLE_LIMIT_MS_ATTR,
        metadata.attributes.get(REPLAY_IDLE_LIMIT_MS_ATTR),
        diagnostics,
    )
    .map(|ms| std::time::Duration::from_millis(ms as u64));

    let recording = match asciicast::parse_inlines_with(inlines, idle_limit) {
        Ok(recording) => recording,
        Err(err) => {
            diagnostics.warn_with_advice(
                format!(
                    "could not parse asciicast replay: {err}; rendering a static terminal preview instead"
                ),
                "Provide a valid asciicast v2 or v3 recording, or drop `format=asciicast` to replay raw ANSI.",
            );
            return render_with_options(writer, inlines, metadata, options, attrs, preview_options);
        }
    };

    // Block `cols`/`rows` override the recording; the asciicast header supplies
    // the size otherwise, so dimensions are never required on the block.
    let size = replay_size_with_default(attrs, metadata, recording.size(), diagnostics);
    let playback_duration = replay_playback_override(metadata, diagnostics);
    // Header metadata paints the replay chrome; read it before `capture` consumes
    // the recording.
    let recorded_theme = recording.theme();
    let title = recording.title().map(str::to_owned);

    // Replay at the recording's native height so its cursor positioning renders
    // the way it was recorded (a live progress region, full-screen TUI, etc.);
    // capturing straight into a shorter `size.rows` would leave output stuck at
    // the top with blank rows below. The captured frames are then windowed to
    // `size.rows` for display (below).
    let capture_size = TerminalSize::new(size.cols, recording.size().rows.max(size.rows));

    // Capture every distinct screen the session produced (deduplicated, but never
    // sampled): a terminal session may rewrite the screen in place (progress
    // bars, status lines, full-screen TUIs), so faithful playback needs each
    // visible state, not a scrolled window of one transcript. Capture errors are
    // caught here because the events come from user-supplied cast data, not acdc.
    let timeline = match recording.capture(capture_size) {
        Ok(timeline) => timeline,
        Err(err) => {
            diagnostics.warn(format!(
                "asciicast replay capture failed: {err}; rendering a static terminal preview instead"
            ));
            return render_blank_preview(&mut writer, size, preview_options.theme);
        }
    };

    // Show a `size.rows`-tall window of each frame, anchoring the last line with
    // content to the bottom so the replay follows the output like a scrolling
    // terminal (and rests on the final lines, not a region the recording cleared).
    let frames = window_frames_to_bottom(timeline.into_frames(), size.rows);

    if frames.is_empty() {
        diagnostics.warn(REPLAY_NO_FRAMES_MESSAGE);
        return render_blank_preview(&mut writer, size, preview_options.theme);
    }

    render_replay_player(
        writer,
        &frames,
        preview_options.theme,
        recorded_theme,
        title.as_deref(),
        playback_duration,
    )
}

/// Window every captured frame to `display_rows`, anchoring each frame's last
/// line with content to the bottom. The replay then follows the output like a
/// scrolling terminal instead of leaving content stuck at the top, and rests on
/// the final lines rather than a region the recording erased at the end.
fn window_frames_to_bottom(frames: Vec<replay::Frame>, display_rows: usize) -> Vec<replay::Frame> {
    let display_rows = display_rows.max(1);
    frames
        .into_iter()
        .map(|frame| replay::Frame {
            at: frame.at,
            grid: window_grid_to_bottom(&frame.grid, display_rows),
        })
        .collect()
}

/// Extract a `display_rows`-tall window of `grid` whose bottom is the last row
/// holding visible content (trailing blank rows are dropped). When the content
/// is shorter than `display_rows` it sits at the top with blank rows below, like
/// fresh terminal output; once it exceeds the window the oldest rows scroll off
/// the top.
fn window_grid_to_bottom(grid: &CellGrid, display_rows: usize) -> CellGrid {
    let cols = grid.cols();
    let content_end = (0..grid.rows())
        .rev()
        .find(|&row| {
            grid.row(row)
                .is_some_and(|cells| cells.iter().any(|cell| !cell.is_blank()))
        })
        .map_or(0, |row| row + 1);
    let start = content_end.saturating_sub(display_rows);
    let mut cells = Vec::with_capacity(cols.saturating_mul(display_rows));
    for offset in 0..display_rows {
        let row = start + offset;
        match grid.row(row) {
            Some(row_cells) if row < content_end => cells.extend_from_slice(row_cells),
            _ => cells.extend(std::iter::repeat_with(Cell::default).take(cols)),
        }
    }
    CellGrid::new(cells, TerminalSize::new(cols, display_rows))
}

/// The `replay-duration-ms` playback override, if set to a positive integer.
fn replay_playback_override(
    metadata: &BlockMetadata<'_>,
    diagnostics: &mut Diagnostics<'_>,
) -> Option<std::time::Duration> {
    positive_attr(
        REPLAY_DURATION_MS_ATTR,
        metadata.attributes.get(REPLAY_DURATION_MS_ATTR),
        diagnostics,
    )
    .map(|ms| std::time::Duration::from_millis(ms as u64))
}

/// Resolve the replay terminal size, letting block `cols`/`rows` (or the
/// `acdc-terminal-cols`/`acdc-terminal-rows` block/document attributes) override
/// `default`. Unlike [`replay_size`], a missing size is not a warning here
/// because the asciicast header supplies the default.
fn replay_size_with_default(
    attrs: &DocumentAttributes<'_>,
    metadata: &BlockMetadata<'_>,
    default: TerminalSize,
    diagnostics: &mut Diagnostics<'_>,
) -> TerminalSize {
    let cols = replay_dimension(attrs, metadata, "cols", "acdc-terminal-cols", diagnostics)
        .unwrap_or(default.cols);
    let rows = replay_dimension(attrs, metadata, "rows", "acdc-terminal-rows", diagnostics)
        .unwrap_or(default.rows);
    TerminalSize::new(cols, rows)
}

/// Resolve one replay dimension by checking, in order: the block's `primary`
/// attribute (`cols`/`rows`), the block's `document` attribute
/// (`acdc-terminal-cols`/`acdc-terminal-rows`), then the document attribute of the same
/// name. Returns `None` when none is set; a present but non-positive value is
/// warned about by [`positive_attr`].
fn replay_dimension(
    attrs: &DocumentAttributes<'_>,
    metadata: &BlockMetadata<'_>,
    primary: &'static str,
    document: &'static str,
    diagnostics: &mut Diagnostics<'_>,
) -> Option<usize> {
    positive_attr(primary, metadata.attributes.get(primary), diagnostics)
        .or_else(|| positive_attr(document, metadata.attributes.get(document), diagnostics))
        .or_else(|| positive_attr(document, attrs.get(document), diagnostics))
}

fn render_blank_preview<W: Write>(
    writer: &mut W,
    size: TerminalSize,
    theme: Theme,
) -> Result<(), Error> {
    let grid = capture_ansi(&[], size)?;
    render_static(writer, &grid, theme)
}

fn replay_size(
    attrs: &DocumentAttributes<'_>,
    metadata: &BlockMetadata<'_>,
    diagnostics: &mut Diagnostics<'_>,
) -> Option<TerminalSize> {
    let cols = replay_dimension(attrs, metadata, "cols", "acdc-terminal-cols", diagnostics);
    let rows = replay_dimension(attrs, metadata, "rows", "acdc-terminal-rows", diagnostics);

    if let (Some(cols), Some(rows)) = (cols, rows) {
        Some(TerminalSize::new(cols, rows))
    } else {
        diagnostics.warn_with_advice(
            "terminal replay requires positive `cols` and `rows` block attributes or `acdc-terminal-cols` and `acdc-terminal-rows` document attributes; rendering a static terminal preview instead",
            "Use a replay block such as `[terminal%replay,cols=80,rows=24]`.",
        );
        None
    }
}

fn positive_attr(
    name: &'static str,
    value: Option<&AttributeValue<'_>>,
    diagnostics: &mut Diagnostics<'_>,
) -> Option<usize> {
    let value = value?;
    let parsed = attr_usize(Some(value));
    if parsed.is_none() {
        diagnostics.warn(format!(
            "terminal replay attribute `{name}` must be a positive integer, got `{value}`"
        ));
    }
    parsed
}

/// Byte offsets where each replay chunk ends. Chunks are split on line feeds
/// (kept with the line) and on bare carriage returns (which begin an in-place
/// refresh), matching how a terminal advances between visible updates.
fn chunk_boundaries(ansi: &[u8]) -> Vec<usize> {
    let mut boundaries = Vec::new();
    let mut start = 0;

    for (index, byte) in ansi.iter().copied().enumerate() {
        match byte {
            b'\n' => {
                boundaries.push(index + 1);
                start = index + 1;
            }
            b'\r' if ansi.get(index + 1) != Some(&b'\n') => {
                if start < index {
                    boundaries.push(index);
                }
                start = index;
            }
            _ => {}
        }
    }

    if start < ansi.len() {
        boundaries.push(ansi.len());
    }

    boundaries
}

/// Build at most `frame_budget` replay events by sampling chunk boundaries and
/// referencing the contiguous bytes between sampled boundaries directly (no
/// copies). Timestamps reflect each sampled chunk's position in the recording.
fn sampled_events<'a>(
    ansi: &'a [u8],
    boundaries: &[usize],
    frame_budget: usize,
    frame_duration: std::time::Duration,
) -> Vec<replay::Event<'a>> {
    let sampled = replay::sampled_indexes(boundaries.len(), frame_budget);
    let mut events = Vec::with_capacity(sampled.len());
    let mut start = 0;

    for index in sampled {
        let Some(&end) = boundaries.get(index) else {
            continue;
        };
        if start < end
            && let Some(bytes) = ansi.get(start..end)
        {
            events.push(replay::Event::write_at(
                Cow::Borrowed(bytes),
                replay_chunk_timestamp(index, frame_duration),
            ));
        }
        start = end;
    }

    events
}

fn replay_chunk_timestamp(
    chunk_index: usize,
    frame_duration: std::time::Duration,
) -> std::time::Duration {
    frame_duration.saturating_mul(u32::try_from(chunk_index + 1).unwrap_or(u32::MAX))
}

/// Whether the recorded output only ever appends below the cursor, so it can be
/// captured with the fast tall-terminal windowing path. Output is append-only
/// when it contains no bare carriage returns (in-place line refreshes) and no
/// escape sequences other than SGR styling (`ESC [ ... m`) and OSC sequences
/// (hyperlinks/titles), which never move the cursor up or erase the screen.
fn is_append_only(ansi: &[u8]) -> bool {
    let mut index = 0;
    while let Some(&byte) = ansi.get(index) {
        match byte {
            b'\r' if ansi.get(index + 1) != Some(&b'\n') => return false,
            0x1b => match ansi.get(index + 1) {
                Some(b'[') => {
                    let mut end = index + 2;
                    while ansi.get(end).is_some_and(|b| !(0x40..=0x7e).contains(b)) {
                        end += 1;
                    }
                    if ansi.get(end) != Some(&b'm') {
                        return false;
                    }
                    index = end + 1;
                }
                Some(b']') => index = skip_osc(ansi, index + 2),
                _ => return false,
            },
            _ => index += 1,
        }
    }
    true
}

/// Advance past an OSC sequence body starting at `start`, stopping after a BEL
/// or ST (`ESC \`) terminator. Returns the index just past the terminator.
fn skip_osc(ansi: &[u8], start: usize) -> usize {
    let mut index = start;
    while let Some(&byte) = ansi.get(index) {
        match byte {
            0x07 => return index + 1,
            0x1b if ansi.get(index + 1) == Some(&b'\\') => return index + 2,
            _ => index += 1,
        }
    }
    index
}

fn is_terminal_language(language: &str) -> bool {
    matches!(
        language,
        "console"
            | "terminal"
            | "shell"
            | "sh"
            | "bash"
            | "zsh"
            | "fish"
            | "powershell"
            | "ps1"
            | "cmd"
    )
}

fn normalize_terminal_newlines(ansi: &[u8]) -> Vec<u8> {
    let mut normalized = Vec::with_capacity(ansi.len());
    let mut previous = None;
    for byte in ansi {
        if *byte == b'\n' && previous != Some(b'\r') {
            normalized.push(b'\r');
        }
        normalized.push(*byte);
        previous = Some(*byte);
    }
    normalized
}

fn render_static<W: Write>(writer: &mut W, grid: &CellGrid, theme: Theme) -> Result<(), Error> {
    // Colours come from the `.terminal-view--{light,dark}` stylesheet classes, so they
    // are overridable with plain CSS (no inline style to fight).
    write!(
        writer,
        "<div class=\"terminal-view terminal-view--{}\" data-cols=\"{}\" data-rows=\"{}\">",
        theme.as_str(),
        grid.cols(),
        grid.rows()
    )?;
    writeln!(
        writer,
        "<pre class=\"terminal-view__screen\" aria-label=\"Terminal preview\">"
    )?;
    for (row_index, row) in grid.rows_iter().enumerate() {
        render_row(writer, row)?;
        if row_index + 1 < grid.rows() {
            writeln!(writer)?;
        }
    }
    writeln!(writer, "</pre>")?;
    writeln!(writer, "</div>")?;
    Ok(())
}

/// The terminal-replay player `<script>`, wrapping the JavaScript in
/// `static/terminal-replay-player.js` (embedded at compile time). Exposed so an
/// embedded-mode consumer can include it themselves; its CSP `script-src` hash is
/// [`REPLAY_PLAYER_SCRIPT_CSP_HASH`].
pub const REPLAY_PLAYER_SCRIPT: &str = concat!(
    "<script>",
    include_str!("../static/terminal-replay-player.js"),
    "</script>"
);

/// CSP `script-src` source for [`REPLAY_PLAYER_SCRIPT`]'s inline code, so a
/// consumer can allowlist the player under a strict policy without
/// `'unsafe-inline'` (e.g. `script-src 'self' 'sha256-...'`). Computed in
/// `build.rs` as the sha256 of `static/terminal-replay-player.js`, so it always
/// matches the embedded script.
pub const REPLAY_PLAYER_SCRIPT_CSP_HASH: &str = env!("ACDC_REPLAY_PLAYER_CSP_HASH");

/// Render a recording as an interactive replay player. This is the single
/// renderer for every replay block (raw ANSI and asciicast alike).
///
/// Every distinct visible screen is captured, but identical rows are pooled and
/// emitted once; each frame is just a list of indices into that pool plus a
/// timestamp. A small inline script swaps the visible rows on a clock. This
/// reproduces in-place rewrites (progress bars, status lines, full-screen TUIs)
/// faithfully and smoothly while keeping the payload compact: a long session
/// costs one copy of each unique line, not one screen per frame.
///
/// The player wears the recording's own colours when its header carried a theme
/// (raw ANSI has none, so it uses the generic light/dark colours). The final
/// frame is rendered into the DOM server-side, so readers with scripting
/// disabled (or who prefer reduced motion) see the finished screen; only the
/// animation is lost.
///
/// # CSS contract
///
/// acdc emits no window chrome; the markup is class-based so a consumer (e.g. an
/// embedding editor) can add its own. The stable seams are the base class
/// `.terminal`, the `.terminal-view--replay` marker (also the player's JS hook), the
/// inner `.terminal-view__viewport` > `.terminal-view__stream` > `.terminal-view__row`, and the
/// `data-title` attribute (the recorded title, present only when the recording
/// carried one, for building custom chrome via `::before{content:attr(data-title)}`).
/// A theme-less replay takes its colours from the `.terminal-view--{light,dark}` class
/// (overridable with plain CSS); a recording's own theme is painted inline
/// (override it with `!important`).
///
/// Per-cell colours are emitted as inline `style` colours (resolved against the
/// recording's own palette via [`resolve_cell_color`]), matching the static
/// terminal preview. Under a strict CSP these inline styles need
/// `style-src 'unsafe-inline'`; the inline player `<script>` can be allowlisted
/// by hash ([`REPLAY_PLAYER_SCRIPT_CSP_HASH`]) for a script-src without
/// `'unsafe-inline'`. With the script blocked, the server-rendered final frame
/// still shows.
fn render_replay_player<W: Write>(
    mut writer: W,
    frames: &[replay::Frame],
    theme: Theme,
    recorded: Option<asciicast::ReplayTheme>,
    title: Option<&str>,
    playback_duration: Option<std::time::Duration>,
) -> Result<(), Error> {
    let rows = frames
        .iter()
        .map(|frame| frame.grid.rows())
        .max()
        .unwrap_or(1);
    let cols = frames
        .iter()
        .map(|frame| frame.grid.cols())
        .max()
        .unwrap_or(0);

    // Pool unique rendered rows; each frame becomes a list of indices into the
    // pool. Identical lines across frames (most of them) are stored once. Cell
    // colours are resolved against the recording's own palette (when it carried
    // one) so the replay shows the colours it was recorded with.
    let palette = recorded.map_or([None; 16], |theme| theme.palette);
    let (pool, frame_rows) = build_row_pool(frames, rows, &palette)?;

    // Frame times scale to `replay-duration-ms` when set, else play at the
    // recording's own (idle-compressed) pace.
    let record_ms = frames.last().map_or(0, |frame| frame.at.as_millis());
    let total_ms = playback_duration
        .map_or(record_ms, |d| d.as_millis())
        .max(1);
    let times: Vec<u128> = frames
        .iter()
        .map(|frame| {
            frame
                .at
                .as_millis()
                .saturating_mul(total_ms)
                .checked_div(record_ms)
                .unwrap_or(0)
        })
        .collect();

    // A recording that carried its own theme is painted faithfully with an inline
    // background/foreground (its palette also colours the cells). Without one (raw
    // ANSI, or a theme-less cast) the generic `.terminal-view--{light,dark}`
    // class colours it, so no inline style is emitted and a consumer can restyle
    // it with plain CSS.
    let colour_style = recorded.map_or_else(String::new, |t| {
        format!(
            " style=\"background-color:{};color:{}\"",
            rgb_css(t.bg),
            rgb_css(t.fg)
        )
    });

    // The replay rests on the full terminal height, so the configured `rows` is
    // also the minimum number of lines shown at the end of the run: the box
    // stays `rows` tall instead of collapsing to the final line's content.
    let final_rows = rows;

    // acdc emits no window chrome; the recorded title, when present, is exposed as
    // `data-title` so a consumer can render its own chrome in CSS
    // (`::before{content:attr(data-title)}`).
    let title_attr = title.map_or_else(String::new, |title| {
        format!(" data-title=\"{}\"", escape_html(title))
    });
    writeln!(
        writer,
        "<div class=\"terminal-view terminal-view--replay terminal-view--{}\"{} data-cols=\"{}\" data-rows=\"{}\" data-frames=\"{}\" data-duration-ms=\"{}\"{}>",
        theme.as_str(),
        colour_style,
        cols,
        rows,
        frames.len(),
        total_ms,
        title_attr
    )?;

    // The screen holds one block per terminal row. The final frame is rendered
    // server-side (trimmed of trailing blank rows) so it shows without scripting;
    // the player builds the rest of the rows, resets to the first frame, and
    // animates from there.
    writeln!(
        writer,
        "<div class=\"terminal-view__viewport\"><div class=\"terminal-view__stream\" aria-label=\"Terminal replay\">"
    )?;
    let final_frame = frame_rows.last().map_or(&[] as &[usize], Vec::as_slice);
    for index in final_frame.iter().take(final_rows) {
        writeln!(
            writer,
            "<div class=\"terminal-view__row\">{}</div>",
            pool.get(*index).map_or("", String::as_str)
        )?;
    }
    writeln!(writer, "</div></div>")?;

    // Payload as inline JSON. `<` is escaped to `<` inside strings, so no
    // recorded content can break out of the script element.
    write!(
        writer,
        "<script type=\"application/json\" class=\"terminal-view__data\">"
    )?;
    write_replay_json(
        &mut writer,
        &ReplayData {
            cols,
            rows,
            final_rows,
            total_ms,
            times: &times,
            pool: &pool,
            frame_rows: &frame_rows,
        },
    )?;
    writeln!(writer, "</script>")?;
    writeln!(writer, "</div>")?;
    writeln!(writer, "{REPLAY_PLAYER_SCRIPT}")?;
    Ok(())
}

/// Pool the unique rendered rows across all frames and map each frame to a list
/// of `rows` indices into that pool. Identical lines (the bulk of a session) are
/// stored once; short frames are padded with blank rows.
fn build_row_pool(
    frames: &[replay::Frame],
    rows: usize,
    palette: &[Option<Rgb>; 16],
) -> Result<(Vec<String>, Vec<Vec<usize>>), Error> {
    // Each rendered row lives once, as a key in `index` mapping its HTML to its
    // assigned pool slot. The pool itself is rebuilt from the map afterwards,
    // placing each row at its slot, so a unique row's HTML is never cloned.
    let mut index: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut frame_rows: Vec<Vec<usize>> = Vec::with_capacity(frames.len());
    for frame in frames {
        let mut indices = Vec::with_capacity(rows);
        for row in 0..rows {
            let html = match frame.grid.row(row) {
                Some(cells) => row_to_html(cells, palette)?,
                None => String::new(),
            };
            let next = index.len();
            indices.push(*index.entry(html).or_insert(next));
        }
        frame_rows.push(indices);
    }
    let mut pool = vec![String::new(); index.len()];
    for (html, slot) in index {
        if let Some(entry) = pool.get_mut(slot) {
            *entry = html;
        }
    }
    Ok((pool, frame_rows))
}

/// Render one terminal row to its HTML string for the player pool, using the
/// same inline-styled markup as the static preview's [`render_row`] but with
/// cell colours resolved against the recording's own palette (see
/// [`resolve_cell_color`]).
fn row_to_html(row: &[Cell], palette: &[Option<Rgb>; 16]) -> Result<String, Error> {
    let mut buffer: Vec<u8> = Vec::new();
    render_row_with(&mut buffer, row, palette)?;
    Ok(String::from_utf8(buffer).unwrap_or_default())
}

/// Resolve a cell colour for the player: a palette index maps to the recording's
/// own palette colour when present, so the replay shows the colours it was
/// recorded with (libghostty resolves indices against its built-in palette;
/// `palette` from the cast header overrides that). Truecolor / 256-colour cells
/// have no index and use their already-resolved RGB.
fn resolve_cell_color(
    index: Option<u8>,
    rgb: Option<Rgb>,
    palette: &[Option<Rgb>; 16],
) -> Option<Rgb> {
    match index {
        Some(slot) => palette.get(usize::from(slot)).copied().flatten().or(rgb),
        None => rgb,
    }
}

/// The replay player's payload, ready to serialise as JSON.
struct ReplayData<'a> {
    cols: usize,
    rows: usize,
    final_rows: usize,
    total_ms: u128,
    times: &'a [u128],
    pool: &'a [String],
    frame_rows: &'a [Vec<usize>],
}

/// Write the player payload as JSON: the unique-row pool, per-frame row indices,
/// and frame times.
fn write_replay_json<W: Write>(writer: &mut W, data: &ReplayData<'_>) -> Result<(), Error> {
    let ReplayData {
        cols,
        rows,
        final_rows,
        total_ms,
        times,
        pool,
        frame_rows,
    } = *data;
    write!(
        writer,
        "{{\"cols\":{cols},\"rows\":{rows},\"finalRows\":{final_rows},\"durationMs\":{total_ms},\"times\":["
    )?;
    for (position, time) in times.iter().enumerate() {
        if position > 0 {
            write!(writer, ",")?;
        }
        write!(writer, "{time}")?;
    }
    write!(writer, "],\"pool\":[")?;
    for (position, html) in pool.iter().enumerate() {
        if position > 0 {
            write!(writer, ",")?;
        }
        write!(writer, "{}", json_string(html))?;
    }
    write!(writer, "],\"frames\":[")?;
    for (position, indices) in frame_rows.iter().enumerate() {
        if position > 0 {
            write!(writer, ",")?;
        }
        write!(writer, "[")?;
        for (slot, index) in indices.iter().enumerate() {
            if slot > 0 {
                write!(writer, ",")?;
            }
            write!(writer, "{index}")?;
        }
        write!(writer, "]")?;
    }
    write!(writer, "]}}")?;
    Ok(())
}

/// Quote and escape `value` as a JSON string. `<` becomes `<` so embedding
/// the JSON inside a `<script>` element can never be broken out of.
fn json_string(value: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '<' => out.push_str("\\u003c"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn replay_frame_budget(playback_ms: u128) -> usize {
    let budget = playback_ms
        .saturating_mul(REPLAY_RENDER_FPS)
        .div_ceil(1000)
        .saturating_add(1)
        .try_into()
        .unwrap_or(MAX_REPLAY_RENDER_FRAMES);
    budget.clamp(2, MAX_REPLAY_RENDER_FRAMES)
}

/// The empty palette used by the static preview: libghostty has already resolved
/// every palette index to RGB, so there is nothing to re-resolve against.
const NO_PALETTE: [Option<Rgb>; 16] = [None; 16];

/// Render a terminal row as inline-styled spans, resolving each cell's colours
/// against `palette` (see [`resolve_cell_color`]). The static preview passes an
/// empty [`NO_PALETTE`] (the cells already carry resolved RGB); the replay player
/// passes the recording's own palette so it shows the colours it was recorded
/// with.
fn render_row_with<W: Write>(
    writer: &mut W,
    row: &[Cell],
    palette: &[Option<Rgb>; 16],
) -> Result<(), Error> {
    let mut current_style = SpanStyle::default();
    let mut buffer = String::new();
    let mut has_open_span = false;

    for cell in row {
        let style = SpanStyle {
            fg: resolve_cell_color(cell.fg_index, cell.fg, palette),
            bg: resolve_cell_color(cell.bg_index, cell.bg, palette),
            decorations: cell.decorations,
        };
        if style != current_style {
            flush_span(writer, &buffer, current_style, has_open_span)?;
            buffer.clear();
            current_style = style;
            has_open_span = style != SpanStyle::default();
        }
        let text = if cell.text.is_empty() {
            " "
        } else {
            cell.text.as_str()
        };
        buffer.push_str(text);
    }

    flush_span(writer, buffer.trim_end(), current_style, has_open_span)?;
    Ok(())
}

fn render_row<W: Write>(writer: &mut W, row: &[Cell]) -> Result<(), Error> {
    render_row_with(writer, row, &NO_PALETTE)
}

fn flush_span<W: Write>(
    writer: &mut W,
    text: &str,
    style: SpanStyle,
    has_open_span: bool,
) -> Result<(), Error> {
    if text.is_empty() {
        return Ok(());
    }

    if has_open_span {
        write!(writer, "<span style=\"{}\">", style_attr(style))?;
    }
    write!(writer, "{}", escape_html(text))?;
    if has_open_span {
        write!(writer, "</span>")?;
    }
    Ok(())
}

fn attr_usize(value: Option<&AttributeValue<'_>>) -> Option<usize> {
    value
        .map(ToString::to_string)
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
}

fn estimate_rows(ansi: &[u8], cols: usize) -> usize {
    let cols = cols.max(1);
    let mut rows = 1;
    let mut col = 0;
    let mut bytes = ansi.iter().copied().peekable();

    while let Some(byte) = bytes.next() {
        match byte {
            b'\x1b' => skip_escape_sequence(&mut bytes),
            b'\r' => col = 0,
            b'\n' => {
                rows += 1;
                col = 0;
            }
            b'\t' => {
                let width = 4 - (col % 4);
                advance_columns(&mut rows, &mut col, cols, width);
            }
            0x00..=0x1f | 0x7f => {}
            _ => advance_columns(&mut rows, &mut col, cols, 1),
        }
    }

    rows.saturating_add(AUTO_ROW_PADDING).max(1)
}

fn advance_columns(rows: &mut usize, col: &mut usize, cols: usize, width: usize) {
    let mut remaining = width;
    while remaining > 0 {
        if *col == cols {
            *rows += 1;
            *col = 0;
        }

        let available = cols - *col;
        let step = remaining.min(available);
        *col += step;
        remaining -= step;
    }
}

fn skip_escape_sequence<I>(bytes: &mut std::iter::Peekable<I>)
where
    I: Iterator<Item = u8>,
{
    match bytes.peek().copied() {
        Some(b'[') => {
            bytes.next();
            for byte in bytes.by_ref() {
                if (0x40..=0x7e).contains(&byte) {
                    break;
                }
            }
        }
        Some(b']') => {
            bytes.next();
            let mut previous = None;
            for byte in bytes.by_ref() {
                if byte == b'\x07' || (previous == Some(b'\x1b') && byte == b'\\') {
                    break;
                }
                previous = Some(byte);
            }
        }
        Some(_) | None => {}
    }
}

fn style_attr(style: SpanStyle) -> String {
    let mut css = String::new();
    if let Some(fg) = style.fg {
        push_decl(&mut css, "color", &rgb_css(fg));
    }
    if let Some(bg) = style.bg {
        push_decl(&mut css, "background-color", &rgb_css(bg));
    }
    if style.decorations.bold {
        push_decl(&mut css, "font-weight", "700");
    }
    if style.decorations.italic {
        push_decl(&mut css, "font-style", "italic");
    }
    if style.decorations.underline && style.decorations.strikethrough {
        push_decl(&mut css, "text-decoration", "underline line-through");
    } else if style.decorations.underline {
        push_decl(&mut css, "text-decoration", "underline");
    } else if style.decorations.strikethrough {
        push_decl(&mut css, "text-decoration", "line-through");
    }
    if style.decorations.dim {
        push_decl(&mut css, "opacity", "0.72");
    }
    if style.decorations.inverse {
        push_decl(&mut css, "filter", "invert(1)");
    }
    css
}

fn push_decl(css: &mut String, property: &str, value: &str) {
    if !css.is_empty() {
        css.push(';');
    }
    css.push_str(property);
    css.push(':');
    css.push_str(value);
}

fn rgb_css(rgb: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", rgb.r, rgb.g, rgb.b)
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use acdc_converters_core::{
        Diagnostics, GeneratorMetadata, Options as ConverterOptions, Warning, WarningSource,
    };
    use acdc_parser::Options as ParserOptions;

    use crate::{Processor, RenderOptions};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn render(
        input: &str,
        variant: crate::HtmlVariant,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let (html, _) = render_with_warnings(input, variant)?;
        Ok(html)
    }

    fn render_with_warnings(
        input: &str,
        variant: crate::HtmlVariant,
    ) -> Result<(String, Vec<Warning>), Box<dyn std::error::Error>> {
        render_with_safe_mode(input, variant, acdc_parser::SafeMode::Unsafe)
    }

    fn render_with_safe_mode(
        input: &str,
        variant: crate::HtmlVariant,
        safe_mode: acdc_parser::SafeMode,
    ) -> Result<(String, Vec<Warning>), Box<dyn std::error::Error>> {
        let parser_options =
            ParserOptions::with_attributes(acdc_converters_core::default_rendering_attributes());
        let parsed = acdc_parser::parse(input, &parser_options)?;
        let doc = parsed.document();
        let options = ConverterOptions::builder()
            .generator_metadata(GeneratorMetadata::new("acdc", "0.1.0"))
            .safe_mode(safe_mode)
            .build();
        let processor = Processor::new_with_variant(options, doc.attributes.clone(), variant);
        let mut output = Vec::new();
        let source = WarningSource::new("html").with_variant(variant.as_str());
        let mut warnings = Vec::new();
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        processor.convert_to_writer(
            doc,
            &mut output,
            &RenderOptions::default(),
            &mut diagnostics,
        )?;
        Ok((String::from_utf8(output)?, warnings))
    }

    #[test]
    fn standard_html_can_include_selectable_terminal_preview() -> TestResult {
        let html = render(
            "= Example\n:acdc-terminal:\n\n[source,console]\n----\n$ acdc --version\nacdc 0.2.0\n----\n\nAfter preview.\n",
            crate::HtmlVariant::Standard,
        )?;

        assert!(html.contains("<div id=\"content\">"));
        assert!(html.contains("<div class=\"listingblock terminal-block\">"));
        assert!(html.contains("<div class=\"terminal-view terminal-view--light\""));
        // Preview colours come from the stylesheet class, not an inline style.
        assert!(html.contains(".terminal-view--light{background-color:#f6f8fa;color:#1f2328}"));
        assert!(html.contains("terminal-view--light\" data-cols="));
        assert!(html.contains("$</span>"));
        assert!(html.contains(" acdc"));
        assert!(html.contains("--version"));
        assert!(html.contains("0.2.0"));
        let preview_offset = html
            .find("terminal-view terminal-view--")
            .ok_or("missing terminal preview")?;
        let after_offset = html
            .find("After preview.")
            .ok_or("missing following paragraph")?;
        assert!(preview_offset < after_offset);
        Ok(())
    }

    #[test]
    fn semantic_html_can_include_selectable_terminal_preview() -> TestResult {
        let html = render(
            "= Example\n:acdc-terminal:\n\n[source,terminal]\n----\n$ echo semantic\nsemantic\n----\n",
            crate::HtmlVariant::Semantic,
        )?;

        assert!(html.contains("<main id=\"content\">"));
        assert!(html.contains("listing-block terminal-block"));
        assert!(html.contains("<div class=\"terminal-view terminal-view--light\""));
        assert!(html.contains("$</span>"));
        assert!(html.contains(" echo semantic"));
        Ok(())
    }

    #[test]
    fn dark_mode_uses_dark_terminal_preview_theme() -> TestResult {
        let html = render(
            "= Example\n:acdc-terminal:\n:dark-mode:\n\n[source,console]\n----\n$ echo dark\n----\n",
            crate::HtmlVariant::Standard,
        )?;

        assert!(html.contains("<div class=\"terminal-view terminal-view--dark\""));
        // Preview colours come from the stylesheet class, not an inline style.
        assert!(html.contains(".terminal-view--dark{background-color:#0d1117;color:#e6edf3}"));
        assert!(html.contains("terminal-view--dark\" data-cols="));
        Ok(())
    }

    #[test]
    fn terminal_preview_preserves_syntax_colors() -> TestResult {
        let html = render(
            "= Example\n:acdc-terminal:\n\n[source,bash]\n----\necho \"hello\"\n----\n",
            crate::HtmlVariant::Standard,
        )?;

        assert!(
            html.contains("<span style=\"color:"),
            "expected terminal preview to include colored spans, got: {html}"
        );
        Ok(())
    }

    #[test]
    fn terminal_session_block_does_not_require_preview_attribute() -> TestResult {
        let html = render(
            "= Example\n\n[terminal]\n----\n$ cargo build\n\x1b[31merror\x1b[0m\n----\n",
            crate::HtmlVariant::Standard,
        )?;

        assert!(html.contains("<div class=\"terminalblock terminal-block\">"));
        assert!(html.contains("<div class=\"terminal-view terminal-view--light\""));
        assert!(html.contains("$ cargo build"));
        assert!(html.contains(">error</span>"));
        assert!(html.contains("<span style=\"color:"));
        Ok(())
    }

    #[test]
    fn terminal_session_block_uses_block_dimensions() -> TestResult {
        let html = render(
            "= Example\n\n[terminal,cols=12,rows=4]\n----\n$ echo dimensions\n----\n",
            crate::HtmlVariant::Standard,
        )?;

        assert!(html.contains("<div class=\"terminal-view terminal-view--light\""));
        assert!(html.contains("data-cols=\"12\""));
        assert!(html.contains("data-rows=\"4\""));
        assert!(html.contains(".terminal-view--light{background-color:#f6f8fa;color:#1f2328}"));
        Ok(())
    }

    #[test]
    fn terminal_session_options_layer_block_dimensions_over_document_dimensions() -> TestResult {
        let html = render(
            "= Example\n:acdc-terminal-cols: 30\n:acdc-terminal-rows: 7\n:dark-mode:\n\n[terminal,cols=12]\n----\n$ echo layered\n----\n",
            crate::HtmlVariant::Standard,
        )?;

        assert!(html.contains("<div class=\"terminal-view terminal-view--dark\""));
        assert!(html.contains("data-cols=\"12\""));
        assert!(html.contains("data-rows=\"7\""));
        assert!(html.contains(".terminal-view--dark{background-color:#0d1117;color:#e6edf3}"));
        Ok(())
    }

    #[test]
    fn semantic_terminal_session_block_uses_semantic_wrapper() -> TestResult {
        let html = render(
            "= Example\n\n.Terminal\n[terminal]\n----\n$ echo semantic\n----\n",
            crate::HtmlVariant::Semantic,
        )?;

        assert!(html.contains("<figure class=\"terminal-block\""));
        assert!(html.contains("<figcaption>Terminal</figcaption>"));
        assert!(html.contains("<div class=\"terminal-view terminal-view--light\""));
        assert!(html.contains("$ echo semantic"));
        Ok(())
    }

    #[test]
    fn literal_terminal_session_block_renders_as_terminal_preview() -> TestResult {
        let html = render(
            "= Example\n\n[terminal,cols=20]\n....\n$ echo literal\n....\n",
            crate::HtmlVariant::Standard,
        )?;

        assert!(html.contains("<div class=\"terminalblock terminal-block\">"));
        assert!(html.contains("data-cols=\"20\""));
        assert!(html.contains("$ echo literal"));
        Ok(())
    }

    #[test]
    fn terminal_replay_block_renders_multiple_frames() -> TestResult {
        let html = render(
            "= Example\n\n[terminal%replay,cols=20,rows=4]\n----\nfirst\n\x1b[31msecond\x1b[0m\n----\n",
            crate::HtmlVariant::Standard,
        )?;

        // Raw ANSI replay renders through the same JS player as asciicast: a
        // player container, an inline JSON payload, the shared init script, and
        // server-rendered rows. No CSS filmstrip and no window chrome.
        assert!(
            html.contains(
                "<div class=\"terminal-view terminal-view--replay terminal-view--light\""
            )
        );
        assert!(html.contains("data-cols=\"20\""));
        assert!(html.contains("data-rows=\"4\""));
        assert!(html.contains("data-frames=\"2\""));
        assert!(html.contains("data-duration-ms=\"1000\""));
        assert!(html.contains("<script type=\"application/json\" class=\"terminal-view__data\">"));
        assert!(html.contains("window.__acdcReplayInit"));
        assert!(html.contains("class=\"terminal-view__row\""));
        // No CSS filmstrip leftovers and no chrome (raw ANSI carries no title).
        assert!(!html.contains("@keyframes terminal-replay-scroll"));
        assert!(!html.contains("terminal-replay__scroll"));
        assert!(!html.contains("class=\"terminal-replay__titlebar\""));
        assert!(!html.contains("data-title"));
        assert!(html.contains("first"));
        assert!(html.contains("second</span>"));
        Ok(())
    }

    #[test]
    fn terminal_replay_blocks_each_emit_their_own_payload() -> TestResult {
        // Two replay blocks in one document each emit their own JSON payload;
        // the single shared init script animates every player on the page.
        let html = render(
            "= Example\n\n[terminal%replay,cols=20,rows=4]\n----\nfirst\nsecond\n----\n\n[terminal%replay,cols=20,rows=4]\n----\nalpha\nbeta\ngamma\n----\n",
            crate::HtmlVariant::Standard,
        )?;

        let payloads = html.matches("class=\"terminal-view__data\"").count();
        assert_eq!(payloads, 2, "each replay block emits its own data payload");
        assert!(html.contains("window.__acdcReplayInit"));
        Ok(())
    }

    #[test]
    fn terminal_replay_accepts_playback_duration_override() -> TestResult {
        let html = render(
            "= Example\n\n[terminal%replay,cols=20,rows=4,replay-duration-ms=250]\n----\nfirst\nsecond\n----\n",
            crate::HtmlVariant::Standard,
        )?;

        assert!(html.contains("data-duration-ms=\"250\""));
        assert!(html.contains("\"durationMs\":250"));
        Ok(())
    }

    #[test]
    fn terminal_replay_samples_dense_frames_for_short_playback() -> TestResult {
        let lines = (0..200)
            .map(|index| format!("line {index}"))
            .collect::<Vec<_>>()
            .join("\n");
        let html = render(
            &format!(
                "= Example\n\n[terminal%replay,cols=20,rows=4,replay-duration-ms=100]\n----\n{lines}\n----\n"
            ),
            crate::HtmlVariant::Standard,
        )?;

        assert!(html.contains("data-frames=\"4\""));
        assert!(html.contains("line 0"));
        assert!(html.contains("line 199"));
        Ok(())
    }

    #[test]
    fn terminal_replay_keeps_carriage_return_refreshes_atomic() {
        // A bare carriage return ends a chunk so an in-place refresh
        // ("working" -> "done") is replayed as its own visible update.
        let ansi = b"working\r\x1b[Kdone\n";
        let boundaries = super::chunk_boundaries(ansi);

        assert_eq!(boundaries, vec![7, ansi.len()]);
        assert_eq!(&ansi[..7], b"working");
        assert_eq!(&ansi[7..], b"\r\x1b[Kdone\n");
        assert!(!super::is_append_only(ansi));
    }

    #[test]
    fn terminal_replay_keeps_crlf_lines_in_one_chunk() {
        let ansi = b"first\r\nsecond\r\n";
        let boundaries = super::chunk_boundaries(ansi);

        assert_eq!(boundaries, vec![7, ansi.len()]);
        assert_eq!(&ansi[..7], b"first\r\n");
        assert_eq!(&ansi[7..], b"second\r\n");
        assert!(super::is_append_only(ansi));
    }

    #[test]
    fn terminal_replay_animates_in_place_refreshes_via_emulator_fallback() -> TestResult {
        // A bare carriage return rewrites the same line in place; the emulator
        // fallback replays each intermediate state rather than jumping to the
        // final value.
        let html = render(
            "= Example\n\n[terminal%replay,cols=20,rows=4]\n----\nWorking 0%\rWorking 50%\rWorking 100%\nDone\n----\n",
            crate::HtmlVariant::Standard,
        )?;

        assert!(html.contains("terminal-view--replay"));
        assert!(html.contains("Working 0%"));
        assert!(html.contains("Working 50%"));
        assert!(html.contains("Working 100%"));
        assert!(html.contains("Done"));
        Ok(())
    }

    #[test]
    fn terminal_replay_requires_explicit_dimensions() -> TestResult {
        let (html, warnings) = render_with_warnings(
            "= Example\n\n[terminal%replay,cols=0]\n----\nfirst\nsecond\n----\n",
            crate::HtmlVariant::Standard,
        )?;

        assert!(!html.contains("terminal-view terminal-view--replay"));
        assert!(html.contains("<div class=\"terminal-view terminal-view--light\""));
        assert!(warnings.iter().any(|warning| {
            warning
                .message
                .contains("terminal replay attribute `cols` must be a positive integer")
        }));
        assert!(warnings.iter().any(|warning| {
            warning
                .message
                .contains("terminal replay requires positive `cols` and `rows`")
        }));
        Ok(())
    }

    #[test]
    fn asciicast_v2_replay_renders_player() -> TestResult {
        let html = render(
            "= Example\n\n[terminal%replay,format=asciicast,cols=20,rows=4]\n----\n{\"version\":2,\"width\":20,\"height\":4}\n[0.0,\"o\",\"first\\r\\n\"]\n[0.5,\"o\",\"second\\r\\n\"]\n----\n",
            crate::HtmlVariant::Standard,
        )?;

        assert!(
            html.contains(
                "<div class=\"terminal-view terminal-view--replay terminal-view--light\""
            )
        );
        // A JSON payload + the shared inline player drive the row swaps.
        assert!(html.contains("<script type=\"application/json\" class=\"terminal-view__data\">"));
        assert!(html.contains("window.__acdcReplayInit"));
        // Final frame is server-rendered (no-JS / reduced-motion fallback).
        assert!(html.contains("class=\"terminal-view__row\""));
        assert!(html.contains("first"));
        assert!(html.contains("second"));
        Ok(())
    }

    #[test]
    fn asciicast_replay_with_recorded_theme_inlines_its_colours() -> TestResult {
        // A cast that recorded its own theme is painted faithfully: the recorded
        // background/foreground are inlined on the container.
        let html = render(
            "= Example\n\n[terminal%replay,format=asciicast,cols=20,rows=4]\n----\n{\"version\":2,\"width\":20,\"height\":4,\"theme\":{\"fg\":\"#c0caf5\",\"bg\":\"#1a1b26\",\"palette\":\"#000000:#ff0000:#00ff00:#ffff00:#0000ff:#ff00ff:#00ffff:#ffffff\"}}\n[0.0,\"o\",\"first\\r\\n\"]\n----\n",
            crate::HtmlVariant::Standard,
        )?;

        assert!(html.contains("terminal-view--replay"));
        assert!(html.contains("style=\"background-color:#1a1b26;color:#c0caf5\""));
        Ok(())
    }

    #[test]
    fn asciicast_replay_without_recorded_theme_is_class_based() -> TestResult {
        // A theme-less cast takes its colours from `.terminal-view--{theme}`,
        // so the container carries no inline colour (overridable with plain CSS).
        let html = render(
            "= Example\n\n[terminal%replay,format=asciicast,cols=20,rows=4]\n----\n{\"version\":2,\"width\":20,\"height\":4}\n[0.0,\"o\",\"first\\r\\n\"]\n----\n",
            crate::HtmlVariant::Standard,
        )?;

        assert!(html.contains("terminal-view--replay terminal-view--light\" data-cols="));
        assert!(!html.contains("terminal-view--replay terminal-view--light\" style="));
        Ok(())
    }

    #[test]
    fn player_script_csp_hash_matches_embedded_script() -> TestResult {
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        use sha2::{Digest, Sha256};

        // The build-time hash must cover exactly the code between the <script>
        // tags. Recomputing it here keeps build.rs and the runtime wrapping in
        // sync without any manual step.
        let inner = super::REPLAY_PLAYER_SCRIPT
            .strip_prefix("<script>")
            .and_then(|script| script.strip_suffix("</script>"))
            .ok_or("player script must be wrapped in <script> tags")?;
        let expected = format!(
            "sha256-{}",
            STANDARD.encode(Sha256::digest(inner.as_bytes()))
        );
        assert_eq!(super::REPLAY_PLAYER_SCRIPT_CSP_HASH, expected);
        Ok(())
    }

    #[test]
    fn terminal_rendering_degrades_to_listing_under_server_safe_mode() -> TestResult {
        // The emulator feeds document-controlled bytes through libghostty-vt, so
        // at Server/Secure it must not run: the block falls back to a plain
        // listing and the author is told why.
        let input = "= Example\n\n[terminal%replay,format=asciicast]\n----\n{\"version\":2,\"width\":30,\"height\":5}\n[0.0,\"o\",\"hi\\r\\n\"]\n----\n";
        let (html, warnings) = render_with_safe_mode(
            input,
            crate::HtmlVariant::Standard,
            acdc_parser::SafeMode::Server,
        )?;

        // Match the rendered element, not the `.terminal-view` CSS rule that the
        // embedded stylesheet always carries.
        assert!(
            !html.contains("class=\"terminal-view"),
            "emulator preview should not render under Server safe mode, got: {html}"
        );
        assert!(!html.contains(super::REPLAY_PLAYER_SCRIPT_CSP_HASH));
        assert!(
            html.contains("class=\"listingblock\""),
            "expected plain listing fallback, got: {html}"
        );
        assert!(
            warnings
                .iter()
                .any(|warning| warning.message.contains("terminal rendering is disabled")),
            "expected a safe-mode fallback warning, got: {warnings:?}"
        );
        Ok(())
    }

    #[test]
    fn asciicast_replay_uses_header_dimensions_without_block_attributes() -> TestResult {
        let html = render(
            "= Example\n\n[terminal%replay,format=asciicast]\n----\n{\"version\":2,\"width\":30,\"height\":5}\n[0.0,\"o\",\"hi\\r\\n\"]\n[0.5,\"o\",\"bye\\r\\n\"]\n----\n",
            crate::HtmlVariant::Standard,
        )?;

        assert!(html.contains("terminal-view terminal-view--replay"));
        assert!(html.contains("data-cols=\"30\""));
        assert!(html.contains("data-rows=\"5\""));
        Ok(())
    }

    #[test]
    fn asciicast_block_dimensions_override_header() -> TestResult {
        let html = render(
            "= Example\n\n[terminal%replay,format=asciicast,cols=12,rows=3]\n----\n{\"version\":2,\"width\":30,\"height\":5}\n[0.0,\"o\",\"hi\\r\\n\"]\n[0.5,\"o\",\"bye\\r\\n\"]\n----\n",
            crate::HtmlVariant::Standard,
        )?;

        assert!(html.contains("data-cols=\"12\""));
        assert!(html.contains("data-rows=\"3\""));
        Ok(())
    }

    #[test]
    fn asciicast_v3_relative_timing_renders() -> TestResult {
        let html = render(
            "= Example\n\n[terminal%replay,format=asciicast,cols=20,rows=4]\n----\n{\"version\":3,\"term\":{\"cols\":20,\"rows\":4}}\n[0.0,\"o\",\"alpha\\r\\n\"]\n[0.3,\"o\",\"beta\\r\\n\"]\n----\n",
            crate::HtmlVariant::Standard,
        )?;

        assert!(html.contains("terminal-view terminal-view--replay"));
        assert!(html.contains("alpha"));
        assert!(html.contains("beta"));
        Ok(())
    }

    #[test]
    fn asciicast_replay_rests_on_the_last_rows_when_taller_than_the_block() -> TestResult {
        // The recording is 6 rows tall but the block asks for rows=3. A single
        // burst prints five lines, so the only captured screen has line4 at the
        // bottom. The replay must rest on the LAST rows (line2..line4), windowed
        // to the block height, not the first lines or a block of blanks.
        let html = render(
            "= Example\n\n[terminal%replay,format=asciicast,rows=3]\n----\n{\"version\":2,\"width\":20,\"height\":6}\n[0.0,\"o\",\"line0\\r\\nline1\\r\\nline2\\r\\nline3\\r\\nline4\\r\\n\"]\n----\n",
            crate::HtmlVariant::Standard,
        )?;

        assert!(html.contains("data-rows=\"3\""));
        assert!(html.contains("line4"), "the last line is visible");
        assert!(
            html.contains("line2"),
            "the window is filled with the last rows"
        );
        assert!(
            !html.contains("line0"),
            "the earliest lines scrolled out of the window"
        );
        Ok(())
    }

    #[test]
    fn asciicast_preserves_special_characters_in_output() -> TestResult {
        // Output bytes containing `<`, `>`, `&` must survive parse -> capture and
        // be HTML-escaped exactly once by the renderer (not double-escaped, which
        // would mean the parser had escaped them before asciicast parsing).
        let html = render(
            "= Example\n\n[terminal%replay,format=asciicast,cols=40,rows=4]\n----\n{\"version\":2,\"width\":40,\"height\":4}\n[0.0,\"o\",\"<div> & </div>\\r\\n\"]\n----\n",
            crate::HtmlVariant::Standard,
        )?;

        assert!(html.contains("&lt;div&gt; &amp; &lt;/div&gt;"));
        assert!(!html.contains("&amp;lt;"));
        Ok(())
    }

    #[test]
    fn asciicast_unsupported_version_falls_back_with_warning() -> TestResult {
        let (html, warnings) = render_with_warnings(
            "= Example\n\n[terminal%replay,format=asciicast,cols=20,rows=4]\n----\n{\"version\":1,\"width\":20,\"height\":4,\"duration\":1.0,\"stdout\":[[0.0,\"x\\r\\n\"]]}\n----\n",
            crate::HtmlVariant::Standard,
        )?;

        assert!(!html.contains("terminal-view terminal-view--replay"));
        assert!(html.contains("<div class=\"terminal-view terminal-view--light\""));
        assert!(
            warnings
                .iter()
                .any(|warning| { warning.message.contains("unsupported asciicast version 1") })
        );
        Ok(())
    }

    #[test]
    fn unknown_replay_format_warns_and_replays_as_ansi() -> TestResult {
        let (html, warnings) = render_with_warnings(
            "= Example\n\n[terminal%replay,format=bogus,cols=20,rows=4]\n----\nfirst\nsecond\n----\n",
            crate::HtmlVariant::Standard,
        )?;

        assert!(html.contains("terminal-view terminal-view--replay"));
        assert!(warnings.iter().any(|warning| {
            warning
                .message
                .contains("unknown terminal replay `format` value `bogus`")
        }));
        Ok(())
    }

    #[test]
    fn auto_rows_follow_content_height_with_padding() {
        assert_eq!(
            super::estimate_rows(b"$ acdc --version\r\nacdc 0.2.0", 80),
            3
        );
        assert_eq!(super::estimate_rows(b"123456", 3), 3);
        assert_eq!(super::estimate_rows(b"1234567", 3), 4);
    }

    #[test]
    fn skips_terminal_preview_without_attribute() -> TestResult {
        let html = render("= Example\n\nPlain HTML\n", crate::HtmlVariant::Standard)?;

        // No preview container is rendered (the `.terminal-view--*` rules in the
        // embedded stylesheet don't count).
        assert!(!html.contains("<div class=\"terminal-view terminal-view--"));
        assert!(html.contains("Plain HTML"));
        Ok(())
    }

    #[test]
    fn linkcss_uses_built_in_stylesheet_for_terminal_preview_styles() -> TestResult {
        let html = render(
            "= Example\n:linkcss:\n:acdc-terminal:\n\n[source,console]\n----\n$ echo linked\n----\n",
            crate::HtmlVariant::Standard,
        )?;

        assert!(html.contains(r#"<link rel="stylesheet" href="./asciidoctor-light-mode.css">"#));
        assert!(html.contains("<div class=\"terminal-view terminal-view--light\""));
        assert!(!html.contains(".terminal-view{"));
        assert!(!html.contains(".terminal-view__screen{"));
        Ok(())
    }

    #[test]
    fn escapes_terminal_text() -> TestResult {
        let html = render(
            "= Example\n:acdc-terminal:\n\n[source,console]\n----\n<&>\n----\n",
            crate::HtmlVariant::Standard,
        )?;

        assert!(html.contains("&lt;"));
        assert!(html.contains("&amp;&gt;"));
        Ok(())
    }
}
