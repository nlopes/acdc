//! Accepted table layout with cell construction deferred until after PEG returns.

use std::mem;

use super::{
    CellParser, CellSpecifier, ContentParser, PreparedTableCellParagraph, RowDestination,
    TableParseParams, parse_table_cell, parse_table_cell_with_parser, parse_table_with_cells,
};
use crate::{
    Block, DelimitedBlockType, Error, TableCell, Warning,
    grammar::{ParserState, helpers::BlockParsingMetadata},
};

pub(in crate::grammar::document) struct TableConstruction<'a> {
    pub(in crate::grammar::document) metadata: BlockParsingMetadata<'a>,
    pub(in crate::grammar::document) params: TableParseParams<'a>,
}

struct CellRequest<'a> {
    source: &'a str,
    offset: usize,
    spec: CellSpecifier,
    group: usize,
}

struct RowRequest<'a> {
    cells: Vec<CellRequest<'a>>,
    destination: Option<RowDestination>,
    warnings: Vec<Warning>,
}

#[derive(Default)]
pub(in crate::grammar::document) struct TableContent<'a> {
    rows: Vec<RowRequest<'a>>,
    cells: Vec<CellRequest<'a>>,
    group: usize,
    warning_start: usize,
}

impl<'a> CellParser<'a> for TableContent<'a> {
    fn begin_row(&mut self, state: &ParserState<'a>) {
        self.warning_start = state.warnings.borrow().len();
    }

    fn begin_cell(&mut self) {
        self.group += 1;
    }

    fn parse(
        &mut self,
        content: &'a str,
        _state: &mut ParserState<'a>,
        offset: usize,
        spec: &CellSpecifier,
        _prepared: &mut Option<Vec<PreparedTableCellParagraph<'a>>>,
    ) -> Result<TableCell<'a>, Error> {
        self.cells.push(CellRequest {
            source: content,
            offset,
            spec: *spec,
            group: self.group,
        });
        Ok(TableCell::with_format(
            Vec::new(),
            spec.colspan,
            spec.rowspan,
            spec.halign,
            spec.valign,
            spec.style,
        ))
    }

    fn finish_row(&mut self, state: &mut ParserState<'a>, destination: Option<RowDestination>) {
        self.rows.push(RowRequest {
            cells: mem::take(&mut self.cells),
            destination,
            warnings: state.warnings.borrow_mut().split_off(self.warning_start),
        });
    }
}

impl<'a> TableConstruction<'a> {
    pub(in crate::grammar::document) fn start(
        self,
        state: &mut ParserState<'a>,
    ) -> Result<(Block<'a>, TableContent<'a>), Error> {
        let mut content = TableContent::default();
        let block = parse_table_with_cells(&self.params, state, &self.metadata, &mut content)?;
        Ok((block, content))
    }
}

impl<'a> TableContent<'a> {
    pub(in crate::grammar::document) fn finish(
        self,
        block: &mut Block<'a>,
        state: &mut ParserState<'a>,
        parser: &mut impl ContentParser<'a>,
    ) -> Result<(), Error> {
        let location = block.location().clone();
        let Block::DelimitedBlock(block) = block else {
            return Ok(());
        };
        let DelimitedBlockType::DelimitedTable(table) = &mut block.inner else {
            return Ok(());
        };
        let mut group = None;
        let mut prepared = None;
        for row in self.rows {
            for (column, request) in row.cells.into_iter().enumerate() {
                if group != Some(request.group) {
                    group = Some(request.group);
                    prepared = None;
                }
                let Some(destination) = row.destination else {
                    // Native numbering and diagnostics from a discarded row
                    // retain their established behavior. External handlers
                    // only run for cells in the accepted table layout.
                    parse_table_cell(
                        request.source,
                        state,
                        request.offset,
                        &request.spec,
                        &mut prepared,
                    )?;
                    continue;
                };
                let cell = parse_table_cell_with_parser(
                    request.source,
                    state,
                    request.offset,
                    &request.spec,
                    &mut prepared,
                    parser,
                )?;
                let slot = crate::extensions::table_cell_mut(
                    table,
                    destination.part,
                    destination.row,
                    column,
                )
                .ok_or_else(|| {
                    Error::Parse(
                        Box::new(state.create_error_source_location(location.clone())),
                        "table builder returned no accepted cell slot".into(),
                    )
                })?;
                *slot = cell;
            }
            for warning in row.warnings {
                state.warnings.borrow_mut().push(warning);
            }
        }
        Ok(())
    }
}
