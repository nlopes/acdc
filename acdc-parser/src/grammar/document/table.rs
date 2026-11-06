//! Table blocks and cell content.

use crate::{
    AttributeName, AttributeValue, Block, ColumnFormat, ColumnStyle, ColumnWidth, DelimitedBlock,
    DelimitedBlockType, DocumentAttribute, DocumentAttributeAssignment, Error, HorizontalAlignment,
    InlineNode, Location, Paragraph, Table, TableColumn, TablePresentation, TableRow, Verbatim,
    VerticalAlignment, Warning, WarningKind,
    blocks::table::{CellSpecifier, MAX_TABLE_COLUMNS},
    grammar::{
        ParserState, document::delimited::check_delimiters, document_parser,
        helpers::BlockParsingMetadata, inline_processing::adjust_and_log_parse_error,
        state::BlockContext,
    },
};
use std::{borrow::Cow, rc::Rc};

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
    limit: impl std::fmt::Display,
) -> Error {
    Error::Parse(
        Box::new(state.create_error_source_location(location)),
        format!("table {resource} request of {requested} exceeds the maximum of {limit}"),
    )
}

/// Read a complete `[multiplier*][halign][.valign][width][style]` record.
/// Invalid records contribute no columns, matching Asciidoctor.
/// Numeric widths outside the supported u32 range return the authored value.
fn parse_column_spec(input: &str) -> Result<Option<(usize, ColumnFormat)>, &str> {
    let (multiplier, input) = if let Some((count, rest)) = input.split_once('*') {
        if count.is_empty() || !count.bytes().all(|byte| byte.is_ascii_digit()) {
            return Ok(None);
        }
        (count.parse().unwrap_or(MAX_TABLE_COLUMNS + 1), rest)
    } else {
        (1, input)
    };
    let alignment_end = input.len() - input.trim_start_matches(['<', '^', '>', '.']).len();
    let (alignment, mut rest) = input.split_at(alignment_end);
    let (horizontal, vertical) = match alignment.as_bytes() {
        [] => (None, None),
        [h @ (b'<' | b'^' | b'>')] | [h @ (b'<' | b'^' | b'>'), b'.'] => (Some(*h), None),
        [b'.', v @ (b'<' | b'^' | b'>')] => (None, Some(*v)),
        [h @ (b'<' | b'^' | b'>'), b'.', v @ (b'<' | b'^' | b'>')] => (Some(*h), Some(*v)),
        _ => return Ok(None),
    };
    let mut format = ColumnFormat {
        halign: match horizontal {
            Some(b'^') => HorizontalAlignment::Center,
            Some(b'>') => HorizontalAlignment::Right,
            _ => HorizontalAlignment::Left,
        },
        valign: match vertical {
            Some(b'^') => VerticalAlignment::Middle,
            Some(b'>') => VerticalAlignment::Bottom,
            _ => VerticalAlignment::Top,
        },
        ..ColumnFormat::default()
    };
    let mut width_error = None;
    if let Some(tail) = rest.strip_prefix('~') {
        format.width = ColumnWidth::Auto;
        rest = tail;
    } else {
        let width_end = rest.bytes().take_while(u8::is_ascii_digit).count();
        let (width, tail) = rest.split_at(width_end);
        let percentage = tail.starts_with('%') && !width.is_empty();
        rest = if percentage {
            tail.strip_prefix('%').unwrap_or(tail)
        } else {
            tail
        };
        if !width.is_empty() {
            match width.parse() {
                Ok(width) => {
                    format.width = if percentage {
                        ColumnWidth::Percentage(width)
                    } else {
                        ColumnWidth::Proportional(width)
                    };
                }
                Err(_) => width_error = Some(width),
            }
        }
    }
    format.style = match rest.as_bytes() {
        [] => ColumnStyle::Default,
        [style] if style.is_ascii_lowercase() => match style {
            b'a' => ColumnStyle::AsciiDoc,
            b'e' => ColumnStyle::Emphasis,
            b'h' => ColumnStyle::Header,
            b'l' => ColumnStyle::Literal,
            b'm' => ColumnStyle::Monospace,
            b's' => ColumnStyle::Strong,
            _ => ColumnStyle::Default,
        },
        _ => return Ok(None),
    };
    if let Some(width) = width_error {
        Err(width)
    } else {
        Ok(Some((multiplier, format)))
    }
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
    let presentation_defaults = TablePresentation::from_attribute_values(
        |_| None,
        |name| state.document_attributes.get(name),
    );
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
        let cols = if cols.contains(' ') {
            Cow::Owned(cols.replace(' ', ""))
        } else {
            Cow::Borrowed(cols.as_ref())
        };
        let mut specs = Vec::new();
        let count_only = !cols.is_empty()
            && cols.bytes().all(|byte| byte.is_ascii_digit())
            && (cols.len() == 1 || !cols.starts_with('0'));
        if count_only {
            let count = cols.parse::<usize>().unwrap_or(MAX_TABLE_COLUMNS + 1);
            if count > MAX_TABLE_COLUMNS {
                return Err(table_limit_error(
                    state,
                    table_location.clone(),
                    "column count",
                    count,
                    MAX_TABLE_COLUMNS,
                ));
            }
            specs.resize(count, ColumnFormat::default());
        } else {
            for input in cols.split(if cols.contains(',') { ',' } else { ';' }) {
                let Some((multiplier, spec)) = parse_column_spec(input).map_err(|width| {
                    table_limit_error(
                        state,
                        table_location.clone(),
                        "column width",
                        width,
                        u32::MAX,
                    )
                })?
                else {
                    continue;
                };
                let column_count = specs.len().saturating_add(multiplier);
                if column_count > MAX_TABLE_COLUMNS {
                    return Err(table_limit_error(
                        state,
                        table_location.clone(),
                        "column count",
                        column_count,
                        MAX_TABLE_COLUMNS,
                    ));
                }
                specs.extend(std::iter::repeat_n(spec, multiplier));
            }
        }

        ((!specs.is_empty()).then_some(specs.len()), specs)
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
        .with_presentation(&metadata, presentation_defaults);

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
