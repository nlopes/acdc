//! Advance ordinary list principals and attachments at committed boundaries.

use crate::{
    Block, Error, ListItem,
    grammar::{ParserState, helpers::BlockParsingMetadata},
};

use super::super::{
    RecognitionError,
    coordinator::{BlockCursor, Execution},
    document_parser,
    input::{BlockInput, ReadRequest},
    metadata::order_block_prefix_events,
};
use super::{
    ListEntry, ListItemHead, ListKind, ListPhase, ListStepKind, OrdinaryListKind,
    OrdinaryListLayout, build_ordinary_list,
};

impl BlockCursor {
    pub(in crate::grammar::document) fn construct_list<'a>(
        &mut self,
        input: &mut impl BlockInput<'a>,
        state: &mut ParserState<'a>,
        entry: &ListEntry<'a>,
        parent_metadata: &BlockParsingMetadata<'a>,
        accepted: &mut Execution<
            '_,
            impl FnMut(&Block<'a>, &mut ParserState<'a>) -> Result<(), Error>,
        >,
    ) -> Result<Result<Block<'a>, Error>, RecognitionError> {
        let metadata = entry.metadata.as_ref().unwrap_or(parent_metadata);
        let start = entry.start + self.base_offset;
        self.offset = entry.body_offset;
        if matches!(entry.kind, ListKind::Callout) {
            return self.construct_callout_list(input, state, start, metadata, accepted);
        }
        if matches!(entry.kind, ListKind::Description) {
            return self.construct_description_list(input, state, start, metadata, accepted);
        }
        let source = match input.prepare(self, state, ReadRequest::ListHead(entry.kind)) {
            Ok(source) => source,
            Err(error) => return Ok(Err(error)),
        };
        let (head, parent_scope, end) = document_parser::first_ordinary_list_item(
            source,
            state,
            self.offset,
            self.base_offset,
            entry.kind,
            metadata,
        )?;
        self.offset = end;
        let result = self.construct_ordinary_list(
            input,
            state,
            (entry.kind, start),
            metadata,
            head,
            accepted,
        );
        state.ordinary_list_scope = parent_scope;
        result
    }

    fn construct_ordinary_list<'a>(
        &mut self,
        input: &mut impl BlockInput<'a>,
        state: &mut ParserState<'a>,
        frame: (ListKind<'a>, usize),
        metadata: &BlockParsingMetadata<'a>,
        head: ListItemHead<'a>,
        accepted: &mut Execution<
            '_,
            impl FnMut(&Block<'a>, &mut ParserState<'a>) -> Result<(), Error>,
        >,
    ) -> Result<Result<Block<'a>, Error>, RecognitionError> {
        let (list_kind, start) = frame;
        let Some((ordered, kind)) = ordinary_parts(list_kind) else {
            return Ok(Err(list_error(
                state,
                start,
                "ordinary list construction received a different list kind",
            )));
        };
        let base_marker = head.marker;
        let (mut item, end) = match head.finish(state, Vec::new(), self.offset, self.base_offset) {
            Ok(item) => item,
            Err(error) => return Ok(Err(error)),
        };
        let mut block = build_ordinary_list(
            state,
            metadata,
            OrdinaryListLayout {
                start,
                end: end + self.base_offset,
                offset: 0,
                ordered,
                has_own_metadata: kind.has_own_metadata,
            },
            vec![item],
        );
        if let Err(error) = accepted.run(&block, state) {
            return Ok(Err(error));
        }
        item = match take_first_item(&mut block, state, start) {
            Ok(item) => item,
            Err(error) => return Ok(Err(error)),
        };
        let mut items = Vec::new();
        loop {
            item.blocks = match self.collect_list_children(
                input,
                state,
                metadata,
                (list_kind, base_marker, item.marker),
                accepted,
            )? {
                Ok(blocks) => blocks,
                Err(error) => return Ok(Err(error)),
            };
            let source = match input.prepare(self, state, ReadRequest::ListEnd) {
                Ok(source) => source,
                Err(error) => return Ok(Err(error)),
            };
            self.offset = document_parser::list_item_end(
                source,
                state,
                self.offset,
                self.base_offset,
                kind.allow_continuation,
            )?;
            if !item.blocks.is_empty() {
                item.location = state.create_location(
                    item.location.absolute_start,
                    self.base_offset + self.offset.saturating_sub(1),
                );
                item.blocks = order_block_prefix_events(item.blocks);
            }
            let source = match input.prepare(
                self,
                state,
                ReadRequest::ListNext(list_kind, base_marker, ListPhase::Sibling),
            ) {
                Ok(source) => source,
                Err(error) => return Ok(Err(error)),
            };
            let last_end = item.location.absolute_end;
            items.push(item);
            let step = document_parser::list_next(
                source,
                state,
                self.offset,
                self.base_offset,
                list_kind,
                base_marker,
                base_marker,
                metadata,
                ListPhase::Sibling,
            )?;
            self.offset = step.end;
            if let ListStepKind::Continue(Some(head)) = step.kind {
                item = match head.finish(state, Vec::new(), self.offset, self.base_offset) {
                    Ok((item, _)) => item,
                    Err(error) => return Ok(Err(error)),
                };
            } else {
                *block.location_mut() = state.create_location(start, last_end);
                if let Some(slot) = crate::extensions::ordinary_list_items_mut(&mut block) {
                    *slot = items;
                }
                return Ok(Ok(block));
            }
        }
    }
    pub(super) fn collect_list_children<'a>(
        &mut self,
        input: &mut impl BlockInput<'a>,
        state: &mut ParserState<'a>,
        metadata: &BlockParsingMetadata<'a>,
        context: (ListKind<'a>, &'a str, &'a str),
        accepted: &mut Execution<
            '_,
            impl FnMut(&Block<'a>, &mut ParserState<'a>) -> Result<(), Error>,
        >,
    ) -> Result<Result<Vec<Block<'a>>, Error>, RecognitionError> {
        let (kind, base_marker, marker) = context;
        let mut blocks = Vec::new();
        let mut phase = ListPhase::AfterPrincipal;
        loop {
            let source =
                match input.prepare(self, state, ReadRequest::ListNext(kind, marker, phase)) {
                    Ok(source) => source,
                    Err(error) => return Ok(Err(error)),
                };
            let step = document_parser::list_next(
                source,
                state,
                self.offset,
                self.base_offset,
                kind,
                base_marker,
                marker,
                metadata,
                phase,
            )?;
            self.offset = step.end;
            let child = match step.kind {
                ListStepKind::Break(Ok(super::super::coordinator::BlockStepKind::List(entry))) => {
                    self.construct_list(input, state, &entry, metadata, accepted)?
                }
                ListStepKind::Break(Ok(kind)) => self.construct(input, state, kind, accepted)?,
                ListStepKind::Break(Err(error)) => Err(error),
                ListStepKind::Continue(None) if phase == ListPhase::AfterPrincipal => {
                    phase = ListPhase::Attachments;
                    continue;
                }
                ListStepKind::Continue(None | Some(_)) => break,
            };
            match child {
                Ok(block) => blocks.push(block),
                Err(_)
                    if !accepted.failed()
                        && matches!(kind, ListKind::Unordered(_) | ListKind::Ordered(_)) => {}
                Err(error) => return Ok(Err(error)),
            }
            phase = ListPhase::Attachments;
        }
        Ok(Ok(blocks))
    }
}

fn list_error(state: &ParserState<'_>, offset: usize, message: &str) -> Error {
    Error::Parse(
        Box::new(state.create_error_source_location(state.create_location(offset, offset))),
        message.into(),
    )
}

fn ordinary_parts(kind: ListKind<'_>) -> Option<(bool, OrdinaryListKind<'_>)> {
    match kind {
        ListKind::Unordered(kind) => Some((false, kind)),
        ListKind::Ordered(kind) => Some((true, kind)),
        ListKind::Callout | ListKind::Description => None,
    }
}

fn take_first_item<'a>(
    block: &mut Block<'a>,
    state: &ParserState<'a>,
    start: usize,
) -> Result<ListItem<'a>, Error> {
    crate::extensions::ordinary_list_items_mut(block)
        .and_then(Vec::pop)
        .ok_or_else(|| list_error(state, start, "ordinary list builder returned no first item"))
}
