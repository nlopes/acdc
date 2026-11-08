//! Block and inline parsing.
//!
//! Each grammar has a `peg.rs` module: `document`, `inlines`, and
//! `inline_preprocessor`. Helpers used by one grammar live beside its PEG rules,
//! grouped by purpose. Shared helpers, parser state, and source mapping stay here.

mod attributes;
mod brackets;
mod character_references;
mod document;
pub(crate) mod footnotes;
pub(crate) mod helpers;
mod inline_boundaries;
mod inline_preprocessor;
mod inline_processing;
pub(crate) mod inlines;
mod line_map;
mod location_mapping;
mod location_walk;
mod marked_text;
mod passthrough_processing;
pub(crate) mod reference_text;
pub(crate) mod reference_walk;
pub(crate) mod setext;
mod source_remap;
mod state;
mod utf8_utils;

pub(crate) use attributes::{
    BlockAttributeMode, block_attribute_line, block_attribute_style, style_overrides_block_form,
};
pub(crate) use document::document_parser;
pub(crate) use inline_preprocessor::{
    InlinePreprocessorParserState, ProcessedContent, inline_preprocessing,
};
pub(crate) use inline_processing::process_inlines;
pub(crate) use inlines::inline_parser;
pub(crate) use line_map::LineMap;
pub(crate) use location_walk::{walk_document_inline_nodes_mut, walk_inline_nodes_mut};
pub use passthrough_processing::parse_text_for_quotes;
pub(crate) use source_remap::{remap_document_to_source, remap_inlines_to_source};
pub(crate) use state::{InlineRules, ParserScope, ParserState};
