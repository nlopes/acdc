//! Document grammar and its supporting modules.

mod author;
mod callouts;
mod construction;
mod coordinator;
mod delimited;
mod doctype;
mod finish;
mod header;
pub(super) mod lists;
mod manpage;
mod metadata;
mod paragraph;
mod peg;
mod quotes;
mod references;
mod revision;
mod sections;
mod table;
mod verbatim;

pub(crate) use coordinator::document;
pub(crate) use peg::document_parser;

type RecognitionError = ::peg::error::ParseError<::peg::str::LineCol>;

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::unreachable
)]
mod tests;
