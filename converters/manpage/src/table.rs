//! Table rendering for manpages using the tbl preprocessor.
//!
//! Tables are rendered using `.TS`/`.TE` macros which are processed by the
//! `tbl` preprocessor before groff. Colspan and rowspan are supported via
//! per-row format lines using `s` (horizontal span) and `^` (vertical span).

use std::io::Write;

use acdc_converters_core::table::{CellKind, GridRow, build_grid, determine_column_count};
use acdc_converters_core::visitor::{Visitor, WritableVisitor};
use acdc_parser::{DelimitedBlock, HorizontalAlignment, Table, TableColumn};

use crate::{Error, ManpageVisitor, Processor};

/// Map horizontal alignment to tbl format character.
fn alignment_prefix(halign: HorizontalAlignment) -> &'static str {
    match halign {
        HorizontalAlignment::Left => "l",
        HorizontalAlignment::Center => "c",
        HorizontalAlignment::Right => "r",
    }
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
fn render_grid_row(grid_row: &GridRow<'_>, processor: &Processor<'_>) -> Result<String, Error> {
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
                // No data entry — tbl's `s` format automatically extends the
                // left cell's data into this column position.
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
    visitor: &mut ManpageVisitor<'_, W>,
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
fn format_cell_with_inlines(
    cell: &TableColumn,
    processor: &Processor<'_>,
) -> Result<String, Error> {
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
