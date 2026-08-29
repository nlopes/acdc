use std::borrow::Cow;
use std::ops::Range;

use crate::{
    Error, InlineNode, InlinePreprocessorParserState, Location, Plain, Position, ProcessedContent,
    SourceLocation,
    grammar::{inline_preprocessor::SourceMap, utf8_utils},
    inline_preprocessing,
    model::{SourceRange, Substitution},
};

use super::{
    ParserState,
    helpers::BlockParsingMetadata,
    inlines::inline_parser,
    state::{InlineContext, InlineRules, ParserScope},
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
    let (file, position) =
        if let Some(range) = SourceRange::find_containing(&state.source_ranges, absolute_offset) {
            let doc_position = state
                .line_map
                .offset_to_position(absolute_offset, state.input);
            (
                range.file.clone(),
                Position::new(
                    state
                        .line_map
                        .source_line(range, state.input, absolute_offset),
                    doc_position.column,
                ),
            )
        } else {
            let doc_position = state
                .line_map
                .offset_to_position(absolute_offset, state.input);
            (state.current_file.as_deref().cloned(), doc_position)
        };

    Error::PegParse(
        Box::new(SourceLocation {
            file,
            location: crate::Location::point(position),
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

#[tracing::instrument(skip_all, fields(content_start, end, offset))]
pub(crate) fn preprocess_inline_content<'a>(
    state: &mut ParserState<'a>,
    content_start: usize,
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
        utf8_utils::safe_decrement_offset(state.input, adjusted_end)
    };
    let location = state.create_location(content_start + offset, content_end_offset);

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
                source_map: SourceMap::default(),
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
    inline_state.set_initial_position(&location, content_start + offset);
    tracing::debug!(
        ?inline_state,
        ?location,
        ?offset,
        content_start,
        ?end,
        "before inline preprocessing run"
    );

    let processed = inline_preprocessing::run(content, &state.document_attributes, &inline_state)?;
    // Drain warnings collected during inline preprocessing and add them to the main
    // parser state for post-parse emission. Dedup is handled by both layers:
    // InlinePreprocessorParserState deduplicates within a single preprocessing run,
    // and ParserState deduplicates across the entire parse. The inline preprocessor
    // attaches preprocessed-buffer locations (`file: None`); resolve each to its
    // originating file + source line as it enters the main state.
    for warning in inline_state.drain_warnings() {
        state.add_inline_preprocessor_warning(warning);
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

fn attribute_value_ranges(
    processed: &ProcessedContent<'_>,
    parent: &ParserState<'_>,
    location: &Location,
) -> Vec<Range<usize>> {
    let mut ranges = processed
        .source_map
        .attribute_value_ranges(location.absolute_start);
    ranges.extend(parent.attribute_value_ranges.iter().filter_map(|range| {
        let start = range.start.max(location.absolute_start);
        let end = range.end.min(location.absolute_end.saturating_add(1));
        (start < end).then(|| {
            start.saturating_sub(location.absolute_start)
                ..end.saturating_sub(location.absolute_start)
        })
    }));
    ranges.sort_unstable_by_key(|range| range.start);
    ranges
}

fn inline_context(
    state: &ParserState<'_>,
    block_metadata: &BlockParsingMetadata<'_>,
    autolinks: bool,
) -> InlineContext {
    let mut rules = state.inline_ctx.rules;
    rules.set(InlineRules::AUTOLINKS, autolinks);
    rules.set(
        InlineRules::HARD_BREAKS,
        block_metadata.hardbreaks || rules.contains(InlineRules::HARD_BREAKS),
    );
    rules.set(
        InlineRules::EOI_HARD_BREAK,
        state.scope == ParserScope::Document,
    );
    InlineContext {
        offset: 0,
        substitutions: block_metadata.substitutions,
        rules,
    }
}

#[tracing::instrument(skip_all, fields(processed=?processed, block_metadata=?block_metadata))]
fn parse_processed_inlines<'a>(
    processed: &'a ProcessedContent<'a>,
    text: &'a str,
    state: &mut ParserState<'a>,
    block_metadata: &BlockParsingMetadata,
    location: &Location,
    autolinks: bool,
) -> Result<Vec<InlineNode<'a>>, Error> {
    let inline_ctx = inline_context(state, block_metadata, autolinks);
    let mut inline_peg_state = ParserState::for_inline_parsing(text, state, inline_ctx);
    inline_peg_state.empty_attribute_offsets = processed
        .source_map
        .empty_attribute_offsets(location.absolute_start);
    inline_peg_state.attribute_value_ranges = attribute_value_ranges(processed, state, location);
    let inlines = if !autolinks {
        inline_parser::inlines_no_autolinks(text, &mut inline_peg_state)
    } else if inline_peg_state.quotes_only {
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

    Ok(inlines)
}

/// Process inline content and retain its preprocessed source without internal placeholders.
///
/// The inline nodes contain the display representation. The returned source restores protected
/// passthrough content for callers such as natural cross-reference lookup.
#[tracing::instrument(skip_all, fields(content_start, end, offset))]
pub(crate) fn process_inlines<'a>(
    state: &mut ParserState<'a>,
    block_metadata: &BlockParsingMetadata,
    content_start: usize,
    end: usize,
    offset: usize,
    content: &'a str,
) -> Result<(Vec<InlineNode<'a>>, &'a str), Error> {
    let (location, processed) = preprocess_inline_content(
        state,
        content_start,
        end,
        offset,
        content,
        block_metadata.substitutions.enabled(&Substitution::Macros),
        block_metadata
            .substitutions
            .enabled(&Substitution::Attributes),
    )?;
    // Promote `processed` to `'a` so both the source text and parsed inline nodes can
    // borrow from the parser arena.
    let processed: &'a ProcessedContent<'a> = state.arena.alloc_with(|| processed);
    let source = processed_text_as_outer(processed, state);
    // After preprocessing, attribute substitution may result in empty content
    // (e.g., {empty} -> ""). In this case, return empty vec without parsing.
    if processed.text.trim().is_empty() {
        return Ok((Vec::new(), source));
    }
    let content =
        parse_processed_inlines(processed, source, state, block_metadata, &location, true)?;
    let inlines =
        super::location_mapping::map_inline_locations(state, processed, &content, &location)?;
    let source = if processed.passthroughs.is_empty() {
        source
    } else {
        let restored =
            super::passthrough_processing::replace_passthrough_placeholders(source, processed);
        state.intern_str(&restored)
    };
    Ok((inlines, source))
}

/// Process inlines with autolinks suppressed.
///
/// Used inside URL macros, mailto macros, and cross-references where nested
/// autolinks would cause incorrect parsing.
#[tracing::instrument(skip_all, fields(content_start, end, offset))]
pub(crate) fn process_inlines_no_autolinks<'a>(
    state: &mut ParserState<'a>,
    block_metadata: &BlockParsingMetadata,
    content_start: usize,
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
        block_metadata.substitutions.enabled(&Substitution::Macros),
        block_metadata
            .substitutions
            .enabled(&Substitution::Attributes),
    )?;
    if processed.text.is_empty() {
        return Ok(Vec::new());
    }
    if processed.text.trim().is_empty() {
        // Whitespace-only text inside a link-style macro (`link:`, URL,
        // `mailto:`, `xref:`) must render literally — asciidoctor preserves
        // `link:https://example.com[ ]` as `<a href="..."> </a>` instead of
        // falling back to the target. Emit one `PlainText` carrying the
        // substituted whitespace and skip the inline parser entirely.
        let text = processed_text_as_outer(&processed, state);
        return Ok(vec![InlineNode::PlainText(Plain {
            content: text,
            location,
            escaped: false,
        })]);
    }
    // Promote `processed` to `'a` by interning into the parser arena.
    let processed: &'a ProcessedContent<'a> = state.arena.alloc_with(|| processed);
    let source = processed_text_as_outer(processed, state);
    let content =
        parse_processed_inlines(processed, source, state, block_metadata, &location, false)?;
    super::location_mapping::map_inline_locations(state, processed, &content, &location)
}
