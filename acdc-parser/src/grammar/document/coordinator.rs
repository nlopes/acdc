//! Ordered construction at accepted block boundaries.

use crate::{
    Block, Document, DocumentAttributes, Error,
    grammar::{ParserState, helpers::PositionWithOffset},
    model::{SectionKind, SectionLevel},
};

use super::{
    RecognitionError,
    construction::{ConstructionMode, DelimitedConstruction, QuoteConstruction},
    delimited::{ContentParser, ContentRequest, parse_content},
    doctype::is_book_doctype,
    document_parser, finish,
    header::ParsedHeader,
    input::{BlockInput, ReadRequest, TextInput},
    lists::ListEntry,
    metadata::{assign_block_caption, order_block_prefix_events},
    sections::expected_child_level,
    table::{InitialAttributes, construction::TableConstruction},
};

/// A handler failure always escapes native content recovery. Effects that
/// completed before failure are never retried or rolled back.
pub(super) struct Execution<'handler, F> {
    callback: &'handler mut F,
    failed: bool,
}

impl<'handler, F> Execution<'handler, F> {
    pub(super) fn new(callback: &'handler mut F) -> Self {
        Self {
            callback,
            failed: false,
        }
    }
    #[inline]
    pub(super) fn run<'a>(
        &mut self,
        block: &Block<'a>,
        state: &mut ParserState<'a>,
    ) -> Result<(), Error>
    where
        F: FnMut(&Block<'a>, &mut ParserState<'a>) -> Result<(), Error>,
    {
        let result = (self.callback)(block, state);
        self.failed |= result.is_err();
        result
    }

    pub(super) fn failed(&self) -> bool {
        self.failed
    }
}

pub(super) struct DocumentPrefix<'a> {
    pub(super) start: PositionWithOffset,
    pub(super) header: ParsedHeader<'a>,
    pub(super) attributes: DocumentAttributes<'a>,
    pub(super) comments: Vec<Block<'a>>,
    pub(super) body_offset: usize,
}

pub(super) struct NestedPrefix<'a> {
    pub(super) blocks: Vec<Block<'a>>,
    pub(super) body_offset: usize,
}

pub(super) struct BlockStep<'a> {
    pub(super) kind: BlockStepKind<'a>,
    pub(super) end: usize,
}

pub(super) enum BlockStepKind<'a> {
    Block(Block<'a>),
    Delimited(DelimitedConstruction<'a>),
    Table(TableConstruction<'a>),
    Quote(QuoteConstruction<'a>),
    List(ListEntry<'a>),
    End(Vec<Block<'a>>),
}

impl<'a> BlockStep<'a> {
    pub(super) fn end(trailing: Vec<Block<'a>>, end: usize) -> Self {
        Self {
            kind: BlockStepKind::End(trailing),
            end,
        }
    }
}

impl<'a> BlockStepKind<'a> {
    #[inline]
    pub(super) fn into_ready(self, state: &ParserState<'a>) -> Result<Block<'a>, Error> {
        if let Self::Block(block) = self {
            Ok(block)
        } else {
            Err(Error::Parse(
                Box::new(state.create_error_source_location(state.create_location(0, 0))),
                "recursive block dispatch returned unfinished content".into(),
            ))
        }
    }
}

#[derive(Clone, Copy, Default)]
pub(super) struct BlockParent {
    pub(super) level: Option<SectionLevel>,
    pub(super) kind: Option<SectionKind>,
}

pub(super) struct BlockCursor {
    pub(super) offset: usize,
    pub(super) base_offset: usize,
    parent: BlockParent,
    mode: ConstructionMode,
}

impl BlockCursor {
    pub(super) fn new(offset: usize, mode: ConstructionMode) -> Self {
        Self {
            offset,
            base_offset: 0,
            parent: BlockParent::default(),
            mode,
        }
    }

    #[cfg(test)]
    pub(super) fn advance<'a>(
        &mut self,
        input: &'a str,
        state: &mut ParserState<'a>,
    ) -> Result<Result<BlockStep<'a>, Error>, RecognitionError> {
        let mut step = BlockStep::end(Vec::new(), self.offset);
        Ok(self.advance_into(input, state, &mut step)?.map(|()| step))
    }

    #[inline]
    fn advance_into<'a>(
        &mut self,
        input: &'a str,
        state: &mut ParserState<'a>,
        step: &mut BlockStep<'a>,
    ) -> Result<Result<(), Error>, RecognitionError> {
        let result = document_parser::next_block(
            input,
            state,
            self.offset,
            self.base_offset,
            self.parent,
            self.mode,
            step,
        )?;
        if result.is_ok() {
            self.offset = step.end;
        }
        Ok(result)
    }

    pub(super) fn section(&self, section: &crate::Section<'_>, state: &ParserState<'_>) -> Self {
        Self {
            offset: self.offset,
            base_offset: self.base_offset,
            parent: BlockParent {
                level: Some(expected_child_level(
                    section.level,
                    section.kind,
                    is_book_doctype(&state.document_attributes),
                )),
                kind: Some(section.kind),
            },
            mode: self.mode,
        }
    }

    /// Ordered construction enters each section after its heading recognition
    /// has returned. The recursive mode retains the ordinary parsing fast path.
    pub(super) fn collect<'a>(
        &mut self,
        input: &mut impl BlockInput<'a>,
        state: &mut ParserState<'a>,
        blocks: &mut Vec<Block<'a>>,
        accepted: &mut Execution<
            '_,
            impl FnMut(&Block<'a>, &mut ParserState<'a>) -> Result<(), Error>,
        >,
    ) -> Result<Result<usize, Error>, RecognitionError> {
        loop {
            let source = match input.prepare(self, state, ReadRequest::Block) {
                Ok(source) => source,
                Err(error) => return Ok(Err(error)),
            };
            let mut step = BlockStep::end(Vec::new(), self.offset);
            if let Err(error) = self.advance_into(source, state, &mut step)? {
                return Ok(Err(error));
            }
            let kind = match step.kind {
                BlockStepKind::End(trailing) => {
                    blocks.extend(trailing);
                    *blocks = order_block_prefix_events(std::mem::take(blocks));
                    return Ok(Ok(self.base_offset + step.end));
                }
                BlockStepKind::Block(block) if !matches!(block, Block::Section(_)) => {
                    if let Err(error) = accepted.run(&block, state) {
                        return Ok(Err(error));
                    }
                    blocks.push(block);
                    continue;
                }
                kind @ (BlockStepKind::Block(_)
                | BlockStepKind::Delimited(_)
                | BlockStepKind::Table(_)
                | BlockStepKind::List(_)
                | BlockStepKind::Quote(_)) => kind,
            };
            if let Err(error) = self.construct_into(input, state, kind, blocks, accepted)? {
                return Ok(Err(error));
            }
        }
    }

    pub(super) fn construct_into<'a>(
        &mut self,
        input: &mut impl BlockInput<'a>,
        state: &mut ParserState<'a>,
        kind: BlockStepKind<'a>,
        output: &mut Vec<Block<'a>>,
        accepted: &mut Execution<
            '_,
            impl FnMut(&Block<'a>, &mut ParserState<'a>) -> Result<(), Error>,
        >,
    ) -> Result<Result<(), Error>, RecognitionError> {
        let needs_caption = matches!(
            &kind,
            BlockStepKind::Delimited(_) | BlockStepKind::Table(_) | BlockStepKind::Quote(_)
        );
        let mut table_content = None;
        let (mut block, request) = match kind {
            BlockStepKind::List(entry) => {
                return self.construct_list_into(
                    input,
                    state,
                    &entry,
                    &crate::grammar::helpers::BlockParsingMetadata::new(
                        state.options.compatibility_mode,
                    ),
                    output,
                    accepted,
                );
            }
            BlockStepKind::Block(block) => (block, None),
            BlockStepKind::Delimited(construction) => match construction.start(state) {
                Ok(started) => started,
                Err(error) => return Ok(Err(error)),
            },
            BlockStepKind::Quote(construction) => match construction.start(state) {
                Ok(started) => started,
                Err(error) => return Ok(Err(error)),
            },
            BlockStepKind::Table(construction) => match construction.start(state) {
                Ok((block, content)) => {
                    table_content = Some(content);
                    (block, None)
                }
                Err(error) => return Ok(Err(error)),
            },
            BlockStepKind::End(_) => {
                return Ok(Err(Error::Parse(
                    Box::new(state.create_error_source_location(state.create_location(
                        self.base_offset + self.offset,
                        self.base_offset + self.offset,
                    ))),
                    "block construction received an end boundary".into(),
                )));
            }
        };
        let child = if let Block::Section(section) = &block
            && self.mode == ConstructionMode::Ordered
        {
            Some(self.section(section, state))
        } else {
            None
        };
        if let Err(error) = accepted.run(&block, state) {
            return Ok(Err(error));
        }
        if let (Block::Section(section), Some(mut child)) = (&mut block, child) {
            let end = match child.collect(input, state, &mut section.content, accepted)? {
                Ok(end) => end,
                Err(error) => return Ok(Err(error)),
            };
            self.offset = child.offset;
            self.base_offset = child.base_offset;
            section.location = state.create_block_location(section.location.absolute_start, end, 0);
        }
        if let Some(request) = request {
            let mut parser = CursorContent { accepted };
            let content = match parse_content(state, request, &mut parser) {
                Ok(content) => content,
                Err(error) => return Ok(Err(error)),
            };
            let Some(body) = crate::extensions::compound_body_mut(&mut block) else {
                return Ok(Err(Error::Parse(
                    Box::new(state.create_error_source_location(block.location().clone())),
                    "compound builder returned no child content slot".into(),
                )));
            };
            *body = content;
        }
        if needs_caption {
            if let Some(content) = table_content
                && let Err(error) =
                    content.finish(&mut block, state, &mut CursorContent { accepted })
            {
                return Ok(Err(error));
            }
            assign_block_caption(state, &mut block);
        }
        output.push(block);
        Ok(Ok(()))
    }
}

struct CursorContent<'h, 'callback, F> {
    accepted: &'h mut Execution<'callback, F>,
}

impl<'a, F> ContentParser<'a> for CursorContent<'_, '_, F>
where
    F: FnMut(&Block<'a>, &mut ParserState<'a>) -> Result<(), Error>,
{
    fn parse(
        &mut self,
        state: &mut ParserState<'a>,
        request: ContentRequest<'a>,
    ) -> Result<Vec<Block<'a>>, Error> {
        let mut cursor = BlockCursor {
            offset: 0,
            base_offset: request.offset,
            parent: BlockParent {
                level: request.parent_section_level,
                kind: None,
            },
            mode: ConstructionMode::Ordered,
        };
        let mut blocks = Vec::new();
        let toc_start = state.toc_entries.len();
        let result = cursor
            .collect(
                &mut TextInput(request.source),
                state,
                &mut blocks,
                self.accepted,
            )
            .and_then(|result| match result {
                Ok(end) => {
                    document_parser::block_sequence_end(request.source, state, end - request.offset)
                        .map(|()| Ok(blocks))
                }
                Err(error) => Ok(Err(error)),
            });
        match result {
            Ok(result) => result,
            Err(error) => {
                // Recovery discards this child tree. Retain no provisional TOC
                // entries for headings that cannot survive in the document.
                state.toc_entries.truncate(toc_start);
                crate::grammar::inline_processing::adjust_and_log_parse_error(
                    &error,
                    request.source,
                    request.offset,
                    state,
                    request.error_context,
                );
                Ok(Vec::new())
            }
        }
    }

    fn nested_document(
        &mut self,
        state: &mut ParserState<'a>,
        source: &'a str,
        offset: usize,
        initial_attributes: &mut InitialAttributes<'a>,
    ) -> Result<Result<Vec<Block<'a>>, Error>, RecognitionError> {
        let prefix = match document_parser::nested_document_prefix(
            source,
            state,
            offset,
            initial_attributes,
        )? {
            Ok(prefix) => prefix,
            Err(error) => return Ok(Err(error)),
        };
        let mut cursor = BlockCursor {
            offset: prefix.body_offset,
            base_offset: offset,
            parent: BlockParent::default(),
            mode: ConstructionMode::Ordered,
        };
        let mut blocks = prefix.blocks;
        let end = match cursor.collect(&mut TextInput(source), state, &mut blocks, self.accepted)? {
            Ok(end) => end,
            Err(error) => return Ok(Err(error)),
        };
        document_parser::block_sequence_end(source, state, end - offset)?;
        Ok(Ok(blocks))
    }
}

pub(crate) fn document<'a>(
    input: &'a str,
    state: &mut ParserState<'a>,
) -> Result<Result<Document<'a>, Error>, RecognitionError> {
    // Registry callbacks still run after parsing. A registry alone must not
    // change construction or diagnostic order before source handlers are wired.
    let prefix = match document_parser::document_prefix(input, state)? {
        Ok(prefix) => prefix,
        Err(error) => return Ok(Err(error)),
    };
    let mut blocks = prefix.comments;
    let end = match document_parser::remaining_blocks(input, state, prefix.body_offset)? {
        Ok((body, end)) => {
            if blocks.is_empty() {
                blocks = body;
            } else {
                blocks.extend(body);
            }
            end
        }
        Err(error) => return Ok(Err(error)),
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

/// Internal accepted-block execution boundary for documents and containers.
/// Complete input uses the same ordered builders as the resumable source path.
pub(super) fn document_with<'a>(
    input: &'a str,
    state: &mut ParserState<'a>,
    mode: ConstructionMode,
    accepted: &mut impl FnMut(&Block<'a>, &mut ParserState<'a>) -> Result<(), Error>,
) -> Result<Result<Document<'a>, Error>, RecognitionError> {
    let prefix = match document_parser::document_prefix(input, state)? {
        Ok(prefix) => prefix,
        Err(error) => return Ok(Err(error)),
    };
    let mut cursor = BlockCursor::new(prefix.body_offset, mode);
    let mut blocks = prefix.comments;
    let mut execution = Execution::new(accepted);
    let end = match cursor.collect(&mut TextInput(input), state, &mut blocks, &mut execution)? {
        Ok(end) => end,
        Err(error) => return Ok(Err(error)),
    };
    Ok(finish::finish(
        state,
        prefix.start,
        end,
        prefix.header,
        prefix.attributes,
        blocks,
    ))
}
