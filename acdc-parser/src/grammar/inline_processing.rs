use crate::{
    Error, InlineNode, InlinePreprocessorParserState, Location, ProcessedContent,
    inline_preprocessing,
};

use super::{
    ParserState,
    document::{BlockParsingMetadata, PositionWithOffset, document_parser},
};

/// Adjust PEG parser error positions to account for substring parsing
///
/// When PEG parses a substring of the document, it reports positions relative to that substring.
/// This function converts those positions to the correct positions in the original document.
pub(crate) fn adjust_peg_error_position(
    err: &peg::error::ParseError<peg::str::LineCol>,
    parsed_text: &str,
    doc_start_offset: usize,
    state: &ParserState,
) -> Error {
    // Calculate the byte offset within the substring where the error occurred
    let mut byte_offset = 0;
    for (line_idx, line) in parsed_text.lines().enumerate() {
        if line_idx + 1 == err.location.line {
            byte_offset += err.location.column - 1; // column is 1-indexed
            break;
        }
        byte_offset += line.len() + 1; // +1 for newline
    }

    // Add the substring's starting position to get the absolute document position
    let absolute_offset = doc_start_offset + byte_offset;

    // Convert the absolute offset back to line:column using the state's line_map
    let doc_position = state.line_map.offset_to_position(absolute_offset);

    let adjusted_error = format!(
        "error at {}:{}: {}",
        doc_position.line,
        doc_position.column,
        err.to_string()
            .split_once(": ")
            .map_or(err.to_string(), |(_, msg)| msg.to_string())
    );
    Error::Parse(adjusted_error)
}

/// Helper for error recovery when parsing from a substring
///
/// Adjusts error positions to the original document and logs the error
pub(crate) fn adjust_and_log_parse_error(
    err: &peg::error::ParseError<peg::str::LineCol>,
    parsed_text: &str,
    doc_start_offset: usize,
    state: &ParserState,
    context: &str,
) {
    let adjusted_error = adjust_peg_error_position(err, parsed_text, doc_start_offset, state);
    tracing::error!(?adjusted_error, "{context}");
}

#[tracing::instrument(skip_all, fields(?start, ?content_start, end, offset))]
pub(crate) fn preprocess_inline_content(
    state: &ParserState,
    start: usize,
    content_start: &PositionWithOffset,
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
    state: &mut ParserState,
    block_metadata: &BlockParsingMetadata,
    location: &Location,
) -> Result<Vec<InlineNode>, Error> {
    let mut inline_peg_state = ParserState::new(&processed.text);
    inline_peg_state.document_attributes = state.document_attributes.clone();
    inline_peg_state.footnote_tracker = state.footnote_tracker.clone();

    let inlines =
        match document_parser::inlines(&processed.text, &mut inline_peg_state, 0, block_metadata) {
            Ok(inlines) => inlines,
            Err(err) => {
                return Err(adjust_peg_error_position(
                    &err,
                    &processed.text,
                    location.absolute_start,
                    state,
                ));
            }
        };

    state.footnote_tracker = inline_peg_state.footnote_tracker.clone();
    Ok(inlines)
}

/// Process inlines
///
/// This function processes inline content by first preprocessing it and then parsing it
/// into inline nodes. Then, it maps the locations of the parsed inline nodes back to their
/// original positions in the source.
#[tracing::instrument(skip_all, fields(?start, ?content_start, end, offset))]
pub(crate) fn process_inlines(
    state: &mut ParserState,
    block_metadata: &BlockParsingMetadata,
    start: usize,
    content_start: &PositionWithOffset,
    end: usize,
    offset: usize,
    content: &str,
) -> Result<(Vec<InlineNode>, Location), Error> {
    // Preprocess the inline content first
    let (initial_location, location, processed) =
        preprocess_inline_content(state, start, content_start, end, offset, content)?;
    let content = parse_inlines(&processed, state, block_metadata, &location)?;
    let content =
        super::location_mapping::map_inline_locations(state, &processed, &content, &location);
    Ok((content, initial_location))
}
