//! Table blocks and cell content.

pub(super) mod construction;
mod streaming;
#[cfg(test)]
mod tests;

use crate::{
    AttributeName, AttributeValue, Block, ColumnFormat, ColumnStyle, ColumnWidth, DelimitedBlock,
    DelimitedBlockType, DocumentAttribute, DocumentAttributeAssignment, Error, HorizontalAlignment,
    InlineNode, Location, Paragraph, Table, TableCell, TablePresentation, TableRow, Verbatim,
    VerticalAlignment, Warning, WarningKind,
    blocks::table::{CellSpecifier, MAX_TABLE_COLUMNS},
    grammar::{
        ParserState,
        document::delimited::{ContentParser, GrammarContent, check_delimiters},
        document_parser,
        helpers::BlockParsingMetadata,
        inline_processing::{PreparedInlines, adjust_and_log_parse_error},
        state::BlockContext,
    },
};
use std::{borrow::Cow, cell::RefCell, rc::Rc};

pub(super) type InitialAttributes<'a> = Vec<(AttributeName<'a>, DocumentAttributeAssignment<'a>)>;

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

pub(super) struct TableSettings {
    pub(super) separator: String,
    pub(super) ncols: Option<usize>,
    pub(super) column_formats: Vec<ColumnFormat>,
}

pub(super) fn table_settings(
    state: &ParserState<'_>,
    block_metadata: &BlockParsingMetadata<'_>,
    table_location: &Location,
    default_separator: &str,
    report_warnings: bool,
) -> Result<TableSettings, Error> {
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
                if report_warnings {
                    state.add_warning(Warning::new(
                        WarningKind::TableUnknownFormat {
                            format: unknown_format.to_string(),
                        },
                        Some(state.create_error_source_location(table_location.clone())),
                    ));
                }
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
                let Some((multiplier, spec, width_error)) = parse_column_spec(input) else {
                    continue;
                };
                if report_warnings && let Some(width) = width_error {
                    state.add_warning(Warning::new(
                        WarningKind::ContentRecovery {
                            message: format!(
                                "table column width `{width}` exceeds the maximum of {}; using proportional width 1",
                                u32::MAX,
                            ).into(),
                        },
                        Some(state.create_error_source_location(table_location.clone())),
                    ));
                }
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

    Ok(TableSettings {
        separator,
        ncols,
        column_formats,
    })
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
/// Oversized widths retain the default width and return the authored value for a warning.
fn parse_column_spec(input: &str) -> Option<(usize, ColumnFormat, Option<&str>)> {
    let (multiplier, input) = if let Some((count, rest)) = input.split_once('*') {
        if count.is_empty() || !count.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
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
        _ => return None,
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
        _ => return None,
    };
    Some((multiplier, format, width_error))
}

/// Parse a table block with the selected separator and source ranges.
pub(super) fn parse_table_block_impl<'input>(
    params: &TableParseParams<'_>,
    state: &mut ParserState<'input>,
    block_metadata: &BlockParsingMetadata<'input>,
) -> Result<Block<'input>, Error> {
    parse_table_with_cells(params, state, block_metadata, &mut NativeCells)
}

trait CellParser<'a> {
    fn begin_row(&mut self, _state: &ParserState<'a>) {}
    fn begin_cell(&mut self) {}
    fn parse(
        &mut self,
        content: &'a str,
        state: &mut ParserState<'a>,
        offset: usize,
        spec: &CellSpecifier,
        prepared: &mut Option<Vec<PreparedTableCellParagraph<'a>>>,
    ) -> Result<TableCell<'a>, Error>;
    fn finish_row(&mut self, _state: &mut ParserState<'a>, _destination: Option<RowDestination>) {}
}

#[derive(Clone, Copy)]
struct RowDestination {
    part: crate::extensions::TablePart,
    row: usize,
}

struct NativeCells;

impl<'a> CellParser<'a> for NativeCells {
    fn parse(
        &mut self,
        content: &'a str,
        state: &mut ParserState<'a>,
        offset: usize,
        spec: &CellSpecifier,
        prepared: &mut Option<Vec<PreparedTableCellParagraph<'a>>>,
    ) -> Result<TableCell<'a>, Error> {
        parse_table_cell(content, state, offset, spec, prepared)
    }
}

#[allow(clippy::too_many_lines)]
fn parse_table_with_cells<'input>(
    params: &TableParseParams<'_>,
    state: &mut ParserState<'input>,
    block_metadata: &BlockParsingMetadata<'input>,
    cells: &mut impl CellParser<'input>,
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

    let TableSettings {
        separator,
        ncols,
        column_formats,
    } = table_settings(
        state,
        block_metadata,
        &table_location,
        default_separator,
        true,
    )?;

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
        cells.begin_row(state);
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
            cells.begin_cell();
            let mut prepared = None;
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
                let parsed = cells.parse(
                    cell_content,
                    state,
                    cell.content_start,
                    &spec,
                    &mut prepared,
                )?;
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
            cells.finish_row(state, None);
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
            cells.finish_row(
                state,
                Some(RowDestination {
                    part: crate::extensions::TablePart::Head,
                    row: 0,
                }),
            );
            header = Some(TableRow { columns });
            has_header = false;
            continue;
        }

        if has_footer && i == raw_rows.len() - 1 {
            cells.finish_row(
                state,
                Some(RowDestination {
                    part: crate::extensions::TablePart::Foot,
                    row: 0,
                }),
            );
            footer = Some(TableRow { columns });
            continue;
        }

        cells.finish_row(
            state,
            Some(RowDestination {
                part: crate::extensions::TablePart::Body,
                row: rows.len(),
            }),
        );
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

pub(super) fn finalize_footnotes(blocks: &mut [Block<'_>]) {
    crate::grammar::location_walk::try_walk_table_cells_mut(blocks, &mut |cell| {
        if let Some(definitions) = &mut cell.footnotes {
            for definition in definitions {
                super::references::finalize_footnote_text(
                    &mut definition.registration_substitutions,
                    &mut definition.content,
                );
            }
        }
        Ok(true)
    })
    .unwrap_or_else(|never: std::convert::Infallible| match never {});
}

/// Prepared content shared only by copies of one authored normal cell.
pub(super) struct PreparedTableCellParagraph<'a> {
    content: PreparedCellContent<'a>,
    location: Location,
}

enum PreparedCellContent<'a> {
    Inlines(PreparedInlines<'a>),
    Simple(Vec<InlineNode<'a>>),
}

impl<'a> PreparedTableCellParagraph<'a> {
    pub(super) fn new(
        state: &mut ParserState<'a>,
        start: usize,
        end: usize,
        offset: usize,
        content: &'a str,
    ) -> Result<Self, Error> {
        let metadata = BlockParsingMetadata::new(state.options.compatibility_mode);
        let inlines = PreparedInlines::new(state, &metadata, start, end, offset, content)?;
        Ok(Self {
            content: PreparedCellContent::Inlines(inlines),
            location: state.create_block_location(start, end, offset),
        })
    }

    pub(super) fn parse(&self, state: &mut ParserState<'a>) -> Result<Block<'a>, Error> {
        let metadata = BlockParsingMetadata::new(state.options.compatibility_mode);
        let content = match &self.content {
            PreparedCellContent::Inlines(inlines) => inlines.parse(state, &metadata, false)?,
            PreparedCellContent::Simple(content) => content.clone(),
        };
        Ok(Block::Paragraph(Paragraph::new(
            content,
            self.location.clone(),
        )))
    }

    pub(super) fn retain_simple_content(&mut self, block: &Block<'a>) {
        if let PreparedCellContent::Inlines(inlines) = &self.content
            && inlines.is_simple_text()
            && let Block::Paragraph(paragraph) = block
            && simple_cell_nodes(&paragraph.content)
        {
            self.content = PreparedCellContent::Simple(paragraph.content.clone());
        }
    }
}

fn simple_cell_nodes(nodes: &[InlineNode<'_>]) -> bool {
    nodes.iter().all(|node| match node {
        InlineNode::PlainText(_) => true,
        InlineNode::BoldText(text) => text.id.is_none() && simple_cell_nodes(&text.content),
        InlineNode::ItalicText(text) => text.id.is_none() && simple_cell_nodes(&text.content),
        InlineNode::MonospaceText(text) => text.id.is_none() && simple_cell_nodes(&text.content),
        InlineNode::Symbol(_)
        | InlineNode::CharacterReference(_)
        | InlineNode::RawText(_)
        | InlineNode::VerbatimText(_)
        | InlineNode::HighlightText(_)
        | InlineNode::SubscriptText(_)
        | InlineNode::SuperscriptText(_)
        | InlineNode::DoubleQuotedSpan(_)
        | InlineNode::SingleQuotedSpan(_)
        | InlineNode::StandaloneCurvedApostrophe(_)
        | InlineNode::LineBreak(_)
        | InlineNode::InlineAnchor(_)
        | InlineNode::Macro(_)
        | InlineNode::CalloutMarker(_) => false,
    })
}

fn parse_normal_cell<'a>(
    content: &'a str,
    state: &mut ParserState<'a>,
    offset: usize,
    reuse: bool,
    prepared: &mut Option<Vec<PreparedTableCellParagraph<'a>>>,
) -> Result<Vec<Block<'a>>, Error> {
    if let Some(paragraphs) = prepared {
        // Run every paragraph action before collecting errors, as the grammar does.
        return paragraphs
            .iter()
            .map(|paragraph| paragraph.parse(state))
            .collect::<Vec<_>>()
            .into_iter()
            .collect();
    }
    let mut capture = reuse.then(Vec::new);
    let result = document_parser::blocks_for_table_cell(content, state, offset, &mut capture);
    if result.as_ref().is_ok_and(Result::is_ok) {
        *prepared = capture;
    }
    result.unwrap_or_else(|error| {
        adjust_and_log_parse_error(
            &error,
            content,
            offset,
            state,
            "Failed parsing table cell content as blocks",
        );
        Ok(Vec::new())
    })
}

fn parse_table_cell<'a>(
    content: &'a str,
    state: &mut ParserState<'a>,
    cell_start_offset: usize,
    spec: &CellSpecifier,
    prepared: &mut Option<Vec<PreparedTableCellParagraph<'a>>>,
) -> Result<TableCell<'a>, Error> {
    parse_table_cell_with_parser(
        content,
        state,
        cell_start_offset,
        spec,
        prepared,
        &mut GrammarContent,
    )
}

fn parse_table_cell_with_parser<'a>(
    content: &'a str,
    state: &mut ParserState<'a>,
    cell_start_offset: usize,
    spec: &CellSpecifier,
    prepared: &mut Option<Vec<PreparedTableCellParagraph<'a>>>,
    parser: &mut impl ContentParser<'a>,
) -> Result<TableCell<'a>, Error> {
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
        return Ok(TableCell::with_format(
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
    let mut footnotes = None;
    let blocks = if spec.style == Some(ColumnStyle::AsciiDoc) {
        // Nested documents have their own attributes and parsing context.
        *prepared = None;
        // An AsciiDoc-style cell is a nested document. It inherits the outer
        // attributes, but its local attributes, section catalog, hard-break
        // state, and pending callout references do not escape into sibling cells or the
        // outer document.
        let next_number = state.footnote_tracker.borrow().next_number();
        let nested_tracker = Rc::new(RefCell::new(state.footnote_tracker.borrow_mut().nested()));
        let outer_tracker = std::mem::replace(&mut state.footnote_tracker, nested_tracker);
        let outer_attributes = Rc::clone(&state.document_attributes);
        let outer_parent_attributes = state
            .nested_parent_attributes
            .replace(Rc::clone(&state.document_attributes));
        let outer_hardbreaks = state.hardbreaks;
        let outer_reftext = state.body_reftext_source;
        let outer_context = std::mem::replace(&mut state.block_context, BlockContext::Document);
        let outer_toc_len = state.toc_entries.len();
        let outer_scope = state.source_scope_offset;
        if state.input_origin.is_some() {
            state.source_scope_offset = cell_start_offset;
        }
        let outer_pending_callouts = std::mem::take(&mut state.pending_callouts);

        initial_attributes = crate::document_attribute::initialize_nested_attributes(Rc::make_mut(
            &mut state.document_attributes,
        ));

        let outer_separator =
            std::mem::replace(&mut state.raw_cell_separator, parser.raw_cell_separator());
        let result =
            parser.nested_document(state, content, cell_start_offset, &mut initial_attributes);
        state.raw_cell_separator = outer_separator;

        if result.is_err() {
            state.footnote_tracker.borrow_mut().discard_definitions();
            outer_tracker.borrow_mut().set_next_number(next_number);
        }
        footnotes = Some(std::mem::take(
            &mut state.footnote_tracker.borrow_mut().footnotes,
        ));
        state.footnote_tracker = outer_tracker;
        state.document_attributes = outer_attributes;
        state.nested_parent_attributes = outer_parent_attributes;
        state.hardbreaks = outer_hardbreaks;
        state.body_reftext_source = outer_reftext;
        state.block_context = outer_context;
        state.toc_entries.truncate(outer_toc_len);
        state.source_scope_offset = outer_scope;
        state.pending_callouts = outer_pending_callouts;
        result
    } else {
        Ok(parse_normal_cell(
            content,
            state,
            cell_start_offset,
            spec.is_duplication && spec.duplication_count > 1,
            prepared,
        ))
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
    let mut column = TableCell::with_format(
        blocks,
        spec.colspan,
        spec.rowspan,
        spec.halign,
        spec.valign,
        spec.style,
    );
    column.initial_attributes = initial_attributes;
    column.footnotes = footnotes;
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
