//! Committed callout-list construction with shared numbering and validation.

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
use super::{CalloutItem, CalloutItemHead, ListEntry, ListKind, build_callout_list};

impl BlockCursor {
    pub(super) fn construct_callout_list<'a>(
        &mut self,
        input: &'a str,
        state: &mut ParserState<'a>,
        entry: &ListEntry<'a>,
        metadata: &BlockParsingMetadata<'a>,
        accepted: &mut Execution<
            '_,
            impl FnMut(&Block<'a>, &mut ParserState<'a>) -> Result<(), Error>,
        >,
    ) -> Result<Result<Block<'a>, Error>, RecognitionError> {
        let (head, (parent_scope, callouts), end) = document_parser::first_callout_list_item(
            input,
            state,
            self.offset,
            self.base_offset,
            metadata,
        )?;
        self.offset = end;
        let result = self.collect_callout_items(input, state, entry, metadata, head, accepted);
        state.callout_list_offset = parent_scope;
        state.pending_callouts.clear();
        result.map(|result| {
            result.map(|items| {
                let end = items.last().map_or(self.offset, |(_, _, end)| *end);
                build_callout_list(
                    state,
                    metadata,
                    state.create_location(entry.start + self.base_offset, end + self.base_offset),
                    items,
                    &callouts,
                )
            })
        })
    }

    fn collect_callout_items<'a>(
        &mut self,
        input: &'a str,
        state: &mut ParserState<'a>,
        entry: &ListEntry<'a>,
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
        let shell = build_callout_list(
            state,
            metadata,
            state.create_location(entry.start + self.base_offset, item.2 + self.base_offset),
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
            self.offset =
                document_parser::list_item_end(input, state, self.offset, self.base_offset, true)?;
            if !item.0.blocks.is_empty() {
                item.2 = self.offset.saturating_sub(1);
                item.0.location = state
                    .create_location(item.0.location.absolute_start, item.2 + self.base_offset);
                item.0.blocks = order_block_prefix_events(item.0.blocks);
            }
            items.push(item);
            let (next, end) = document_parser::next_callout_list_item(
                input,
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
            } else {
                return Ok(Ok(items));
            }
        }
    }
}
