//! Document grammar and its supporting modules.

mod author;
mod callouts;
mod compound;
mod construction;
mod coordinator;
mod delimited;
mod doctype;
mod finish;
mod header;
mod input;
pub(super) mod lists;
mod manpage;
mod metadata;
mod paragraph;
mod peg;
mod quotes;
mod references;
mod revision;
mod sections;
mod source;
mod table;
mod verbatim;

pub(crate) use coordinator::document;
pub(crate) use peg::document_parser;
pub(crate) use source::{Assignments as SourceAssignments, SourceHandler, document_from_source};

type RecognitionError = ::peg::error::ParseError<::peg::str::LineCol>;

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::unreachable
)]
mod tests;
