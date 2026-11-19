//! Requests retained after a block recognizer has returned.

use crate::{
    Block, Error,
    extensions::{ExtensionBlock, Invocation},
    grammar::{ParserState, helpers::BlockParsingMetadata},
};

use super::{
    coordinator::BlockStepKind,
    delimited::{
        ContentParser, ContentRequest, DelimitedParams, build_delimited_block_with_parser,
    },
    metadata::assign_block_caption,
    quotes::{QuoteParams, build_quote_with_parser},
};

/// Attach metadata after the content alternative commits. Keeping the final
/// step representation avoids copying ready blocks through another large enum.
#[inline]
pub(super) fn commit_content<'a>(
    state: &ParserState<'a>,
    metadata: BlockParsingMetadata<'a>,
    mut content: BlockStepKind<'a>,
) -> BlockStepKind<'a> {
    match &mut content {
        BlockStepKind::Block(block) => assign_block_caption(state, block),
        BlockStepKind::Delimited(request) => request.metadata = metadata,
        BlockStepKind::Table(request) => request.metadata = metadata,
        BlockStepKind::Quote(request) => request.metadata = metadata,
        BlockStepKind::List(entry) => entry.metadata = Some(Box::new(metadata)),
        BlockStepKind::End(_) => {}
    }
    content
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
    pub(super) metadata: BlockParsingMetadata<'a>,
    pub(super) params: QuoteParams<'a>,
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
        let p = &self.params;
        if let Some(style) = self.metadata.metadata.style
            && let Some(registry) = &state.options.extensions
            && let Some(context) = p.kind.raw_context()
            && let Some((processor, descriptor)) = registry.raw_style(style, context)
        {
            let mut metadata = self.metadata.metadata;
            descriptor.bind_arguments(&mut metadata);
            super::delimited::resolve_delimited_close(state, p)?;
            let location = state.create_block_location(p.start, p.end, p.offset);
            let body_start = p.content_start + p.offset;
            let body_location = if p.source_text.is_empty() {
                state.create_location(body_start, body_start)
            } else {
                state.create_block_location(
                    p.content_start,
                    p.content_start + p.source_text.len(),
                    p.offset,
                )
            };
            metadata.move_positional_attributes_to_attributes();
            return Ok((
                Block::Extension(Box::new(ExtensionBlock {
                    processor,
                    invocation: Invocation::Style {
                        context,
                        body: p.source_text,
                        body_location: state.create_error_source_location(body_location),
                        location: state.create_error_source_location(location.clone()),
                    },
                    title: self.metadata.title,
                    metadata,
                    location,
                    placeholder: None,
                })),
                None,
            ));
        }
        let mut content = CaptureContent::default();
        let block =
            build_delimited_block_with_parser(state, &self.metadata, &self.params, &mut content)?;
        Ok((block, content.request))
    }
}
