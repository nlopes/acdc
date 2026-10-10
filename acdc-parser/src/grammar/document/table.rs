//! Table blocks and cell content.

use crate::{
    AttributeName, AttributeValue, Block, ColumnStyle, ColumnWidth, DelimitedBlock,
    DelimitedBlockType, DocumentAttribute, DocumentAttributeAssignment, Error, HorizontalAlignment,
    InlineNode, Location, Paragraph, Table, TableColumn, TablePresentation, TableRow, Verbatim,
    VerticalAlignment, Warning, WarningKind,
    blocks::table::{CellSpecifier, MAX_TABLE_COLUMNS},
    grammar::{
        ParserState, document::delimited::check_delimiters, document_parser,
        helpers::BlockParsingMetadata, inline_processing::adjust_and_log_parse_error,
        state::BlockContext,
    },
    model::strip_quotes,
};
use std::rc::Rc;

/// The matched closing delimiter, or an unterminated table.
/// An unterminated table ends at end of input and produces a warning.
#[derive(Clone, Copy)]
pub(super) enum TableClosing<'a> {
    Terminated {
        close_delim: &'a str,
        close_start: usize,
    },
    Unterminated,
}

/// Source ranges and delimiters for a table block.
pub(super) struct TableParseParams<'a> {
    pub(super) start: usize,
    pub(super) offset: usize,
    pub(super) table_start: usize,
    pub(super) content_start: usize,
    pub(super) content_end: usize,
    pub(super) end: usize,
    pub(super) open_delim: &'a str,
    pub(super) content: &'a str,
    pub(super) default_separator: &'a str,
    pub(super) closing: TableClosing<'a>,
}

fn table_limit_error(
    state: &ParserState<'_>,
    location: Location,
    resource: &str,
    requested: impl std::fmt::Display,
    limit: usize,
) -> Error {
    Error::Parse(
        Box::new(state.create_error_source_location(location)),
        format!("table {resource} request of {requested} exceeds the maximum of {limit}"),
    )
}

/// Parse a table block with the selected separator and source ranges.
#[allow(clippy::too_many_lines)]
pub(super) fn parse_table_block_impl<'input>(
    params: &TableParseParams<'_>,
    state: &mut ParserState<'input>,
    block_metadata: &BlockParsingMetadata<'input>,
) -> Result<Block<'input>, Error> {
    let &TableParseParams {
        start,
        offset,
        table_start,
        content_start,
        content_end: _content_end,
        end,
        open_delim,
        content,
        default_separator,
        closing,
    } = params;

    let mut metadata = block_metadata.metadata.clone();
    metadata.move_positional_attributes_to_attributes();
    let presentation =
        TablePresentation::from_attributes(&metadata, |name| (state.document_attributes).get(name));
    let location = state.create_block_location(start, end, offset);
    let table_location = state.create_block_location(table_start, end, offset);
    let open_delimiter_location = state.create_location(
        table_start + offset,
        table_start + offset + open_delim.len().saturating_sub(1),
    );
    let close_delimiter_location = match closing {
        TableClosing::Terminated {
            close_delim,
            close_start,
        } => {
            check_delimiters(
                open_delim,
                close_delim,
                "table",
                state.create_error_source_location(state.create_block_location(start, end, offset)),
            )?;
            Some(state.create_block_location(close_start, end, offset))
        }
        TableClosing::Unterminated => {
            state.add_warning(Warning::new(
                WarningKind::UnterminatedTable {
                    delimiter: open_delim.to_string(),
                },
                Some(state.create_error_source_location(open_delimiter_location.clone())),
            ));
            None
        }
    };

    let separator = if let Some(AttributeValue::String(sep)) =
        block_metadata.metadata.attributes.get("separator")
    {
        sep.to_string()
    } else if let Some(AttributeValue::String(format)) =
        block_metadata.metadata.attributes.get("format")
    {
        match &**format {
            "csv" => ",",
            "dsv" => ":",
            "tsv" => "\t",
            unknown_format => {
                state.add_warning(Warning::new(
                    WarningKind::TableUnknownFormat {
                        format: unknown_format.to_string(),
                    },
                    Some(state.create_error_source_location(table_location.clone())),
                ));
                default_separator
            }
        }
        .to_string()
    } else {
        default_separator.to_string()
    };

    let (ncols, column_formats) = if let Some(AttributeValue::String(cols)) =
        block_metadata.metadata.attributes.get("cols")
    {
        // Parse cols attribute
        // Full syntax: [multiplier*][halign][valign][width][style]
        // Examples: "3*", "^.>2a", "2*>.^1m", "<,^,>", "15%,30%,55%"
        let mut specs = Vec::new();

        for part in cols.split(',') {
            let s = strip_quotes(part.trim());

            // Check for "N*" notation (e.g., "3*" means 3 columns with same spec)
            let (multiplier, spec_str) = if let Some(pos) = s.find('*') {
                let mult_str = &s[..pos];
                let mult = mult_str.parse::<usize>().unwrap_or_else(|_| {
                    if !mult_str.is_empty() && mult_str.bytes().all(|b| b.is_ascii_digit()) {
                        MAX_TABLE_COLUMNS + 1
                    } else {
                        1
                    }
                });
                (mult, &s[pos + 1..])
            } else {
                (1, s)
            };

            let mut halign = HorizontalAlignment::default();
            let mut valign = VerticalAlignment::default();
            let mut width = ColumnWidth::default();
            let mut style = ColumnStyle::default();

            // Parse style (last character if it's a letter: a, d, e, h, l, m, s)
            let spec_str = if let Some(last_char) = spec_str.chars().last() {
                match last_char {
                    'a' => {
                        style = ColumnStyle::AsciiDoc;
                        &spec_str[..spec_str.len() - 1]
                    }
                    'd' => {
                        style = ColumnStyle::Default;
                        &spec_str[..spec_str.len() - 1]
                    }
                    'e' => {
                        style = ColumnStyle::Emphasis;
                        &spec_str[..spec_str.len() - 1]
                    }
                    'h' => {
                        style = ColumnStyle::Header;
                        &spec_str[..spec_str.len() - 1]
                    }
                    'l' => {
                        style = ColumnStyle::Literal;
                        &spec_str[..spec_str.len() - 1]
                    }
                    'm' => {
                        style = ColumnStyle::Monospace;
                        &spec_str[..spec_str.len() - 1]
                    }
                    's' => {
                        style = ColumnStyle::Strong;
                        &spec_str[..spec_str.len() - 1]
                    }
                    _ => spec_str,
                }
            } else {
                spec_str
            };

            // Parse vertical alignment markers: .<, .^, .>
            if spec_str.contains(".<") {
                valign = VerticalAlignment::Top;
            } else if spec_str.contains(".^") {
                valign = VerticalAlignment::Middle;
            } else if spec_str.contains(".>") {
                valign = VerticalAlignment::Bottom;
            }

            // Parse horizontal alignment markers: <, ^, > (not preceded by .)
            for (i, c) in spec_str.char_indices() {
                let prev_char = if i > 0 {
                    spec_str.chars().nth(i - 1)
                } else {
                    None
                };
                if prev_char == Some('.') {
                    continue; // This is a vertical alignment marker
                }
                match c {
                    '<' => halign = HorizontalAlignment::Left,
                    '^' => halign = HorizontalAlignment::Center,
                    '>' => halign = HorizontalAlignment::Right,
                    _ => {}
                }
            }

            // Parse width: integer (proportional), percentage, or ~ (auto)
            // The ~ (tilde) for auto-width was added in Asciidoctor 1.5.7
            // See: https://github.com/asciidoctor/asciidoctor/issues/1844
            // Remove alignment markers to find the width
            let width_str: String = spec_str
                .chars()
                .filter(|c| !matches!(c, '<' | '^' | '>' | '.'))
                .collect();
            if !width_str.is_empty() {
                if width_str == "~" {
                    width = ColumnWidth::Auto;
                } else if width_str.ends_with('%') {
                    if let Ok(pct) = width_str.trim_end_matches('%').parse::<u32>() {
                        width = ColumnWidth::Percentage(pct);
                    }
                } else if let Ok(prop) = width_str.parse::<u32>() {
                    width = ColumnWidth::Proportional(prop);
                }
            }

            // Add the spec for each column in the multiplier (including defaults)
            let spec = crate::ColumnFormat {
                halign,
                valign,
                width,
                style,
            };
            let column_count = specs.len().saturating_add(multiplier);
            if column_count > MAX_TABLE_COLUMNS {
                return Err(table_limit_error(
                    state,
                    table_location.clone(),
                    "column count",
                    column_count.to_string(),
                    MAX_TABLE_COLUMNS,
                ));
            }
            specs.extend(std::iter::repeat_n(spec, multiplier));
        }

        (Some(specs.len()), specs)
    } else {
        (None, Vec::new())
    };

    let mut has_header = block_metadata.metadata.options.contains(&"header");

    // Keep the source span of an incomplete row for the warning.
    let mut dropped_span = None;
    let raw_rows = Table::parse_rows_with_positions(
        content,
        &separator,
        &mut has_header,
        content_start + offset,
        ncols,
        &mut dropped_span,
    )
    .map_err(|violation| {
        table_limit_error(
            state,
            state.create_location(violation.start, violation.end),
            violation.resource,
            violation.requested,
            violation.limit,
        )
    })?;

    if let Some((start, end)) = dropped_span {
        state.add_warning(Warning::new(
            WarningKind::TableIncompleteRow,
            Some(state.create_error_source_location(state.create_location(start, end))),
        ));
    }

    // An explicit `noheader` overrides header detection.
    if block_metadata.metadata.options.contains(&"noheader") {
        has_header = false;
    }
    let has_footer = block_metadata.metadata.options.contains(&"footer");

    let mut header = None;
    let mut footer = None;

    let mut rows = Vec::with_capacity(raw_rows.len());

    // Track rowspan state: maps column positions to remaining rowspan count.
    // When a cell has rowspan > 1, we track how many more rows it occupies.
    // Each entry: (column_position, remaining_rows, colspan_width)
    let mut active_rowspans: Vec<(usize, usize, usize)> = Vec::new();

    for (i, row) in raw_rows.iter().enumerate() {
        let is_header_row = has_header;
        // Each raw cell produces at least one `columns` entry; duplication
        // produces more but is bounded by the table's column limit.
        let mut columns = Vec::with_capacity(row.len());
        for cell in row {
            let cell_count = if cell.spec.is_duplication {
                cell.spec.duplication_count
            } else {
                1
            };
            if cell_count == 0 {
                continue;
            }
            let cell_content = state.intern_str(&cell.content);
            // Duplicates share source text, but each parse must apply its own
            // footnote and document-attribute effects.
            for _ in 0..cell_count {
                // Column defaults follow generated cell order; spans do not
                // advance this source-row index in asciidoctor.
                let column_index = columns.len();
                let mut spec = cell.spec;
                if is_header_row {
                    // Semantic header rows always use normal substitutions.
                    spec.style = None;
                } else if spec.style.is_none()
                    && let Some(col_format) = column_formats.get(column_index)
                    && col_format.style != ColumnStyle::Default
                {
                    spec.style = Some(col_format.style);
                }
                let parsed = parse_table_cell(cell_content, state, cell.content_start, &spec)?;
                columns.push(parsed);
            }
        }

        // Row location from first cell (falls back to the table location
        // if the row is empty, which shouldn't happen in practice).
        let row_location = if let Some(first) = row.first() {
            state.create_location(first.start, first.end)
        } else {
            table_location.clone()
        };

        let occupied_from_rowspans: usize = active_rowspans
            .iter()
            .map(|(_pos, _remaining, width)| *width)
            .sum();

        // Logical column count = columns occupied by rowspans + colspans of new cells
        let logical_col_count: usize =
            occupied_from_rowspans + columns.iter().map(|c| c.colspan).sum::<usize>();
        if logical_col_count > MAX_TABLE_COLUMNS {
            return Err(table_limit_error(
                state,
                row_location.clone(),
                "column count",
                logical_col_count.to_string(),
                MAX_TABLE_COLUMNS,
            ));
        }

        if let Some(ncols) = ncols
            && logical_col_count != ncols
        {
            let has_overflow = columns.iter().any(|c| c.colspan > ncols);
            if has_overflow {
                state.add_warning(Warning::new(
                    WarningKind::TableCellOverflow {
                        actual: logical_col_count,
                        expected: ncols,
                    },
                    Some(state.create_error_source_location(row_location)),
                ));
            } else {
                state.add_warning(Warning::new(
                    WarningKind::TableColumnCount {
                        actual: logical_col_count,
                        expected: ncols,
                        occupied_from_rowspans,
                    },
                    Some(state.create_error_source_location(row_location)),
                ));
            }
            continue;
        }

        // Update active rowspans for this row:
        // 1. Decrement remaining count for existing rowspans
        // 2. Remove rowspans that are now exhausted
        active_rowspans.retain_mut(|(_pos, remaining, _width)| {
            *remaining -= 1;
            *remaining > 0
        });

        // 3. Add new rowspans from current row's cells
        let mut col_position = 0;
        for (_, active_pos, _, colspan) in active_rowspans.iter().map(|(p, r, c)| (*p, *p, *r, *c))
        {
            if col_position == active_pos {
                col_position += colspan;
            }
        }
        for cell in &columns {
            // Skip over positions occupied by rowspans
            while active_rowspans
                .iter()
                .any(|(pos, _, width)| col_position >= *pos && col_position < pos + width)
            {
                if let Some((_, _, width)) = active_rowspans
                    .iter()
                    .find(|(pos, _, w)| col_position >= *pos && col_position < pos + w)
                {
                    col_position += width;
                }
            }
            if cell.rowspan > 1 {
                active_rowspans.push((col_position, cell.rowspan - 1, cell.colspan));
            }
            col_position += cell.colspan;
        }

        if is_header_row {
            header = Some(TableRow { columns });
            has_header = false;
            continue;
        }

        if has_footer && i == raw_rows.len() - 1 {
            footer = Some(TableRow { columns });
            continue;
        }

        rows.push(TableRow { columns });
    }

    let table = Table::new(rows, table_location.clone())
        .with_header(header)
        .with_footer(footer)
        .with_columns(column_formats)
        .with_presentation(presentation);

    Ok(Block::DelimitedBlock(DelimitedBlock {
        source_text: None,
        metadata: metadata.clone(),
        delimiter: state.intern_str(open_delim),
        inner: DelimitedBlockType::DelimitedTable(table),
        title: block_metadata.title.clone(),
        location,
        open_delimiter_location: Some(open_delimiter_location),
        close_delimiter_location,
    }))
}

fn parse_table_cell<'a>(
    content: &'a str,
    state: &mut ParserState<'a>,
    cell_start_offset: usize,
    spec: &CellSpecifier,
) -> Result<TableColumn<'a>, Error> {
    // Literal cells keep their source text intact. Unlike listing blocks, they
    // do not run attribute, macro, quote, or callout substitutions.
    if spec.style == Some(ColumnStyle::Literal) {
        let location = if content.is_empty() {
            state.create_location(cell_start_offset, cell_start_offset)
        } else {
            state.create_block_location(0, content.len(), cell_start_offset)
        };
        let blocks = vec![Block::Paragraph(Paragraph::new(
            vec![InlineNode::VerbatimText(Verbatim {
                content,
                location: location.clone(),
            })],
            location,
        ))];
        return Ok(TableColumn::with_format(
            blocks,
            spec.colspan,
            spec.rowspan,
            spec.halign,
            spec.valign,
            spec.style,
        ));
    }

    // Markdown blockquotes are only parsed when cell has AsciiDoc style ('a' prefix).
    // This matches asciidoctor behavior where `> text` is only a blockquote in 'a' style cells.
    let mut initial_attributes = Vec::new();
    let blocks = if spec.style == Some(ColumnStyle::AsciiDoc) {
        // An AsciiDoc-style cell is a nested document. It inherits the outer
        // attributes, but its local attributes, section catalog, hard-break
        // state, and pending callout references do not escape into sibling cells or the
        // outer document.
        let outer_attributes = Rc::clone(&state.document_attributes);
        let outer_parent_attributes = state
            .nested_parent_attributes
            .replace(Rc::clone(&state.document_attributes));
        let outer_hardbreaks = state.hardbreaks;
        let outer_context = std::mem::replace(&mut state.block_context, BlockContext::Document);
        let outer_toc_len = state.toc_entries.len();
        let outer_pending_callouts = std::mem::take(&mut state.pending_callouts);

        initial_attributes = crate::document_attribute::initialize_nested_attributes(Rc::make_mut(
            &mut state.document_attributes,
        ));

        let result = document_parser::nested_document_blocks(
            content,
            state,
            cell_start_offset,
            &mut initial_attributes,
        );

        state.document_attributes = outer_attributes;
        state.nested_parent_attributes = outer_parent_attributes;
        state.hardbreaks = outer_hardbreaks;
        state.block_context = outer_context;
        state.toc_entries.truncate(outer_toc_len);
        state.pending_callouts = outer_pending_callouts;
        result
    } else {
        document_parser::blocks_for_table_cell(content, state, cell_start_offset)
    }
    .unwrap_or_else(|error| {
        adjust_and_log_parse_error(
            &error,
            content,
            cell_start_offset,
            state,
            "Failed parsing table cell content as blocks",
        );
        Ok(Vec::new())
    })?;
    let mut column = TableColumn::with_format(
        blocks,
        spec.colspan,
        spec.rowspan,
        spec.halign,
        spec.valign,
        spec.style,
    );
    column.initial_attributes = initial_attributes;
    Ok(column)
}

pub(super) fn normalize_nested_header<'a>(
    state: &mut ParserState<'a>,
    header: Vec<Result<Block<'a>, Error>>,
    initial_attributes: &mut Vec<(AttributeName<'a>, DocumentAttributeAssignment<'a>)>,
) -> Result<Vec<Block<'a>>, Error> {
    let mut header = header.into_iter().collect::<Result<Vec<_>, _>>()?;
    let normalized = crate::document_attribute::normalize_toc_attributes(Rc::make_mut(
        &mut state.document_attributes,
    ));
    // Header entries replay their normalized values when converters enter the cell.
    for block in &mut header {
        if let Block::DocumentAttribute(event) = block
            && event.is_accepted()
            && let Some((_, assignment)) = normalized.iter().find(|(name, _)| *name == event.name)
        {
            *event = DocumentAttribute::accepted(
                event.name.clone(),
                assignment.clone(),
                event.location.clone(),
            );
        }
    }
    initial_attributes.extend(normalized);
    Ok(header)
}
