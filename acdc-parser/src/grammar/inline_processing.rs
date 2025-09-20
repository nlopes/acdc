use crate::{
    Error, InlineNode, InlinePreprocessorParserState, Location, ProcessedContent,
    inline_preprocessing,
};

use super::document::{BlockParsingMetadata, ParserState, Position, document_parser};

#[tracing::instrument(skip_all, fields(?start, ?content_start, end, offset))]
pub(crate) fn preprocess_inline_content(
    state: &ParserState,
    start: usize,
    content_start: &Position,
    end: usize,
    offset: usize,
    content: &str,
) -> Result<(Location, Location, ProcessedContent), Error> {
    // Create initial location for the entire content before inline processing
    let initial_location = state.create_location(start + offset, (end + offset).saturating_sub(1));
    // parse the inline content - this needs to be handed over to the inline preprocessing
    let mut inline_state = InlinePreprocessorParserState::new();

    // We adjust the start and end positions to account for the content start offset
    let location = state.create_location(
        content_start.offset + offset,
        (end + offset).saturating_sub(1),
    );
    inline_state.set_initial_position(&location, content_start.offset + offset);
    tracing::info!(
        ?inline_state,
        ?location,
        ?offset,
        ?content_start,
        ?end,
        "before inline preprocessing run"
    );

    let processed = inline_preprocessing::run(content, &state.document_attributes, &inline_state)?;
    Ok((initial_location, location, processed))
}

#[tracing::instrument(skip_all, fields(processed=?processed, block_metadata=?block_metadata))]
pub(crate) fn parse_inlines(
    processed: &ProcessedContent,
    document_attributes: &crate::DocumentAttributes,
    block_metadata: &BlockParsingMetadata,
) -> Result<Vec<InlineNode>, Error> {
    let mut inline_peg_state = ParserState::new(&processed.text);
    inline_peg_state.document_attributes = document_attributes.clone();
    Ok(document_parser::inlines(
        &processed.text,
        &mut inline_peg_state,
        0,
        block_metadata,
    )?)
}

/// Process inlines
///
/// This function processes inline content by first preprocessing it and then parsing it
/// into inline nodes. Then, it maps the locations of the parsed inline nodes back to their
/// original positions in the source.
#[tracing::instrument(skip_all, fields(?start, ?content_start, end, offset))]
pub(crate) fn process_inlines(
    state: &ParserState,
    block_metadata: &BlockParsingMetadata,
    start: usize,
    content_start: &Position,
    end: usize,
    offset: usize,
    content: &str,
) -> Result<(Vec<InlineNode>, Location), Error> {
    // Preprocess the inline content first
    let (initial_location, location, processed) =
        preprocess_inline_content(state, start, content_start, end, offset, content)?;
    let content = parse_inlines(&processed, &state.document_attributes, block_metadata)?;
    let content =
        super::location_mapping::map_inline_locations(state, &processed, &content, &location);
    Ok((content, initial_location))
}
