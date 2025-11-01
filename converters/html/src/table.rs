use std::io::Write;

use acdc_parser::{AttributeValue, Block, BlockMetadata, Table};

use crate::{Processor, Render, RenderOptions};

pub(crate) fn render_table_with_metadata<W: Write>(
    table: &Table,
    w: &mut W,
    processor: &Processor,
    options: &RenderOptions,
    metadata: &BlockMetadata,
) -> Result<(), crate::Error> {
    // Generate table classes based on metadata
    let classes = ["tableblock", "frame-all", "grid-all", "stretch"];

    writeln!(w, "<table class=\"{}\">", classes.join(" "))?;

    // Generate colgroup if cols attribute exists
    if let Some(cols_value) = metadata.attributes.get("cols") {
        // Parse cols attribute - it's a comma-separated list of column specs
        // For now, we'll just count them and generate equal widths
        let cols_str = match cols_value {
            AttributeValue::String(s) => s.as_str(),
            _ => "",
        };
        let col_count = cols_str.split(',').count();
        let col_count_f64 = f64::from(u32::try_from(col_count).unwrap_or(1));
        let width_percent = 100.0 / col_count_f64;
        writeln!(w, "<colgroup>")?;
        for _ in 0..col_count {
            writeln!(w, "<col style=\"width: {width_percent:.0}%;\" />")?;
        }
        writeln!(w, "</colgroup>")?;
    }

    // Render header
    if let Some(header) = &table.header {
        writeln!(w, "<thead>")?;
        writeln!(w, "<tr>")?;
        for cell in &header.columns {
            write!(w, "<th class=\"tableblock halign-left valign-top\">")?;
            render_cell_content(&cell.content, w, processor, options)?;
            writeln!(w, "</th>")?;
        }
        writeln!(w, "</tr>")?;
        writeln!(w, "</thead>")?;
    }

    // Render body
    writeln!(w, "<tbody>")?;
    for row in &table.rows {
        writeln!(w, "<tr>")?;
        for cell in &row.columns {
            write!(w, "<td class=\"tableblock halign-left valign-top\">")?;
            render_cell_content(&cell.content, w, processor, options)?;
            writeln!(w, "</td>")?;
        }
        writeln!(w, "</tr>")?;
    }
    writeln!(w, "</tbody>")?;

    // Render footer if present
    if let Some(footer) = &table.footer {
        writeln!(w, "<tfoot>")?;
        writeln!(w, "<tr>")?;
        for cell in &footer.columns {
            write!(w, "<td class=\"tableblock halign-left valign-top\">")?;
            render_cell_content(&cell.content, w, processor, options)?;
            writeln!(w, "</td>")?;
        }
        writeln!(w, "</tr>")?;
        writeln!(w, "</tfoot>")?;
    }

    writeln!(w, "</table>")?;
    Ok(())
}

/// Render cell content as paragraphs with tableblock class
fn render_cell_content<W: Write>(
    blocks: &[Block],
    w: &mut W,
    processor: &Processor,
    options: &RenderOptions,
) -> Result<(), crate::Error> {
    for block in blocks {
        // For paragraphs in table cells, use <p class="tableblock"> instead of the default paragraph rendering
        if let Block::Paragraph(para) = block {
            write!(w, "<p class=\"tableblock\">")?;
            crate::inlines::render_inlines(&para.content, w, processor, options)?;
            writeln!(w, "</p>")?;
        } else {
            // For other block types, render normally
            block.render(w, processor, options)?;
        }
    }
    Ok(())
}
