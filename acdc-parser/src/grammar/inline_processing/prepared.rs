//! Immutable inline preprocessing that can be reused at the same source span.

use super::{
    BlockParsingMetadata, Error, InlineNode, Location, ParserScope, ParserState, ProcessedContent,
    parse_processed_inlines, preprocess_inline_content, processed_text_as_outer,
};

/// Reuse requires the same source span, attributes, and substitution plan.
/// Parsing still runs for every occurrence to register its own semantic effects.
pub(crate) struct PreparedInlines<'a> {
    location: Location,
    processed: ProcessedContent<'a>,
    source: &'a str,
}

impl<'a> PreparedInlines<'a> {
    pub(crate) fn new(
        state: &mut ParserState<'a>,
        metadata: &BlockParsingMetadata,
        start: usize,
        end: usize,
        offset: usize,
        content: &'a str,
    ) -> Result<Self, Error> {
        let (location, mut processed) =
            preprocess_inline_content(state, start, end, offset, content, metadata.substitutions)?;
        let source = processed_text_as_outer(&processed, state);
        processed.attribute_substitutions = metadata.substitutions;
        Ok(Self {
            location,
            processed,
            source,
        })
    }

    pub(super) fn is_empty(&self, state: &ParserState<'_>, verbatim: bool) -> bool {
        // Whitespace remains content inside formatting, including attribute expansions.
        self.source.is_empty()
            || (!verbatim && state.scope == ParserScope::Document && self.source.trim().is_empty())
    }

    pub(super) fn source(&self) -> &'a str {
        self.source
    }

    pub(super) fn restored_source(&self, state: &ParserState<'a>) -> &'a str {
        if self.processed.passthroughs.is_empty() {
            self.source
        } else {
            let restored = crate::grammar::passthrough_processing::replace_passthrough_placeholders(
                self.source,
                &self.processed,
            );
            state.intern_str(&restored)
        }
    }

    pub(crate) fn parse(
        &self,
        state: &mut ParserState<'a>,
        metadata: &BlockParsingMetadata,
        verbatim: bool,
    ) -> Result<Vec<InlineNode<'a>>, Error> {
        if self.is_empty(state, verbatim) {
            return Ok(Vec::new());
        }
        let content = parse_processed_inlines(
            &self.processed,
            self.source,
            state,
            metadata,
            &self.location,
            true,
            verbatim,
        )?;
        crate::grammar::location_mapping::map_paragraph_locations(
            state,
            &self.processed,
            content,
            &self.location,
        )
    }
}
