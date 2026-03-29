use crate::{
    Error, InlineNode, InlinePreprocessorParserState, Location, ProcessedContent,
    inline_preprocessing,
};

use super::{
    ParserState,
    helpers::{BlockParsingMetadata, PositionWithOffset},
    inlines::inline_parser,
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

    // Resolve file and line from source ranges (for included content)
    let (file, position) = if let Some(range) = state
        .source_ranges
        .iter()
        .rev()
        .find(|r| r.contains(absolute_offset))
    {
        let line_in_file = state
            .input
            .get(range.start_offset..absolute_offset)
            .map_or(0, |s| s.matches('\n').count());
        let doc_position = state
            .line_map
            .offset_to_position(absolute_offset, &state.input);
        (
            Some(range.file.clone()),
            crate::Position {
                line: range.start_line + line_in_file,
                column: doc_position.column,
            },
        )
    } else {
        let doc_position = state
            .line_map
            .offset_to_position(absolute_offset, &state.input);
        (state.current_file.clone(), doc_position)
    };

    Error::PegParse(
        Box::new(crate::SourceLocation {
            file,
            positioning: crate::Positioning::Position(position),
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
    macros_enabled: bool,
    attributes_enabled: bool,
) -> Result<(Location, ProcessedContent), Error> {
    // First, ensure the end position is on a valid UTF-8 boundary
    let mut adjusted_end = end + offset;
    if adjusted_end > 0 && adjusted_end <= state.input.len() {
        // If not on a boundary, round forward to the next valid boundary
        while adjusted_end < state.input.len() && !state.input.is_char_boundary(adjusted_end) {
            adjusted_end += 1;
        }
    }

    let mut inline_state = InlinePreprocessorParserState::new(
        content,
        state.line_map.clone(),
        &state.input,
        macros_enabled,
        attributes_enabled,
    );

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
    std::mem::swap(
        &mut inline_peg_state.document_attributes,
        &mut state.document_attributes,
    );
    std::mem::swap(
        &mut inline_peg_state.footnote_tracker,
        &mut state.footnote_tracker,
    );
    inline_peg_state.quotes_only = state.quotes_only;
    inline_peg_state.outer_constrained_delimiter = state.outer_constrained_delimiter;

    let inlines = if inline_peg_state.quotes_only {
        inline_parser::quotes_only_inlines(
            &processed.text,
            &mut inline_peg_state,
            0,
            block_metadata,
        )
    } else {
        inline_parser::inlines(&processed.text, &mut inline_peg_state, 0, block_metadata)
    };

    std::mem::swap(
        &mut inline_peg_state.document_attributes,
        &mut state.document_attributes,
    );
    std::mem::swap(
        &mut inline_peg_state.footnote_tracker,
        &mut state.footnote_tracker,
    );

    match inlines {
        Ok(inlines) => Ok(inlines),
        Err(err) => Err(adjust_peg_error_position(
            &err,
            &processed.text,
            location.absolute_start,
            state,
        )),
    }
}

#[tracing::instrument(skip_all, fields(processed=?processed, block_metadata=?block_metadata))]
pub(crate) fn parse_inlines_no_autolinks(
    processed: &ProcessedContent,
    state: &mut ParserState,
    block_metadata: &BlockParsingMetadata,
    location: &Location,
) -> Result<Vec<InlineNode>, Error> {
    let mut inline_peg_state = ParserState::new(&processed.text);
    std::mem::swap(
        &mut inline_peg_state.document_attributes,
        &mut state.document_attributes,
    );
    std::mem::swap(
        &mut inline_peg_state.footnote_tracker,
        &mut state.footnote_tracker,
    );
    inline_peg_state.outer_constrained_delimiter = state.outer_constrained_delimiter;

    let inlines = inline_parser::inlines_no_autolinks(
        &processed.text,
        &mut inline_peg_state,
        0,
        block_metadata,
    );

    std::mem::swap(
        &mut inline_peg_state.document_attributes,
        &mut state.document_attributes,
    );
    std::mem::swap(
        &mut inline_peg_state.footnote_tracker,
        &mut state.footnote_tracker,
    );

    match inlines {
        Ok(inlines) => Ok(inlines),
        Err(err) => Err(adjust_peg_error_position(
            &err,
            &processed.text,
            location.absolute_start,
            state,
        )),
    }
}

/// Process inlines from already-preprocessed content.
///
/// Skips the inline preprocessor (attribute substitution, passthrough extraction) because
/// the content is a substring of text that was already preprocessed at the paragraph level.
/// Falls back to full processing if the content contains passthrough placeholders.
#[tracing::instrument(skip_all, fields(?content_start, end, offset))]
pub(crate) fn process_inlines_preprocessed(
    state: &mut ParserState,
    block_metadata: &BlockParsingMetadata,
    content_start: &PositionWithOffset,
    end: usize,
    offset: usize,
    content: &str,
) -> Result<Vec<InlineNode>, Error> {
    // Passthrough placeholders require full processing for reconstruction
    if content.contains('\u{FFFD}') {
        return process_inlines(state, block_metadata, content_start, end, offset, content);
    }

    // Content is already preprocessed — skip the preprocessor entirely
    let mut adjusted_end = end + offset;
    if adjusted_end > 0 && adjusted_end <= state.input.len() {
        while adjusted_end < state.input.len() && !state.input.is_char_boundary(adjusted_end) {
            adjusted_end += 1;
        }
    }
    let content_end_offset = if adjusted_end == 0 {
        0
    } else {
        crate::grammar::utf8_utils::safe_decrement_offset(&state.input, adjusted_end)
    };
    let location = state.create_location(content_start.offset + offset, content_end_offset);

    let processed = ProcessedContent {
        text: content.to_string(),
        passthroughs: Vec::new(),
        source_map: super::inline_preprocessor::SourceMap::default(),
    };

    if processed.text.trim().is_empty() {
        return Ok(Vec::new());
    }

    let result = parse_inlines(&processed, state, block_metadata, &location)?;
    super::location_mapping::map_inline_locations(state, &processed, &result, &location)
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
    let (location, processed) = preprocess_inline_content(
        state,
        content_start,
        end,
        offset,
        content,
        block_metadata.macros_enabled,
        block_metadata.attributes_enabled,
    )?;
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
    let (location, processed) = preprocess_inline_content(
        state,
        content_start,
        end,
        offset,
        content,
        block_metadata.macros_enabled,
        block_metadata.attributes_enabled,
    )?;
    if processed.text.trim().is_empty() {
        return Ok(Vec::new());
    }
    let content = parse_inlines_no_autolinks(&processed, state, block_metadata, &location)?;
    super::location_mapping::map_inline_locations(state, &processed, &content, &location)
}
