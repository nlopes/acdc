use acdc_converters_core::table::calculate_column_widths;
use acdc_converters_core::visitor::{WritableVisitor, WritableVisitorExt};
use acdc_parser::{
    Block, BlockMetadata, ColumnFormat, ColumnStyle, HorizontalAlignment, InlineNode, Table,
    TableColumn, VerticalAlignment,
};

use crate::{Error, HtmlVariant, Processor, RenderOptions};

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

/// Get effective alignment for a cell, considering cell-level overrides.
fn get_effective_halign(
    columns: &[ColumnFormat],
    col_index: usize,
    cell: &TableColumn,
) -> HorizontalAlignment {
    cell.halign.unwrap_or_else(|| {
        columns
            .get(col_index)
            .map_or_else(HorizontalAlignment::default, |c| c.halign)
    })
}

/// Get effective vertical alignment for a cell, considering cell-level overrides.
fn get_effective_valign(
    columns: &[ColumnFormat],
    col_index: usize,
    cell: &TableColumn,
) -> VerticalAlignment {
    cell.valign.unwrap_or_else(|| {
        columns
            .get(col_index)
            .map_or_else(VerticalAlignment::default, |c| c.valign)
    })
}

/// Get effective style for a cell, considering cell-level overrides.
/// Returns `None` if the effective style is `Default` (no wrapper needed).
fn get_effective_style(
    columns: &[ColumnFormat],
    col_index: usize,
    cell: &TableColumn,
) -> Option<ColumnStyle> {
    cell.style.or_else(|| {
        columns.get(col_index).and_then(|c| {
            if c.style == ColumnStyle::Default {
                None
            } else {
                Some(c.style)
            }
        })
    })
}

/// Format colspan/rowspan attributes for a table cell.
/// Returns an empty string if both are 1 (default).
fn format_span_attrs(cell: &TableColumn) -> String {
    use std::fmt::Write;
    let mut attrs = String::new();
    if cell.colspan > 1 {
        let _ = write!(attrs, " colspan=\"{}\"", cell.colspan);
    }
    if cell.rowspan > 1 {
        let _ = write!(attrs, " rowspan=\"{}\"", cell.rowspan);
    }
    attrs
}

/// Render cell content with support for nested blocks and cell styles.
///
/// # Arguments
/// * `blocks` - The content blocks to render
/// * `visitor` - The HTML visitor
/// * `wrap_paragraph` - Whether paragraphs get `<p class="tableblock">` wrappers
/// * `style` - Optional cell style (Strong, Emphasis, Monospace, Literal, Header, `AsciiDoc`)
fn render_cell_content<V>(
    blocks: &[Block],
    visitor: &mut V,
    _processor: &Processor,
    _options: &RenderOptions,
    wrap_paragraph: bool,
    style: Option<ColumnStyle>,
) -> Result<(), Error>
where
    V: WritableVisitor<Error = Error>,
{
    for block in blocks {
        // For paragraphs in table cells, use <p class="tableblock"> for body cells only
        if let Block::Paragraph(para) = block {
            // Literal style uses different structure entirely
            if style == Some(ColumnStyle::Literal) {
                let writer = visitor.writer_mut();
                write!(writer, "<div class=\"literal\"><pre>")?;
                let _ = writer;
                visitor.visit_inline_nodes(&para.content)?;
                let writer = visitor.writer_mut();
                write!(writer, "</pre></div>")?;
            } else if wrap_paragraph {
                let writer = visitor.writer_mut();
                write!(writer, "<p class=\"tableblock\">")?;
                let _ = writer;

                // Apply style wrapper inside the paragraph
                render_styled_content(visitor, &para.content, style)?;

                let writer = visitor.writer_mut();
                write!(writer, "</p>")?;
            } else {
                // Header cells: output content directly without <p> wrapper
                render_styled_content(visitor, &para.content, style)?;
            }
        } else {
            // For other block types, use visitor
            visitor.visit_block(block)?;
        }
    }
    Ok(())
}

/// Render inline content with optional style wrappers.
fn render_styled_content<V>(
    visitor: &mut V,
    content: &[InlineNode],
    style: Option<ColumnStyle>,
) -> Result<(), Error>
where
    V: WritableVisitor<Error = Error>,
{
    match style {
        Some(ColumnStyle::Strong) => {
            let writer = visitor.writer_mut();
            write!(writer, "<strong>")?;
            let _ = writer;
            visitor.visit_inline_nodes(content)?;
            let writer = visitor.writer_mut();
            write!(writer, "</strong>")?;
        }
        Some(ColumnStyle::Emphasis) => {
            let writer = visitor.writer_mut();
            write!(writer, "<em>")?;
            let _ = writer;
            visitor.visit_inline_nodes(content)?;
            let writer = visitor.writer_mut();
            write!(writer, "</em>")?;
        }
        Some(ColumnStyle::Monospace) => {
            let writer = visitor.writer_mut();
            write!(writer, "<code>")?;
            let _ = writer;
            visitor.visit_inline_nodes(content)?;
            let writer = visitor.writer_mut();
            write!(writer, "</code>")?;
        }
        // Default, Header, AsciiDoc, Literal (handled elsewhere) - no content wrapper
        // Wildcard handles any future non-exhaustive variants
        Some(
            ColumnStyle::Default
            | ColumnStyle::Header
            | ColumnStyle::AsciiDoc
            | ColumnStyle::Literal
            | _,
        )
        | None => {
            visitor.visit_inline_nodes(content)?;
        }
    }
    Ok(())
}

/// Render table caption with number if title exists.
///
/// Per-block `[caption="..."]` attribute overrides the prefix entirely and does NOT increment
/// the table counter (following `AsciiDoc` specification).
///
/// Caption can be disabled with:
/// - `:table-caption!:` at document level (disables for all tables)
/// - `[caption=""]` at block level (disables for specific table)
fn render_table_caption<V>(
    visitor: &mut V,
    title: &[InlineNode],
    processor: &Processor,
    metadata: &BlockMetadata,
) -> Result<(), Error>
where
    V: WritableVisitor<Error = Error>,
{
    if !title.is_empty() {
        // Check for per-block caption override (does NOT increment counter)
        let prefix = if let Some(custom_caption) = metadata.attributes.get_string("caption") {
            if custom_caption.is_empty() {
                String::new()
            } else {
                custom_caption
            }
        } else {
            processor.caption_prefix("table-caption", &processor.table_counter, "Table")
        };

        visitor.render_title_with_wrapper(
            title,
            &format!("<caption class=\"title\">{prefix}"),
            "</caption>\n",
        )?;
    }
    Ok(())
}

/// Render colgroup with column width styles
fn render_colgroup<W: std::io::Write + ?Sized>(
    writer: &mut W,
    table: &Table,
    metadata: &BlockMetadata,
) -> Result<(), Error> {
    // Generate colgroup - either from cols attribute or inferred from table structure
    let col_count = if let Some(cols_str) = metadata.attributes.get_string("cols") {
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
        writeln!(writer, "<colgroup>")?;

        // Use parsed column widths if available, otherwise fall back to equal distribution
        let widths = if table.columns.is_empty() {
            // Fall back to equal distribution
            let width = 100.0 / f64::from(u32::try_from(col_count).unwrap_or(1));
            vec![width; col_count]
        } else {
            calculate_column_widths(&table.columns)
        };

        for width in widths {
            if width == 0.0 {
                // Auto width - let the browser decide
                writeln!(writer, "<col>")?;
            } else if (width - width.round()).abs() < 0.0001 {
                // Match asciidoctor's 4-decimal precision for non-round percentages
                writeln!(writer, "<col style=\"width: {width:.0}%;\">")?;
            } else {
                writeln!(writer, "<col style=\"width: {width:.4}%;\">")?;
            }
        }
        writeln!(writer, "</colgroup>")?;
    }

    Ok(())
}

/// Get frame class from metadata (default: all).
fn get_frame_class(metadata: &BlockMetadata) -> &'static str {
    metadata
        .attributes
        .get_string("frame")
        .map_or("frame-all", |frame| match frame.as_str() {
            "ends" | "topbot" => "frame-ends",
            "sides" => "frame-sides",
            "none" => "frame-none",
            _ => "frame-all",
        })
}

/// Get grid class from metadata (default: all).
fn get_grid_class(metadata: &BlockMetadata) -> &'static str {
    metadata
        .attributes
        .get_string("grid")
        .map_or("grid-all", |grid| match grid.as_str() {
            "rows" => "grid-rows",
            "cols" => "grid-cols",
            "none" => "grid-none",
            _ => "grid-all",
        })
}

/// Get stripes class from metadata (only if specified).
fn get_stripes_class(metadata: &BlockMetadata) -> Option<&'static str> {
    metadata
        .attributes
        .get_string("stripes")
        .and_then(|stripes| match stripes.as_str() {
            "even" => Some("stripes-even"),
            "odd" => Some("stripes-odd"),
            "all" => Some("stripes-all"),
            "hover" => Some("stripes-hover"),
            _ => None,
        })
}

/// Get width style from metadata (returns empty string if not specified).
fn get_width_style(metadata: &BlockMetadata) -> String {
    metadata
        .attributes
        .get_string("width")
        .map_or_else(String::new, |w| format!(" style=\"width: {w};\""))
}

/// Get sizing class based on %autowidth option.
fn get_sizing_class(metadata: &BlockMetadata) -> &'static str {
    if metadata.options.contains(&"autowidth".to_string()) {
        "fit-content"
    } else {
        "stretch"
    }
}

/// Render a single body cell with appropriate tag and style.
fn render_body_cell<V>(
    cell: &TableColumn,
    col_index: usize,
    columns: &[ColumnFormat],
    visitor: &mut V,
    processor: &Processor,
    options: &RenderOptions,
    semantic: bool,
) -> Result<(), Error>
where
    V: WritableVisitor<Error = Error>,
{
    let halign = halign_class(get_effective_halign(columns, col_index, cell));
    let valign = valign_class(get_effective_valign(columns, col_index, cell));
    let style = get_effective_style(columns, col_index, cell);
    let span_attrs = format_span_attrs(cell);

    // Header-styled cells in body use <th> instead of <td>
    let tag = if style == Some(ColumnStyle::Header) {
        "th"
    } else {
        "td"
    };

    let cell_class_prefix = if semantic { "" } else { "tableblock " };
    let writer = visitor.writer_mut();
    write!(
        writer,
        "<{tag} class=\"{cell_class_prefix}{halign} {valign}\"{span_attrs}>"
    )?;
    let _ = writer;
    render_cell_content(&cell.content, visitor, processor, options, !semantic, style)?;
    let writer = visitor.writer_mut();
    writeln!(writer, "</{tag}>")?;
    Ok(())
}

/// Render table with support for nested blocks in cells
#[allow(clippy::too_many_lines)]
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
    let semantic = processor.variant() == HtmlVariant::Semantic;
    let writer = visitor.writer_mut();

    // Build table classes
    let frame = get_frame_class(metadata);
    let grid = get_grid_class(metadata);
    let sizing = get_sizing_class(metadata);

    // Semantic mode: wrap in <div class="table-block">, no "tableblock" prefix on table
    let mut class_parts = if semantic {
        writeln!(writer, "<div class=\"table-block\">")?;
        format!("{frame} {grid} {sizing}")
    } else {
        format!("tableblock {frame} {grid} {sizing}")
    };

    // Add stripes class if specified
    if let Some(stripes) = get_stripes_class(metadata) {
        class_parts.push(' ');
        class_parts.push_str(stripes);
    }

    // Add custom roles/classes from metadata
    for role in &metadata.roles {
        class_parts.push(' ');
        class_parts.push_str(role);
    }

    // Get width style
    let width_style = get_width_style(metadata);

    writeln!(writer, "<table class=\"{class_parts}\"{width_style}>")?;

    // Render caption with table number if title exists
    let _ = writer;
    render_table_caption(visitor, title, processor, metadata)?;

    // Render colgroup with column widths
    render_colgroup(visitor.writer_mut(), table, metadata)?;

    // Cell class prefix: "tableblock " for standard, "" for semantic
    let cell_class_prefix = if semantic { "" } else { "tableblock " };

    // Render header
    if let Some(header) = &table.header {
        let writer = visitor.writer_mut();
        writeln!(writer, "<thead>")?;
        writeln!(writer, "<tr>")?;
        let _ = writer;
        for (col_index, cell) in header.columns.iter().enumerate() {
            let halign = halign_class(get_effective_halign(&table.columns, col_index, cell));
            let valign = valign_class(get_effective_valign(&table.columns, col_index, cell));
            let style = get_effective_style(&table.columns, col_index, cell);
            let span_attrs = format_span_attrs(cell);
            let writer = visitor.writer_mut();
            write!(
                writer,
                "<th class=\"{cell_class_prefix}{halign} {valign}\"{span_attrs}>"
            )?;
            let _ = writer;
            render_cell_content(&cell.content, visitor, processor, options, false, style)?;
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
            render_body_cell(
                cell,
                col_index,
                &table.columns,
                visitor,
                processor,
                options,
                semantic,
            )?;
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
            let halign = halign_class(get_effective_halign(&table.columns, col_index, cell));
            let valign = valign_class(get_effective_valign(&table.columns, col_index, cell));
            let style = get_effective_style(&table.columns, col_index, cell);
            let span_attrs = format_span_attrs(cell);
            let writer = visitor.writer_mut();
            write!(
                writer,
                "<td class=\"{cell_class_prefix}{halign} {valign}\"{span_attrs}>"
            )?;
            let _ = writer;
            render_cell_content(&cell.content, visitor, processor, options, !semantic, style)?;
            let writer = visitor.writer_mut();
            writeln!(writer, "</td>")?;
        }
        let writer = visitor.writer_mut();
        writeln!(writer, "</tr>")?;
        writeln!(writer, "</tfoot>")?;
    }

    let writer = visitor.writer_mut();
    writeln!(writer, "</table>")?;
    if semantic {
        writeln!(writer, "</div>")?;
    }
    Ok(())
}
