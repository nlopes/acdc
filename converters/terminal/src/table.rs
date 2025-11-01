use std::io::{self, BufWriter};

use acdc_converters_common::visitor::{Visitor, WritableVisitor};
use comfy_table::{Cell, Color, ContentArrangement, Table};

use crate::{Error, Processor, TerminalVisitor};

pub(crate) fn visit_table<V: WritableVisitor<Error = Error>>(
    tbl: &acdc_parser::Table,
    visitor: &mut V,
    processor: &Processor,
) -> Result<(), Error> {
    let mut table_widget = Table::new();
    table_widget
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_width(80)
        .load_preset(comfy_table::presets::UTF8_FULL)
        .apply_modifier(comfy_table::modifiers::UTF8_ROUND_CORNERS);

    if let Some(header) = &tbl.header {
        let header_cells = header
            .columns
            .iter()
            .map(|col| {
                let buffer = Vec::new();
                let inner = BufWriter::new(buffer);
                let mut temp_visitor = TerminalVisitor::new(inner, processor.clone());
                col.content
                    .iter()
                    .try_for_each(|block| temp_visitor.visit_block(block))?;
                let buffer = temp_visitor
                    .into_writer()
                    .into_inner()
                    .map_err(io::IntoInnerError::into_error)?;
                Ok(Cell::new(String::from_utf8(buffer).unwrap_or_default())
                    .fg(Color::Green)
                    .add_attribute(comfy_table::Attribute::Bold))
            })
            .collect::<Result<Vec<_>, Error>>()?;
        table_widget.set_header(header_cells);
    }

    for row in &tbl.rows {
        let cells = row
            .columns
            .iter()
            .map(|col| {
                let buffer = Vec::new();
                let inner = BufWriter::new(buffer);
                let mut temp_visitor = TerminalVisitor::new(inner, processor.clone());
                col.content
                    .iter()
                    .try_for_each(|block| temp_visitor.visit_block(block))?;
                let buffer = temp_visitor
                    .into_writer()
                    .into_inner()
                    .map_err(io::IntoInnerError::into_error)?;
                Ok(Cell::new(String::from_utf8(buffer).unwrap_or_default()))
            })
            .collect::<Result<Vec<_>, Error>>()?;
        table_widget.add_row(cells);
    }

    let w = visitor.writer_mut();
    writeln!(w, "{table_widget}")?;
    Ok(())
}
