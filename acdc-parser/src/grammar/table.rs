use crate::{TableColumn, model::SectionLevel};

use super::{ParserState, document_parser};

pub(crate) fn parse_table_cell(
    content: &str,
    state: &mut ParserState,
    cell_start_offset: usize,
    parent_section_level: Option<SectionLevel>,
) -> TableColumn {
    let content = document_parser::blocks(content, state, cell_start_offset, parent_section_level)
        .expect("valid blocks inside table cell")
        .unwrap_or_else(|_e| {
            //TODO(nlopes): tracing::error!(e, "Error parsing table cell content as blocks");
            Vec::new()
        });
    TableColumn { content }
}
