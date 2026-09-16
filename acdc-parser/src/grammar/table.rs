use std::rc::Rc;

use crate::{
    AttributeName, Block, ColumnStyle, DocumentAttribute, DocumentAttributeAssignment, Error,
    InlineNode, Paragraph, TableColumn, Verbatim, blocks::table::ParsedCell, model::SectionLevel,
};

use super::{ParserState, document_parser, inline_processing::adjust_and_log_parse_error};

pub(crate) fn parse_table_cell<'a>(
    content: &'a str,
    state: &mut ParserState<'a>,
    cell_start_offset: usize,
    parent_section_level: Option<SectionLevel>,
    cell: &ParsedCell,
) -> Result<TableColumn<'a>, Error> {
    // Literal cells keep their source text intact. Unlike listing blocks, they
    // do not run attribute, macro, quote, or callout substitutions.
    if cell.style == Some(ColumnStyle::Literal) {
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
            cell.colspan,
            cell.rowspan,
            cell.halign,
            cell.valign,
            cell.style,
        ));
    }

    // Markdown blockquotes are only parsed when cell has AsciiDoc style ('a' prefix).
    // This matches asciidoctor behavior where `> text` is only a blockquote in 'a' style cells.
    let mut initial_attributes = Vec::new();
    let blocks = if cell.style == Some(ColumnStyle::AsciiDoc) {
        // An AsciiDoc-style cell is a nested document. It inherits the outer
        // attributes, but its local attributes, section catalog, hard-break
        // state, and callout adjacency do not escape into sibling cells or the
        // outer document.
        let outer_attributes = Rc::clone(&state.document_attributes);
        let outer_parent_attributes = state
            .nested_parent_attributes
            .replace(Rc::clone(&state.document_attributes));
        let outer_hardbreaks = state.hardbreaks;
        let outer_toc_len = state.toc_entries.len();
        let outer_last_block_was_verbatim = state.last_block_was_verbatim;
        let outer_last_verbatim_callouts = std::mem::take(&mut state.last_verbatim_callouts);

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
        state.toc_entries.truncate(outer_toc_len);
        state.last_block_was_verbatim = outer_last_block_was_verbatim;
        state.last_verbatim_callouts = outer_last_verbatim_callouts;
        result
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
    let mut column = TableColumn::with_format(
        blocks,
        cell.colspan,
        cell.rowspan,
        cell.halign,
        cell.valign,
        cell.style,
    );
    column.initial_attributes = initial_attributes;
    Ok(column)
}

pub(crate) fn normalize_nested_header<'a>(
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
