//! Terminal render-state capture as acdc-owned cell grid types.

use libghostty_vt::{
    RenderState, Terminal, TerminalOptions,
    render::{CellIterator, RowIterator},
    style::{RgbColor, Underline},
};

/// Error type returned while capturing ANSI bytes into a [`CellGrid`].
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum Error {
    /// The requested terminal dimensions cannot be represented by Ghostty.
    #[error("terminal dimensions exceed u16 limits: {cols} columns by {rows} rows")]
    TerminalSizeTooLarge {
        /// Requested column count.
        cols: usize,
        /// Requested row count.
        rows: usize,
    },
    /// Ghostty render-state processing failed.
    #[error(transparent)]
    Ghostty(#[from] libghostty_vt::Error),
}

/// Terminal dimensions used for render-state capture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalSize {
    /// Number of terminal columns.
    pub cols: usize,
    /// Number of terminal rows.
    pub rows: usize,
}

impl TerminalSize {
    /// Create a terminal size.
    #[must_use]
    pub const fn new(cols: usize, rows: usize) -> Self {
        Self { cols, rows }
    }

    fn as_u16(self) -> Result<(u16, u16), Error> {
        let cols = u16::try_from(self.cols).map_err(|_| Error::TerminalSizeTooLarge {
            cols: self.cols,
            rows: self.rows,
        })?;
        let rows = u16::try_from(self.rows).map_err(|_| Error::TerminalSizeTooLarge {
            cols: self.cols,
            rows: self.rows,
        })?;
        Ok((cols, rows))
    }
}

impl Default for TerminalSize {
    fn default() -> Self {
        Self { cols: 80, rows: 25 }
    }
}

/// RGB color resolved by the terminal render state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rgb {
    /// Red channel.
    pub r: u8,
    /// Green channel.
    pub g: u8,
    /// Blue channel.
    pub b: u8,
}

impl From<RgbColor> for Rgb {
    fn from(value: RgbColor) -> Self {
        Self {
            r: value.r,
            g: value.g,
            b: value.b,
        }
    }
}

/// Text decorations attached to a rendered terminal cell.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "terminal SGR decorations are independent boolean flags"
)]
pub struct CellDecorations {
    /// Bold/intense text.
    pub bold: bool,
    /// Italic text.
    pub italic: bool,
    /// Underlined text.
    pub underline: bool,
    /// Faint/dim text.
    pub dim: bool,
    /// Inverse video text.
    pub inverse: bool,
    /// Struck-through text.
    pub strikethrough: bool,
}

impl CellDecorations {
    #[must_use]
    fn is_plain(self) -> bool {
        !self.bold
            && !self.italic
            && !self.underline
            && !self.dim
            && !self.inverse
            && !self.strikethrough
    }
}

/// One rendered terminal cell.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Cell {
    /// Grapheme cluster rendered in this cell.
    pub text: String,
    /// Explicit foreground color, if any.
    pub fg: Option<Rgb>,
    /// Explicit background color, if any.
    pub bg: Option<Rgb>,
    /// Text decorations for this cell.
    pub decorations: CellDecorations,
}

impl Cell {
    /// Returns true when the cell contains no visible text or style.
    #[must_use]
    pub fn is_blank(&self) -> bool {
        self.text.trim().is_empty()
            && self.fg.is_none()
            && self.bg.is_none()
            && self.decorations.is_plain()
    }
}

/// A fixed-size terminal cell grid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellGrid {
    cells: Vec<Cell>,
    cols: usize,
    rows: usize,
}

impl CellGrid {
    /// Create a cell grid from cells in row-major order.
    #[must_use]
    pub fn new(cells: Vec<Cell>, size: TerminalSize) -> Self {
        debug_assert_eq!(cells.len(), size.cols * size.rows);
        Self {
            cells,
            cols: size.cols,
            rows: size.rows,
        }
    }

    /// Number of columns.
    #[must_use]
    pub const fn cols(&self) -> usize {
        self.cols
    }

    /// Number of rows.
    #[must_use]
    pub const fn rows(&self) -> usize {
        self.rows
    }

    /// Get a cell by row and column.
    #[must_use]
    pub fn cell(&self, row: usize, col: usize) -> Option<&Cell> {
        if row >= self.rows || col >= self.cols {
            return None;
        }
        self.cells.get(row * self.cols + col)
    }

    /// Get a row by index.
    #[must_use]
    pub fn row(&self, row: usize) -> Option<&[Cell]> {
        if row >= self.rows {
            return None;
        }
        let start = row * self.cols;
        let end = start + self.cols;
        self.cells.get(start..end)
    }

    /// Iterate over rows.
    pub fn rows_iter(&self) -> impl Iterator<Item = &[Cell]> {
        self.cells.chunks(self.cols)
    }

    /// Text for a row, with trailing whitespace removed.
    #[must_use]
    pub fn row_text(&self, row: usize) -> String {
        self.row(row).map_or_else(String::new, |cells| {
            cells
                .iter()
                .map(|cell| cell.text.as_str())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
    }
}

/// Capture ANSI terminal bytes into a stable acdc-owned cell grid.
///
/// # Errors
///
/// Returns an error if Ghostty cannot create or query the render state.
pub fn capture_ansi(ansi: &[u8], size: TerminalSize) -> Result<CellGrid, Error> {
    let (cols, rows) = size.as_u16()?;
    let mut terminal = Terminal::new(TerminalOptions {
        cols,
        rows,
        max_scrollback: 0,
    })?;
    terminal.vt_write(ansi);

    let mut render_state = RenderState::new()?;
    let snapshot = render_state.update(&terminal)?;
    let mut row_iterator = RowIterator::new()?;
    let mut cell_iterator = CellIterator::new()?;
    let mut row_iteration = row_iterator.update(&snapshot)?;
    let mut rendered_cells = Vec::with_capacity(size.cols * size.rows);

    while let Some(row) = row_iteration.next() {
        let mut cell_iteration = cell_iterator.update(row)?;
        let row_start = rendered_cells.len();
        while let Some(cell) = cell_iteration.next() {
            let style = cell.style()?;
            rendered_cells.push(Cell {
                text: cell.graphemes()?.iter().collect(),
                fg: cell.fg_color()?.map(Rgb::from),
                bg: cell.bg_color()?.map(Rgb::from),
                decorations: CellDecorations {
                    bold: style.bold,
                    italic: style.italic,
                    underline: style.underline != Underline::None,
                    dim: style.faint,
                    inverse: style.inverse,
                    strikethrough: style.strikethrough,
                },
            });
        }
        debug_assert_eq!(rendered_cells.len() - row_start, size.cols);
    }
    debug_assert_eq!(rendered_cells.len(), size.cols * size.rows);

    Ok(CellGrid::new(rendered_cells, size))
}
