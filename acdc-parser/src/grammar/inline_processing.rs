use std::borrow::Cow;

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
            .offset_to_position(absolute_offset, state.input);
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
            .offset_to_position(absolute_offset, state.input);
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
pub(crate) fn preprocess_inline_content<'a>(
    state: &mut ParserState<'a>,
    content_start: &PositionWithOffset,
    end: usize,
    offset: usize,
    content: &'a str,
    macros_enabled: bool,
    attributes_enabled: bool,
) -> Result<(Location, ProcessedContent<'a>), Error> {
    // First, ensure the end position is on a valid UTF-8 boundary
    let mut adjusted_end = end + offset;
    if adjusted_end > 0 && adjusted_end <= state.input.len() {
        // If not on a boundary, round forward to the next valid boundary
        while adjusted_end < state.input.len() && !state.input.is_char_boundary(adjusted_end) {
            adjusted_end += 1;
        }
    }

    // We adjust the start and end positions to account for the content start offset
    let content_end_offset = if adjusted_end == 0 {
        0
    } else {
        crate::grammar::utf8_utils::safe_decrement_offset(state.input, adjusted_end)
    };
    let location = state.create_location(content_start.offset + offset, content_end_offset);

    // Fast path: skip the preprocessing PEG pass when content has no trigger characters.
    // The preprocessor only modifies content containing { (attribute/counter references),
    // + (constrained/unconstrained passthroughs), or pass: (macro passthroughs).
    let needs_preprocessing = content.as_bytes().iter().any(|&b| b == b'{' || b == b'+')
        || (macros_enabled && content.contains("pass:"));

    if !needs_preprocessing {
        // Hot path: no preprocessing trigger characters. Borrow directly from
        // the input instead of allocating — this is the single largest
        // per-node cost the profiler shows on inline-heavy documents.
        return Ok((
            location,
            ProcessedContent {
                text: Cow::Borrowed(content),
                passthroughs: Vec::new(),
                source_map: crate::grammar::inline_preprocessor::SourceMap::default(),
            },
        ));
    }

    let mut inline_state = InlinePreprocessorParserState::new(
        content,
        state.line_map.clone(),
        state.input,
        state.arena,
        macros_enabled,
        attributes_enabled,
    );
    inline_state.set_initial_position(&location, content_start.offset + offset);
    tracing::debug!(
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

/// Extract the inline-parsable text from a `ProcessedContent` at `'a`.
/// `Cow::Borrowed` preserves the outer lifetime directly; `Cow::Owned` is
/// interned into the parser arena so downstream `InlineNode`s can carry `'a`.
fn processed_text_as_outer<'a>(
    processed: &ProcessedContent<'a>,
    state: &ParserState<'a>,
) -> &'a str {
    match &processed.text {
        Cow::Borrowed(s) => s,
        Cow::Owned(s) => state.intern_str(s),
    }
}

#[tracing::instrument(skip_all, fields(processed=?processed, block_metadata=?block_metadata))]
pub(crate) fn parse_inlines<'a>(
    processed: &'a ProcessedContent<'a>,
    state: &mut ParserState<'a>,
    block_metadata: &BlockParsingMetadata,
    location: &Location,
) -> Result<Vec<InlineNode<'a>>, Error> {
    let text: &'a str = processed_text_as_outer(processed, state);
    let mut inline_peg_state = ParserState::for_inline_parsing(text, state);
    inline_peg_state.inline_ctx.offset = 0;
    inline_peg_state.inline_ctx.macros_enabled = block_metadata.macros_enabled;
    inline_peg_state.inline_ctx.attributes_enabled = block_metadata.attributes_enabled;
    inline_peg_state.inline_ctx.allow_autolinks = true;

    let inlines = if inline_peg_state.quotes_only {
        inline_parser::quotes_only_inlines(text, &mut inline_peg_state)
    } else {
        inline_parser::inlines(text, &mut inline_peg_state)
    };

    let inlines = match inlines {
        Ok(inlines) => inlines,
        Err(err) => {
            return Err(adjust_peg_error_position(
                &err,
                text,
                location.absolute_start,
                state,
            ));
        }
    };

    state.footnote_tracker = inline_peg_state.footnote_tracker.clone();
    Ok(inlines)
}

#[tracing::instrument(skip_all, fields(processed=?processed, block_metadata=?block_metadata))]
pub(crate) fn parse_inlines_no_autolinks<'a>(
    processed: &'a ProcessedContent<'a>,
    state: &mut ParserState<'a>,
    block_metadata: &BlockParsingMetadata,
    location: &Location,
) -> Result<Vec<InlineNode<'a>>, Error> {
    let text: &'a str = processed_text_as_outer(processed, state);
    let mut inline_peg_state = ParserState::for_inline_parsing(text, state);
    inline_peg_state.inline_ctx.offset = 0;
    inline_peg_state.inline_ctx.macros_enabled = block_metadata.macros_enabled;
    inline_peg_state.inline_ctx.attributes_enabled = block_metadata.attributes_enabled;
    inline_peg_state.inline_ctx.allow_autolinks = false;

    let inlines = match inline_parser::inlines_no_autolinks(text, &mut inline_peg_state) {
        Ok(inlines) => inlines,
        Err(err) => {
            return Err(adjust_peg_error_position(
                &err,
                text,
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
pub(crate) fn process_inlines<'a>(
    state: &mut ParserState<'a>,
    block_metadata: &BlockParsingMetadata,
    content_start: &PositionWithOffset,
    end: usize,
    offset: usize,
    content: &'a str,
) -> Result<Vec<InlineNode<'a>>, Error> {
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
    // Promote `processed` to `'a` by interning into the parser arena so the
    // inline parser and location remapper can hand back `InlineNode<'a>`.
    let processed: &'a ProcessedContent<'a> = state.arena.alloc_with(|| processed);
    let content = parse_inlines(processed, state, block_metadata, &location)?;
    super::location_mapping::map_inline_locations(state, processed, &content, &location)
}

/// Process inlines with autolinks suppressed.
///
/// Used inside URL macros, mailto macros, and cross-references where nested
/// autolinks would cause incorrect parsing.
#[tracing::instrument(skip_all, fields(?content_start, end, offset))]
pub(crate) fn process_inlines_no_autolinks<'a>(
    state: &mut ParserState<'a>,
    block_metadata: &BlockParsingMetadata,
    content_start: &PositionWithOffset,
    end: usize,
    offset: usize,
    content: &'a str,
) -> Result<Vec<InlineNode<'a>>, Error> {
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
    // Promote `processed` to `'a` by interning into the parser arena.
    let processed: &'a ProcessedContent<'a> = state.arena.alloc_with(|| processed);
    let content = parse_inlines_no_autolinks(processed, state, block_metadata, &location)?;
    super::location_mapping::map_inline_locations(state, processed, &content, &location)
}
