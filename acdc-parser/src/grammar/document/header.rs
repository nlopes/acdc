//! Document header boundary and source span.

use crate::{BlockMetadata, Error, Header, Location, grammar::ParserState};

use super::metadata::{AttributeOrAnchorLine, push_metadata_anchor};

/// The result of parsing the lines at the start of a document.
///
/// `header` is present only when the document has a title. A document
/// without a title can still start with attribute entries and comments;
/// `untitled_location` records their span.
#[derive(Debug)]
pub(in crate::grammar::document) struct ParsedHeader<'input> {
    pub(in crate::grammar::document) header: Option<Header<'input>>,
    pub(in crate::grammar::document) untitled_location: Option<Location>,
}

/// The inclusive span of the header lines from `start` up to the exclusive
/// `end` offset, which ends the last header line.
pub(super) fn header_span(state: &ParserState<'_>, start: usize, end: usize) -> Location {
    let mut location = state.create_location(start, end);
    location.absolute_end = state.source_step(
        location.absolute_end,
        crate::grammar::utf8_utils::RoundDirection::Backward,
    );
    location.end.column = location.end.column.saturating_sub(1);
    location
}

/// Merge the anchors and attribute lists above the document title.
/// The last ID and style win; roles, options and named attributes accumulate.
pub(super) fn merge_header_metadata<'input>(
    state: &ParserState<'input>,
    lines: impl IntoIterator<Item = AttributeOrAnchorLine<'input>>,
) -> Result<BlockMetadata<'input>, Error> {
    let mut metadata = BlockMetadata::default();
    for line in lines {
        match line {
            AttributeOrAnchorLine::Anchor(anchor) => {
                push_metadata_anchor(state, &mut metadata, anchor)?;
            }
            AttributeOrAnchorLine::Attributes(attributes) => {
                let attr_metadata = attributes.metadata;
                if attr_metadata.id.is_some() {
                    metadata.id = attr_metadata.id;
                }
                if attr_metadata.style.is_some() {
                    metadata.style = attr_metadata.style;
                }
                metadata.roles.append(attr_metadata.roles);
                metadata.options.append(attr_metadata.options);
                for (name, value) in attr_metadata.attributes.iter() {
                    metadata.attributes.set(name.clone(), value.clone());
                }
                metadata.positional_attributes = attr_metadata.positional_attributes;
                if attr_metadata.reftext_source.is_some() {
                    metadata.reftext_source = attr_metadata.reftext_source;
                }
            }
        }
    }
    Ok(metadata)
}
