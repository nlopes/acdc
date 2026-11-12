//! Committed description-list items and native attachment recovery.

use crate::{
    Block, Error,
    grammar::{ParserState, helpers::BlockParsingMetadata},
};

use super::super::{
    RecognitionError,
    coordinator::{BlockCursor, BlockStepKind, Execution},
    document_parser,
};
use super::{
    DescriptionItemHead, DescriptionListScope, ListEntry, build_description_list,
    warn_discarded_attachment,
};

#[derive(Default)]
struct DescriptionChildren<'a> {
    blocks: Vec<Block<'a>>,
    errors: Vec<Error>,
}

impl BlockCursor {
    pub(super) fn construct_description_list<'a>(
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
        let (head, parent_scope, end) = document_parser::first_description_list_item(
            input,
            state,
            self.offset,
            self.base_offset,
            metadata,
        )?;
        self.offset = end;
        let result = self.collect_description_items(
            input,
            state,
            entry,
            metadata,
            (head, parent_scope),
            accepted,
        );
        state.description_list_scope = parent_scope;
        result.map(|result| {
            result.map(|items| {
                let end = items.last().map_or(self.offset + self.base_offset, |item| {
                    item.location.absolute_end
                });
                build_description_list(
                    metadata,
                    state.create_location(entry.start + self.base_offset, end),
                    items,
                )
            })
        })
    }

    fn collect_description_items<'a>(
        &mut self,
        input: &'a str,
        state: &mut ParserState<'a>,
        entry: &ListEntry<'a>,
        metadata: &BlockParsingMetadata<'a>,
        initial: (DescriptionItemHead<'a>, Option<DescriptionListScope<'a>>),
        accepted: &mut Execution<
            '_,
            impl FnMut(&Block<'a>, &mut ParserState<'a>) -> Result<(), Error>,
        >,
    ) -> Result<Result<Vec<crate::DescriptionListItem<'a>>, Error>, RecognitionError> {
        let (mut head, parent_scope) = initial;
        let mut items = Vec::new();
        let mut first = true;
        loop {
            let DescriptionItemHead { source, principal } = head;
            let principal = match principal {
                Ok(principal) => principal,
                Err(error) => return Ok(Err(error)),
            };
            if first {
                let end = source.principal_start + source.principal_content.trim_end().len();
                let location = state.create_block_location(
                    entry.start,
                    end.max(source.term.delimiter_end),
                    self.base_offset,
                );
                let shell = build_description_list(metadata, location, Vec::new());
                if let Err(error) = accepted.run(&shell, state) {
                    return Ok(Err(error));
                }
                first = false;
            }
            let children = match self.collect_description_children(
                input,
                state,
                metadata,
                source.term.delimiter,
                accepted,
            )? {
                Ok(children) => children,
                Err(error) => return Ok(Err(error)),
            };
            // Native attachment diagnostics follow all child parsing in this
            // item. Preserve that order while keeping handler failures fatal.
            for error in children.errors {
                warn_discarded_attachment(state, &error);
            }
            items.push(source.build(state, principal, children.blocks, self.base_offset));
            let (next, end) = document_parser::next_description_list_item(
                input,
                state,
                self.offset,
                self.base_offset,
                metadata,
                parent_scope,
            )?;
            self.offset = end;
            if let Some(next) = next {
                head = next;
            } else {
                return Ok(Ok(items));
            }
        }
    }

    fn collect_description_children<'a>(
        &mut self,
        input: &'a str,
        state: &mut ParserState<'a>,
        metadata: &BlockParsingMetadata<'a>,
        delimiter: &'a str,
        accepted: &mut Execution<
            '_,
            impl FnMut(&Block<'a>, &mut ParserState<'a>) -> Result<(), Error>,
        >,
    ) -> Result<Result<DescriptionChildren<'a>, Error>, RecognitionError> {
        let mut children = DescriptionChildren::default();
        let mut continuing = false;
        loop {
            let step = document_parser::description_list_next(
                input,
                state,
                self.offset,
                self.base_offset,
                metadata,
                delimiter,
                continuing,
            )?;
            self.offset = step.end;
            continuing = step.continuing;
            let Some(child) = step.child else {
                return Ok(Ok(children));
            };
            let child = match child {
                Ok(BlockStepKind::List(entry)) => {
                    self.construct_list(input, state, &entry, metadata, accepted)?
                }
                Ok(kind) => self.construct(input, state, kind, accepted)?,
                Err(error) => Err(error),
            };
            match child {
                Ok(block) => children.blocks.push(block),
                Err(error) if accepted.failed() => return Ok(Err(error)),
                Err(error) if step.warn_on_error => children.errors.push(error),
                Err(_) => {}
            }
        }
    }
}
