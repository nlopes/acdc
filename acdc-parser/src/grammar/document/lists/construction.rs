//! Advance ordinary list principals and attachments at committed boundaries.

use crate::{
    Block, Error,
    grammar::{ParserState, helpers::BlockParsingMetadata},
};

use super::super::{
    RecognitionError,
    coordinator::{BlockCursor, Execution},
    document_parser,
    metadata::order_block_prefix_events,
};
use super::{
    ListEntry, ListItemHead, ListKind, ListPhase, ListStepKind, OrdinaryListLayout,
    build_ordinary_list,
};

impl BlockCursor {
    pub(in crate::grammar::document) fn construct_list<'a>(
        &mut self,
        input: &'a str,
        state: &mut ParserState<'a>,
        entry: &ListEntry<'a>,
        parent_metadata: &BlockParsingMetadata<'a>,
        accepted: &mut Execution<
            '_,
            impl FnMut(&Block<'a>, &mut ParserState<'a>) -> Result<(), Error>,
        >,
    ) -> Result<Result<Block<'a>, Error>, RecognitionError> {
        let metadata = entry.metadata.as_ref().unwrap_or(parent_metadata);
        self.offset = entry.body_offset;
        if matches!(entry.kind, ListKind::Callout) {
            return self.construct_callout_list(input, state, entry, metadata, accepted);
        }
        if matches!(entry.kind, ListKind::Description) {
            return self.construct_description_list(input, state, entry, metadata, accepted);
        }
        let (head, parent_scope, end) = document_parser::first_ordinary_list_item(
            input,
            state,
            self.offset,
            self.base_offset,
            entry.kind,
            metadata,
        )?;
        self.offset = end;
        let result = self.construct_ordinary_list(input, state, entry, metadata, head, accepted);
        state.ordinary_list_scope = parent_scope;
        result
    }

    fn construct_ordinary_list<'a>(
        &mut self,
        input: &'a str,
        state: &mut ParserState<'a>,
        entry: &ListEntry<'a>,
        metadata: &BlockParsingMetadata<'a>,
        head: ListItemHead<'a>,
        accepted: &mut Execution<
            '_,
            impl FnMut(&Block<'a>, &mut ParserState<'a>) -> Result<(), Error>,
        >,
    ) -> Result<Result<Block<'a>, Error>, RecognitionError> {
        let (ordered, kind) = match entry.kind {
            ListKind::Unordered(kind) => (false, kind),
            ListKind::Ordered(kind) => (true, kind),
            ListKind::Callout | ListKind::Description => {
                return Ok(Err(list_error(
                    state,
                    entry.start + self.base_offset,
                    "ordinary list construction received a different list kind",
                )));
            }
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
                start: entry.start,
                end,
                offset: self.base_offset,
                ordered,
                has_own_metadata: kind.has_own_metadata,
            },
            vec![item],
        );
        if let Err(error) = accepted.run(&block, state) {
            return Ok(Err(error));
        }
        let Some(first) = crate::extensions::ordinary_list_items_mut(&mut block).and_then(Vec::pop)
        else {
            return Ok(Err(list_error(
                state,
                entry.start + self.base_offset,
                "ordinary list builder returned no first item",
            )));
        };
        item = first;
        let mut items = Vec::new();
        loop {
            item.blocks = match self.collect_list_children(
                input,
                state,
                metadata,
                (entry.kind, base_marker, item.marker),
                accepted,
            )? {
                Ok(blocks) => blocks,
                Err(error) => return Ok(Err(error)),
            };
            self.offset = document_parser::list_item_end(
                input,
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
            let last_end = item.location.absolute_end;
            items.push(item);
            let step = document_parser::list_next(
                input,
                state,
                self.offset,
                self.base_offset,
                entry.kind,
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
                *block.location_mut() =
                    state.create_location(entry.start + self.base_offset, last_end);
                if let Some(slot) = crate::extensions::ordinary_list_items_mut(&mut block) {
                    *slot = items;
                }
                return Ok(Ok(block));
            }
        }
    }
    pub(super) fn collect_list_children<'a>(
        &mut self,
        input: &'a str,
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
            let step = document_parser::list_next(
                input,
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
