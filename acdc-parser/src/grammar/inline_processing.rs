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
    let doc_position = state
        .line_map
        .offset_to_position(absolute_offset, &state.input);

    Error::PegParse(
        Box::new(crate::SourceLocation {
            file: state.current_file.clone(),
            positioning: crate::Positioning::Position(doc_position),
        }),
        err.to_string()
            .split_once(": ")
            .map_or(err.to_string(), |(_, msg)| msg.to_string()),
    )
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
    tracing::error!(?adjusted_error, ?context, "Parsing error occurred");
}

#[tracing::instrument(skip_all, fields(?content_start, end, offset))]
pub(crate) fn preprocess_inline_content(
    state: &mut ParserState,
    content_start: &PositionWithOffset,
    end: usize,
    offset: usize,
    content: &str,
) -> Result<(Location, ProcessedContent), Error> {
    // First, ensure the end position is on a valid UTF-8 boundary
    let mut adjusted_end = end + offset;
    if adjusted_end > 0 && adjusted_end <= state.input.len() {
        // If not on a boundary, round forward to the next valid boundary
        while adjusted_end < state.input.len() && !state.input.is_char_boundary(adjusted_end) {
            adjusted_end += 1;
        }
    }

    let mut inline_state =
        InlinePreprocessorParserState::new(content, state.line_map.clone(), &state.input);

    // We adjust the start and end positions to account for the content start offset
    let content_end_offset = if adjusted_end == 0 {
        0
    } else {
        crate::grammar::utf8_utils::safe_decrement_offset(&state.input, adjusted_end)
    };
    let location = state.create_location(content_start.offset + offset, content_end_offset);
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
    // Drain warnings collected during inline preprocessing and add them to the main
    // parser state for post-parse emission. Dedup is handled by both layers:
    // InlinePreprocessorParserState deduplicates within a single preprocessing run,
    // and ParserState deduplicates across the entire parse.
    for warning in inline_state.drain_warnings() {
        state.add_warning(warning);
    }
    Ok((location, processed))
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
    inline_peg_state.quotes_only = state.quotes_only;

    let inlines = if inline_peg_state.quotes_only {
        document_parser::quotes_only_inlines(
            &processed.text,
            &mut inline_peg_state,
            0,
            block_metadata,
        )
    } else {
        document_parser::inlines(&processed.text, &mut inline_peg_state, 0, block_metadata)
    };

    let inlines = match inlines {
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

#[tracing::instrument(skip_all, fields(processed=?processed, block_metadata=?block_metadata))]
pub(crate) fn parse_inlines_no_autolinks(
    processed: &ProcessedContent,
    state: &mut ParserState,
    block_metadata: &BlockParsingMetadata,
    location: &Location,
) -> Result<Vec<InlineNode>, Error> {
    let mut inline_peg_state = ParserState::new(&processed.text);
    inline_peg_state.document_attributes = state.document_attributes.clone();
    inline_peg_state.footnote_tracker = state.footnote_tracker.clone();

    let inlines = match document_parser::inlines_no_autolinks(
        &processed.text,
        &mut inline_peg_state,
        0,
        block_metadata,
    ) {
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
#[tracing::instrument(skip_all, fields(?content_start, end, offset))]
pub(crate) fn process_inlines(
    state: &mut ParserState,
    block_metadata: &BlockParsingMetadata,
    content_start: &PositionWithOffset,
    end: usize,
    offset: usize,
    content: &str,
) -> Result<Vec<InlineNode>, Error> {
    let (location, processed) =
        preprocess_inline_content(state, content_start, end, offset, content)?;
    // After preprocessing, attribute substitution may result in empty content
    // (e.g., {empty} -> ""). In this case, return empty vec without parsing.
    if processed.text.trim().is_empty() {
        return Ok(Vec::new());
    }
    let content = parse_inlines(&processed, state, block_metadata, &location)?;
    super::location_mapping::map_inline_locations(state, &processed, &content, &location)
}

/// Process inlines with autolinks suppressed.
///
/// Used inside URL macros, mailto macros, and cross-references where nested
/// autolinks would cause incorrect parsing.
#[tracing::instrument(skip_all, fields(?content_start, end, offset))]
pub(crate) fn process_inlines_no_autolinks(
    state: &mut ParserState,
    block_metadata: &BlockParsingMetadata,
    content_start: &PositionWithOffset,
    end: usize,
    offset: usize,
    content: &str,
) -> Result<Vec<InlineNode>, Error> {
    let (location, processed) =
        preprocess_inline_content(state, content_start, end, offset, content)?;
    if processed.text.trim().is_empty() {
        return Ok(Vec::new());
    }
    let content = parse_inlines_no_autolinks(&processed, state, block_metadata, &location)?;
    super::location_mapping::map_inline_locations(state, &processed, &content, &location)
}
