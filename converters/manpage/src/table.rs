//! Table rendering for manpages using the tbl preprocessor.
//!
//! Tables are rendered using `.TS`/`.TE` macros which are processed by the
//! `tbl` preprocessor before groff.

use std::io::Write;

use acdc_converters_common::visitor::{Visitor, WritableVisitor};
use acdc_parser::{DelimitedBlock, Table, TableColumn};

use crate::{
    Error, ManpageVisitor,
    document::extract_plain_text,
    escape::{EscapeMode, manify},
};

/// Visit a table.
pub fn visit_table<W: Write>(
    table: &Table,
    block: &DelimitedBlock,
    visitor: &mut ManpageVisitor<W>,
) -> Result<(), Error> {
    let w = visitor.writer_mut();

    // Table title if present
    if !block.title.is_empty() {
        writeln!(w, ".PP")?;
        write!(w, "\\fB")?;
        visitor.visit_inline_nodes(&block.title)?;
        let w = visitor.writer_mut();
        writeln!(w, "\\fP")?;
    }

    let w = visitor.writer_mut();

    // Start table with tbl preprocessor
    // Use tab(:) as delimiter - safer than default tab character
    writeln!(w, ".TS")?;
    writeln!(w, "allbox tab(:);")?;

    // Build format specification
    let num_cols = table
        .header
        .as_ref()
        .map(|h| h.columns.len())
        .or_else(|| table.rows.first().map(|r| r.columns.len()))
        .unwrap_or(1);

    // Header format: bold columns
    if table.header.is_some() {
        let header_fmt: String = (0..num_cols).map(|_| "lb").collect::<Vec<_>>().join(" ");
        writeln!(w, "{header_fmt}")?;
    }

    // Body format: left-aligned columns
    let body_fmt: String = (0..num_cols).map(|_| "l").collect::<Vec<_>>().join(" ");
    writeln!(w, "{body_fmt}.")?;

    // Header row
    if let Some(header) = &table.header {
        let cells: Vec<String> = header.columns.iter().map(format_cell).collect();
        writeln!(w, "{}", cells.join(":"))?;
    }

    // Body rows
    for row in &table.rows {
        let cells: Vec<String> = row.columns.iter().map(format_cell).collect();
        writeln!(w, "{}", cells.join(":"))?;
    }

    // Footer row
    if let Some(footer) = &table.footer {
        let cells: Vec<String> = footer.columns.iter().map(format_cell).collect();
        writeln!(w, "{}", cells.join(":"))?;
    }

    // End table
    writeln!(w, ".TE")?;

    Ok(())
}

/// Format a table cell for tbl output.
fn format_cell(cell: &TableColumn) -> String {
    // Extract plain text from cell content (which is Vec<Block>)
    let mut text = String::new();
    for block in &cell.content {
        if let acdc_parser::Block::Paragraph(para) = block {
            text.push_str(&extract_plain_text(&para.content));
        }
    }
    let escaped = manify(&text, EscapeMode::Normalize);

    // Wrap in T{ T} if content contains special characters
    if escaped.contains(':') || escaped.contains('\n') {
        format!("T{{\n{escaped}\nT}}")
    } else {
        escaped.into_owned()
    }
}
