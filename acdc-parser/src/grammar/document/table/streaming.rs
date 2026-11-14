//! Read `AsciiDoc` cell bodies cooperatively, retaining the shared table layout.

use std::collections::HashMap;

use crate::{
    Block, ColumnStyle, Error, SourceLocation, TableCell, Warning,
    blocks::table::{
        CellPlacement, CellSpecifier, implicit_header, inferred_columns, next_cell_boundary,
    },
    grammar::ParserState,
};

use super::super::{
    RecognitionError,
    compound::retain_body,
    construction::ConstructionMode,
    coordinator::{BlockCursor, Execution},
    delimited::{ContentParser, ContentRequest},
    document_parser,
    source::SourceInput,
};
use super::{
    CellParser, InitialAttributes, PreparedTableCellParagraph, TableClosing, TableSettings,
    construction::TableConstruction, parse_table_cell_with_parser, parse_table_with_cells,
    table_settings,
};

struct CellReader<'i, 'h, 'callback, 'a, F> {
    input: &'i mut SourceInput<'a>,
    accepted: &'h mut Execution<'callback, F>,
    start: usize,
    end: usize,
    separator: char,
}

impl<'a, F> ContentParser<'a> for CellReader<'_, '_, '_, 'a, F>
where
    F: FnMut(&mut Block<'a>, &mut ParserState<'a>, &mut Vec<Block<'a>>) -> Result<bool, Error>,
{
    fn raw_cell_separator(&self) -> Option<char> {
        Some(self.separator)
    }

    fn parse(
        &mut self,
        state: &mut ParserState<'a>,
        request: ContentRequest<'a>,
    ) -> Result<Vec<Block<'a>>, Error> {
        super::super::coordinator::CursorContent {
            accepted: self.accepted,
        }
        .parse(state, request)
    }

    fn nested_document(
        &mut self,
        state: &mut ParserState<'a>,
        _source: &'a str,
        _offset: usize,
        initial: &mut InitialAttributes<'a>,
    ) -> Result<Result<Vec<Block<'a>>, Error>, RecognitionError> {
        self.input.reader_attributes(
            &state.document_attributes,
            state.nested_parent_attributes.as_ref(),
        );
        let source = match self.input.prepare_header_from(self.start, state) {
            Ok(source) => source,
            Err(error) => return Ok(Err(error)),
        };
        let leading = source.len() - source.trim_start().len();
        let start = self.start + leading;
        let prefix = match document_parser::nested_document_prefix(
            source.trim_start(),
            state,
            start,
            initial,
        )? {
            Ok(prefix) => prefix,
            Err(error) => return Ok(Err(error)),
        };
        let mut cursor = BlockCursor::new(prefix.body_offset, ConstructionMode::Ordered);
        cursor.base_offset = start;
        let mut blocks = prefix.blocks;
        let result = cursor.collect(self.input, state, &mut blocks, self.accepted)?;
        match result {
            Ok(end) => {
                self.end = self.input.cell_end().unwrap_or(end);
                Ok(Ok(blocks))
            }
            Err(error) => Ok(Err(error)),
        }
    }
}

struct CommittedCell<'a> {
    cell: TableCell<'a>,
    warnings: Vec<Warning>,
    macro_callbacks: usize,
    used: bool,
    location: SourceLocation,
}

struct CompletedCells<'p, P> {
    cells: HashMap<usize, CommittedCell<'p>>,
    parser: P,
}

impl<'a, P: ContentParser<'a>> CellParser<'a> for CompletedCells<'a, P> {
    fn parse(
        &mut self,
        source: &'a str,
        state: &mut ParserState<'a>,
        offset: usize,
        spec: &CellSpecifier,
        prepared: &mut Option<Vec<PreparedTableCellParagraph<'a>>>,
    ) -> Result<TableCell<'a>, Error> {
        if spec.style == Some(ColumnStyle::AsciiDoc)
            && let Some(committed) = self.cells.get_mut(&offset)
            && !committed.used
        {
            committed.used = true;
            for warning in std::mem::take(&mut committed.warnings) {
                state.add_warning(warning);
            }
            let mut cell = committed.cell.clone();
            cell.colspan = spec.colspan;
            cell.rowspan = spec.rowspan;
            cell.halign = spec.halign;
            cell.valign = spec.valign;
            return Ok(cell);
        }
        parse_table_cell_with_parser(source, state, offset, spec, prepared, &mut self.parser)
    }
}

struct TableReader<'a> {
    delimiter: &'a str,
    separator: char,
    settings: TableSettings,
    placement: CellPlacement,
    header: bool,
    cells: HashMap<usize, CommittedCell<'a>>,
}

impl<'a> TableReader<'a> {
    fn style(&self, mut spec: CellSpecifier) -> CellSpecifier {
        if self.header {
            spec.style = None;
        } else if spec.style.is_none()
            && let Some(column) = self.settings.column_formats.get(self.placement.generated())
            && column.style != ColumnStyle::Default
        {
            spec.style = Some(column.style);
        }
        if spec.is_duplication && spec.duplication_count == 0 {
            spec.style = None;
        }
        spec
    }

    fn cell<F>(
        &mut self,
        input: &mut SourceInput<'a>,
        state: &mut ParserState<'a>,
        start: usize,
        spec: CellSpecifier,
        accepted: &mut Execution<'_, F>,
    ) -> Result<usize, Error>
    where
        F: FnMut(&mut Block<'a>, &mut ParserState<'a>, &mut Vec<Block<'a>>) -> Result<bool, Error>,
    {
        let before = accepted.macro_callbacks();
        input.begin_cell(self.separator, self.delimiter, start);
        let mut warnings = Vec::new();
        let result = if spec.style == Some(ColumnStyle::AsciiDoc) {
            // Replay grammar diagnostics when the final layout consumes this
            // cell. Reader diagnostics keep their original shared collector.
            let outer_warnings = std::mem::take(&mut state.warnings);
            let outer_warnings_start = std::mem::replace(&mut state.warnings_start, 0);
            let mut reader = CellReader {
                input,
                accepted,
                start,
                end: start,
                separator: self.separator,
            };
            let result =
                parse_table_cell_with_parser("", state, start, &spec, &mut None, &mut reader)
                    .map(|cell| (reader.end, Some(cell)));
            warnings = state.warnings.borrow_mut().split_off(0);
            state.warnings = outer_warnings;
            state.warnings_start = outer_warnings_start;
            result
        } else {
            let mut end = start;
            loop {
                match input.line(end, state) {
                    Ok(Some(line)) => end += line.len(),
                    Ok(None) => break Ok((input.cell_end().unwrap_or(end), None)),
                    Err(error) => break Err(error),
                }
            }
        };
        input.end_cell();
        input.reader_attributes(
            &state.document_attributes,
            state.nested_parent_attributes.as_ref(),
        );
        let (end, cell) = match result {
            Ok(parsed) => parsed,
            Err(error) => {
                for warning in warnings {
                    state.add_warning(warning);
                }
                return Err(error);
            }
        };
        if let Some(cell) = cell {
            let text = retain_body(state, start, end).ok_or_else(|| {
                Error::Extension(
                    Box::new(
                        state.create_error_source_location(state.create_location(start, start)),
                    ),
                    "missing committed table cell source".into(),
                )
            })?;
            let key = start + text.len() - text.trim_start().len();
            self.cells.insert(
                key,
                CommittedCell {
                    cell,
                    warnings,
                    macro_callbacks: accepted.macro_callbacks() - before,
                    used: false,
                    location: state.create_error_source_location(state.create_location(key, key)),
                },
            );
        }
        Ok(end)
    }

    fn collect<F>(
        &mut self,
        input: &mut SourceInput<'a>,
        state: &mut ParserState<'a>,
        mut offset: usize,
        accepted: &mut Execution<'_, F>,
    ) -> Result<(usize, Option<&'a str>), Error>
    where
        F: FnMut(&mut Block<'a>, &mut ParserState<'a>, &mut Vec<Block<'a>>) -> Result<bool, Error>,
    {
        while let Some(line) = input.raw_line(offset, state)? {
            if document_parser::source_block_close(line, state, self.delimiter, true).is_ok() {
                return Ok((offset, Some(line.trim_end_matches('\n'))));
            }
            if let Some((_, content_start, spec)) = next_cell_boundary(line, self.separator, true) {
                spec.validate(offset, offset + content_start)
                    .map_err(|violation| {
                        super::table_limit_error(
                            state,
                            state.create_location(violation.start, violation.end),
                            violation.resource,
                            violation.requested,
                            violation.limit,
                        )
                    })?;
                while self.placement.at_row_end() {
                    self.placement
                        .finish_row(offset, offset + content_start)
                        .map_err(|violation| {
                            super::table_limit_error(
                                state,
                                state.create_location(violation.start, violation.end),
                                violation.resource,
                                violation.requested,
                                violation.limit,
                            )
                        })?;
                    self.header = false;
                }
                let style = self.style(spec);
                self.placement.place(spec);
                offset = self.cell(input, state, offset + content_start, style, accepted)?;
            } else {
                offset += line.len();
            }
        }
        Ok((offset, None))
    }
}

impl BlockCursor {
    pub(in crate::grammar::document) fn construct_streamed_table<'a, F>(
        &mut self,
        input: &mut SourceInput<'a>,
        state: &mut ParserState<'a>,
        construction: TableConstruction<'a>,
        output: &mut Vec<Block<'a>>,
        accepted: &mut Execution<'_, F>,
    ) -> Result<(), Error>
    where
        F: FnMut(&mut Block<'a>, &mut ParserState<'a>, &mut Vec<Block<'a>>) -> Result<bool, Error>,
    {
        let TableConstruction {
            metadata,
            mut params,
        } = construction;
        let location = state.create_block_location(params.start, params.end, params.offset);
        // The final builder reports diagnostics using the completed table span.
        let settings =
            table_settings(state, &metadata, &location, params.default_separator, false)?;
        if !matches!(settings.separator.as_str(), "|" | "!") {
            return self.construct_buffered_table(
                input,
                state,
                TableConstruction { metadata, params },
                output,
                accepted,
            );
        }
        let start = params.content_start + params.offset;
        let first = match input.raw_line(start, state) {
            Ok(Some(line)) => line,
            Ok(None) => "",
            Err(error) => return Err(error),
        };
        let mut lines = vec![first.trim_end_matches('\n')];
        lines.extend(input.table_header_lookahead());
        let columns = settings
            .ncols
            .unwrap_or_else(|| inferred_columns(first, &settings.separator));
        let header = !metadata.metadata.options.contains(&"noheader")
            && (metadata.metadata.options.contains(&"header")
                || implicit_header(&lines, &settings.separator));
        let separator = settings.separator.chars().next().unwrap_or('|');
        let mut reader = TableReader {
            delimiter: params.open_delim,
            separator,
            settings,
            placement: CellPlacement::new(columns),
            header,
            cells: HashMap::new(),
        };
        let (end, close) = reader.collect(input, state, start, accepted)?;
        params.end = end - params.offset + close.map_or(0, str::len);
        params.content_end = end.saturating_sub(usize::from(close.is_some())) - params.offset;
        params.content =
            retain_body(state, start, params.content_end + params.offset).ok_or_else(|| {
                Error::Extension(
                    Box::new(state.create_error_source_location(location)),
                    "missing committed table source".into(),
                )
            })?;
        params.closing = close.map_or(TableClosing::Unterminated, |close_delim| {
            TableClosing::Terminated {
                close_delim,
                close_start: end - params.offset,
            }
        });
        let mut parser = CompletedCells {
            cells: reader.cells,
            parser: super::super::coordinator::CursorContent { accepted },
        };
        let mut block = parse_table_with_cells(&params, state, &metadata, &mut parser)?;
        if let Some(discarded) = parser
            .cells
            .values()
            .filter(|cell| !cell.used && cell.macro_callbacks > 0)
            .min_by_key(|cell| cell.location.location.absolute_start)
        {
            return Err(Error::Extension(
                Box::new(discarded.location.clone()),
                "table layout discarded a cell after its block macro executed".into(),
            ));
        }
        match accepted.run(&mut block, state, output) {
            Ok(true) => {
                super::super::metadata::assign_block_caption(state, &mut block);
                output.push(block);
            }
            Ok(false) => {}
            Err(error) => return Err(error),
        }
        self.offset = params.end;
        self.base_offset = params.offset;
        Ok(())
    }
}

impl BlockCursor {
    fn construct_buffered_table<'a, F>(
        &mut self,
        input: &mut SourceInput<'a>,
        state: &mut ParserState<'a>,
        mut construction: TableConstruction<'a>,
        output: &mut Vec<Block<'a>>,
        accepted: &mut Execution<'_, F>,
    ) -> Result<(), Error>
    where
        F: FnMut(&mut Block<'a>, &mut ParserState<'a>, &mut Vec<Block<'a>>) -> Result<bool, Error>,
    {
        let params = &mut construction.params;
        let start = params.content_start + params.offset;
        let mut end = start;
        let mut close = None;
        while let Some(line) = input.raw_line(end, state)? {
            if document_parser::source_block_close(line, state, params.open_delim, true).is_ok() {
                close = Some(line.trim_end_matches('\n'));
                break;
            }
            end += line.len();
        }
        params.end = end - params.offset + close.map_or(0, str::len);
        params.content_end = end.saturating_sub(usize::from(close.is_some())) - params.offset;
        params.content =
            retain_body(state, start, params.content_end + params.offset).ok_or_else(|| {
                Error::Extension(
                    Box::new(
                        state.create_error_source_location(state.create_location(start, start)),
                    ),
                    "missing buffered table source".into(),
                )
            })?;
        params.closing = close.map_or(TableClosing::Unterminated, |close_delim| {
            TableClosing::Terminated {
                close_delim,
                close_start: end - params.offset,
            }
        });
        self.offset = params.end;
        self.base_offset = params.offset;
        let (mut block, content) = construction.start(state)?;
        if accepted.run(&mut block, state, output)? {
            content.finish(
                &mut block,
                state,
                &mut super::super::coordinator::CursorContent { accepted },
            )?;
            super::super::metadata::assign_block_caption(state, &mut block);
            output.push(block);
        }
        Ok(())
    }
}
