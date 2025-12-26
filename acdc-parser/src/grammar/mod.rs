mod attributes;
mod document;
mod inline_preprocessor;
mod inline_processing;
mod line_map;
mod location_mapping;
pub(crate) mod manpage;
mod marked_text;
mod markup_patterns;
mod passthrough_processing;
mod revision;
pub(crate) mod setext;
mod state;
mod table;
pub(crate) mod utf8_utils;

pub(crate) use document::{BlockParsingMetadata, document_parser};
pub(crate) use inline_preprocessor::{
    InlinePreprocessorParserState, ProcessedContent, inline_preprocessing,
};
pub(crate) use line_map::LineMap;
pub(crate) use state::ParserState;
