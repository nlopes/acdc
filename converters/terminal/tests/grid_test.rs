//! Cell-grid verification tests for the terminal converter via `libghostty-vt`.
//!
//! These tests complement the byte-level fixture tests in `integration_test.rs`
//! by piping converter output through a real VT emulator and asserting on the
//! resulting cell grid. Targeting cells (char + style) instead of raw ANSI
//! bytes catches regressions where an SGR reordering preserves the visual
//! output, and it verifies that the emitted bytes actually render the way
//! the converter intends.

#![allow(
    clippy::pedantic,
    clippy::indexing_slicing,
    reason = "test code: relaxed lints for readability and deterministic fixture inputs"
)]

use std::ops::Range;

use acdc_converters_core::{Converter, Options as ConverterOptions, default_rendering_attributes};
use acdc_converters_terminal::{Capabilities, Processor};
use acdc_parser::Options as ParserOptions;
use libghostty_vt::{
    RenderState, Terminal, TerminalOptions,
    render::{CellIterator, RowIterator},
    style::{RgbColor, Underline},
};

type TestError = Box<dyn std::error::Error>;

const TEST_COLS: u16 = 80;
const TEST_ROWS: u16 = 200;

/// A single cell extracted from the `libghostty-vt` render state.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Cell {
    ch: char,
    bold: bool,
    italic: bool,
    underlined: bool,
    #[allow(dead_code, reason = "available for future colour-sensitive assertions")]
    fg: Option<RgbColor>,
    #[allow(dead_code, reason = "available for future colour-sensitive assertions")]
    bg: Option<RgbColor>,
    hyperlink: bool,
}

/// A materialised 2D grid of cells (`rows[row_index][col_index]`).
#[derive(Debug)]
struct Grid {
    rows: Vec<Vec<Cell>>,
}

impl Grid {
    fn cell(&self, row: usize, col: usize) -> &Cell {
        &self.rows[row][col]
    }

    fn row_text(&self, row: usize) -> String {
        self.rows[row].iter().map(|c| c.ch).collect()
    }

    /// Find the first `(row, col)` where `needle` starts in the grid.
    fn find_text(&self, needle: &str) -> Option<(usize, usize)> {
        for (r, row) in self.rows.iter().enumerate() {
            let text: String = row.iter().map(|c| c.ch).collect();
            if let Some(col) = text.find(needle) {
                return Some((r, col));
            }
        }
        None
    }
}

/// Parse AsciiDoc, run the terminal converter, and feed the bytes through
/// `libghostty-vt` to produce a cell grid. OSC 8 hyperlink emission is
/// disabled.
fn render_adoc(adoc: &str) -> Result<Grid, TestError> {
    render_adoc_inner(adoc, false)
}

/// Same as `render_adoc` but forces OSC 8 hyperlink emission on regardless
/// of the host environment's `TERM` variable.
fn render_adoc_osc8(adoc: &str) -> Result<Grid, TestError> {
    render_adoc_inner(adoc, true)
}

fn render_adoc_inner(adoc: &str, force_osc8: bool) -> Result<Grid, TestError> {
    let parser_opts = ParserOptions::with_attributes(default_rendering_attributes());
    let doc = acdc_parser::parse(adoc, &parser_opts)?;

    let capabilities = Capabilities {
        unicode: true,
        osc8_links: force_osc8,
    };

    let processor = Processor::new(ConverterOptions::default(), doc.attributes.clone())
        .with_terminal_width(usize::from(TEST_COLS))
        .with_capabilities(capabilities);

    let mut buf = Vec::new();
    processor.write_to(&doc, &mut buf, None)?;

    drive_terminal(&buf, TEST_COLS, TEST_ROWS)
}

fn drive_terminal(bytes: &[u8], cols: u16, rows: u16) -> Result<Grid, TestError> {
    let mut terminal = Terminal::new(TerminalOptions {
        cols,
        rows,
        max_scrollback: 10_000,
    })?;
    terminal.vt_write(bytes);

    let mut render_state = RenderState::new()?;
    let mut row_iter = RowIterator::new()?;
    let mut cell_iter = CellIterator::new()?;

    let snapshot = render_state.update(&terminal)?;
    let mut rows_iteration = row_iter.update(&snapshot)?;

    let mut grid_rows: Vec<Vec<Cell>> = Vec::with_capacity(rows as usize);
    while let Some(row) = rows_iteration.next() {
        let mut row_cells: Vec<Cell> = Vec::with_capacity(cols as usize);
        let mut cells_iteration = cell_iter.update(row)?;
        while let Some(cell) = cells_iteration.next() {
            let graphemes = cell.graphemes()?;
            // Use U+0020 (SPACE) as the placeholder for empty / bg-only cells
            // so that string searches against the grid behave naturally.
            let ch = graphemes
                .first()
                .copied()
                .filter(|c| *c != '\0')
                .unwrap_or(' ');
            let style = cell.style()?;
            let fg = cell.fg_color()?;
            let bg = cell.bg_color()?;
            let hyperlink = cell.raw_cell()?.has_hyperlink()?;
            row_cells.push(Cell {
                ch,
                bold: style.bold,
                italic: style.italic,
                underlined: style.underline != Underline::None,
                fg,
                bg,
                hyperlink,
            });
        }
        grid_rows.push(row_cells);
    }

    Ok(Grid { rows: grid_rows })
}

// ---------------------------------------------------------------------
// Assertion helpers
// ---------------------------------------------------------------------

fn assert_cells_bold(grid: &Grid, row: usize, cols: Range<usize>) {
    for col in cols {
        let cell = grid.cell(row, col);
        assert!(cell.bold, "expected bold at ({row}, {col}); got {cell:?}");
    }
}

fn assert_cells_italic(grid: &Grid, row: usize, cols: Range<usize>) {
    for col in cols {
        let cell = grid.cell(row, col);
        assert!(
            cell.italic,
            "expected italic at ({row}, {col}); got {cell:?}"
        );
    }
}

fn assert_cells_hyperlinked(grid: &Grid, row: usize, cols: Range<usize>) {
    for col in cols {
        let cell = grid.cell(row, col);
        assert!(
            cell.hyperlink,
            "expected hyperlink at ({row}, {col}); got {cell:?}"
        );
    }
}

fn assert_cell_no_hyperlink(grid: &Grid, row: usize, col: usize) {
    let cell = grid.cell(row, col);
    assert!(
        !cell.hyperlink,
        "expected no hyperlink at ({row}, {col}); got {cell:?}"
    );
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[test]
fn section_header_is_bold() -> Result<(), TestError> {
    let adoc = "= Document Title\n\n== Section Title\n\nBody paragraph.\n";
    let grid = render_adoc(adoc)?;

    let (row, col) = grid
        .find_text("Section Title")
        .ok_or("'Section Title' not found in grid")?;
    assert_cells_bold(&grid, row, col..col + "Section Title".len());
    Ok(())
}

#[test]
fn plain_paragraph_has_no_inline_styling() -> Result<(), TestError> {
    let adoc = "Just a plain sentence on its own.\n";
    let grid = render_adoc(adoc)?;

    let (row, col) = grid
        .find_text("Just a plain sentence")
        .ok_or("sentence not found")?;
    let cell = grid.cell(row, col);
    assert!(!cell.bold, "plain paragraph should not be bold: {cell:?}");
    assert!(
        !cell.italic,
        "plain paragraph should not be italic: {cell:?}"
    );
    Ok(())
}

#[test]
fn bold_and_italic_inline_formatting() -> Result<(), TestError> {
    let adoc = "This has *bold* and _italic_ words.\n";
    let grid = render_adoc(adoc)?;

    let (row, col) = grid.find_text("bold").ok_or("'bold' not found")?;
    assert_cells_bold(&grid, row, col..col + "bold".len());

    let (row, col) = grid.find_text("italic").ok_or("'italic' not found")?;
    assert_cells_italic(&grid, row, col..col + "italic".len());
    Ok(())
}

#[test]
fn unordered_list_shows_bullet_and_items() -> Result<(), TestError> {
    let adoc = "* First item\n* Second item\n";
    let grid = render_adoc(adoc)?;

    let (row, _) = grid
        .find_text("First item")
        .ok_or("'First item' not found")?;
    let text = grid.row_text(row);
    // The terminal converter picks different bullet glyphs based on nesting
    // and Unicode support; accept any of them.
    assert!(
        text.contains('\u{2022}')   // •
            || text.contains('\u{25E6}') // ◦
            || text.contains('\u{25AA}') // ▪
            || text.contains('*'),
        "row {row} missing bullet char; full row: {text:?}"
    );

    assert!(
        grid.find_text("Second item").is_some(),
        "second list item missing"
    );
    Ok(())
}

#[test]
fn ordered_list_shows_numbers() -> Result<(), TestError> {
    let adoc = ". First item\n. Second item\n";
    let grid = render_adoc(adoc)?;

    assert!(grid.find_text("1.").is_some(), "missing '1.' prefix");
    assert!(grid.find_text("2.").is_some(), "missing '2.' prefix");
    assert!(
        grid.find_text("First item").is_some(),
        "missing first item text"
    );
    assert!(
        grid.find_text("Second item").is_some(),
        "missing second item text"
    );
    Ok(())
}

#[test]
fn note_admonition_renders_caption_and_body() -> Result<(), TestError> {
    let adoc = "NOTE: A helpful note.\n";
    let grid = render_adoc(adoc)?;

    // The caption is either the word "NOTE" or the ℹ icon, depending on
    // capability detection.
    let has_caption = grid.find_text("NOTE").is_some() || grid.find_text("\u{2139}").is_some();
    assert!(has_caption, "expected NOTE caption somewhere in grid");

    assert!(
        grid.find_text("A helpful note").is_some(),
        "expected note body text"
    );
    Ok(())
}

#[test]
fn warning_admonition_renders_caption_and_body() -> Result<(), TestError> {
    let adoc = "WARNING: Be careful now.\n";
    let grid = render_adoc(adoc)?;

    let has_caption = grid.find_text("WARNING").is_some() || grid.find_text("\u{26A0}").is_some();
    assert!(has_caption, "expected WARNING caption somewhere in grid");

    assert!(
        grid.find_text("Be careful now").is_some(),
        "expected warning body text"
    );
    Ok(())
}

#[test]
fn table_draws_box_borders_and_cells() -> Result<(), TestError> {
    let adoc = "|===\n| A | B\n| 1 | 2\n|===\n";
    let grid = render_adoc(adoc)?;

    // Unicode box-drawing block: U+2500..=U+257F.
    let has_box_char = grid.rows.iter().flatten().any(|cell| {
        let code = u32::from(cell.ch);
        (0x2500..=0x257F).contains(&code)
    });
    assert!(has_box_char, "expected table box-drawing characters");

    assert!(grid.find_text("A").is_some(), "missing cell 'A'");
    assert!(grid.find_text("B").is_some(), "missing cell 'B'");
    assert!(grid.find_text("1").is_some(), "missing cell '1'");
    assert!(grid.find_text("2").is_some(), "missing cell '2'");
    Ok(())
}

#[test]
fn source_block_preserves_content_verbatim() -> Result<(), TestError> {
    let adoc = "----\nlet x = 42;\n----\n";
    let grid = render_adoc(adoc)?;

    assert!(
        grid.find_text("let x = 42;").is_some(),
        "expected source block content rendered verbatim"
    );
    Ok(())
}

#[test]
fn url_macro_sets_hyperlink_on_link_text_when_osc8_enabled() -> Result<(), TestError> {
    let adoc = "See https://example.com[Example Site] for details.\n";
    let grid = render_adoc_osc8(adoc)?;

    let (row, col) = grid
        .find_text("Example Site")
        .ok_or("'Example Site' not found in grid")?;
    let text_len = "Example Site".len();

    assert_cells_hyperlinked(&grid, row, col..col + text_len);

    if col > 0 {
        assert_cell_no_hyperlink(&grid, row, col - 1);
    }
    assert_cell_no_hyperlink(&grid, row, col + text_len);
    Ok(())
}

#[test]
fn url_macro_has_no_hyperlink_when_osc8_disabled() -> Result<(), TestError> {
    let adoc = "See https://example.com[Example Site] for details.\n";
    let grid = render_adoc(adoc)?;

    let (row, col) = grid
        .find_text("Example Site")
        .ok_or("'Example Site' not found in grid")?;

    for offset in 0.."Example Site".len() {
        assert_cell_no_hyperlink(&grid, row, col + offset);
    }
    Ok(())
}
