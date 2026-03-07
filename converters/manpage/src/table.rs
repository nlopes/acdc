//! Table rendering for manpages using the tbl preprocessor.
//!
//! Tables are rendered using `.TS`/`.TE` macros which are processed by the
//! `tbl` preprocessor before groff. Colspan and rowspan are supported via
//! per-row format lines using `s` (horizontal span) and `^` (vertical span).

use std::io::Write;

use acdc_converters_core::visitor::{Visitor, WritableVisitor};
use acdc_parser::{DelimitedBlock, HorizontalAlignment, Table, TableColumn, TableRow};

use crate::{Error, ManpageVisitor, Processor};

/// Map horizontal alignment to tbl format character.
fn alignment_prefix(halign: HorizontalAlignment) -> &'static str {
    match halign {
        HorizontalAlignment::Left => "l",
        HorizontalAlignment::Center => "c",
        HorizontalAlignment::Right => "r",
    }
}

/// What occupies a logical cell position in the tbl grid.
enum CellKind {
    /// A real cell with content. `cell_index` indexes into the AST row's `columns` vec.
    Content { cell_index: usize },
    /// Horizontal span from the left (`s` in tbl format, empty data).
    HSpan,
    /// Vertical span from above (`^` in tbl format, `\^` in data).
    VSpan,
}

/// Metadata for each logical row in the grid.
struct GridRow<'a> {
    cells: Vec<CellKind>,
    ast_row: &'a TableRow,
    is_header: bool,
}

/// Determine the true logical column count, accounting for spans.
fn determine_column_count(table: &Table) -> usize {
    if !table.columns.is_empty() {
        return table.columns.len();
    }

    let all_rows = table
        .header
        .iter()
        .chain(table.rows.iter())
        .chain(table.footer.iter());

    all_rows
        .map(|row| row.columns.iter().map(|c| c.colspan.max(1)).sum::<usize>())
        .max()
        .unwrap_or(1)
}

/// Build the logical grid from all table rows.
fn build_grid(table: &Table, num_cols: usize) -> Vec<GridRow<'_>> {
    let all_rows: Vec<(&TableRow, bool)> = table
        .header
        .iter()
        .map(|r| (r, true))
        .chain(table.rows.iter().map(|r| (r, false)))
        .chain(table.footer.iter().map(|r| (r, false)))
        .collect();

    let mut grid = Vec::with_capacity(all_rows.len());
    let mut rowspan_remaining = vec![0usize; num_cols];

    for (ast_row, is_header) in &all_rows {
        let mut row_cells = Vec::with_capacity(num_cols);
        let mut cell_cursor = 0;
        let mut col = 0;

        while col < num_cols {
            if let Some(remaining) = rowspan_remaining.get_mut(col)
                && *remaining > 0
            {
                row_cells.push(CellKind::VSpan);
                *remaining -= 1;
                col += 1;
                continue;
            }

            let Some(cell) = ast_row.columns.get(cell_cursor) else {
                // Shouldn't happen in well-formed input; fill remaining
                row_cells.push(CellKind::HSpan);
                col += 1;
                continue;
            };

            let colspan = cell.colspan.max(1);
            let rowspan = cell.rowspan.max(1);

            row_cells.push(CellKind::Content {
                cell_index: cell_cursor,
            });

            // Fill horizontal span markers for extra colspan columns
            for _ in 1..colspan {
                if row_cells.len() < num_cols {
                    row_cells.push(CellKind::HSpan);
                }
            }

            // Set rowspan tracking for all columns this cell covers
            for i in 0..colspan {
                if let Some(remaining) = rowspan_remaining.get_mut(col + i) {
                    *remaining = rowspan - 1;
                }
            }

            col += colspan;
            cell_cursor += 1;
        }

        grid.push(GridRow {
            cells: row_cells,
            ast_row,
            is_header: *is_header,
        });
    }

    grid
}

/// Generate a tbl format entry for a single cell position.
fn format_entry(
    kind: &CellKind,
    col_index: usize,
    row: &GridRow<'_>,
    col_alignments: &[&str],
) -> String {
    match kind {
        CellKind::Content { cell_index } => {
            let align =
                if let Some(halign) = row.ast_row.columns.get(*cell_index).and_then(|c| c.halign) {
                    alignment_prefix(halign)
                } else {
                    col_alignments.get(col_index).copied().unwrap_or("l")
                };
            if row.is_header {
                format!("{align}tB")
            } else {
                format!("{align}t")
            }
        }
        CellKind::HSpan => "s".to_string(),
        CellKind::VSpan => "^".to_string(),
    }
}

/// Render a data row from the grid, producing a colon-separated string.
fn render_grid_row(grid_row: &GridRow<'_>, processor: &Processor) -> Result<String, Error> {
    let mut data_cells = Vec::with_capacity(grid_row.cells.len());

    for kind in &grid_row.cells {
        match kind {
            CellKind::Content { cell_index } => {
                if let Some(cell) = grid_row.ast_row.columns.get(*cell_index) {
                    data_cells.push(format_cell_with_inlines(cell, processor)?);
                } else {
                    data_cells.push(String::new());
                }
            }
            CellKind::HSpan => {
                data_cells.push(String::new());
            }
            CellKind::VSpan => {
                data_cells.push("\\^".to_string());
            }
        }
    }

    Ok(data_cells.join(":"))
}

/// Visit a table.
pub(crate) fn visit_table<W: Write>(
    table: &Table,
    _block: &DelimitedBlock,
    visitor: &mut ManpageVisitor<W>,
) -> Result<(), Error> {
    let processor = visitor.processor.clone();

    let num_cols = determine_column_count(table);

    // Build alignment specs from column format info
    let col_alignments: Vec<&str> = if table.columns.is_empty() {
        vec!["l"; num_cols]
    } else {
        table
            .columns
            .iter()
            .map(|col| alignment_prefix(col.halign))
            .collect()
    };

    // Build the logical grid
    let grid = build_grid(table, num_cols);

    // Generate format lines
    let format_lines: Vec<String> = grid
        .iter()
        .map(|row| {
            row.cells
                .iter()
                .enumerate()
                .map(|(col_idx, kind)| format_entry(kind, col_idx, row, &col_alignments))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();

    // Pre-render all data rows
    let data_rows: Vec<String> = grid
        .iter()
        .map(|row| render_grid_row(row, &processor))
        .collect::<Result<Vec<_>, _>>()?;

    // Write output
    let w = visitor.writer_mut();

    writeln!(w, ".TS")?;
    writeln!(w, "allbox tab(:);")?;

    // Write format lines: all but last end with newline, last ends with "."
    if let Some((last, rest)) = format_lines.split_last() {
        for fmt in rest {
            writeln!(w, "{fmt}")?;
        }
        writeln!(w, "{last}.")?;
    }

    // Write data rows
    for data_row in &data_rows {
        writeln!(w, "{data_row}")?;
    }

    writeln!(w, ".TE")?;

    Ok(())
}

/// Format a table cell with inline formatting preserved.
fn format_cell_with_inlines(cell: &TableColumn, processor: &Processor) -> Result<String, Error> {
    let mut buf = Vec::new();
    let mut cell_visitor = ManpageVisitor::new(&mut buf, processor.clone());

    for block in &cell.content {
        if let acdc_parser::Block::Paragraph(para) = block {
            cell_visitor.visit_inline_nodes(&para.content)?;
        } else {
            cell_visitor.visit_block(block)?;
        }
    }

    let text = String::from_utf8_lossy(&buf).into_owned();

    // Wrap in T{ T} if content contains tbl special characters or formatting
    if text.contains(':') || text.contains('\n') || text.contains("\\f") {
        Ok(format!("T{{\n{text}\nT}}"))
    } else {
        Ok(text)
    }
}
