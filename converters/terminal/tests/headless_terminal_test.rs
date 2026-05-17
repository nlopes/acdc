use acdc_converters_core::{Converter, Options as ConverterOptions};
use acdc_converters_terminal::Processor;
use acdc_parser::Options as ParserOptions;
use libghostty_vt::{
    RenderState, Terminal, TerminalOptions,
    render::{CellIterator, RowIterator},
    style::{RgbColor, StyleColor, Underline},
};

type Error = Box<dyn std::error::Error>;

#[derive(Clone, Debug, PartialEq, Eq)]
struct RenderedCell {
    text: String,
    fg: Option<RgbColor>,
    bg: Option<RgbColor>,
    raw_fg: StyleColor,
    raw_bg: StyleColor,
    decorations: Vec<Decoration>,
}

impl RenderedCell {
    fn has_decoration(&self, decoration: Decoration) -> bool {
        self.decorations.contains(&decoration)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Decoration {
    Bold,
    Italic,
    Underline,
    Dim,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RenderedGrid {
    cells: Vec<RenderedCell>,
    cols: usize,
    rows: usize,
}

impl RenderedGrid {
    fn cell(&self, row: usize, col: usize) -> Option<&RenderedCell> {
        if row >= self.rows || col >= self.cols {
            return None;
        }

        self.cells.get(row * self.cols + col)
    }

    fn row(&self, row: usize) -> Option<&[RenderedCell]> {
        if row >= self.rows {
            return None;
        }

        let start = row * self.cols;
        let end = start + self.cols;
        self.cells.get(start..end)
    }

    fn row_text(&self, row: usize) -> String {
        self.row(row).map_or_else(String::new, |cells| {
            cells
                .iter()
                .map(|cell| cell.text.as_str())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
    }

    fn find_text(&self, text: &str) -> Option<(usize, usize)> {
        (0..self.rows).find_map(|row_index| {
            let row = self.row(row_index)?;
            let mut row_text = String::new();
            let mut cell_indexes = Vec::new();
            for (cell_index, cell) in row.iter().enumerate() {
                for ch in cell.text.chars() {
                    row_text.push(ch);
                    cell_indexes.push(cell_index);
                }
            }
            row_text
                .find(text)
                .map(|byte_index| {
                    row_text
                        .char_indices()
                        .take_while(|(index, _)| *index < byte_index)
                        .count()
                })
                .and_then(|char_index| cell_indexes.get(char_index).copied())
                .map(|cell_index| (row_index, cell_index))
        })
    }

    fn debug_rows(&self) -> String {
        (0..self.rows)
            .map(|index| format!("{index}: {:?}", self.row_text(index)))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

fn render_to_grid(asciidoc: &str, cols: u16, rows: u16) -> Result<RenderedGrid, Error> {
    let parser_options =
        ParserOptions::with_attributes(acdc_converters_core::default_rendering_attributes());
    let parsed = acdc_parser::parse(asciidoc, &parser_options)?;
    let doc = parsed.document();

    let processor = Processor::new(ConverterOptions::default(), doc.attributes.clone())
        .with_terminal_width(usize::from(cols));
    let mut output = Vec::new();
    let source = acdc_converters_core::WarningSource::new("terminal");
    let mut warnings = Vec::new();
    let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
    processor.write_to(doc, &mut output, None, None, &mut diagnostics)?;

    let mut terminal = Terminal::new(TerminalOptions {
        cols,
        rows,
        max_scrollback: 0,
    })?;
    terminal.vt_write(&output);

    let mut render_state = RenderState::new()?;
    let snapshot = render_state.update(&terminal)?;
    let mut row_iterator = RowIterator::new()?;
    let mut cell_iterator = CellIterator::new()?;
    let mut row_iteration = row_iterator.update(&snapshot)?;
    let cols = usize::from(cols);
    let rows = usize::from(rows);
    let mut rendered_cells = Vec::with_capacity(cols * rows);

    while let Some(row) = row_iteration.next() {
        let mut cell_iteration = cell_iterator.update(row)?;
        let row_start = rendered_cells.len();
        while let Some(cell) = cell_iteration.next() {
            let style = cell.style()?;
            let mut decorations = Vec::new();
            if style.bold {
                decorations.push(Decoration::Bold);
            }
            if style.italic {
                decorations.push(Decoration::Italic);
            }
            if style.underline != Underline::None {
                decorations.push(Decoration::Underline);
            }
            if style.faint {
                decorations.push(Decoration::Dim);
            }
            rendered_cells.push(RenderedCell {
                text: cell.graphemes()?.iter().collect::<String>(),
                fg: cell.fg_color()?,
                bg: cell.bg_color()?,
                raw_fg: style.fg_color,
                raw_bg: style.bg_color,
                decorations,
            });
        }
        debug_assert_eq!(rendered_cells.len() - row_start, cols);
    }
    debug_assert_eq!(rendered_cells.len(), cols * rows);

    Ok(RenderedGrid {
        cells: rendered_cells,
        cols,
        rows,
    })
}

fn assert_span_style(grid: &RenderedGrid, text: &str, predicate: impl Fn(&RenderedCell) -> bool) {
    let Some((row, col)) = grid.find_text(text) else {
        assert!(
            grid.find_text(text).is_some(),
            "text {text:?} was not rendered in grid:\n{}",
            grid.debug_rows()
        );
        return;
    };

    for offset in 0..text.chars().count() {
        let Some(cell) = grid.cell(row, col + offset) else {
            assert!(
                grid.cell(row, col + offset).is_some(),
                "missing cell at row {row}, col {}",
                col + offset
            );
            return;
        };
        assert!(
            predicate(cell),
            "cell at row {row}, col {} in {text:?} did not match expected style: {cell:?}\n{}",
            col + offset,
            grid.debug_rows()
        );
    }
}

fn assert_row_contains(grid: &RenderedGrid, row: usize, text: &str) {
    let actual = grid.row_text(row);
    assert!(
        actual.contains(text),
        "expected row {row} to contain {text:?}, got {actual:?}\n{}",
        grid.debug_rows()
    );
}

fn assert_grid_contains(grid: &RenderedGrid, text: &str) {
    assert!(
        grid.find_text(text).is_some(),
        "expected grid to contain {text:?}\n{}",
        grid.debug_rows()
    );
}

#[test]
fn smoke_renders_plain_text_into_cells() -> Result<(), Error> {
    let grid = render_to_grid("hello\n", 20, 5)?;

    assert_row_contains(&grid, 0, "hello");
    assert_eq!(grid.cell(0, 0).map(|cell| cell.text.as_str()), Some("h"));
    assert_eq!(grid.cell(0, 4).map(|cell| cell.text.as_str()), Some("o"));
    Ok(())
}

#[test]
fn inline_styles_survive_terminal_emulation() -> Result<(), Error> {
    let grid = render_to_grid("*bold* _italic_ [.underline]#under#\n", 80, 5)?;

    assert_span_style(&grid, "bold", |cell| cell.has_decoration(Decoration::Bold));
    assert_span_style(&grid, "italic", |cell| {
        cell.has_decoration(Decoration::Italic)
    });
    assert_span_style(&grid, "under", |cell| {
        cell.has_decoration(Decoration::Underline)
    });
    Ok(())
}

#[test]
fn section_header_is_bold_and_rendered_with_rules() -> Result<(), Error> {
    let grid = render_to_grid("= Document title\n\n== First section\n", 40, 8)?;

    assert_row_contains(&grid, 0, "Document title");
    assert_span_style(&grid, "Document title", |cell| {
        cell.has_decoration(Decoration::Bold) && cell.has_decoration(Decoration::Underline)
    });
    assert_row_contains(&grid, 4, "First section");
    assert_span_style(&grid, "First section", |cell| {
        cell.has_decoration(Decoration::Bold)
    });
    Ok(())
}

#[test]
fn lists_render_visible_markers() -> Result<(), Error> {
    let grid = render_to_grid("* unordered\n* second\n\n. ordered\n. next\n", 40, 10)?;

    assert!(
        grid.find_text("• unordered")
            .or_else(|| grid.find_text("* unordered"))
            .is_some(),
        "unordered list marker was not rendered"
    );
    assert!(grid.find_text("1. ordered").is_some());
    Ok(())
}

#[test]
fn admonition_renders_caption_and_border_cells() -> Result<(), Error> {
    let grid = render_to_grid("NOTE: Pay attention.\n", 60, 20)?;

    assert_grid_contains(&grid, "Note:");
    assert_grid_contains(&grid, "Pay attention.");
    Ok(())
}

#[test]
fn table_content_renders_after_terminal_emulation() -> Result<(), Error> {
    let grid = render_to_grid(
        r"|===
|Name |Value

|Alpha |1
|Beta |2
|===
",
        60,
        30,
    )?;

    assert_grid_contains(&grid, "Name");
    assert_grid_contains(&grid, "Alpha");
    assert_grid_contains(&grid, "Beta");
    Ok(())
}

#[test]
fn source_blocks_and_callouts_render_visible_cells() -> Result<(), Error> {
    let grid = render_to_grid(
        r"[source,rust]
----
fn main() { // <1>
}
----
<1> entry point
",
        60,
        30,
    )?;

    assert_grid_contains(&grid, "main()");
    assert_grid_contains(&grid, "<1>");
    assert_grid_contains(&grid, "entry point");
    Ok(())
}
