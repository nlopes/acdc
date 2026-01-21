use crate::{ColumnStyle, Error, TableColumn, blocks::table::ParsedCell, model::SectionLevel};

use super::{ParserState, document_parser, inline_processing::adjust_and_log_parse_error};

pub(crate) fn parse_table_cell(
    content: &str,
    state: &mut ParserState,
    cell_start_offset: usize,
    parent_section_level: Option<SectionLevel>,
    cell: &ParsedCell,
) -> Result<TableColumn, Error> {
    // Markdown blockquotes are only parsed when cell has AsciiDoc style ('a' prefix).
    // This matches asciidoctor behavior where `> text` is only a blockquote in 'a' style cells.
    let blocks = if cell.style == Some(ColumnStyle::AsciiDoc) {
        document_parser::blocks(content, state, cell_start_offset, parent_section_level)
    } else {
        document_parser::blocks_for_table_cell(
            content,
            state,
            cell_start_offset,
            parent_section_level,
        )
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
    Ok(TableColumn::with_format(
        blocks,
        cell.colspan,
        cell.rowspan,
        cell.halign,
        cell.valign,
        cell.style,
    ))
}
