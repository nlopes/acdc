//! Commit compound children before advancing the enclosing source reader.

use crate::{
    Block, Error,
    grammar::{ParserState, inline_processing::parse_error_warning},
};

use super::{
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
    ) -> Result<(), Error> {
        let mut params = construction.params;
        // The closing delimiter is resolved after the child cursor finishes.
        // Build the accepted shell with the existing builder without issuing an
        // unterminated-block warning at the temporary end of the input window.
        construction.params.close = Some((params.end, params.open_delim));
        let (mut block, request) = construction.start(state)?;
        let Some(request) = request else {
            return Err(Error::Parse(
                Box::new(state.create_error_source_location(block.location().clone())),
                "streamed compound builder requested no child content".into(),
            ));
        };
        if !accepted.run(&mut block, state, output)? {
            return Ok(());
        }
        let mut child =
            BlockCursor::content(self.offset, self.base_offset, request.parent_section_level);
        let previous_context = std::mem::replace(&mut state.block_context, request.context);
        let previous_scope = std::mem::replace(&mut state.source_scope_offset, request.offset);
        input.push_boundary(params.open_delim);
        let mut body = Vec::new();
        let callbacks = accepted.block_callbacks();
        let toc_start = state.toc_entries.len();
        let result = child.collect(input, state, &mut body, accepted);
        input.pop_boundary();
        state.block_context = previous_context;
        state.source_scope_offset = previous_scope;
        let (mut end, mut close, recovery) = match result {
            Ok(Err(error)) => return Err(error),
            Ok(Ok(end)) => {
                let (end, close) = compound_close(state, end, params.open_delim);
                let warning = if close.is_none() {
                    state.input.get(child.offset..).and_then(|tail| {
                        request.section_boundary_warning(
                            tail,
                            child.base_offset + child.offset,
                            state,
                        )
                    })
                } else {
                    None
                };
                (end, close, warning)
            }
            Err(error) => {
                let warning = parse_error_warning(
                    &error,
                    state.input_offset(0),
                    state,
                    request.error_context,
                );
                // A nested cursor may have changed windows before failing. Find
                // the owner's close in the committed source, starting at its body.
                (request.offset, None, Some(warning))
            }
        };
        if recovery.is_some() {
            if let Some(error) = accepted.discarded_callback(callbacks, request.error_context) {
                return Err(error);
            }
            body.clear();
            state.toc_entries.truncate(toc_start);
            (end, close) = input.delimited_boundary(end, params.open_delim, false, state)?;
        }
        params.close = close.map(|_| (end - params.offset, params.open_delim));
        params.end =
            end - params.offset + close.map_or(0, |line| line.trim_end_matches('\n').len());
        finish_compound(state, &params, end, &mut block, body)?;
        if let Some(warning) = recovery {
            state.add_warning(warning);
        }
        self.base_offset = params.offset;
        self.offset = params.end;
        assign_block_caption(state, &mut block);
        output.push(block);
        Ok(())
    }
}

fn compound_close<'a>(
    state: &mut ParserState<'a>,
    mut end: usize,
    delimiter: &str,
) -> (usize, Option<&'a str>) {
    while let Some(line) =
        source_tail(state, end).and_then(|tail| tail.split_inclusive('\n').next())
    {
        if !line.trim().is_empty() {
            let close = document_parser::source_block_close(line, state, delimiter, false).is_ok();
            return (end, close.then_some(line));
        }
        end += line.len();
    }
    (end, None)
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
    let crate::model::compound_block!(delimited) = block else {
        return Err(Error::Parse(
            Box::new(state.create_error_source_location(block.location().clone())),
            "compound builder returned no child content slot".into(),
        ));
    };
    delimited.close_delimiter_location = close_location;
    delimited.source_text = Some(source);
    delimited.body = body;
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
