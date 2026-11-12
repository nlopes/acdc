//! Committed callout-list construction with shared numbering and validation.

use crate::{
    Block, Error,
    grammar::{ParserState, helpers::BlockParsingMetadata},
};

use super::super::{
    RecognitionError,
    coordinator::{BlockCursor, Execution},
    document_parser,
    input::{BlockInput, ReadRequest},
    metadata::order_block_prefix_events,
};
use super::{CalloutItem, CalloutItemHead, ListKind, build_callout_list};

impl BlockCursor {
    pub(super) fn construct_callout_list<'a>(
        &mut self,
        input: &mut impl BlockInput<'a>,
        state: &mut ParserState<'a>,
        start: usize,
        metadata: &BlockParsingMetadata<'a>,
        accepted: &mut Execution<
            '_,
            impl FnMut(&Block<'a>, &mut ParserState<'a>) -> Result<(), Error>,
        >,
    ) -> Result<Result<Block<'a>, Error>, RecognitionError> {
        let source = match input.prepare(self, state, ReadRequest::ListHead(ListKind::Callout)) {
            Ok(source) => source,
            Err(error) => return Ok(Err(error)),
        };
        let (head, (parent_scope, callouts), end) = document_parser::first_callout_list_item(
            source,
            state,
            self.offset,
            self.base_offset,
            metadata,
        )?;
        self.offset = end;
        let result = self.collect_callout_items(input, state, start, metadata, head, accepted);
        state.callout_list_offset = parent_scope;
        state.pending_callouts.clear();
        result.map(|result| {
            result.map(|items| {
                let end = items
                    .last()
                    .map_or(self.offset + self.base_offset, |(_, _, end)| *end);
                build_callout_list(
                    state,
                    metadata,
                    state.create_location(start, end),
                    items,
                    &callouts,
                )
            })
        })
    }

    fn collect_callout_items<'a>(
        &mut self,
        input: &mut impl BlockInput<'a>,
        state: &mut ParserState<'a>,
        start: usize,
        metadata: &BlockParsingMetadata<'a>,
        head: CalloutItemHead<'a>,
        accepted: &mut Execution<
            '_,
            impl FnMut(&Block<'a>, &mut ParserState<'a>) -> Result<(), Error>,
        >,
    ) -> Result<Result<Vec<CalloutItem<'a>>, Error>, RecognitionError> {
        let mut item = match head.finish(state, Ok(Vec::new()), self.offset, self.base_offset) {
            Ok(item) => item,
            Err(error) => return Ok(Err(error)),
        };
        item.2 += self.base_offset;
        let shell = build_callout_list(
            state,
            metadata,
            state.create_location(start, item.2),
            Vec::new(),
            &[],
        );
        if let Err(error) = accepted.run(&shell, state) {
            return Ok(Err(error));
        }
        let mut items = Vec::new();
        loop {
            item.0.blocks = match self.collect_list_children(
                input,
                state,
                metadata,
                (ListKind::Callout, "", ""),
                accepted,
            )? {
                Ok(blocks) => blocks,
                Err(error) => return Ok(Err(error)),
            };
            let source = match input.prepare(self, state, ReadRequest::ListEnd) {
                Ok(source) => source,
                Err(error) => return Ok(Err(error)),
            };
            self.offset =
                document_parser::list_item_end(source, state, self.offset, self.base_offset, true)?;
            if !item.0.blocks.is_empty() {
                item.2 = self.base_offset + self.offset.saturating_sub(1);
                item.0.location = state.create_location(item.0.location.absolute_start, item.2);
                item.0.blocks = order_block_prefix_events(item.0.blocks);
            }
            items.push(item);
            let source = match input.prepare(
                self,
                state,
                ReadRequest::ListNext(ListKind::Callout, "", super::ListPhase::Sibling),
            ) {
                Ok(source) => source,
                Err(error) => return Ok(Err(error)),
            };
            let (next, end) = document_parser::next_callout_list_item(
                source,
                state,
                self.offset,
                self.base_offset,
                metadata,
            )?;
            self.offset = end;
            if let Some(head) = next {
                item = match head.finish(state, Ok(Vec::new()), self.offset, self.base_offset) {
                    Ok(item) => item,
                    Err(error) => return Ok(Err(error)),
                };
                item.2 += self.base_offset;
            } else {
                return Ok(Ok(items));
            }
        }
    }
}
