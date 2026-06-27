mod attributes;
mod author;
pub(crate) mod doctype;
mod document;
pub(crate) mod helpers;
mod inline_preprocessor;
mod inline_processing;
pub(crate) mod inlines;
mod line_map;
mod location_mapping;
mod location_walk;
pub(crate) mod manpage;
mod marked_text;
mod passthrough_processing;
mod revision;
pub(crate) mod setext;
mod source_remap;
mod state;
mod table;
pub(crate) mod utf8_utils;

pub(crate) use document::document_parser;
pub(crate) use inline_preprocessor::{
    InlinePreprocessorParserState, ProcessedContent, inline_preprocessing,
};
pub(crate) use inlines::inline_parser;
pub(crate) use line_map::LineMap;
pub use passthrough_processing::parse_text_for_quotes;
pub(crate) use source_remap::{remap_document_to_source, remap_inlines_to_source};
pub(crate) use state::ParserState;
