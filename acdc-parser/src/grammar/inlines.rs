//! Inline grammar and its supporting modules.

mod attributes;
mod index_terms;
mod links;
mod peg;
mod recognition;
mod text;

pub(crate) use peg::inline_parser;
