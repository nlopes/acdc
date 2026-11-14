//! Commit compound children before advancing the enclosing source reader.

use crate::{
    Block, Error,
    grammar::{ParserState, state::BlockContext},
};

use super::{
    RecognitionError,
    construction::{ConstructionMode, DelimitedConstruction},
    coordinator::{BlockCursor, Execution},
    delimited::resolve_delimited_close,
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
        let (mut block, _) = match construction.start(state) {
            Ok(started) => started,
            Err(error) => return Ok(Err(error)),
        };
        match accepted.run(&mut block, state, output) {
            Ok(true) => {}
            Ok(false) => return Ok(Ok(())),
            Err(error) => return Ok(Err(error)),
        }
        let mut child = BlockCursor::new(self.offset, ConstructionMode::Ordered);
        child.base_offset = self.base_offset;
        let previous_context = std::mem::replace(&mut state.block_context, BlockContext::Compound);
        let previous_scope = std::mem::replace(
            &mut state.source_scope_offset,
            params.content_start + params.offset,
        );
        input.push_boundary(params.open_delim);
        let mut body = Vec::new();
        let result = child.collect(input, state, &mut body, accepted);
        input.pop_boundary();
        state.block_context = previous_context;
        state.source_scope_offset = previous_scope;
        let mut end = match result? {
            Ok(end) => end,
            Err(error) => return Ok(Err(error)),
        };
        while let Some(line) =
            source_tail(state, end).and_then(|tail| tail.split_inclusive('\n').next())
        {
            if !line.trim().is_empty() {
                break;
            }
            end += line.len();
        }
        let close = source_tail(state, end)
            .and_then(|tail| tail.split_inclusive('\n').next())
            .filter(|line| {
                document_parser::source_block_close(line, state, params.open_delim, false).is_ok()
            });
        params.close = close.map(|_| (end - params.offset, params.open_delim));
        params.end =
            end - params.offset + close.map_or(0, |line| line.trim_end_matches('\n').len());
        let close_location = match resolve_delimited_close(state, &params) {
            Ok(location) => location,
            Err(error) => return Ok(Err(error)),
        };
        let Some(source) = retain_body(state, params.content_start + params.offset, end) else {
            return Ok(Err(Error::Parse(
                Box::new(state.create_error_source_location(block.location().clone())),
                "missing committed compound source".into(),
            )));
        };
        *block.location_mut() =
            state.create_block_location(params.start, params.end, params.offset);
        if let Block::DelimitedBlock(delimited) = &mut block {
            delimited.close_delimiter_location = close_location;
            delimited.source_text = Some(source);
        }
        let Some(slot) = crate::extensions::compound_body_mut(&mut block) else {
            return Ok(Err(Error::Parse(
                Box::new(state.create_error_source_location(block.location().clone())),
                "compound builder returned no child content slot".into(),
            )));
        };
        *slot = body;
        child.offset =
            end - child.base_offset + close.map_or(0, |line| line.trim_end_matches('\n').len());
        self.offset = child.offset;
        self.base_offset = child.base_offset;
        assign_block_caption(state, &mut block);
        output.push(block);
        Ok(Ok(()))
    }
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
