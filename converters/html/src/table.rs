use acdc_converters_common::visitor::WritableVisitor;
use acdc_parser::{AttributeValue, Block, BlockMetadata, Table};

use crate::{Error, Processor, RenderOptions};

/// Render cell content with support for nested blocks
fn render_cell_content<V>(
    blocks: &[Block],
    visitor: &mut V,
    _processor: &Processor,
    _options: &RenderOptions,
) -> Result<(), Error>
where
    V: WritableVisitor<Error = Error>,
{
    for block in blocks {
        // For paragraphs in table cells, use <p class="tableblock"> instead of the default paragraph rendering
        if let Block::Paragraph(para) = block {
            let writer = visitor.writer_mut();
            write!(writer, "<p class=\"tableblock\">")?;
            let _ = writer;
            visitor.visit_inline_nodes(&para.content)?;
            let writer = visitor.writer_mut();
            write!(writer, "</p>")?;
        } else {
            // For other block types, use visitor
            visitor.visit_block(block)?;
        }
    }
    Ok(())
}

/// Render table with support for nested blocks in cells
pub(crate) fn render_table<V>(
    table: &Table,
    visitor: &mut V,
    processor: &Processor,
    options: &RenderOptions,
    metadata: &BlockMetadata,
) -> Result<(), Error>
where
    V: WritableVisitor<Error = Error>,
{
    let writer = visitor.writer_mut();
    let classes = ["tableblock", "frame-all", "grid-all", "stretch"];

    writeln!(writer, "<table class=\"{}\">", classes.join(" "))?;

    // Generate colgroup if cols attribute exists
    if let Some(cols_value) = metadata.attributes.get("cols") {
        let cols_str = match cols_value {
            AttributeValue::String(s) => s.as_str(),
            _ => "",
        };
        let col_count = cols_str.split(',').count();
        let col_count_f64 = f64::from(u32::try_from(col_count).unwrap_or(1));
        let width_percent = 100.0 / col_count_f64;
        writeln!(writer, "<colgroup>")?;
        for _ in 0..col_count {
            writeln!(writer, "<col style=\"width: {width_percent:.0}%;\" />")?;
        }
        writeln!(writer, "</colgroup>")?;
    }

    // Render header
    if let Some(header) = &table.header {
        writeln!(writer, "<thead>")?;
        writeln!(writer, "<tr>")?;
        let _ = writer;
        for cell in &header.columns {
            let writer = visitor.writer_mut();
            write!(writer, "<th class=\"tableblock halign-left valign-top\">")?;
            let _ = writer;
            render_cell_content(&cell.content, visitor, processor, options)?;
            let writer = visitor.writer_mut();
            writeln!(writer, "</th>")?;
        }
        let writer = visitor.writer_mut();
        writeln!(writer, "</tr>")?;
        writeln!(writer, "</thead>")?;
    }

    // Render body
    let writer = visitor.writer_mut();
    writeln!(writer, "<tbody>")?;
    let _ = writer;
    for row in &table.rows {
        let writer = visitor.writer_mut();
        writeln!(writer, "<tr>")?;
        let _ = writer;
        for cell in &row.columns {
            let writer = visitor.writer_mut();
            write!(writer, "<td class=\"tableblock halign-left valign-top\">")?;
            let _ = writer;
            render_cell_content(&cell.content, visitor, processor, options)?;
            let writer = visitor.writer_mut();
            writeln!(writer, "</td>")?;
        }
        let writer = visitor.writer_mut();
        writeln!(writer, "</tr>")?;
    }
    let writer = visitor.writer_mut();
    writeln!(writer, "</tbody>")?;

    // Render footer if present
    if let Some(footer) = &table.footer {
        let writer = visitor.writer_mut();
        writeln!(writer, "<tfoot>")?;
        writeln!(writer, "<tr>")?;
        let _ = writer;
        for cell in &footer.columns {
            let writer = visitor.writer_mut();
            write!(writer, "<td class=\"tableblock halign-left valign-top\">")?;
            let _ = writer;
            render_cell_content(&cell.content, visitor, processor, options)?;
            let writer = visitor.writer_mut();
            writeln!(writer, "</td>")?;
        }
        let writer = visitor.writer_mut();
        writeln!(writer, "</tr>")?;
        writeln!(writer, "</tfoot>")?;
    }

    let writer = visitor.writer_mut();
    writeln!(writer, "</table>")?;
    Ok(())
}
