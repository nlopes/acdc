//! Commit compound children before advancing the enclosing source reader.

use crate::{
    Block, Error,
    grammar::{ParserState, state::BlockContext},
};

use super::{
    RecognitionError,
    construction::DelimitedConstruction,
    coordinator::{BlockCursor, Execution},
    delimited::{DelimitedParams, resolve_delimited_close},
    document_parser,
    metadata::assign_block_caption,
    source::SourceInput,
};

impl BlockCursor {
    pub(super) fn construct_streamed_compound<'a>(
        &mut self,
        input: &mut SourceInput<'a>,
        state: &mut ParserState<'a>,
        mut construction: DelimitedConstruction<'a>,
        output: &mut Vec<Block<'a>>,
        accepted: &mut Execution<
            '_,
            impl FnMut(&mut Block<'a>, &mut ParserState<'a>, &mut Vec<Block<'a>>) -> Result<bool, Error>,
        >,
    ) -> Result<Result<(), Error>, RecognitionError> {
        let mut params = construction.params;
        // The closing delimiter is resolved after the child cursor finishes.
        // Build the accepted shell with the existing builder without issuing an
        // unterminated-block warning at the temporary end of the input window.
        construction.params.close = Some((params.end, params.open_delim));
        let (mut block, request) = match construction.start(state) {
            Ok(started) => started,
            Err(error) => return Ok(Err(error)),
        };
        match accepted.run(&mut block, state, output) {
            Ok(true) => {}
            Ok(false) => return Ok(Ok(())),
            Err(error) => return Ok(Err(error)),
        }
        let mut child = BlockCursor::content(
            self.offset,
            self.base_offset,
            request.and_then(|r| r.parent_section_level),
        );
        let context = request.map_or(BlockContext::Compound, |r| r.context);
        let previous_context = std::mem::replace(&mut state.block_context, context);
        let previous_scope = std::mem::replace(
            &mut state.source_scope_offset,
            params.content_start + params.offset,
        );
        input.push_boundary(params.open_delim);
        let mut body = Vec::new();
        let callbacks = accepted.block_callbacks();
        let toc_start = state.toc_entries.len();
        let result = child.collect(input, state, &mut body, accepted);
        input.pop_boundary();
        state.block_context = previous_context;
        state.source_scope_offset = previous_scope;
        let mut end = match result {
            Ok(Ok(end)) => end,
            Ok(Err(error)) => return Ok(Err(error)),
            // Report an executed processor before propagating a child grammar failure.
            Err(error) => {
                return match accepted.discarded_callback(callbacks, "Compound block content") {
                    Some(error) => Ok(Err(error)),
                    None => Err(error),
                };
            }
        };
        let unparsed = state.input.get(child.offset..);
        while let Some(line) =
            source_tail(state, end).and_then(|tail| tail.split_inclusive('\n').next())
        {
            if !line.trim().is_empty() {
                break;
            }
            end += line.len();
        }
        let mut close = source_tail(state, end)
            .and_then(|tail| tail.split_inclusive('\n').next())
            .filter(|line| {
                document_parser::source_block_close(line, state, params.open_delim, false).is_ok()
            });
        // If the child sequence stopped at a heading, consume the rest of its
        // quote without running processors in the discarded text.
        let recovery = if close.is_none()
            && let (Some(request), Some(tail)) = (request, unparsed)
            && let Some(warning) =
                request.section_boundary_warning(tail, child.base_offset + child.offset, state)
        {
            if let Some(error) = accepted.discarded_callback(callbacks, request.error_context) {
                return Ok(Err(error));
            }
            body.clear();
            state.toc_entries.truncate(toc_start);
            (end, close) = match input.delimited_boundary(end, params.open_delim, false, state) {
                Ok(boundary) => boundary,
                Err(error) => return Ok(Err(error)),
            };
            Some(warning)
        } else {
            None
        };
        params.close = close.map(|_| (end - params.offset, params.open_delim));
        params.end =
            end - params.offset + close.map_or(0, |line| line.trim_end_matches('\n').len());
        if let Err(error) = finish_compound(state, &params, end, &mut block, body) {
            return Ok(Err(error));
        }
        if let Some(warning) = recovery {
            state.add_warning(warning);
        }
        self.base_offset = child.base_offset;
        self.offset = params.end + params.offset - self.base_offset;
        assign_block_caption(state, &mut block);
        output.push(block);
        Ok(Ok(()))
    }
}

fn finish_compound<'a>(
    state: &mut ParserState<'a>,
    params: &DelimitedParams<'a>,
    end: usize,
    block: &mut Block<'a>,
    body: Vec<Block<'a>>,
) -> Result<(), Error> {
    let close_location = resolve_delimited_close(state, params)?;
    let Some(source) = retain_body(state, params.content_start + params.offset, end) else {
        return Err(Error::Parse(
            Box::new(state.create_error_source_location(block.location().clone())),
            "missing committed compound source".into(),
        ));
    };
    *block.location_mut() = state.create_block_location(params.start, params.end, params.offset);
    if let Block::DelimitedBlock(delimited) = block {
        delimited.close_delimiter_location = close_location;
        delimited.source_text = Some(source);
    }
    let Some(slot) = crate::extensions::compound_body_mut(block) else {
        return Err(Error::Parse(
            Box::new(state.create_error_source_location(block.location().clone())),
            "compound builder returned no child content slot".into(),
        ));
    };
    *slot = body;
    Ok(())
}

pub(super) fn retain_body<'a>(
    state: &ParserState<'a>,
    start: usize,
    end: usize,
) -> Option<&'a str> {
    state.source_slice(start..end).or_else(|| {
        let origin = state.input_origin.as_ref()?;
        let view = origin.index.window(start..end, state.arena)?;
        let base = view.origin.as_ref().map_or(0, |origin| origin.base);
        view.input.get(start - base..end - base)
    })
}

fn source_tail<'a>(state: &ParserState<'a>, offset: usize) -> Option<&'a str> {
    state
        .source_tail(offset)
        .filter(|tail| !tail.is_empty())
        .or_else(|| {
            state
                .input_origin
                .as_ref()
                .and_then(|origin| origin.index.tail(offset))
        })
}
