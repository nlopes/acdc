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
