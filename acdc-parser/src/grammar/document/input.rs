//! Supply stable input to one accepted construction step.

use crate::{Error, grammar::ParserState};

use super::{
    coordinator::BlockCursor,
    lists::{ListKind, ListPhase},
};

#[derive(Clone, Copy)]
pub(super) enum ReadRequest<'a> {
    Block,
    ListHead(ListKind<'a>),
    ListNext(ListKind<'a>, &'a str, ListPhase),
    ListEnd,
}

pub(super) trait BlockInput<'a> {
    /// Complete text does not expose resumable reader operations.
    fn source_input(&mut self) -> Option<&mut super::source::SourceInput<'a>> {
        None
    }

    fn at_boundary(
        &mut self,
        source: &'a str,
        cursor: &BlockCursor,
        state: &mut ParserState<'a>,
    ) -> bool {
        self.source_input()
            .and_then(|source| source.boundary())
            .is_some_and(|delimiter| {
                source
                    .get(cursor.offset..)
                    .and_then(|tail| tail.split_inclusive('\n').next())
                    .is_some_and(|line| {
                        super::document_parser::source_block_close(line, state, delimiter, false)
                            .is_ok()
                    })
            })
    }

    /// Source failures cannot be recovered as malformed nested content.
    fn failed(&self) -> bool {
        false
    }

    fn prepare(
        &mut self,
        cursor: &mut BlockCursor,
        state: &mut ParserState<'a>,
        request: ReadRequest<'a>,
    ) -> Result<&'a str, Error>;
}

pub(super) struct TextInput<'a>(pub(super) &'a str);

impl<'a> BlockInput<'a> for TextInput<'a> {
    #[inline]
    fn prepare(
        &mut self,
        _: &mut BlockCursor,
        _: &mut ParserState<'a>,
        _: ReadRequest<'a>,
    ) -> Result<&'a str, Error> {
        Ok(self.0)
    }
}
