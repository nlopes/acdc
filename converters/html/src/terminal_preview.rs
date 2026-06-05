//! Selectable HTML rendering for terminal cell-grid previews.

use std::{borrow::Cow, io::Write};

use acdc_converters_core::{Diagnostics, Options, code::detect_language};
use acdc_converters_terminal::{
    cell_grid::{Cell, CellDecorations, CellGrid, Rgb, TerminalSize, capture_ansi},
    replay::{self, Options as ReplayOptions},
};
use acdc_parser::{AttributeValue, BlockMetadata, DocumentAttributes, InlineNode};

use crate::Error;

const DEFAULT_COLS: usize = 80;
const AUTO_ROW_PADDING: usize = 1;
const MAX_AUTO_ROWS: usize = 200;
const REPLAY_OPTION: &str = "replay";
const REPLAY_FRAME_DURATION_MS: u64 = 500;
const REPLAY_DURATION_MS_ATTR: &str = "replay-duration-ms";
const REPLAY_RENDER_FPS: u128 = 30;
const MAX_REPLAY_RENDER_FRAMES: usize = 120;

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

    const fn colors(self) -> (Rgb, Rgb) {
        match self {
            Self::Dark => (
                Rgb {
                    r: 13,
                    g: 17,
                    b: 23,
                },
                Rgb {
                    r: 230,
                    g: 237,
                    b: 243,
                },
            ),
            Self::Light => (
                Rgb {
                    r: 246,
                    g: 248,
                    b: 250,
                },
                Rgb {
                    r: 31,
                    g: 35,
                    b: 40,
                },
            ),
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
        let document_cols = attr_usize(attrs.get("terminal-cols"));
        let document_rows = attr_usize(attrs.get("terminal-rows"));
        let document_theme = Theme::from_document_attributes(attrs);

        Self {
            cols: metadata
                .and_then(|metadata| {
                    attr_usize(metadata.attributes.get("cols"))
                        .or_else(|| attr_usize(metadata.attributes.get("terminal-cols")))
                })
                .or(document_cols)
                .unwrap_or(DEFAULT_COLS),
            rows: metadata
                .and_then(|metadata| {
                    attr_usize(metadata.attributes.get("rows"))
                        .or_else(|| attr_usize(metadata.attributes.get("terminal-rows")))
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

impl From<&Cell> for SpanStyle {
    fn from(cell: &Cell) -> Self {
        Self {
            fg: cell.fg,
            bg: cell.bg,
            decorations: cell.decorations,
        }
    }
}

pub(crate) fn is_enabled(attrs: &DocumentAttributes<'_>) -> bool {
    attrs
        .get("terminal-preview")
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

    render_grid(&mut writer, &grid, preview_options.theme)?;
    Ok(())
}

fn render_replay<W: Write>(
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
    let playback_duration = positive_attr(
        REPLAY_DURATION_MS_ATTR,
        metadata.attributes.get(REPLAY_DURATION_MS_ATTR),
        diagnostics,
    )
    .map(|ms| std::time::Duration::from_millis(ms as u64));

    let ansi = normalize_terminal_newlines(&acdc_converters_terminal::render_listing_to_ansi(
        options,
        attrs.clone(),
        inlines,
        metadata,
        size.cols,
        preview_options.theme == Theme::Dark,
    )?);
    let boundaries = chunk_boundaries(&ansi);
    let frame_duration = std::time::Duration::from_millis(REPLAY_FRAME_DURATION_MS);
    let estimated_playback_ms = frame_duration
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
        frame_duration,
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
        let replay_options = ReplayOptions::new(tall).with_default_frame_duration(frame_duration);
        replay::capture_windowed(events, replay_options, size.rows)?.into_frames()
    } else {
        let replay_options = ReplayOptions::new(size).with_default_frame_duration(frame_duration);
        replay::capture(events, replay_options)?.into_frames()
    };

    if frames.is_empty() {
        diagnostics.warn(
            "terminal replay produced no visible frames; rendering a static terminal preview instead",
        );
        let grid = capture_ansi(&ansi, size)?;
        return render_grid(&mut writer, &grid, preview_options.theme);
    }

    render_replay_filmstrip(
        &mut writer,
        &frames,
        preview_options.theme,
        playback_duration,
    )?;
    Ok(())
}

fn replay_size(
    attrs: &DocumentAttributes<'_>,
    metadata: &BlockMetadata<'_>,
    diagnostics: &mut Diagnostics<'_>,
) -> Option<TerminalSize> {
    let cols = positive_attr("cols", metadata.attributes.get("cols"), diagnostics)
        .or_else(|| {
            positive_attr(
                "terminal-cols",
                metadata.attributes.get("terminal-cols"),
                diagnostics,
            )
        })
        .or_else(|| positive_attr("terminal-cols", attrs.get("terminal-cols"), diagnostics));
    let rows = positive_attr("rows", metadata.attributes.get("rows"), diagnostics)
        .or_else(|| {
            positive_attr(
                "terminal-rows",
                metadata.attributes.get("terminal-rows"),
                diagnostics,
            )
        })
        .or_else(|| positive_attr("terminal-rows", attrs.get("terminal-rows"), diagnostics));

    if let (Some(cols), Some(rows)) = (cols, rows) {
        Some(TerminalSize::new(cols, rows))
    } else {
        diagnostics.warn_with_advice(
            "terminal replay requires positive `cols` and `rows` block attributes or `terminal-cols` and `terminal-rows` document attributes; rendering a static terminal preview instead",
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
    let sampled = sampled_indexes(boundaries.len(), frame_budget);
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

fn render_grid<W: Write>(writer: &mut W, grid: &CellGrid, theme: Theme) -> Result<(), Error> {
    let (bg, fg) = theme.colors();
    write!(
        writer,
        "<div class=\"terminal-preview terminal-preview--{}\" style=\"background-color:{};color:{}\" data-cols=\"{}\" data-rows=\"{}\">",
        theme.as_str(),
        rgb_css(bg),
        rgb_css(fg),
        grid.cols(),
        grid.rows()
    )?;
    writeln!(
        writer,
        "<pre class=\"terminal-preview__screen\" aria-label=\"Terminal preview\">"
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

/// Render the replay as a single "filmstrip": every sampled frame stacked into
/// one `<pre>`, with a clipped viewport showing one frame at a time and a single
/// stepped `translateY` animation advancing through them. This keeps the DOM to
/// one grid and one animation regardless of recording length, instead of N
/// stacked frame layers. Playback ends on the final frame (no scroll-back).
fn render_replay_filmstrip<W: Write>(
    writer: &mut W,
    frames: &[replay::Frame],
    theme: Theme,
    playback_duration: Option<std::time::Duration>,
) -> Result<(), Error> {
    let Some(first) = frames.first() else {
        return Ok(());
    };
    let (bg, fg) = theme.colors();
    let frame_duration = std::time::Duration::from_millis(REPLAY_FRAME_DURATION_MS);
    let first_at = first.at;
    let total_duration = frames.last().map_or(frame_duration, |frame| {
        frame.at.saturating_sub(first_at) + frame_duration
    });
    let total_ms = playback_duration
        .unwrap_or(total_duration)
        .as_millis()
        .max(1);
    let cols = first.grid.cols();
    let rows = first.grid.rows();
    let animation_name = replay_animation_name(frames, rows, total_duration);

    write!(
        writer,
        "<div class=\"terminal-preview terminal-replay terminal-preview--{}\" style=\"background-color:{};color:{}\" data-cols=\"{}\" data-rows=\"{}\" data-frames=\"{}\" data-duration-ms=\"{}\">",
        theme.as_str(),
        rgb_css(bg),
        rgb_css(fg),
        cols,
        rows,
        frames.len(),
        total_ms
    )?;
    render_replay_scroll_keyframes(
        writer,
        &animation_name,
        frames,
        rows,
        first_at,
        total_duration,
    )?;
    // The viewport keeps the terminal padding and any horizontal scrolling; the
    // clip is exactly `rows` tall (font-size pins `em` to the screen's line box)
    // and hides everything but the current frame as the filmstrip scrolls.
    writeln!(
        writer,
        "<div class=\"terminal-replay__viewport\" style=\"overflow:auto;max-width:100%;padding:18px;font-size:14px\">"
    )?;
    writeln!(
        writer,
        "<div class=\"terminal-replay__clip\" style=\"position:relative;overflow:hidden;width:max-content;height:{}\">",
        row_em(rows)
    )?;
    writeln!(
        writer,
        "<pre class=\"terminal-preview__screen terminal-replay__scroll\" aria-label=\"Terminal replay\" style=\"margin:0;padding:0;animation:{animation_name} {total_ms}ms step-end both\">"
    )?;
    let mut first_row = true;
    for frame in frames {
        for row in frame.grid.rows_iter() {
            if !first_row {
                writeln!(writer)?;
            }
            first_row = false;
            render_row(writer, row)?;
        }
    }
    writeln!(writer, "</pre>")?;
    writeln!(writer, "</div>")?;
    writeln!(writer, "</div>")?;
    writeln!(writer, "</div>")?;
    Ok(())
}

/// A deterministic, per-block CSS animation name. Every replay block emits its
/// own `@keyframes`, so a single shared global name would let a later block's
/// keyframes override an earlier block's, animating the earlier block with the
/// wrong offsets. The name hashes the inputs that define the keyframes (frame
/// timings and row geometry); blocks that animate identically share a name,
/// which is harmless.
fn replay_animation_name(
    frames: &[replay::Frame],
    rows: usize,
    total_duration: std::time::Duration,
) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    rows.hash(&mut hasher);
    total_duration.hash(&mut hasher);
    frames.len().hash(&mut hasher);
    for frame in frames {
        frame.at.hash(&mut hasher);
    }
    format!("terminal-replay-scroll-{:016x}", hasher.finish())
}

fn render_replay_scroll_keyframes<W: Write>(
    writer: &mut W,
    name: &str,
    frames: &[replay::Frame],
    rows: usize,
    first_at: std::time::Duration,
    total_duration: std::time::Duration,
) -> Result<(), Error> {
    writeln!(writer, "<style>")?;
    write!(writer, "@keyframes {name}{{")?;
    for (index, frame) in frames.iter().enumerate() {
        let percent = replay_frame_percent(frame.at.saturating_sub(first_at), total_duration);
        let offset = row_em(index.saturating_mul(rows));
        write!(writer, "{percent:.4}%{{transform:translateY(-{offset})}}")?;
    }
    // Hold the final frame to the end of the timeline.
    let last_offset = row_em(frames.len().saturating_sub(1).saturating_mul(rows));
    writeln!(writer, "100%{{transform:translateY(-{last_offset})}}}}")?;
    writeln!(writer, "</style>")?;
    Ok(())
}

/// A length in hundredths of an `em`, formatted as a CSS `em` value
/// (e.g. `145` -> `1.45em`).
#[derive(Clone, Copy)]
struct Em(usize);

impl std::fmt::Display for Em {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{:02}em", self.0 / 100, self.0 % 100)
    }
}

/// Height of `rows` terminal rows, matching the screen's `font: 14px/1.45`
/// line box (1.45em per row).
fn row_em(rows: usize) -> Em {
    Em(rows.saturating_mul(145))
}

fn replay_frame_percent(elapsed: std::time::Duration, total_duration: std::time::Duration) -> f64 {
    let total = total_duration.as_secs_f64();
    if total == 0.0 {
        return 0.0;
    }
    (elapsed.as_secs_f64() / total * 100.0).clamp(0.0, 100.0)
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

fn sampled_indexes(item_count: usize, budget: usize) -> Vec<usize> {
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

fn render_row<W: Write>(writer: &mut W, row: &[Cell]) -> Result<(), Error> {
    let mut current_style = SpanStyle::default();
    let mut buffer = String::new();
    let mut has_open_span = false;

    for cell in row {
        let style = SpanStyle::from(cell);
        if style != current_style {
            flush_span(writer, &buffer, current_style, has_open_span)?;
            buffer.clear();
            current_style = style;
            has_open_span = !is_default_style(style);
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

fn is_default_style(style: SpanStyle) -> bool {
    style.fg.is_none()
        && style.bg.is_none()
        && !style.decorations.bold
        && !style.decorations.italic
        && !style.decorations.underline
        && !style.decorations.dim
        && !style.decorations.inverse
        && !style.decorations.strikethrough
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
        let parser_options =
            ParserOptions::with_attributes(acdc_converters_core::default_rendering_attributes());
        let parsed = acdc_parser::parse(input, &parser_options)?;
        let doc = parsed.document();
        let options = ConverterOptions::builder()
            .generator_metadata(GeneratorMetadata::new("acdc", "0.1.0"))
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
            "= Example\n:terminal-preview:\n\n[source,console]\n----\n$ acdc --version\nacdc 0.2.0\n----\n\nAfter preview.\n",
            crate::HtmlVariant::Standard,
        )?;

        assert!(html.contains("<div id=\"content\">"));
        assert!(html.contains("<div class=\"listingblock terminal-preview-block\">"));
        assert!(html.contains("<div class=\"terminal-preview terminal-preview--light\""));
        assert!(html.contains("background-color:#f6f8fa;color:#1f2328"));
        assert!(html.contains("$</span>"));
        assert!(html.contains(" acdc"));
        assert!(html.contains("--version"));
        assert!(html.contains("0.2.0"));
        let preview_offset = html
            .find("terminal-preview")
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
            "= Example\n:terminal-preview:\n\n[source,terminal]\n----\n$ echo semantic\nsemantic\n----\n",
            crate::HtmlVariant::Semantic,
        )?;

        assert!(html.contains("<main id=\"content\">"));
        assert!(html.contains("listing-block terminal-preview-block"));
        assert!(html.contains("<div class=\"terminal-preview terminal-preview--light\""));
        assert!(html.contains("$</span>"));
        assert!(html.contains(" echo semantic"));
        Ok(())
    }

    #[test]
    fn dark_mode_uses_dark_terminal_preview_theme() -> TestResult {
        let html = render(
            "= Example\n:terminal-preview:\n:dark-mode:\n\n[source,console]\n----\n$ echo dark\n----\n",
            crate::HtmlVariant::Standard,
        )?;

        assert!(html.contains("<div class=\"terminal-preview terminal-preview--dark\""));
        assert!(html.contains("background-color:#0d1117;color:#e6edf3"));
        Ok(())
    }

    #[test]
    fn terminal_preview_preserves_syntax_colors() -> TestResult {
        let html = render(
            "= Example\n:terminal-preview:\n\n[source,bash]\n----\necho \"hello\"\n----\n",
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

        assert!(html.contains("<div class=\"terminalblock terminal-preview-block\">"));
        assert!(html.contains("<div class=\"terminal-preview terminal-preview--light\""));
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

        assert!(html.contains("<div class=\"terminal-preview terminal-preview--light\""));
        assert!(html.contains("data-cols=\"12\""));
        assert!(html.contains("data-rows=\"4\""));
        assert!(html.contains("background-color:#f6f8fa;color:#1f2328"));
        Ok(())
    }

    #[test]
    fn terminal_session_options_layer_block_dimensions_over_document_dimensions() -> TestResult {
        let html = render(
            "= Example\n:terminal-cols: 30\n:terminal-rows: 7\n:dark-mode:\n\n[terminal,cols=12]\n----\n$ echo layered\n----\n",
            crate::HtmlVariant::Standard,
        )?;

        assert!(html.contains("<div class=\"terminal-preview terminal-preview--dark\""));
        assert!(html.contains("data-cols=\"12\""));
        assert!(html.contains("data-rows=\"7\""));
        assert!(html.contains("background-color:#0d1117;color:#e6edf3"));
        Ok(())
    }

    #[test]
    fn semantic_terminal_session_block_uses_semantic_wrapper() -> TestResult {
        let html = render(
            "= Example\n\n.Terminal\n[terminal]\n----\n$ echo semantic\n----\n",
            crate::HtmlVariant::Semantic,
        )?;

        assert!(html.contains("<figure class=\"terminal-block terminal-preview-block\""));
        assert!(html.contains("<figcaption>Terminal</figcaption>"));
        assert!(html.contains("<div class=\"terminal-preview terminal-preview--light\""));
        assert!(html.contains("$ echo semantic"));
        Ok(())
    }

    #[test]
    fn literal_terminal_session_block_renders_as_terminal_preview() -> TestResult {
        let html = render(
            "= Example\n\n[terminal,cols=20]\n....\n$ echo literal\n....\n",
            crate::HtmlVariant::Standard,
        )?;

        assert!(html.contains("<div class=\"terminalblock terminal-preview-block\">"));
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

        assert!(
            html.contains(
                "<div class=\"terminal-preview terminal-replay terminal-preview--light\""
            )
        );
        assert!(html.contains("data-cols=\"20\""));
        assert!(html.contains("data-rows=\"4\""));
        assert!(html.contains("data-frames=\"2\""));
        assert!(html.contains("data-duration-ms=\"1000\""));
        // Single filmstrip grid + one stepped scroll animation, no per-frame
        // layers or visibility toggling.
        assert!(html.contains("@keyframes terminal-replay-scroll"));
        assert!(html.contains("terminal-replay__scroll"));
        assert!(html.contains("transform:translateY("));
        assert!(html.contains("1000ms step-end both"));
        assert!(!html.contains("<script>"));
        assert!(!html.contains("terminal-replay__frame"));
        assert!(!html.contains("terminal-replay__stage"));
        assert!(!html.contains("terminal-replay__scrollback"));
        assert!(!html.contains("@keyframes terminal-replay-frame-"));
        assert!(html.contains("first"));
        assert!(html.contains("second</span>"));
        Ok(())
    }

    #[test]
    fn terminal_replay_blocks_use_distinct_animation_names() -> TestResult {
        // Two replay blocks in one document must not share a global
        // `@keyframes` name, or the later block's keyframes would override the
        // earlier block's and animate it with the wrong offsets.
        let html = render(
            "= Example\n\n[terminal%replay,cols=20,rows=4]\n----\nfirst\nsecond\n----\n\n[terminal%replay,cols=20,rows=4]\n----\nalpha\nbeta\ngamma\n----\n",
            crate::HtmlVariant::Standard,
        )?;

        let names: Vec<&str> = html
            .split("@keyframes ")
            .skip(1)
            .filter_map(|rest| rest.split('{').next())
            .filter(|name| name.starts_with("terminal-replay-scroll-"))
            .collect();

        assert_eq!(
            names.len(),
            2,
            "each replay block defines its own keyframes"
        );
        assert_ne!(
            names.first(),
            names.get(1),
            "animation names must be unique per block"
        );
        for name in &names {
            assert!(
                html.contains(&format!("animation:{name} ")),
                "each block animates with its own keyframes name"
            );
        }
        Ok(())
    }

    #[test]
    fn terminal_replay_accepts_playback_duration_override() -> TestResult {
        let html = render(
            "= Example\n\n[terminal%replay,cols=20,rows=4,replay-duration-ms=250]\n----\nfirst\nsecond\n----\n",
            crate::HtmlVariant::Standard,
        )?;

        assert!(html.contains("data-duration-ms=\"250\""));
        assert!(html.contains("250ms step-end both"));
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

        assert!(html.contains("terminal-replay__scroll"));
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

        assert!(!html.contains("terminal-replay"));
        assert!(html.contains("<div class=\"terminal-preview terminal-preview--light\""));
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

        assert!(!html.contains("terminal-preview--"));
        assert!(html.contains("Plain HTML"));
        Ok(())
    }

    #[test]
    fn linkcss_uses_built_in_stylesheet_for_terminal_preview_styles() -> TestResult {
        let html = render(
            "= Example\n:linkcss:\n:terminal-preview:\n\n[source,console]\n----\n$ echo linked\n----\n",
            crate::HtmlVariant::Standard,
        )?;

        assert!(html.contains(r#"<link rel="stylesheet" href="./asciidoctor-light-mode.css">"#));
        assert!(html.contains("<div class=\"terminal-preview terminal-preview--light\""));
        assert!(!html.contains(".terminal-preview{"));
        assert!(!html.contains(".terminal-preview__screen{"));
        Ok(())
    }

    #[test]
    fn escapes_terminal_text() -> TestResult {
        let html = render(
            "= Example\n:terminal-preview:\n\n[source,console]\n----\n<&>\n----\n",
            crate::HtmlVariant::Standard,
        )?;

        assert!(html.contains("&lt;"));
        assert!(html.contains("&amp;&gt;"));
        Ok(())
    }
}
