use crate::{Error, TableColumn, model::SectionLevel};

use super::{ParserState, document_parser, inline_processing::adjust_and_log_parse_error};

pub(crate) fn parse_table_cell(
    content: &str,
    state: &mut ParserState,
    cell_start_offset: usize,
    parent_section_level: Option<SectionLevel>,
    colspan: usize,
    rowspan: usize,
) -> Result<TableColumn, Error> {
    let blocks = document_parser::blocks(content, state, cell_start_offset, parent_section_level)
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
    Ok(TableColumn::with_spans(blocks, colspan, rowspan))
}
