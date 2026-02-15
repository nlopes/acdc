use std::fmt::Write as FmtWrite;
use std::io::Write;

use acdc_converters_core::visitor::{Visitor, WritableVisitor};
use acdc_parser::{Block, DelimitedBlock, HorizontalAlignment, Table, TableColumn, TableRow};

use crate::{Error, ManpageHtmlVisitor};

fn alignment_style(halign: HorizontalAlignment) -> &'static str {
    match halign {
        HorizontalAlignment::Left => "text-align:left",
        HorizontalAlignment::Center => "text-align:center",
        HorizontalAlignment::Right => "text-align:right",
    }
}

fn render_cell<W: Write>(
    cell: &TableColumn,
    tag: &str,
    visitor: &mut ManpageHtmlVisitor<W>,
) -> Result<(), Error> {
    let mut attrs = String::new();

    if cell.colspan > 1 {
        let _ = write!(attrs, " colspan=\"{}\"", cell.colspan);
    }
    if cell.rowspan > 1 {
        let _ = write!(attrs, " rowspan=\"{}\"", cell.rowspan);
    }
    if let Some(halign) = cell.halign {
        let _ = write!(attrs, " style=\"{}\"", alignment_style(halign));
    }

    write!(visitor.writer_mut(), "<{tag}{attrs}>")?;

    for block in &cell.content {
        if let Block::Paragraph(para) = block {
            visitor.visit_inline_nodes(&para.content)?;
        } else {
            visitor.visit_block(block)?;
        }
    }

    write!(visitor.writer_mut(), "</{tag}>")?;
    Ok(())
}

fn render_row<W: Write>(
    row: &TableRow,
    tag: &str,
    visitor: &mut ManpageHtmlVisitor<W>,
) -> Result<(), Error> {
    write!(visitor.writer_mut(), "<tr>")?;
    for cell in &row.columns {
        render_cell(cell, tag, visitor)?;
    }
    write!(visitor.writer_mut(), "</tr>")?;
    Ok(())
}

pub(crate) fn visit_table<W: Write>(
    table: &Table,
    _block: &DelimitedBlock,
    visitor: &mut ManpageHtmlVisitor<W>,
) -> Result<(), Error> {
    write!(visitor.writer_mut(), "<table>")?;

    if let Some(header) = &table.header {
        write!(visitor.writer_mut(), "<thead>")?;
        render_row(header, "th", visitor)?;
        write!(visitor.writer_mut(), "</thead>")?;
    }

    write!(visitor.writer_mut(), "<tbody>")?;
    for row in &table.rows {
        render_row(row, "td", visitor)?;
    }
    write!(visitor.writer_mut(), "</tbody>")?;

    if let Some(footer) = &table.footer {
        write!(visitor.writer_mut(), "<tfoot>")?;
        render_row(footer, "td", visitor)?;
        write!(visitor.writer_mut(), "</tfoot>")?;
    }

    write!(visitor.writer_mut(), "</table>")?;

    Ok(())
}
