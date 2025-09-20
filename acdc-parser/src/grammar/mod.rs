mod author_revision;
mod document;
mod inline_preprocessor;
mod inline_processing;
mod location_mapping;
mod markup_patterns;
mod passthrough_processing;
mod position_tracker;

pub(crate) use document::{ParserState, document_parser};
pub(crate) use inline_preprocessor::{
    InlinePreprocessorParserState, ProcessedContent, inline_preprocessing,
};
pub(crate) use location_mapping::{LocationMappingContext, map_formatted_inline_locations};
pub(crate) use position_tracker::{LineMap, PositionTracker};
