//! Requests retained after a block recognizer has returned.

use std::ops::ControlFlow;

use crate::{
    Block, Error,
    grammar::{ParserState, helpers::BlockParsingMetadata},
};

use super::{
    coordinator::BlockStepKind,
    delimited::{
        ContentParser, ContentRequest, DelimitedParams, build_delimited_block_with_parser,
    },
    lists::{ListEntry, ListKind},
    metadata::assign_block_caption,
    quotes::{QuoteParams, build_quote_with_parser},
    table::{TableParseParams, construction::TableConstruction},
};

/// Shared block dispatch retains the metadata until its content form commits.
pub(super) type RecognizedContent<'a> = ControlFlow<DeferredContent<'a>, Block<'a>>;

pub(super) enum DeferredContent<'a> {
    Delimited(DelimitedParams<'a>),
    Table(TableParseParams<'a>),
    Quote(QuoteParams<'a>),
    List {
        start: usize,
        body_offset: usize,
        kind: ListKind<'a>,
    },
}

pub(super) fn commit_content<'a>(
    state: &mut ParserState<'a>,
    metadata: BlockParsingMetadata<'a>,
    content: RecognizedContent<'a>,
) -> BlockStepKind<'a> {
    match content {
        ControlFlow::Continue(mut block) => {
            assign_block_caption(state, &mut block);
            BlockStepKind::Block(block)
        }
        ControlFlow::Break(DeferredContent::Delimited(params)) => {
            BlockStepKind::Delimited(DelimitedConstruction { metadata, params })
        }
        ControlFlow::Break(DeferredContent::Table(params)) => {
            BlockStepKind::Table(TableConstruction { metadata, params })
        }
        ControlFlow::Break(DeferredContent::Quote(params)) => {
            BlockStepKind::Quote(QuoteConstruction { metadata, params })
        }
        ControlFlow::Break(DeferredContent::List {
            start,
            body_offset,
            kind,
        }) => BlockStepKind::List(ListEntry {
            start,
            body_offset,
            metadata: Some(metadata),
            kind,
        }),
    }
}

/// Ordinary parsing retains complete PEG construction. The ordered mode
/// returns accepted parents before constructing their child blocks.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ConstructionMode {
    Recursive,
    Ordered,
}

pub(super) struct DelimitedConstruction<'a> {
    pub(super) metadata: BlockParsingMetadata<'a>,
    pub(super) params: DelimitedParams<'a>,
}

pub(super) struct QuoteConstruction<'a> {
    metadata: BlockParsingMetadata<'a>,
    params: QuoteParams<'a>,
}

impl<'a> QuoteConstruction<'a> {
    pub(super) fn start(
        self,
        state: &mut ParserState<'a>,
    ) -> Result<(Block<'a>, Option<ContentRequest<'a>>), Error> {
        let mut content = CaptureContent::default();
        let block = build_quote_with_parser(state, &self.metadata, &self.params, &mut content)?;
        Ok((block, content.request))
    }
}

#[derive(Default)]
struct CaptureContent<'a> {
    request: Option<ContentRequest<'a>>,
}

impl<'a> ContentParser<'a> for CaptureContent<'a> {
    fn parse(
        &mut self,
        state: &mut ParserState<'a>,
        request: ContentRequest<'a>,
    ) -> Result<Vec<Block<'a>>, Error> {
        if self.request.is_some() {
            return Err(Error::Parse(
                Box::new(state.create_error_source_location(
                    state.create_location(request.offset, request.offset),
                )),
                "compound builder requested more than one child body".into(),
            ));
        }
        self.request = Some(request);
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests;

impl<'a> DelimitedConstruction<'a> {
    /// Build the parent with its existing builder, retaining the child request
    /// until the accepted parent handler releases its borrow.
    pub(super) fn start(
        self,
        state: &mut ParserState<'a>,
    ) -> Result<(Block<'a>, Option<ContentRequest<'a>>), Error> {
        let mut content = CaptureContent::default();
        let block =
            build_delimited_block_with_parser(state, &self.metadata, &self.params, &mut content)?;
        Ok((block, content.request))
    }
}
