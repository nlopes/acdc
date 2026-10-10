//! Document grammar and its supporting modules.

mod author;
mod callouts;
mod delimited;
mod doctype;
mod lists;
mod manpage;
mod metadata;
mod peg;
mod references;
mod revision;
mod sections;
mod table;
mod verbatim;

pub(crate) use peg::document_parser;

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::unreachable
)]
mod tests;
