//! Table rendering for manpages using the tbl preprocessor.
//!
//! Tables are rendered using `.TS`/`.TE` macros which are processed by the
//! `tbl` preprocessor before groff. Colspan and rowspan are supported via
//! per-row format lines using `s` (horizontal span) and `^` (vertical span).

use std::io::Write;

use acdc_converters_core::{
    Diagnostics, TraversalContext,
    table::{CellKind, GridRow, build_grid, calculate_column_widths, determine_column_count},
    visitor::{Visitor, WritableVisitor},
};
use acdc_parser::{
    BlockMetadata, ColumnFormat, ColumnStyle, ColumnWidth, DelimitedBlock, HorizontalAlignment,
    Table, TableColumn, TableFrame, TableGrid, TablePresentation, TableStripes, VerticalAlignment,
};

use crate::{Error, ManpageVisitor, Processor};

/// Map horizontal alignment to tbl format character.
fn alignment_prefix(halign: HorizontalAlignment) -> &'static str {
    match halign {
        HorizontalAlignment::Left => "l",
        HorizontalAlignment::Center => "c",
        HorizontalAlignment::Right => "r",
    }
}

fn vertical_alignment_modifier(valign: VerticalAlignment) -> &'static str {
    match valign {
        VerticalAlignment::Top => "t",
        VerticalAlignment::Middle => "",
        VerticalAlignment::Bottom => "d",
    }
}

fn effective_halign<'a>(
    cell: &'a TableColumn<'a>,
    column_index: usize,
    columns: &[ColumnFormat],
) -> HorizontalAlignment {
    cell.halign.unwrap_or_else(|| {
        columns
            .get(column_index)
            .map_or(HorizontalAlignment::Left, |c| c.halign)
    })
}

fn effective_valign<'a>(
    cell: &'a TableColumn<'a>,
    column_index: usize,
    columns: &[ColumnFormat],
) -> VerticalAlignment {
    cell.valign.unwrap_or_else(|| {
        columns
            .get(column_index)
            .map_or(VerticalAlignment::Top, |c| c.valign)
    })
}

fn effective_style<'a>(
    cell: &'a TableColumn<'a>,
    column_index: usize,
    columns: &[ColumnFormat],
) -> ColumnStyle {
    cell.style.unwrap_or_else(|| {
        columns
            .get(column_index)
            .map_or(ColumnStyle::Default, |column| column.style)
    })
}

/// Generate a tbl format entry for a single cell position.
fn format_entry(
    kind: &CellKind,
    row: &GridRow<'_>,
    column_index: usize,
    columns: &[ColumnFormat],
    width_modifiers: &[String],
) -> String {
    match kind {
        CellKind::Content { cell_index } => {
            let Some(cell) = row.ast_row.columns.get(*cell_index) else {
                return "l".to_string();
            };
            let align = alignment_prefix(effective_halign(cell, column_index, columns));
            let valign = vertical_alignment_modifier(effective_valign(cell, column_index, columns));
            let width = width_modifiers.get(column_index).map_or("", String::as_str);
            if row.is_header {
                format!("{align}{valign}B{width}")
            } else {
                format!("{align}{valign}{width}")
            }
        }
        CellKind::HSpan => "s".to_string(),
        CellKind::VSpan => "^".to_string(),
    }
}

fn has_hspan_origin(grid: &[GridRow<'_>], row_index: usize, column_index: usize) -> bool {
    let mut row_index = row_index;
    while row_index > 0 {
        row_index -= 1;
        match grid
            .get(row_index)
            .and_then(|row| row.cells.get(column_index))
        {
            Some(CellKind::VSpan) => {}
            Some(CellKind::HSpan) => return true,
            Some(CellKind::Content { .. }) | None => return false,
        }
    }
    false
}

fn format_row(
    grid: &[GridRow<'_>],
    row_index: usize,
    columns: &[ColumnFormat],
    width_modifiers: &[String],
    presentation: TablePresentation,
    allbox: bool,
) -> String {
    let Some(row) = grid.get(row_index) else {
        return String::new();
    };
    let side_frame = !allbox && matches!(presentation.frame(), TableFrame::Sides);
    let column_rules =
        !allbox && matches!(presentation.grid(), TableGrid::All | TableGrid::Columns);
    let mut output = String::new();

    if side_frame {
        output.push_str("| ");
    }
    for (column_index, kind) in row.cells.iter().enumerate() {
        if column_index > 0 {
            let spans_boundary = matches!(kind, CellKind::HSpan)
                || matches!(kind, CellKind::VSpan)
                    && has_hspan_origin(grid, row_index, column_index);
            if column_rules && !spans_boundary {
                output.push_str("| ");
            }
        }
        output.push_str(&format_entry(
            kind,
            row,
            column_index,
            columns,
            width_modifiers,
        ));
        if column_index + 1 < row.cells.len() {
            output.push(' ');
        }
    }
    if side_frame {
        output.push_str(" |");
    }
    output
}

fn warn_once(diagnostics: &mut Diagnostics<'_>, message: &'static str, advice: &'static str) {
    if !diagnostics
        .warnings()
        .iter()
        .any(|warning| warning.message == message)
    {
        diagnostics.warn_with_advice(message, advice);
    }
}

fn parse_table_width(
    metadata: &BlockMetadata<'_>,
    diagnostics: &mut Diagnostics<'_>,
) -> Option<u32> {
    let width = metadata.attributes.get_string("width")?;
    let value = width.trim().trim_end_matches('%');
    match value.parse::<u32>() {
        Ok(width) if width > 0 => Some(width.min(100)),
        Ok(_) | Err(_) => {
            warn_once(
                diagnostics,
                "table width is not a positive percentage; using content-determined width",
                "Set `width` to a percentage from 1% through 100%, or use `%autowidth`.",
            );
            None
        }
    }
}

struct WidthPlan {
    modifiers: Vec<String>,
    expand: bool,
}

fn width_plan<'a>(
    table: &'a Table<'a>,
    num_cols: usize,
    metadata: &BlockMetadata<'_>,
    diagnostics: &mut Diagnostics<'_>,
) -> WidthPlan {
    let has_width_attribute = metadata.attributes.get("width").is_some();
    let requested_width = parse_table_width(metadata, diagnostics);
    let autowidth = metadata.options.contains(&"autowidth")
        && !metadata.roles.contains(&"stretch")
        && !has_width_attribute;
    let stretches = metadata.roles.contains(&"stretch")
        || !autowidth && (!has_width_attribute || requested_width == Some(100));
    let has_nondefault_column_width = table
        .columns
        .iter()
        .any(|column| column.width != ColumnWidth::default());

    if autowidth && !has_nondefault_column_width {
        return WidthPlan {
            modifiers: vec![String::new(); num_cols],
            expand: false,
        };
    }
    if stretches && !has_nondefault_column_width {
        return WidthPlan {
            modifiers: vec!["x".to_string(); num_cols],
            expand: false,
        };
    }
    if !has_nondefault_column_width && requested_width.is_none() {
        return WidthPlan {
            modifiers: vec![String::new(); num_cols],
            expand: false,
        };
    }

    let widths = if table.columns.is_empty() {
        let divisor = f64::from(u32::try_from(num_cols.max(1)).unwrap_or(u32::MAX));
        vec![100.0 / divisor; num_cols]
    } else {
        calculate_column_widths(&table.columns)
    };
    let nominal_width = 60.0 * f64::from(requested_width.unwrap_or(100)) / 100.0;
    let modifiers = (0..num_cols)
        .map(|index| {
            widths.get(index).map_or_else(String::new, |width| {
                if *width > 0.0 {
                    format!("w({:.0}n)", (nominal_width * width / 100.0).max(1.0))
                } else {
                    String::new()
                }
            })
        })
        .collect();

    WidthPlan {
        modifiers,
        expand: stretches,
    }
}

fn table_is_centered(metadata: &BlockMetadata<'_>, diagnostics: &mut Diagnostics<'_>) -> bool {
    let alignment = metadata.attributes.get_string("align");
    let float = metadata.attributes.get_string("float");
    if metadata.attributes.get("float").is_some() {
        warn_once(
            diagnostics,
            "table floats are not supported by portable roff; preserving table alignment without text wrapping",
            "Use `align=left` or `align=center` when text wrapping around the table is not required.",
        );
    }

    let Some(value) = alignment.as_ref().or(float.as_ref()) else {
        return false;
    };
    match value.as_ref() {
        "left" => false,
        "center" => true,
        "right" if alignment.is_none() => false,
        "right" => {
            warn_once(
                diagnostics,
                "right table alignment is not supported by portable tbl; using left alignment",
                "Use `align=left` or `align=center` for consistent GNU groff and mandoc output.",
            );
            false
        }
        _ => {
            warn_once(
                diagnostics,
                "unsupported table alignment; using left alignment",
                "Set `align` to `left` or `center` for portable manpage output.",
            );
            false
        }
    }
}

fn has_rowspans<'a>(table: &'a Table<'a>) -> bool {
    table
        .header
        .iter()
        .chain(&table.rows)
        .chain(&table.footer)
        .flat_map(|row| &row.columns)
        .any(|cell| cell.rowspan > 1)
}

/// Render a data row from the grid, producing a colon-separated string.
fn render_grid_row<'a>(
    grid_row: &GridRow<'a>,
    columns: &[ColumnFormat],
    processor: &Processor<'a>,
    traversal: &mut TraversalContext<'a>,
    diagnostics: &mut Diagnostics<'_>,
) -> Result<String, Error> {
    let mut data_cells = Vec::with_capacity(grid_row.cells.len());

    for (column_index, kind) in grid_row.cells.iter().enumerate() {
        match kind {
            CellKind::Content { cell_index } => {
                if let Some(cell) = grid_row.ast_row.columns.get(*cell_index) {
                    data_cells.push(format_cell_with_inlines(
                        cell,
                        column_index,
                        columns,
                        processor,
                        traversal,
                        diagnostics,
                    )?);
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

pub(crate) fn visit_table<'a, W: Write>(
    traversal: &mut TraversalContext<'a>,
    table: &'a Table<'a>,
    block: &'a DelimitedBlock<'a>,
    visitor: &mut ManpageVisitor<'a, '_, W>,
) -> Result<(), Error> {
    let processor = visitor.processor;
    let num_cols = determine_column_count(table);
    let grid = build_grid(table, num_cols);
    let presentation = table.presentation().unwrap_or_else(|| {
        TablePresentation::from_attributes(&block.metadata, |name| {
            (processor.parser_options.document_attributes()).get(name)
        })
    });
    if presentation.stripes() != TableStripes::None {
        warn_once(
            &mut visitor.diagnostics,
            "table row stripes are not supported by portable roff; rendering rows without shading",
            "Use table rules or explicit text labels when row grouping must remain visible in a manpage.",
        );
    }
    let allbox = presentation.frame() == TableFrame::All && presentation.grid() == TableGrid::All;
    let row_rules = !allbox && matches!(presentation.grid(), TableGrid::All | TableGrid::Rows);
    if row_rules && has_rowspans(table) {
        warn_once(
            &mut visitor.diagnostics,
            "row rules across row-spanning cells are not exact in portable tbl; drawing full-width rules",
            "Use `grid=cols` or avoid row spans when uninterrupted spanning cells are required.",
        );
    }
    let centered = table_is_centered(&block.metadata, &mut visitor.diagnostics);
    let width_plan = width_plan(table, num_cols, &block.metadata, &mut visitor.diagnostics);

    let format_lines: Vec<String> = grid
        .iter()
        .enumerate()
        .map(|(row_index, _)| {
            format_row(
                &grid,
                row_index,
                &table.columns,
                &width_plan.modifiers,
                presentation,
                allbox,
            )
        })
        .collect();

    let data_rows: Vec<String> = grid
        .iter()
        .map(|row| {
            render_grid_row(
                row,
                &table.columns,
                processor,
                traversal,
                &mut visitor.diagnostics,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;

    let w = visitor.writer_mut();
    writeln!(w, ".TS")?;
    let mut options = Vec::with_capacity(4);
    if allbox {
        options.push("allbox");
    } else if presentation.frame() == TableFrame::All {
        options.push("box");
    }
    if centered {
        options.push("center");
    }
    if width_plan.expand {
        options.push("expand");
    }
    options.push("tab(:)");
    writeln!(w, "{};", options.join(" "))?;

    if let Some((last, rest)) = format_lines.split_last() {
        for fmt in rest {
            writeln!(w, "{fmt}")?;
        }
        writeln!(w, "{last}.")?;
    }

    let ends_frame = !allbox && presentation.frame() == TableFrame::Ends;
    if ends_frame {
        writeln!(w, "_")?;
    }
    for (row_index, data_row) in data_rows.iter().enumerate() {
        writeln!(w, "{data_row}")?;
        if row_rules && row_index + 1 < data_rows.len() {
            writeln!(w, "_")?;
        }
    }
    if ends_frame {
        writeln!(w, "_")?;
    }

    writeln!(w, ".TE")?;

    Ok(())
}

/// Format a table cell with inline formatting preserved.
fn format_cell_with_inlines<'a>(
    cell: &'a TableColumn<'a>,
    column_index: usize,
    columns: &[ColumnFormat],
    processor: &Processor<'a>,
    traversal: &mut TraversalContext<'a>,
    diagnostics: &mut Diagnostics<'_>,
) -> Result<String, Error> {
    let mut buf = Vec::new();
    let mut cell_visitor = ManpageVisitor::new(&mut buf, processor, diagnostics.reborrow());
    let style = effective_style(cell, column_index, columns);
    let scoped = style == ColumnStyle::AsciiDoc;
    let mut render = |traversal: &mut TraversalContext<'a>| {
        cell.content.iter().try_for_each(|block| {
            if let acdc_parser::Block::Paragraph(para) = block {
                cell_visitor.visit_inline_nodes(traversal, &para.content)
            } else {
                traversal.visit_block(&mut cell_visitor, block)
            }
        })
    };
    if scoped {
        traversal.with_table_cell(cell, render)
    } else {
        render(traversal)
    }?;

    let text = String::from_utf8_lossy(&buf).into_owned();
    let text = match style {
        ColumnStyle::Strong => format!("\\fB{text}\\fP"),
        ColumnStyle::Emphasis => format!("\\fI{text}\\fP"),
        ColumnStyle::Monospace => format!("\\f(CR{text}\\fP"),
        ColumnStyle::Literal => format!(".nf\n{text}\n.fi"),
        ColumnStyle::AsciiDoc | ColumnStyle::Default | ColumnStyle::Header | _ => text,
    };

    // Wrap in T{ T} if content contains tbl special characters or formatting
    if text.contains(':') || text.contains('\n') || text.contains("\\f") {
        Ok(format!("T{{\n{text}\nT}}"))
    } else {
        Ok(text)
    }
}
