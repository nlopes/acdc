use acdc_converters_common::visitor::{WritableVisitor, WritableVisitorExt};
use acdc_parser::{
    AttributeValue, Block, BlockMetadata, ColumnFormat, HorizontalAlignment, InlineNode, Table,
    VerticalAlignment,
};

use crate::{Error, Processor, RenderOptions};

/// Convert horizontal alignment to CSS class name
fn halign_class(halign: HorizontalAlignment) -> &'static str {
    match halign {
        HorizontalAlignment::Left => "halign-left",
        HorizontalAlignment::Center => "halign-center",
        HorizontalAlignment::Right => "halign-right",
    }
}

/// Convert vertical alignment to CSS class name
fn valign_class(valign: VerticalAlignment) -> &'static str {
    match valign {
        VerticalAlignment::Top => "valign-top",
        VerticalAlignment::Middle => "valign-middle",
        VerticalAlignment::Bottom => "valign-bottom",
    }
}

/// Get column format for a given column index, defaulting to left/top if not specified
fn get_column_format(columns: &[ColumnFormat], col_index: usize) -> ColumnFormat {
    columns.get(col_index).cloned().unwrap_or_default()
}

/// Render cell content with support for nested blocks
/// `wrap_paragraph` controls whether paragraphs get <p class="tableblock"> wrappers.
/// Headers should NOT have wrappers, body cells should have them.
fn render_cell_content<V>(
    blocks: &[Block],
    visitor: &mut V,
    _processor: &Processor,
    _options: &RenderOptions,
    wrap_paragraph: bool,
) -> Result<(), Error>
where
    V: WritableVisitor<Error = Error>,
{
    for block in blocks {
        // For paragraphs in table cells, use <p class="tableblock"> for body cells only
        if let Block::Paragraph(para) = block {
            if wrap_paragraph {
                let writer = visitor.writer_mut();
                write!(writer, "<p class=\"tableblock\">")?;
                let _ = writer;
                visitor.visit_inline_nodes(&para.content)?;
                let writer = visitor.writer_mut();
                write!(writer, "</p>")?;
            } else {
                // Header cells: output content directly without <p> wrapper
                visitor.visit_inline_nodes(&para.content)?;
            }
        } else {
            // For other block types, use visitor
            visitor.visit_block(block)?;
        }
    }
    Ok(())
}

/// Render table caption with number if title exists
fn render_table_caption<V>(
    visitor: &mut V,
    title: &[InlineNode],
    processor: &Processor,
) -> Result<(), Error>
where
    V: WritableVisitor<Error = Error>,
{
    if !title.is_empty() {
        let count = processor.table_counter.get() + 1;
        processor.table_counter.set(count);
        let caption = processor
            .document_attributes
            .get("table-caption")
            .and_then(|v| match v {
                AttributeValue::String(s) => Some(s.as_str()),
                AttributeValue::Bool(_) | AttributeValue::None | AttributeValue::Inlines(_) => None,
            })
            .unwrap_or("Table");
        visitor.render_title_with_wrapper(
            title,
            &format!("<caption class=\"title\">{caption} {count}. "),
            "</caption>\n",
        )?;
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
    title: &[InlineNode],
) -> Result<(), Error>
where
    V: WritableVisitor<Error = Error>,
{
    let mut writer = visitor.writer_mut();
    let classes = ["tableblock", "frame-all", "grid-all", "stretch"];

    writeln!(writer, "<table class=\"{}\">", classes.join(" "))?;

    // Render caption with table number if title exists
    let _ = writer;
    render_table_caption(visitor, title, processor)?;
    writer = visitor.writer_mut();

    // Generate colgroup - either from cols attribute or inferred from table structure
    let col_count = if let Some(cols_value) = metadata.attributes.get("cols") {
        let cols_str = match cols_value {
            AttributeValue::String(s) => s.trim_matches('"'),
            AttributeValue::Bool(_) | AttributeValue::None | AttributeValue::Inlines(_) => "",
        };

        // Handle multiplier syntax like "3*" or "2*~"
        if let Some(asterisk_pos) = cols_str.find('*') {
            let count_str = &cols_str[..asterisk_pos];
            count_str.parse::<usize>().unwrap_or(1)
        } else {
            // Regular comma-separated format
            cols_str.split(',').count()
        }
    } else {
        // Infer column count from header or first row
        if let Some(header) = &table.header {
            header.columns.len()
        } else if let Some(first_row) = table.rows.first() {
            first_row.columns.len()
        } else {
            0
        }
    };

    if col_count > 0 {
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
        for (col_index, cell) in header.columns.iter().enumerate() {
            let spec = get_column_format(&table.columns, col_index);
            let halign = halign_class(spec.halign);
            let valign = valign_class(spec.valign);
            let writer = visitor.writer_mut();
            write!(writer, "<th class=\"tableblock {halign} {valign}\">")?;
            let _ = writer;
            render_cell_content(&cell.content, visitor, processor, options, false)?;
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
        for (col_index, cell) in row.columns.iter().enumerate() {
            let spec = get_column_format(&table.columns, col_index);
            let halign = halign_class(spec.halign);
            let valign = valign_class(spec.valign);
            let writer = visitor.writer_mut();
            write!(writer, "<td class=\"tableblock {halign} {valign}\">")?;
            let _ = writer;
            render_cell_content(&cell.content, visitor, processor, options, true)?;
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
        for (col_index, cell) in footer.columns.iter().enumerate() {
            let spec = get_column_format(&table.columns, col_index);
            let halign = halign_class(spec.halign);
            let valign = valign_class(spec.valign);
            let writer = visitor.writer_mut();
            write!(writer, "<td class=\"tableblock {halign} {valign}\">")?;
            let _ = writer;
            render_cell_content(&cell.content, visitor, processor, options, true)?;
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
