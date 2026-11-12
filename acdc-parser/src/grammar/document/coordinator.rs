//! Ordered construction at accepted block boundaries.

use crate::{
    Block, Document, DocumentAttributes, Error,
    grammar::{ParserState, helpers::PositionWithOffset},
};

use super::{document_parser, finish, header::ParsedHeader, metadata::order_block_prefix_events};

type RecognitionError = peg::error::ParseError<peg::str::LineCol>;

pub(super) struct DocumentPrefix<'a> {
    pub(super) start: PositionWithOffset,
    pub(super) header: ParsedHeader<'a>,
    pub(super) attributes: DocumentAttributes<'a>,
    pub(super) comments: Vec<Block<'a>>,
    pub(super) body_offset: usize,
}

pub(super) struct BlockStep<'a> {
    pub(super) block: Option<Block<'a>>,
    pub(super) trailing: Vec<Block<'a>>,
    pub(super) end: usize,
    pub(super) complete: bool,
}

impl<'a> BlockStep<'a> {
    pub(super) fn block(block: Block<'a>, end: usize) -> Self {
        Self {
            block: Some(block),
            trailing: Vec::new(),
            end,
            complete: false,
        }
    }

    pub(super) fn end(trailing: Vec<Block<'a>>, end: usize) -> Self {
        Self {
            block: None,
            trailing,
            end,
            complete: true,
        }
    }
}

pub(super) struct BlockCursor {
    offset: usize,
}

impl BlockCursor {
    pub(super) fn new(offset: usize) -> Self {
        Self { offset }
    }

    pub(super) fn advance<'a>(
        &mut self,
        input: &'a str,
        state: &mut ParserState<'a>,
    ) -> Result<Result<BlockStep<'a>, Error>, RecognitionError> {
        let step = document_parser::next_block(input, state, self.offset)?;
        if let Ok(step) = &step {
            self.offset = step.end;
        }
        Ok(step)
    }
}

pub(crate) fn document<'a>(
    input: &'a str,
    state: &mut ParserState<'a>,
) -> Result<Result<Document<'a>, Error>, RecognitionError> {
    let prefix = match document_parser::document_prefix(input, state)? {
        Ok(prefix) => prefix,
        Err(error) => return Ok(Err(error)),
    };
    let mut cursor = BlockCursor::new(prefix.body_offset);
    let mut blocks = prefix.comments;
    let end = loop {
        let step = match cursor.advance(input, state)? {
            Ok(step) => step,
            Err(error) => return Ok(Err(error)),
        };
        if let Some(block) = step.block {
            blocks.push(block);
        }
        if step.complete {
            blocks.extend(step.trailing);
            break step.end;
        }
    };
    Ok(finish::finish(
        state,
        prefix.start,
        end,
        prefix.header,
        prefix.attributes,
        order_block_prefix_events(blocks),
    ))
}
