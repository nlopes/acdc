use std::io::Write;

use acdc_converters_core::{TraversalContext, table::column_widths};
use acdc_parser::{
    CaptionKind, ColumnStyle, HorizontalAlignment, TableFrame, TableGrid, TablePresentation,
    TableStripes, VerticalAlignment,
    ast::{
        AttributeValueRef, AttributesRef, BlockKind, ColumnFormatRef, ContentRef, InlineKind,
        TableCellRef, TableRef,
    },
};

use crate::{
    Converter, Error, HtmlVariant, HtmlVisitor,
    inlines::{escape_attribute, escape_pcdata},
};

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
    column: Option<ColumnFormatRef<'_>>,
    cell: TableCellRef<'_>,
) -> HorizontalAlignment {
    cell.halign().unwrap_or_else(|| {
        column.map_or_else(HorizontalAlignment::default, ColumnFormatRef::halign)
    })
}

/// Get effective vertical alignment for a cell, considering cell-level overrides.
fn get_effective_valign(
    column: Option<ColumnFormatRef<'_>>,
    cell: TableCellRef<'_>,
) -> VerticalAlignment {
    cell.valign()
        .unwrap_or_else(|| column.map_or_else(VerticalAlignment::default, ColumnFormatRef::valign))
}

/// Uses the cell style, falling back to a nondefault column style.
fn get_effective_style(
    column: Option<ColumnFormatRef<'_>>,
    cell: TableCellRef<'_>,
) -> Option<ColumnStyle> {
    cell.style().or_else(|| {
        column
            .map(ColumnFormatRef::style)
            .filter(|style| *style != ColumnStyle::Default)
    })
}

/// Format colspan/rowspan attributes for a table cell.
/// Returns an empty string if both are 1 (default).
fn format_span_attrs(cell: TableCellRef<'_>) -> String {
    use std::fmt::Write;
    let mut attrs = String::new();
    if cell.colspan() > 1 {
        let _ = write!(attrs, " colspan=\"{}\"", cell.colspan());
    }
    if cell.rowspan() > 1 {
        let _ = write!(attrs, " rowspan=\"{}\"", cell.rowspan());
    }
    attrs
}

/// Whether inline content is effectively empty (no nodes, or only whitespace
/// plain text). asciidoctor renders such a body cell as an empty `<td>` with no
/// `<p class="tableblock">` wrapper — e.g. a blank cell or one containing only
/// `{empty}`.
fn inline_nodes_blank(content: ContentRef<'_>) -> bool {
    content.inlines().all(|node| {
        if let InlineKind::Plain(plain) = node.kind() {
            plain.text().trim().is_empty()
        } else {
            false
        }
    })
}

/// Render cell content with support for nested blocks and cell styles.
///
/// # Arguments
/// * `column` - The cell and its nested attribute initialization
/// * `visitor` - The HTML visitor
/// * `wrap_paragraph` - Whether paragraphs get `<p class="tableblock">` wrappers
/// * `style` - Optional cell style (Strong, Emphasis, Monospace, Literal, Header, `AsciiDoc`)
/// * `header` - Whether to flatten paragraph boundaries into header text
fn render_cell_content<'a, W: Write>(
    traversal: &mut TraversalContext<'a>,
    cell: TableCellRef<'a>,
    visitor: &mut HtmlVisitor<'a, '_, W>,
    wrap_paragraph: bool,
    style: Option<ColumnStyle>,
    header: bool,
) -> Result<(), Error> {
    if style == Some(ColumnStyle::AsciiDoc) {
        // AsciiDoc cells contain full blocks. Their visitor preserves paragraph
        // metadata and substitution settings within the cell's attribute scope.
        return traversal.with_table_cell(cell, |traversal| {
            if wrap_paragraph {
                writeln!(visitor.writer_mut(), "<div class=\"content\">")?;
            }
            traversal.visit_blocks(visitor, cell.blocks())?;
            visitor.render_footnotes(traversal, cell.footnotes(), true)?;
            if wrap_paragraph {
                writeln!(visitor.writer_mut(), "</div>")?;
            }
            Ok(())
        });
    }
    // Bare cell text is sufficient for one paragraph. Multiple blocks need
    // paragraph boundaries even when the variant omits table paragraph classes.
    let has_multiple_blocks = cell
        .blocks()
        .filter(|block| {
            !matches!(
                block.kind(),
                BlockKind::Comment(_) | BlockKind::DocumentAttribute(_)
            )
        })
        .take(2)
        .count()
        == 2;
    let needs_paragraphs = wrap_paragraph || (has_multiple_blocks && !header);
    for (index, block) in cell.blocks().enumerate() {
        if let BlockKind::Paragraph(para) = block.kind() {
            // Header cells use one text flow, but source paragraph breaks
            // must still separate words when the browser collapses whitespace.
            if header && index > 0 {
                writeln!(visitor.writer_mut())?;
            }
            if style == Some(ColumnStyle::Literal) {
                let writer = visitor.writer_mut();
                write!(writer, "<div class=\"literal\"><pre>")?;
                let _ = writer;
                for node in para.content().inlines() {
                    if let InlineKind::Verbatim(verbatim) = node.kind() {
                        write!(visitor.writer_mut(), "{}", escape_pcdata(verbatim.text()))?;
                    } else {
                        visitor.visit_inline_ref(traversal, node)?;
                    }
                }
                let writer = visitor.writer_mut();
                write!(writer, "</pre></div>")?;
            } else if needs_paragraphs {
                // A blank body cell (empty or only `{empty}`) renders as an
                // empty <td> with no <p class="tableblock"> wrapper.
                if inline_nodes_blank(para.content()) {
                    continue;
                }
                let writer = visitor.writer_mut();
                let open = if wrap_paragraph {
                    "<p class=\"tableblock\">"
                } else {
                    "<p>"
                };
                write!(writer, "{open}")?;
                let _ = writer;

                render_styled_content(traversal, visitor, para.content(), style)?;

                let writer = visitor.writer_mut();
                write!(writer, "</p>")?;
            } else {
                render_styled_content(traversal, visitor, para.content(), style)?;
            }
        } else {
            traversal.visit_block(visitor, block)?;
        }
    }
    visitor.render_footnotes(traversal, cell.footnotes(), true)
}

/// Render inline content with optional style wrappers.
fn render_styled_content<'a, W: Write>(
    traversal: &mut TraversalContext<'a>,
    visitor: &mut HtmlVisitor<'a, '_, W>,
    content: ContentRef<'_>,
    style: Option<ColumnStyle>,
) -> Result<(), Error> {
    match style {
        Some(ColumnStyle::Strong) => {
            let writer = visitor.writer_mut();
            write!(writer, "<strong>")?;
            let _ = writer;
            visitor.visit_content(traversal, content)?;
            let writer = visitor.writer_mut();
            write!(writer, "</strong>")?;
        }
        Some(ColumnStyle::Emphasis) => {
            let writer = visitor.writer_mut();
            write!(writer, "<em>")?;
            let _ = writer;
            visitor.visit_content(traversal, content)?;
            let writer = visitor.writer_mut();
            write!(writer, "</em>")?;
        }
        Some(ColumnStyle::Monospace) => {
            let writer = visitor.writer_mut();
            write!(writer, "<code>")?;
            let _ = writer;
            visitor.visit_content(traversal, content)?;
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
            visitor.visit_content(traversal, content)?;
        }
    }
    Ok(())
}

/// Render colgroup with column width styles
fn render_colgroup<W: Write + ?Sized>(writer: &mut W, table: TableRef<'_>) -> Result<(), Error> {
    // Generate colgroup - either from cols attribute or inferred from table structure
    let col_count = if let Some(cols_str) = table
        .attributes()
        .get("cols")
        .and_then(AttributeValueRef::as_str)
    {
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
        if let Some(header) = table.header() {
            header.cells().count()
        } else if let Some(first_row) = table.rows().next() {
            first_row.cells().count()
        } else {
            0
        }
    };

    if col_count > 0 {
        writeln!(writer, "<colgroup>")?;

        // Use parsed column widths if available, otherwise fall back to equal distribution
        let widths = if table.columns().next().is_none() {
            // Fall back to equal distribution
            let width = 100.0 / f64::from(u32::try_from(col_count).unwrap_or(1));
            vec![Some(width); col_count]
        } else {
            column_widths(table.columns())
        };

        for width in widths {
            let Some(width) = width else {
                writeln!(writer, "<col>")?;
                continue;
            };
            if (width - width.round()).abs() < 0.0001 {
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

fn frame_class(frame: TableFrame) -> &'static str {
    match frame {
        TableFrame::Ends => "frame-ends",
        TableFrame::Sides => "frame-sides",
        TableFrame::None => "frame-none",
        TableFrame::All | _ => "frame-all",
    }
}

fn grid_class(grid: TableGrid) -> &'static str {
    match grid {
        TableGrid::Rows => "grid-rows",
        TableGrid::Columns => "grid-cols",
        TableGrid::None => "grid-none",
        TableGrid::All | _ => "grid-all",
    }
}

fn stripes_class(stripes: TableStripes) -> Option<&'static str> {
    match stripes {
        TableStripes::All => Some("stripes-all"),
        TableStripes::Odd => Some("stripes-odd"),
        TableStripes::Even => Some("stripes-even"),
        TableStripes::Hover => Some("stripes-hover"),
        TableStripes::None | _ => None,
    }
}

/// Compute the table sizing class and inline width style, matching asciidoctor:
/// - explicit `width=100%` (or `100`) → `stretch` class, no inline style;
/// - any other explicit `width` → inline `style="width: N;"`, no sizing class;
/// - `%autowidth` with no `width` → `fit-content` class;
/// - otherwise (default full width) → `stretch` class.
fn table_sizing(attributes: AttributesRef<'_>) -> (Option<&'static str>, String) {
    if let Some(width) = attributes.get("width").and_then(AttributeValueRef::as_str) {
        if width.trim_end_matches('%') == "100" {
            (Some("stretch"), String::new())
        } else {
            (
                None,
                format!(" style=\"width: {};\"", escape_attribute(width)),
            )
        }
    } else if matches!(attributes.get("options"), Some(AttributeValueRef::Tokens(options)) if options.contains("autowidth"))
    {
        (Some("fit-content"), String::new())
    } else {
        (Some("stretch"), String::new())
    }
}

/// Render a single body cell with appropriate tag and style.
fn render_body_cell<'a, W: Write>(
    traversal: &mut TraversalContext<'a>,
    cell: TableCellRef<'a>,
    column: Option<ColumnFormatRef<'_>>,
    visitor: &mut HtmlVisitor<'a, '_, W>,
    converter: &Converter<'_>,
) -> Result<(), Error> {
    let halign = halign_class(get_effective_halign(column, cell));
    let valign = valign_class(get_effective_valign(column, cell));
    let style = get_effective_style(column, cell);
    let span_attrs = format_span_attrs(cell);
    let semantic = converter.variant() == HtmlVariant::Semantic;

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
    render_cell_content(traversal, cell, visitor, !semantic, style, false)?;
    let writer = visitor.writer_mut();
    writeln!(writer, "</{tag}>")?;
    Ok(())
}

/// Render table with support for nested blocks in cells
#[allow(clippy::too_many_lines)]
pub(crate) fn render_table<'a, W: Write>(
    traversal: &mut TraversalContext<'a>,
    table: TableRef<'a>,
    visitor: &mut HtmlVisitor<'a, '_, W>,
    converter: &Converter<'_>,
) -> Result<(), Error> {
    let attributes = Some(table.attributes());
    let semantic = converter.variant() == HtmlVariant::Semantic;
    let writer = visitor.writer_mut();

    // Build table classes. Order matches asciidoctor:
    // frame, grid, stripes, sizing (stretch/fit-content), float, roles.
    let presentation = table.presentation().unwrap_or_else(|| {
        TablePresentation::resolve(attributes, converter.document_attributes().into())
    });
    let frame = frame_class(presentation.frame());
    let grid = grid_class(presentation.grid());
    let (sizing, width_style) = table_sizing(table.attributes());

    // Semantic mode: wrap in <div class="table-block">, no "tableblock" prefix on table
    let mut class_parts = if semantic {
        writeln!(writer, "<div class=\"table-block\">")?;
        format!("{frame} {grid}")
    } else {
        format!("tableblock {frame} {grid}")
    };

    // Add stripes class if specified
    if let Some(stripes) = stripes_class(presentation.stripes()) {
        class_parts.push(' ');
        class_parts.push_str(stripes);
    }

    // Add sizing class (absent when an explicit non-100% width is used)
    if let Some(sizing) = sizing {
        class_parts.push(' ');
        class_parts.push_str(sizing);
    }

    // Add float class (e.g. `center`) if specified
    if let Some(float) = table
        .attributes()
        .get("float")
        .and_then(AttributeValueRef::as_str)
    {
        class_parts.push(' ');
        class_parts.push_str(float);
    }

    // Add custom roles/classes from metadata
    for role in table.roles().iter() {
        class_parts.push(' ');
        class_parts.push_str(role);
    }

    write!(writer, "<table")?;
    crate::write_element_id(writer, table.id())?;
    let class_parts = crate::inlines::escape_role(&class_parts);
    writeln!(writer, " class=\"{class_parts}\"{width_style}>")?;

    // Render caption with table number if title exists
    let _ = writer;
    visitor.render_captioned_title_with_wrapper(
        traversal,
        table.title(),
        attributes,
        Some(CaptionKind::Table),
        "<caption class=\"title\">",
        "</caption>\n",
    )?;

    // Render colgroup with column widths
    render_colgroup(visitor.writer_mut(), table)?;

    // Cell class prefix: "tableblock " for standard, "" for semantic
    let cell_class_prefix = if semantic { "" } else { "tableblock " };

    // Render header
    if let Some(header) = table.header() {
        let writer = visitor.writer_mut();
        writeln!(writer, "<thead>")?;
        writeln!(writer, "<tr>")?;
        let _ = writer;
        for (col_index, cell) in header.cells().enumerate() {
            let halign = halign_class(get_effective_halign(table.columns().nth(col_index), cell));
            let valign = valign_class(get_effective_valign(table.columns().nth(col_index), cell));
            let span_attrs = format_span_attrs(cell);
            let writer = visitor.writer_mut();
            write!(
                writer,
                "<th class=\"{cell_class_prefix}{halign} {valign}\"{span_attrs}>"
            )?;
            let _ = writer;
            render_cell_content(traversal, cell, visitor, false, None, true)?;
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
    for row in table.rows() {
        let writer = visitor.writer_mut();
        writeln!(writer, "<tr>")?;
        let _ = writer;
        for (col_index, cell) in row.cells().enumerate() {
            render_body_cell(
                traversal,
                cell,
                table.columns().nth(col_index),
                visitor,
                converter,
            )?;
        }
        let writer = visitor.writer_mut();
        writeln!(writer, "</tr>")?;
    }
    let writer = visitor.writer_mut();
    writeln!(writer, "</tbody>")?;

    // Render footer if present
    if let Some(footer) = table.footer() {
        let writer = visitor.writer_mut();
        writeln!(writer, "<tfoot>")?;
        writeln!(writer, "<tr>")?;
        let _ = writer;
        for (col_index, cell) in footer.cells().enumerate() {
            let halign = halign_class(get_effective_halign(table.columns().nth(col_index), cell));
            let valign = valign_class(get_effective_valign(table.columns().nth(col_index), cell));
            let style = get_effective_style(table.columns().nth(col_index), cell);
            let span_attrs = format_span_attrs(cell);
            let writer = visitor.writer_mut();
            write!(
                writer,
                "<td class=\"{cell_class_prefix}{halign} {valign}\"{span_attrs}>"
            )?;
            let _ = writer;
            render_cell_content(traversal, cell, visitor, !semantic, style, false)?;
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
