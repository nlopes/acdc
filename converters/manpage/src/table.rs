//! Table rendering for manpages using the tbl preprocessor.
//!
//! Tables are rendered using `.TS`/`.TE` macros which are processed by the
//! `tbl` preprocessor before groff.

use std::io::Write;

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

/// Visit a table.
pub(crate) fn visit_table<W: Write>(
    table: &Table,
    _block: &DelimitedBlock,
    visitor: &mut ManpageVisitor<W>,
) -> Result<(), Error> {
    // Note: Table title is already rendered by the parent visit_delimited_block

    // Clone processor for cell rendering
    let processor = visitor.processor.clone();

    // Build format specification
    let num_cols = table
        .header
        .as_ref()
        .map(|h| h.columns.len())
        .or_else(|| table.rows.first().map(|r| r.columns.len()))
        .unwrap_or(1);

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

    // Header format string
    let header_fmt = if table.header.is_some() {
        Some(
            col_alignments
                .iter()
                .map(|a| format!("{a}tB"))
                .collect::<Vec<_>>()
                .join(" "),
        )
    } else {
        None
    };

    // Body format string
    let body_fmt: String = col_alignments
        .iter()
        .map(|a| format!("{a}t"))
        .collect::<Vec<_>>()
        .join(" ");

    // Pre-render all cell contents
    let header_cells = if let Some(header) = &table.header {
        Some(render_row_cells(&header.columns, &processor)?)
    } else {
        None
    };

    let body_rows: Vec<String> = table
        .rows
        .iter()
        .map(|row| render_row_cells(&row.columns, &processor))
        .collect::<Result<Vec<_>, _>>()?;

    let footer_cells = if let Some(footer) = &table.footer {
        Some(render_row_cells(&footer.columns, &processor)?)
    } else {
        None
    };

    // Now write everything to the output
    let w = visitor.writer_mut();

    writeln!(w, ".TS")?;
    writeln!(w, "allbox tab(:);")?;

    if let Some(hfmt) = &header_fmt {
        writeln!(w, "{hfmt}")?;
    }
    writeln!(w, "{body_fmt}.")?;

    if let Some(cells) = &header_cells {
        writeln!(w, "{cells}")?;
    }

    for row_str in &body_rows {
        writeln!(w, "{row_str}")?;
    }

    if let Some(cells) = &footer_cells {
        writeln!(w, "{cells}")?;
    }

    writeln!(w, ".TE")?;

    Ok(())
}

/// Render a row's cells as a colon-separated string.
fn render_row_cells(columns: &[TableColumn], processor: &Processor) -> Result<String, Error> {
    let cells: Result<Vec<String>, Error> = columns
        .iter()
        .map(|c| format_cell_with_inlines(c, processor))
        .collect();
    Ok(cells?.join(":"))
}

/// Format a table cell with inline formatting preserved.
fn format_cell_with_inlines(cell: &TableColumn, processor: &Processor) -> Result<String, Error> {
    let mut buf = Vec::new();
    let mut cell_visitor = ManpageVisitor::new(&mut buf, processor.clone());

    for block in &cell.content {
        if let acdc_parser::Block::Paragraph(para) = block {
            cell_visitor.visit_inline_nodes(&para.content)?;
        }
    }

    let text = String::from_utf8_lossy(&buf).to_string();

    // Wrap in T{ T} if content contains tbl special characters or formatting
    if text.contains(':') || text.contains('\n') || text.contains("\\f") {
        Ok(format!("T{{\n{text}\nT}}"))
    } else {
        Ok(text)
    }
}
