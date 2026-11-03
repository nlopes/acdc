//! Inline grammar and its supporting modules.

mod attributes;
mod index_terms;
mod links;
mod peg;
mod recognition;
mod text;
pub(super) mod typography;
mod ui;

pub(super) use links::force_local_file_xref;
pub(crate) use peg::inline_parser;
