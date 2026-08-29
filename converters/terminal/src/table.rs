use std::io::{self, BufWriter};

use acdc_converters_core::Diagnostics;
use acdc_converters_core::table::{
    CellKind, GridRow, build_grid, calculate_column_widths, determine_column_count, table_has_spans,
};
use acdc_converters_core::visitor::{Visitor, WritableVisitor};
use acdc_parser::{
    BlockMetadata, ColumnStyle, HorizontalAlignment, TableFrame, TableGrid, TablePresentation,
    TableStripes, VerticalAlignment,
};
use comfy_table::{
    Attribute, Cell, CellAlignment, ColumnConstraint, ContentArrangement, Table, Width,
};

use crate::{Error, Processor, TerminalVisitor};

/// Map `HorizontalAlignment` to comfy-table `CellAlignment`.
fn map_alignment(align: HorizontalAlignment) -> CellAlignment {
    match align {
        HorizontalAlignment::Left => CellAlignment::Left,
        HorizontalAlignment::Center => CellAlignment::Center,
        HorizontalAlignment::Right => CellAlignment::Right,
    }
}

/// Render a cell's content to a string, applying column style formatting.
fn render_cell_content(
    col: &acdc_parser::TableColumn,
    col_index: usize,
    columns: &[acdc_parser::ColumnFormat],
    processor: &Processor<'_>,
    diagnostics: &mut Diagnostics<'_>,
) -> Result<Cell, Error> {
    let buffer = Vec::new();
    let inner = BufWriter::new(buffer);
    let mut temp_visitor = TerminalVisitor::new(inner, processor.clone(), diagnostics.reborrow());
    col.content
        .iter()
        .try_for_each(|block| temp_visitor.visit_block(block))?;
    let buffer = temp_visitor
        .into_writer()
        .into_inner()
        .map_err(io::IntoInnerError::into_error)?;
    let text = String::from_utf8(buffer).unwrap_or_default();

    Ok(apply_cell_format(
        Cell::new(text),
        col,
        col_index,
        columns,
        processor,
    ))
}

fn apply_cell_format(
    mut cell: Cell,
    col: &acdc_parser::TableColumn<'_>,
    col_index: usize,
    columns: &[acdc_parser::ColumnFormat],
    processor: &Processor<'_>,
) -> Cell {
    // Determine effective alignment: cell override > column default
    let effective_halign = col
        .halign
        .or_else(|| columns.get(col_index).map(|c| c.halign));
    if let Some(align) = effective_halign {
        cell = cell.set_alignment(map_alignment(align));
    }

    // Determine effective style: cell override > column default
    let effective_style = col
        .style
        .or_else(|| columns.get(col_index).map(|c| c.style));
    if let Some(style) = effective_style {
        match style {
            ColumnStyle::Strong => {
                cell = cell.add_attribute(Attribute::Bold);
            }
            ColumnStyle::Emphasis => {
                cell = cell.add_attribute(Attribute::Italic);
            }
            ColumnStyle::Header => {
                cell = cell
                    .fg(processor.appearance.colors.table_header)
                    .add_attribute(Attribute::Bold);
            }
            // Default, AsciiDoc, Literal, Monospace: no extra styling
            // (terminal is already monospace, literal just means no inline processing)
            ColumnStyle::Default
            | ColumnStyle::AsciiDoc
            | ColumnStyle::Literal
            | ColumnStyle::Monospace
            | _ => {}
        }
    }

    cell
}

fn effective_valign(
    col: &acdc_parser::TableColumn<'_>,
    col_index: usize,
    columns: &[acdc_parser::ColumnFormat],
) -> VerticalAlignment {
    col.valign
        .or_else(|| columns.get(col_index).map(|column| column.valign))
        .unwrap_or_default()
}

fn pad_cell_vertically(
    cell: Cell,
    col: &acdc_parser::TableColumn<'_>,
    col_index: usize,
    columns: &[acdc_parser::ColumnFormat],
    processor: &Processor<'_>,
    max_lines: usize,
) -> Cell {
    let content = cell.content();
    let line_count = content.lines().count().max(1);
    let padding = max_lines.saturating_sub(line_count);
    if padding == 0 {
        return cell;
    }
    let (before, after) = if effective_valign(col, col_index, columns) == VerticalAlignment::Top {
        (0, padding)
    } else if effective_valign(col, col_index, columns) == VerticalAlignment::Middle {
        (padding / 2, padding - padding / 2)
    } else {
        (padding, 0)
    };
    let padded = format!("{}{}{}", "\n".repeat(before), content, "\n".repeat(after));
    apply_cell_format(Cell::new(padded), col, col_index, columns, processor)
}

fn render_row_cells(
    row: &acdc_parser::TableRow<'_>,
    columns: &[acdc_parser::ColumnFormat],
    processor: &Processor<'_>,
    diagnostics: &mut Diagnostics<'_>,
) -> Result<Vec<Cell>, Error> {
    let cells = row
        .columns
        .iter()
        .enumerate()
        .map(|(index, col)| render_cell_content(col, index, columns, processor, diagnostics))
        .collect::<Result<Vec<_>, _>>()?;
    let max_lines = cells
        .iter()
        .map(|cell| cell.content().lines().count().max(1))
        .max()
        .unwrap_or(1);
    Ok(cells
        .into_iter()
        .zip(&row.columns)
        .enumerate()
        .map(|(index, (cell, col))| {
            pad_cell_vertically(cell, col, index, columns, processor, max_lines)
        })
        .collect())
}

/// Find the visible-character positions of column boundaries in a table output.
///
/// Scans for separator lines (containing `┼`, `╪`, `┬`, `┴`) and returns the
/// visible-char positions of the internal column separators.
fn find_column_boundaries(output: &str) -> Vec<usize> {
    // Find any separator line to extract column boundary positions
    for line in output.lines() {
        let boundaries: Vec<usize> = line
            .char_indices()
            .filter_map(|(byte_pos, ch)| {
                // Internal column junction characters
                if matches!(ch, '┼' | '╪' | '┬' | '┴' | '┆') {
                    // Convert byte position to visible char position
                    Some(visible_position(line, byte_pos))
                } else {
                    None
                }
            })
            .collect();
        if !boundaries.is_empty() {
            return boundaries;
        }
    }
    vec![]
}

/// Get the visible character position for a given byte offset in a line,
/// skipping ANSI escape sequences.
fn visible_position(line: &str, target_byte: usize) -> usize {
    let mut visible = 0;
    let mut in_escape = false;
    for (byte_pos, ch) in line.char_indices() {
        if byte_pos == target_byte {
            return visible;
        }
        if ch == '\x1b' {
            in_escape = true;
        } else if in_escape {
            if ch.is_ascii_alphabetic() {
                in_escape = false;
            }
        } else {
            visible += 1;
        }
    }
    visible
}

/// Get the byte offset for a given visible character position in a line.
fn byte_offset_for_visible(line: &str, target_visible: usize) -> Option<usize> {
    let mut visible = 0;
    let mut in_escape = false;
    for (byte_pos, ch) in line.char_indices() {
        // Skip ANSI escape sequences entirely — they are never visible targets
        if ch == '\x1b' {
            in_escape = true;
            continue;
        }
        if in_escape {
            if ch.is_ascii_alphabetic() {
                in_escape = false;
            }
            continue;
        }
        // Visible character — check if it's the target
        if visible == target_visible {
            return Some(byte_pos);
        }
        visible += 1;
    }
    None
}

/// Classify output lines into content rows and separator rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LineType {
    /// Top border of the table
    TopBorder,
    /// Content line belonging to grid row N
    Content(usize),
    /// Separator between grid rows N and N+1
    Separator(usize),
    /// Bottom border of the table
    BottomBorder,
}

fn classify_lines(output: &str, num_grid_rows: usize) -> Vec<LineType> {
    let lines: Vec<&str> = output.lines().collect();
    let mut result = Vec::with_capacity(lines.len());

    // Track which grid row we're in
    let mut grid_row: usize = 0;
    let mut seen_content = false;

    for line in &lines {
        let first_visible = line.chars().find(|c| !c.is_whitespace());
        let is_separator = first_visible.is_some_and(|c| matches!(c, '╭' | '╞' | '├' | '╰'));

        if is_separator {
            if !seen_content {
                // First separator = top border
                result.push(LineType::TopBorder);
            } else if grid_row >= num_grid_rows.saturating_sub(1) {
                result.push(LineType::BottomBorder);
            } else {
                // Separator between grid_row and grid_row+1
                result.push(LineType::Separator(grid_row));
                grid_row += 1;
            }
        } else {
            seen_content = true;
            result.push(LineType::Content(grid_row));
        }
    }

    result
}

/// Remove internal borders for spanned cells in the rendered table output.
#[allow(clippy::too_many_lines)]
fn remove_span_borders(
    output: &str,
    grid: &[GridRow<'_>],
    num_cols: usize,
    boundaries: &[usize],
    line_types: &[LineType],
) -> String {
    if boundaries.is_empty() || boundaries.len() < num_cols.saturating_sub(1) {
        return output.to_string();
    }

    let lines: Vec<&str> = output.lines().collect();
    let mut result_lines: Vec<String> = Vec::with_capacity(lines.len());

    for (line_idx, line) in lines.iter().enumerate() {
        let Some(line_type) = line_types.get(line_idx) else {
            result_lines.push((*line).to_string());
            continue;
        };

        let mut modified = (*line).to_string();

        match *line_type {
            LineType::Content(row_idx) => {
                if let Some(grid_row) = grid.get(row_idx) {
                    // Remove vertical separators for HSpan cells or VSpan cells
                    // that continue an HSpan from above
                    for (col_idx, kind) in grid_row.cells.iter().enumerate() {
                        if col_idx == 0 {
                            continue;
                        }
                        let should_remove = match kind {
                            CellKind::HSpan => true,
                            CellKind::VSpan => {
                                // Check if the cell above at this position was
                                // HSpan (part of a colspan that also has rowspan)
                                is_hspan_origin(grid, row_idx, col_idx)
                            }
                            CellKind::Content { .. } => false,
                        };
                        if should_remove {
                            let boundary_idx = col_idx - 1;
                            if let Some(&vis_pos) = boundaries.get(boundary_idx) {
                                replace_char_at_visible(&mut modified, vis_pos, ' ');
                            }
                        }
                    }
                }
            }
            LineType::Separator(row_above) => {
                let row_below = row_above + 1;

                // First: clear horizontal line segments for VSpan cells
                for col_idx in 0..num_cols {
                    let vspan_below = grid
                        .get(row_below)
                        .and_then(|r| r.cells.get(col_idx))
                        .is_some_and(|k| matches!(k, CellKind::VSpan));

                    if vspan_below {
                        let left_boundary = if col_idx == 0 {
                            1
                        } else {
                            boundaries.get(col_idx - 1).map_or(1, |b| b + 1)
                        };
                        let right_boundary = boundaries
                            .get(col_idx)
                            .copied()
                            .unwrap_or_else(|| visible_line_len(&modified));

                        for vis_pos in left_boundary..right_boundary {
                            let ch_at = char_at_visible(&modified, vis_pos);
                            if matches!(ch_at, Some('╌' | '─' | '═')) {
                                replace_char_at_visible(&mut modified, vis_pos, ' ');
                            }
                        }
                    }
                }

                // Fix junction characters at column boundaries
                for boundary_idx in 0..boundaries.len() {
                    let col_left = boundary_idx;
                    let col_right = boundary_idx + 1;

                    // Determine which directions the junction connects.
                    // A direction is "open" (no line) when a span crosses it.
                    let left_cell_below = grid.get(row_below).and_then(|r| r.cells.get(col_left));
                    let right_cell_below = grid.get(row_below).and_then(|r| r.cells.get(col_right));

                    // LEFT: no horizontal going left if col_left below is VSpan
                    let has_left = !left_cell_below.is_some_and(|k| matches!(k, CellKind::VSpan));
                    // RIGHT: no horizontal going right if col_right below is VSpan
                    let has_right = !right_cell_below.is_some_and(|k| matches!(k, CellKind::VSpan));
                    // UP: no vertical going up if col_right above is HSpan (or
                    // VSpan tracing to HSpan)
                    let has_up = !grid
                        .get(row_above)
                        .and_then(|r| r.cells.get(col_right))
                        .is_some_and(|k| {
                            matches!(k, CellKind::HSpan)
                                || (matches!(k, CellKind::VSpan)
                                    && is_hspan_origin(grid, row_above, col_right))
                        });
                    // DOWN: no vertical going down if col_right below is HSpan
                    // (or VSpan tracing to HSpan)
                    let has_down = !right_cell_below.is_some_and(|k| {
                        matches!(k, CellKind::HSpan)
                            || (matches!(k, CellKind::VSpan)
                                && is_hspan_origin(grid, row_below, col_right))
                    });

                    // All four = original ┼, skip
                    if has_up && has_down && has_left && has_right {
                        continue;
                    }

                    let replacement = match (has_up, has_down, has_left, has_right) {
                        (true, true, false, true) => '├',
                        (true, true, true, false) => '┤',
                        (true, false, true, true) => '┴',
                        (false, true, true, true) => '┬',
                        (true, true, false, false) => '┆',
                        (false, false, true, true) => '╌',
                        (false, true, false, true) => '╭',
                        (false, true, true, false) => '╮',
                        (true, false, false, true) => '╰',
                        (true, false, true, false) => '╯',
                        _ => ' ',
                    };

                    if let Some(&vis_pos) = boundaries.get(boundary_idx) {
                        replace_char_at_visible(&mut modified, vis_pos, replacement);
                    }
                }

                // Fix left edge for VSpan
                let vspan_first = grid
                    .get(row_below)
                    .and_then(|r| r.cells.first())
                    .is_some_and(|k| matches!(k, CellKind::VSpan));
                if vspan_first {
                    replace_char_at_visible(&mut modified, 0, '│');
                }

                // Fix right edge for VSpan
                let vspan_last = grid
                    .get(row_below)
                    .and_then(|r| r.cells.last())
                    .is_some_and(|k| matches!(k, CellKind::VSpan));
                if vspan_last {
                    let last_pos = visible_line_len(&modified).saturating_sub(1);
                    replace_char_at_visible(&mut modified, last_pos, '│');
                }
            }
            LineType::TopBorder | LineType::BottomBorder => {
                // Handle HSpan on the top/bottom border
                for boundary_idx in 0..boundaries.len() {
                    let col_right = boundary_idx + 1;

                    let row_to_check = if *line_type == LineType::TopBorder {
                        0
                    } else {
                        grid.len().saturating_sub(1)
                    };

                    let hspan = grid
                        .get(row_to_check)
                        .and_then(|r| r.cells.get(col_right))
                        .is_some_and(|k| matches!(k, CellKind::HSpan));

                    if hspan && let Some(&vis_pos) = boundaries.get(boundary_idx) {
                        replace_char_at_visible(&mut modified, vis_pos, '─');
                    }
                }
            }
        }

        result_lines.push(modified);
    }

    result_lines.join("\n")
}

/// Get the character at a visible position (skipping ANSI escapes).
fn char_at_visible(line: &str, target_visible: usize) -> Option<char> {
    byte_offset_for_visible(line, target_visible).and_then(|off| line[off..].chars().next())
}

/// Replace the character at a visible position with a replacement char.
fn replace_char_at_visible(line: &mut String, target_visible: usize, replacement: char) {
    if let Some(byte_off) = byte_offset_for_visible(line, target_visible)
        && let Some(old_char) = line[byte_off..].chars().next()
    {
        let old_len = old_char.len_utf8();
        let mut new_bytes = [0u8; 4];
        let new_str = replacement.encode_utf8(&mut new_bytes);
        line.replace_range(byte_off..byte_off + old_len, new_str);
    }
}

/// Count visible characters in a line (skipping ANSI escapes).
fn visible_line_len(line: &str) -> usize {
    let mut visible = 0;
    let mut in_escape = false;
    for ch in line.chars() {
        if ch == '\x1b' {
            in_escape = true;
        } else if in_escape {
            if ch.is_ascii_alphabetic() {
                in_escape = false;
            }
        } else {
            visible += 1;
        }
    }
    visible
}

/// Check whether a `VSpan` cell at (`row_idx`, `col_idx`) traces back to an
/// `HSpan` origin — i.e., the cell it continues from (upward) was `HSpan`.
/// This means the vertical separator should also be removed for `VSpan` rows
/// that are part of a combined colspan+rowspan.
fn is_hspan_origin(grid: &[GridRow<'_>], row_idx: usize, col_idx: usize) -> bool {
    // Walk upward through VSpan cells until we find the row that isn't VSpan
    let mut r = row_idx;
    while r > 0 {
        r -= 1;
        match grid.get(r).and_then(|row| row.cells.get(col_idx)) {
            Some(CellKind::VSpan) => {}
            Some(CellKind::HSpan) => return true,
            _ => return false,
        }
    }
    false
}

fn stripe_row(stripes: TableStripes, row: usize) -> bool {
    match stripes {
        TableStripes::All => true,
        TableStripes::Odd => row & 1 == 0,
        TableStripes::Even => row & 1 == 1,
        TableStripes::None | TableStripes::Hover | _ => false,
    }
}

fn remove_vertical_at(line: &mut String, position: usize, horizontal: char) {
    let Some(character) = char_at_visible(line, position) else {
        return;
    };
    let replacement = if matches!(character, '│' | '┆') {
        ' '
    } else if character == ' ' {
        return;
    } else {
        horizontal
    };
    replace_char_at_visible(line, position, replacement);
}

fn apply_table_presentation(
    output: &str,
    has_header: bool,
    boundaries: &[usize],
    line_types: &[LineType],
    presentation: TablePresentation,
) -> String {
    let vertical_frame = matches!(presentation.frame(), TableFrame::All | TableFrame::Sides);
    let horizontal_frame = matches!(presentation.frame(), TableFrame::All | TableFrame::Ends);
    let column_rule = matches!(presentation.grid(), TableGrid::All | TableGrid::Columns);
    let row_rule = matches!(presentation.grid(), TableGrid::All | TableGrid::Rows);
    let mut result = Vec::with_capacity(line_types.len());

    for (line, line_type) in output.lines().zip(line_types.iter().copied()) {
        let mut line = line.to_string();
        let last = visible_line_len(&line).saturating_sub(1);

        match line_type {
            LineType::TopBorder | LineType::BottomBorder => {
                if !horizontal_frame {
                    continue;
                }
                if !vertical_frame {
                    remove_vertical_at(&mut line, 0, '─');
                    remove_vertical_at(&mut line, last, '─');
                }
                if !column_rule {
                    for &boundary in boundaries {
                        remove_vertical_at(&mut line, boundary, '─');
                    }
                }
            }
            LineType::Content(_) => {
                if !vertical_frame {
                    replace_char_at_visible(&mut line, 0, ' ');
                    replace_char_at_visible(&mut line, last, ' ');
                }
                if !column_rule {
                    for &boundary in boundaries {
                        replace_char_at_visible(&mut line, boundary, ' ');
                    }
                }
            }
            LineType::Separator(row_above) => {
                let is_header_separator = has_header && row_above == 0;
                if !is_header_separator && !row_rule {
                    continue;
                }
                let horizontal = if is_header_separator { '═' } else { '╌' };
                if !vertical_frame {
                    remove_vertical_at(&mut line, 0, horizontal);
                    remove_vertical_at(&mut line, last, horizontal);
                }
                if !column_rule {
                    for &boundary in boundaries {
                        remove_vertical_at(&mut line, boundary, horizontal);
                    }
                }
            }
        }

        result.push(line);
    }

    result.join("\n")
}

/// Build `comfy_table` cells from a logical grid row.
fn build_comfy_cells_from_grid(
    grid_row: &GridRow<'_>,
    tbl: &acdc_parser::Table,
    processor: &Processor<'_>,
    diagnostics: &mut Diagnostics<'_>,
) -> Result<Vec<Cell>, Error> {
    let mut cells = Vec::with_capacity(grid_row.cells.len());
    for kind in &grid_row.cells {
        let cell = match kind {
            CellKind::Content { cell_index } => {
                if let Some(col) = grid_row.ast_row.columns.get(*cell_index) {
                    let mut c = render_cell_content(
                        col,
                        *cell_index,
                        &tbl.columns,
                        processor,
                        diagnostics,
                    )?;
                    if grid_row.is_header {
                        c = c
                            .fg(processor.appearance.colors.table_header)
                            .add_attribute(Attribute::Bold);
                    }
                    if grid_row.is_footer {
                        c = c
                            .fg(processor.appearance.colors.table_footer)
                            .add_attribute(Attribute::Bold);
                    }
                    c
                } else {
                    Cell::new("")
                }
            }
            CellKind::HSpan | CellKind::VSpan => Cell::new(""),
        };
        cells.push(cell);
    }
    let max_lines = cells
        .iter()
        .map(|cell| cell.content().lines().count().max(1))
        .max()
        .unwrap_or(1);
    for (grid_index, kind) in grid_row.cells.iter().enumerate() {
        let CellKind::Content { cell_index } = kind else {
            continue;
        };
        let Some(col) = grid_row.ast_row.columns.get(*cell_index) else {
            continue;
        };
        let Some(cell) = cells.get_mut(grid_index) else {
            continue;
        };
        *cell = pad_cell_vertically(
            cell.clone(),
            col,
            *cell_index,
            &tbl.columns,
            processor,
            max_lines,
        );
        if grid_row.is_header {
            *cell = cell
                .clone()
                .fg(processor.appearance.colors.table_header)
                .add_attribute(Attribute::Bold);
        }
        if grid_row.is_footer {
            *cell = cell
                .clone()
                .fg(processor.appearance.colors.table_footer)
                .add_attribute(Attribute::Bold);
        }
    }
    Ok(cells)
}

/// Configure column width constraints on the table widget.
fn apply_column_widths(table_widget: &mut Table, tbl: &acdc_parser::Table, num_cols: usize) {
    if !tbl.columns.is_empty() {
        let widths = calculate_column_widths(&tbl.columns);
        // Widths may have fewer entries than num_cols if cols attribute doesn't
        // match the logical column count. Pad with auto (ContentWidth).
        let constraints: Vec<ColumnConstraint> = (0..num_cols)
            .map(|i| {
                widths
                    .get(i)
                    .and_then(|width| {
                        let clamped = width.clamp(0.0, 100.0).round();
                        format!("{clamped:.0}")
                            .parse::<u16>()
                            .ok()
                            .filter(|&p| p > 0)
                            .map(|percent| ColumnConstraint::Absolute(Width::Percentage(percent)))
                    })
                    .unwrap_or(ColumnConstraint::ContentWidth)
            })
            .collect();
        table_widget.set_constraints(constraints);
    }
}

fn render_table_without_spans(
    table_widget: &mut Table,
    tbl: &acdc_parser::Table,
    visitor: &mut TerminalVisitor<'_, '_, impl std::io::Write>,
    processor: &Processor<'_>,
    presentation: TablePresentation,
) -> Result<String, Error> {
    apply_column_widths(table_widget, tbl, tbl.columns.len());

    if let Some(header) = &tbl.header {
        let header_cells: Vec<_> =
            render_row_cells(header, &tbl.columns, processor, &mut visitor.diagnostics)?
                .into_iter()
                .map(|cell| {
                    cell.fg(processor.appearance.colors.table_header)
                        .add_attribute(Attribute::Bold)
                })
                .collect();
        table_widget.set_header(header_cells);
    }

    for (row_idx, row) in tbl.rows.iter().enumerate() {
        let cells = render_row_cells(row, &tbl.columns, processor, &mut visitor.diagnostics)?
            .into_iter()
            .map(|mut cell| {
                if stripe_row(presentation.stripes(), row_idx) {
                    cell = cell.add_attribute(Attribute::Dim);
                }
                cell
            })
            .collect::<Vec<_>>();
        table_widget.add_row(cells);
    }

    if let Some(footer) = &tbl.footer {
        let footer_cells: Vec<_> =
            render_row_cells(footer, &tbl.columns, processor, &mut visitor.diagnostics)?
                .into_iter()
                .map(|cell| {
                    cell.fg(processor.appearance.colors.table_footer)
                        .add_attribute(Attribute::Bold)
                })
                .collect();
        table_widget.add_row(footer_cells);
    }

    let rendered = format!("{table_widget}");
    let boundaries = find_column_boundaries(&rendered);
    let row_count =
        usize::from(tbl.header.is_some()) + tbl.rows.len() + usize::from(tbl.footer.is_some());
    let line_types = classify_lines(&rendered, row_count);
    Ok(apply_table_presentation(
        &rendered,
        tbl.header.is_some(),
        &boundaries,
        &line_types,
        presentation,
    ))
}

pub(crate) fn visit_table<W: std::io::Write>(
    tbl: &acdc_parser::Table,
    metadata: &BlockMetadata<'_>,
    visitor: &mut TerminalVisitor<'_, '_, W>,
    processor: &Processor<'_>,
) -> Result<(), Error> {
    let terminal_width = processor.terminal_width;
    let presentation = tbl.presentation().unwrap_or_else(|| {
        TablePresentation::from_attributes(metadata, &processor.document_attributes)
    });

    let mut table_widget = Table::new();
    table_widget
        .set_content_arrangement(ContentArrangement::Dynamic)
        .load_style(comfy_table::presets::UTF8_FULL.with_rounded_corners())
        .enforce_styling();
    if let Some(width) = table_width(metadata, terminal_width) {
        table_widget.set_width(u16::try_from(width).unwrap_or(80));
    }

    if table_has_spans(tbl) {
        let num_cols = determine_column_count(tbl);
        let grid = build_grid(tbl, num_cols);

        apply_column_widths(&mut table_widget, tbl, num_cols);

        // Separate header rows from body/footer rows
        let mut header_set = false;
        let mut body_row_idx = 0;
        for grid_row in &grid {
            let cells =
                build_comfy_cells_from_grid(grid_row, tbl, processor, &mut visitor.diagnostics)?;

            if grid_row.is_header && !header_set {
                table_widget.set_header(cells);
                header_set = true;
            } else {
                let cells =
                    if !grid_row.is_footer && stripe_row(presentation.stripes(), body_row_idx) {
                        cells
                            .into_iter()
                            .map(|c| c.add_attribute(Attribute::Dim))
                            .collect()
                    } else {
                        cells
                    };
                table_widget.add_row(cells);
                if !grid_row.is_footer {
                    body_row_idx += 1;
                }
            }
        }

        // Post-process to remove internal borders for spanned cells
        let rendered = format!("{table_widget}");
        let boundaries = find_column_boundaries(&rendered);
        let line_types = classify_lines(&rendered, grid.len());
        let result = remove_span_borders(&rendered, &grid, num_cols, &boundaries, &line_types);
        let result =
            apply_table_presentation(&result, header_set, &boundaries, &line_types, presentation);

        let result = align_table(&result, metadata, terminal_width);
        let w = visitor.writer_mut();
        writeln!(w, "{result}")?;
        return Ok(());
    }

    let rendered =
        render_table_without_spans(&mut table_widget, tbl, visitor, processor, presentation)?;
    let rendered = align_table(&rendered, metadata, terminal_width);
    let w = visitor.writer_mut();
    writeln!(w, "{rendered}")?;
    Ok(())
}

fn table_width(metadata: &BlockMetadata<'_>, terminal_width: usize) -> Option<usize> {
    let constrained = metadata.attributes.get_string("width").and_then(|width| {
        let width = width.trim().trim_end_matches('%');
        width.parse::<usize>().ok().filter(|width| *width > 0)
    });
    if constrained.is_none()
        && metadata.options.contains(&"autowidth")
        && !metadata.roles.contains(&"stretch")
    {
        return None;
    }
    let percent = constrained.unwrap_or(100).min(100);
    Some((terminal_width.saturating_mul(percent) / 100).max(1))
}

fn align_table(output: &str, metadata: &BlockMetadata<'_>, terminal_width: usize) -> String {
    let alignment = metadata
        .attributes
        .get_string("align")
        .or_else(|| metadata.attributes.get_string("float"))
        .and_then(|value| matches!(value.as_ref(), "left" | "center" | "right").then_some(value));
    let Some(alignment) = alignment else {
        return output.to_string();
    };
    let width = output.lines().map(visible_line_len).max().unwrap_or(0);
    let remaining = terminal_width.saturating_sub(width);
    let indent = match alignment.as_ref() {
        "center" => remaining / 2,
        "right" => remaining,
        _ => 0,
    };
    if indent == 0 {
        return output.to_string();
    }
    let prefix = " ".repeat(indent);
    output
        .lines()
        .map(|line| format!("{prefix}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::create_test_processor;
    use acdc_parser::{Block, DelimitedBlockType};

    /// Parse an `AsciiDoc` string and extract the first table from the document.
    ///
    /// Leaks the parsed document so the returned `Table<'static>` borrows
    /// from memory that lives for the rest of the test process.
    #[allow(clippy::expect_used)]
    fn parse_table(adoc: &str) -> acdc_parser::Table<'static> {
        let options = acdc_parser::Options::default();
        let parsed = acdc_parser::parse(adoc, &options).expect("Failed to parse AsciiDoc");
        let parsed: &'static acdc_parser::ParseResult = Box::leak(Box::new(parsed));
        parsed
            .document()
            .blocks
            .iter()
            .find_map(|block| {
                if let Block::DelimitedBlock(db) = block
                    && let DelimitedBlockType::DelimitedTable(table) = &db.inner
                {
                    return Some(table.clone());
                }
                None
            })
            .expect("No table found in document")
    }

    #[test]
    fn test_table_with_footer() -> Result<(), Error> {
        let adoc = r"
[%header%footer]
|===
| Header 1 | Header 2

| Cell 1 | Cell 2

| Footer 1 | Footer 2
|===
";
        let table = parse_table(adoc);

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut warnings = Vec::new();
        let source = acdc_converters_core::WarningSource::new("terminal");
        let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
        let mut visitor =
            crate::TerminalVisitor::new(buffer, processor.clone(), diagnostics.reborrow());

        visit_table(&table, &BlockMetadata::default(), &mut visitor, &processor)?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(
            output_str.contains("Header 1"),
            "Output should contain header"
        );
        assert!(
            output_str.contains("Cell 1"),
            "Output should contain body cell"
        );
        assert!(
            output_str.contains("Footer 1"),
            "Output should contain footer"
        );

        Ok(())
    }

    #[test]
    fn test_table_without_footer() -> Result<(), Error> {
        let adoc = r"
|===
| Cell
|===
";
        let table = parse_table(adoc);

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut warnings = Vec::new();
        let source = acdc_converters_core::WarningSource::new("terminal");
        let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
        let mut visitor =
            crate::TerminalVisitor::new(buffer, processor.clone(), diagnostics.reborrow());

        visit_table(&table, &BlockMetadata::default(), &mut visitor, &processor)?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(output_str.contains("Cell"), "Output should contain cell");

        Ok(())
    }

    #[test]
    fn test_table_with_alignment() -> Result<(), Error> {
        let adoc = r#"
[cols="<,^,>"]
|===
| Left | Center | Right
|===
"#;
        let table = parse_table(adoc);

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut warnings = Vec::new();
        let source = acdc_converters_core::WarningSource::new("terminal");
        let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
        let mut visitor =
            crate::TerminalVisitor::new(buffer, processor.clone(), diagnostics.reborrow());

        visit_table(&table, &BlockMetadata::default(), &mut visitor, &processor)?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(output_str.contains("Left"), "Should contain left cell");
        assert!(output_str.contains("Center"), "Should contain center cell");
        assert!(output_str.contains("Right"), "Should contain right cell");

        Ok(())
    }

    #[test]
    fn test_table_with_colspan() -> Result<(), Error> {
        let adoc = r#"
[cols="3*"]
|===
| A | B | C

2+| Spans two columns | D
| E | F | G
|===
"#;
        let table = parse_table(adoc);

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut warnings = Vec::new();
        let source = acdc_converters_core::WarningSource::new("terminal");
        let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
        let mut visitor =
            crate::TerminalVisitor::new(buffer, processor.clone(), diagnostics.reborrow());

        visit_table(&table, &BlockMetadata::default(), &mut visitor, &processor)?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(
            output_str.contains("Spans two columns"),
            "Output should contain spanning cell content"
        );
        assert!(output_str.contains('D'), "Output should contain cell D");
        assert!(output_str.contains('E'), "Output should contain cell E");

        Ok(())
    }

    #[test]
    fn test_table_with_rowspan() -> Result<(), Error> {
        let adoc = r"
|===
| A | B | C

.2+| Spans rows | D | E
| F | G
| H | I | J
|===
";
        let table = parse_table(adoc);

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut warnings = Vec::new();
        let source = acdc_converters_core::WarningSource::new("terminal");
        let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
        let mut visitor =
            crate::TerminalVisitor::new(buffer, processor.clone(), diagnostics.reborrow());

        visit_table(&table, &BlockMetadata::default(), &mut visitor, &processor)?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(
            output_str.contains("Spans rows"),
            "Output should contain rowspan cell content"
        );
        assert!(output_str.contains('H'), "Output should contain cell H");

        Ok(())
    }

    #[test]
    fn test_table_with_combined_span() -> Result<(), Error> {
        let adoc = r"
|===
| A | B | C | D

2.2+| Big cell | E | F
| G | H
| I | J | K | L
|===
";
        let table = parse_table(adoc);

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut warnings = Vec::new();
        let source = acdc_converters_core::WarningSource::new("terminal");
        let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
        let mut visitor =
            crate::TerminalVisitor::new(buffer, processor.clone(), diagnostics.reborrow());

        visit_table(&table, &BlockMetadata::default(), &mut visitor, &processor)?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(
            output_str.contains("Big cell"),
            "Output should contain combined span cell content"
        );
        assert!(output_str.contains('L'), "Output should contain cell L");

        Ok(())
    }

    #[test]
    fn test_table_with_column_styles() -> Result<(), Error> {
        let adoc = r#"
[cols="s,e"]
|===
| Strong Column | Emphasis Column
|===
"#;
        let table = parse_table(adoc);

        let buffer = Vec::new();
        let processor = create_test_processor();
        let mut warnings = Vec::new();
        let source = acdc_converters_core::WarningSource::new("terminal");
        let mut diagnostics = acdc_converters_core::Diagnostics::new(&source, &mut warnings);
        let mut visitor =
            crate::TerminalVisitor::new(buffer, processor.clone(), diagnostics.reborrow());

        visit_table(&table, &BlockMetadata::default(), &mut visitor, &processor)?;
        let output = visitor.into_writer();

        let output_str = String::from_utf8_lossy(&output);
        assert!(
            output_str.contains("Strong Column"),
            "Should contain strong cell"
        );
        assert!(
            output_str.contains("Emphasis Column"),
            "Should contain emphasis cell"
        );

        Ok(())
    }
}
