#[allow(unused_imports)]
use crate::{
    Bold, CurvedApostrophe, CurvedQuotation, Form, Highlight, InlineNode, Italic, Location,
    Monospace, Plain, ProcessedContent, StandaloneCurvedApostrophe, Subscript, Superscript,
};

use super::document::ParserState;

/// Context for location mapping operations
pub(crate) struct LocationMappingContext<'a> {
    pub state: &'a ParserState,
    pub processed: &'a ProcessedContent,
    pub base_location: &'a Location,
}

/// Trait for formatted inline elements that have form, content, and location
pub(crate) trait FormattedInline {
    fn location(&self) -> &Location;
    fn location_mut(&mut self) -> &mut Location;
    fn content(&self) -> &Vec<InlineNode>;
    fn content_mut(&mut self) -> &mut Vec<InlineNode>;
    fn form(&self) -> &Form;
}

// Implementations for all formatted inline types
impl FormattedInline for Bold {
    fn location(&self) -> &Location {
        &self.location
    }
    fn location_mut(&mut self) -> &mut Location {
        &mut self.location
    }
    fn content(&self) -> &Vec<InlineNode> {
        &self.content
    }
    fn content_mut(&mut self) -> &mut Vec<InlineNode> {
        &mut self.content
    }
    fn form(&self) -> &Form {
        &self.form
    }
}

impl FormattedInline for Italic {
    fn location(&self) -> &Location {
        &self.location
    }
    fn location_mut(&mut self) -> &mut Location {
        &mut self.location
    }
    fn content(&self) -> &Vec<InlineNode> {
        &self.content
    }
    fn content_mut(&mut self) -> &mut Vec<InlineNode> {
        &mut self.content
    }
    fn form(&self) -> &Form {
        &self.form
    }
}

impl FormattedInline for Monospace {
    fn location(&self) -> &Location {
        &self.location
    }
    fn location_mut(&mut self) -> &mut Location {
        &mut self.location
    }
    fn content(&self) -> &Vec<InlineNode> {
        &self.content
    }
    fn content_mut(&mut self) -> &mut Vec<InlineNode> {
        &mut self.content
    }
    fn form(&self) -> &Form {
        &self.form
    }
}

impl FormattedInline for Highlight {
    fn location(&self) -> &Location {
        &self.location
    }
    fn location_mut(&mut self) -> &mut Location {
        &mut self.location
    }
    fn content(&self) -> &Vec<InlineNode> {
        &self.content
    }
    fn content_mut(&mut self) -> &mut Vec<InlineNode> {
        &mut self.content
    }
    fn form(&self) -> &Form {
        &self.form
    }
}

impl FormattedInline for Subscript {
    fn location(&self) -> &Location {
        &self.location
    }
    fn location_mut(&mut self) -> &mut Location {
        &mut self.location
    }
    fn content(&self) -> &Vec<InlineNode> {
        &self.content
    }
    fn content_mut(&mut self) -> &mut Vec<InlineNode> {
        &mut self.content
    }
    fn form(&self) -> &Form {
        &self.form
    }
}

impl FormattedInline for Superscript {
    fn location(&self) -> &Location {
        &self.location
    }
    fn location_mut(&mut self) -> &mut Location {
        &mut self.location
    }
    fn content(&self) -> &Vec<InlineNode> {
        &self.content
    }
    fn content_mut(&mut self) -> &mut Vec<InlineNode> {
        &mut self.content
    }
    fn form(&self) -> &Form {
        &self.form
    }
}

impl FormattedInline for CurvedQuotation {
    fn location(&self) -> &Location {
        &self.location
    }
    fn location_mut(&mut self) -> &mut Location {
        &mut self.location
    }
    fn content(&self) -> &Vec<InlineNode> {
        &self.content
    }
    fn content_mut(&mut self) -> &mut Vec<InlineNode> {
        &mut self.content
    }
    fn form(&self) -> &Form {
        &self.form
    }
}

impl FormattedInline for CurvedApostrophe {
    fn location(&self) -> &Location {
        &self.location
    }
    fn location_mut(&mut self) -> &mut Location {
        &mut self.location
    }
    fn content(&self) -> &Vec<InlineNode> {
        &self.content
    }
    fn content_mut(&mut self) -> &mut Vec<InlineNode> {
        &mut self.content
    }
    fn form(&self) -> &Form {
        &self.form
    }
}

/// Generic function for mapping formatted inline locations with form-awareness
pub(crate) fn map_formatted_inline_locations<T: FormattedInline>(
    mut inline: T,
    mapping_ctx: &LocationMappingContext,
) -> T {
    // Get the form first to avoid borrowing issues
    let form = inline.form().clone();
    let content = inline.content().clone();
    let location = inline.location().clone();

    // Create a form-aware location mapper - this provides more accurate location mapping!
    let map_loc = create_location_mapper(
        mapping_ctx.state,
        mapping_ctx.processed,
        mapping_ctx.base_location,
        Some(&form), // Pass the form information for precise mapping
    );

    // Map outer location with attribute extension
    let mapped_outer = map_loc(&location);
    let extended_location =
        extend_attribute_location_if_needed(mapping_ctx.state, mapping_ctx.processed, mapped_outer);
    *inline.location_mut() = extended_location;

    // Map inner content locations
    let mapped_content = map_inner_content_locations(
        content,
        map_loc.as_ref(),
        mapping_ctx.state,
        mapping_ctx.processed,
        mapping_ctx.base_location,
    );
    *inline.content_mut() = mapped_content;

    inline
}

/// Location mapping coordinate transformations during inline processing.
///
/// # Location Mapping Overview
///
/// The inline parser operates on preprocessed text that may have undergone attribute
/// substitutions and other transformations. This creates a complex coordinate mapping problem:
///
/// 1. **Original document coordinates**: Character positions in the raw `AsciiDoc` source
/// 2. **Preprocessed coordinates**: Character positions after attribute substitution/processing
/// 3. **Parsed inline coordinates**: Relative positions within the preprocessed content
///
/// ## Coordinate Transformation Pipeline
///
/// ```text
/// Original:      "{greeting} _world_!"
/// Preprocessed:  "hello _world_!"
/// Parsed inline: ["hello ", ItalicText("world"), "!"]
/// ```
///
/// The mapping process:
/// 1. Take parsed inline locations (relative to preprocessed text)
/// 2. Convert to preprocessed absolute coordinates
/// 3. Use source map to find original document coordinates
/// 4. Convert to human-readable line/column positions
///
/// ## Special Cases
///
/// **Attribute Substitutions**: When `{greeting}` becomes `hello`, the location mapping
/// may collapse to a single point. We detect this and expand the location to cover the
/// full original attribute span for better error reporting and IDE support.
///
/// **Nested Content**: Formatted text like `**{greeting}**` requires mapping both the
/// outer formatting markers and inner content locations correctly.
/// Map a single location from preprocessed coordinates to original document coordinates.
///
/// This is the core coordinate transformation that:
/// 1. Converts preprocessed-relative offsets to document-absolute offsets
/// 2. Uses the preprocessor source map to find original positions
/// 3. Computes human-readable line/column positions
pub(crate) fn create_location_mapper<'a>(
    state: &'a ParserState,
    processed: &'a ProcessedContent,
    base_location: &'a Location,
    form: Option<&'a Form>,
) -> Box<dyn Fn(&Location) -> Location + 'a> {
    Box::new(move |loc: &Location| -> Location {
        tracing::info!(?base_location, ?loc, "mapping inline location");

        // Convert processed-relative absolute offsets into document-absolute offsets
        let mut processed_abs_start = base_location.absolute_start + loc.absolute_start;
        let mut processed_abs_end = base_location.absolute_start + loc.absolute_end;

        // Fix for collapsed locations (where absolute_start == absolute_end)
        if loc.absolute_start == loc.absolute_end {
            if loc.absolute_start == 0 && base_location.absolute_start < base_location.absolute_end
            {
                // Special case: single character inside constrained formatting like "*s*"
                // Check if this is constrained formatting (which has single-character delimiters)
                let is_constrained_single_char = if let Some(form) = form {
                    matches!(form, Form::Constrained)
                } else {
                    // Fallback: use magic number for backward compatibility
                    let base_length = base_location.absolute_end - base_location.absolute_start;
                    base_length <= 5
                };

                if is_constrained_single_char {
                    // "*s*" has length 3, constrained formatting uses single delimiters
                    // The "s" should be at position 1-2, not 0-0
                    processed_abs_start = base_location.absolute_start + 1;
                    processed_abs_end = base_location.absolute_start + 2;
                } else {
                    // General case: expand collapsed locations by 1 to represent the character
                    processed_abs_end += 1;
                }
            } else {
                // General case: expand collapsed locations by 1 to represent the character
                processed_abs_end += 1;
            }
        }

        // Map those through the preprocessor source map back to original source
        let mapped_abs_start = processed.source_map.map_position(processed_abs_start);
        let mapped_abs_end = processed.source_map.map_position(processed_abs_end);

        // Compute human positions from the document's line map
        let start_pos = state.line_map.offset_to_position(mapped_abs_start);
        let mut end_pos = state.line_map.offset_to_position(mapped_abs_end);

        // For single-character content inside constrained formatting, ensure both start and end column point to the same character
        let is_single_char_fix = mapped_abs_end == mapped_abs_start + 1
            && loc.absolute_start == 0
            && base_location.absolute_start < base_location.absolute_end;
        if is_single_char_fix {
            // Check if this is constrained formatting (which has single-character delimiters)
            let is_constrained_single_char = if let Some(form) = form {
                matches!(form, Form::Constrained)
            } else {
                // Fallback: use magic number for backward compatibility
                let base_length = base_location.absolute_end - base_location.absolute_start;
                base_length <= 5
            };

            if is_constrained_single_char {
                end_pos.column = start_pos.column;
            }
        }

        Location {
            absolute_start: mapped_abs_start,
            absolute_end: mapped_abs_end,
            start: start_pos,
            end: end_pos,
        }
    })
}

/// Apply attribute substitution location extension if needed.
///
/// When attribute substitutions collapse locations to a single point (e.g., `{attr}` -> `value`),
/// we need to extend the location back to cover the original attribute span for better UX.
pub(crate) fn extend_attribute_location_if_needed(
    state: &ParserState,
    processed: &ProcessedContent,
    mut location: Location,
) -> Location {
    // Check if location is collapsed and we have attribute replacements to consider
    if location.absolute_start == location.absolute_end
        && !processed.source_map.replacements.is_empty()
    {
        // Find the attribute replacement that contains this collapsed location
        if let Some(attr_replacement) = processed.source_map.replacements.iter().find(|rep| {
            rep.kind == crate::grammar::inline_preprocessor::ProcessedKind::Attribute
                && location.absolute_start >= rep.absolute_start
                && location.absolute_start < rep.processed_end
        }) {
            tracing::debug!(from=?location, to=?attr_replacement,
                "Extending collapsed location to full attribute span",
            );

            // Extend location to cover the full original attribute
            let start_pos = state
                .line_map
                .offset_to_position(attr_replacement.absolute_start);
            let end_pos = state
                .line_map
                .offset_to_position(attr_replacement.absolute_end);
            location = Location {
                absolute_start: attr_replacement.absolute_start,
                absolute_end: attr_replacement.absolute_end,
                start: start_pos,
                end: end_pos,
            };
        }
    }
    location
}

/// Map locations for inner content within formatted text (bold, italic, etc.).
///
/// This handles the complex case where formatted text contains nested content that may
/// include attribute substitutions requiring location extension.
pub(crate) fn map_inner_content_locations(
    content: Vec<InlineNode>,
    map_loc: &dyn Fn(&Location) -> Location,
    state: &ParserState,
    processed: &ProcessedContent,
    base_location: &Location,
) -> Vec<InlineNode> {
    content
        .into_iter()
        .map(|node| match node {
            InlineNode::PlainText(mut inner_plain) => {
                // Replace passthrough placeholders in the content
                let content = super::passthrough_processing::replace_passthrough_placeholders(
                    &inner_plain.content,
                    processed,
                );
                inner_plain.content = content;

                // Map to document coordinates first (use normal location mapping for inner content)
                let mut mapped = map_loc(&inner_plain.location);

                // For single-character content, ensure start and end columns are the same
                if inner_plain.content.chars().count() == 1 {
                    mapped.end.column = mapped.start.column;
                }

                // Apply attribute location extension if needed
                inner_plain.location =
                    extend_attribute_location_if_needed(state, processed, mapped);
                InlineNode::PlainText(inner_plain)
            }
            marked_text @ (InlineNode::ItalicText(_)
            | InlineNode::BoldText(_)
            | InlineNode::MonospaceText(_)
            | InlineNode::HighlightText(_)
            | InlineNode::SubscriptText(_)
            | InlineNode::SuperscriptText(_)
            | InlineNode::CurvedQuotationText(_)
            | InlineNode::CurvedApostropheText(_)) => {
                let mapping_ctx = LocationMappingContext {
                    state,
                    processed,
                    base_location,
                };
                marked_text.map_formatted_inline_locations(&mapping_ctx)
            }
            other => other,
        })
        .collect()
}

/// Remap the location of an inline node to final document coordinates
pub(crate) fn remap_inline_node_location(node: &mut InlineNode, base_offset: usize) {
    match node {
        InlineNode::PlainText(plain) => {
            plain.location.absolute_start += base_offset;
            plain.location.absolute_end += base_offset;
            plain.location.start.column += base_offset;
            plain.location.end.column += base_offset;
        }
        InlineNode::BoldText(bold) => {
            bold.location.absolute_start += base_offset;
            bold.location.absolute_end += base_offset;
            bold.location.start.column += base_offset;
            bold.location.end.column += base_offset;
            // Recursively remap nested content
            for nested_node in &mut bold.content {
                remap_inline_node_location(nested_node, base_offset);
            }
        }
        InlineNode::ItalicText(italic) => {
            italic.location.absolute_start += base_offset;
            italic.location.absolute_end += base_offset;
            italic.location.start.column += base_offset;
            italic.location.end.column += base_offset;
            // Recursively remap nested content
            for nested_node in &mut italic.content {
                remap_inline_node_location(nested_node, base_offset);
            }
        }
        InlineNode::SuperscriptText(superscript) => {
            superscript.location.absolute_start += base_offset;
            superscript.location.absolute_end += base_offset;
            superscript.location.start.column += base_offset;
            superscript.location.end.column += base_offset;
            // Recursively remap nested content
            for nested_node in &mut superscript.content {
                remap_inline_node_location(nested_node, base_offset);
            }
        }
        InlineNode::SubscriptText(subscript) => {
            subscript.location.absolute_start += base_offset;
            subscript.location.absolute_end += base_offset;
            subscript.location.start.column += base_offset;
            subscript.location.end.column += base_offset;
            // Recursively remap nested content
            for nested_node in &mut subscript.content {
                remap_inline_node_location(nested_node, base_offset);
            }
        }
        InlineNode::CurvedQuotationText(curved_quotation) => {
            curved_quotation.location.absolute_start += base_offset;
            curved_quotation.location.absolute_end += base_offset;
            curved_quotation.location.start.column += base_offset;
            curved_quotation.location.end.column += base_offset;
            // Recursively remap nested content
            for nested_node in &mut curved_quotation.content {
                remap_inline_node_location(nested_node, base_offset);
            }
        }
        InlineNode::CurvedApostropheText(curved_apostrophe) => {
            curved_apostrophe.location.absolute_start += base_offset;
            curved_apostrophe.location.absolute_end += base_offset;
            curved_apostrophe.location.start.column += base_offset;
            curved_apostrophe.location.end.column += base_offset;
            // Recursively remap nested content
            for nested_node in &mut curved_apostrophe.content {
                remap_inline_node_location(nested_node, base_offset);
            }
        }
        InlineNode::MonospaceText(monospace) => {
            monospace.location.absolute_start += base_offset;
            monospace.location.absolute_end += base_offset;
            monospace.location.start.column += base_offset;
            monospace.location.end.column += base_offset;
            // Recursively remap nested content
            for nested_node in &mut monospace.content {
                remap_inline_node_location(nested_node, base_offset);
            }
        }
        InlineNode::HighlightText(highlight) => {
            highlight.location.absolute_start += base_offset;
            highlight.location.absolute_end += base_offset;
            highlight.location.start.column += base_offset;
            highlight.location.end.column += base_offset;
            // Recursively remap nested content
            for nested_node in &mut highlight.content {
                remap_inline_node_location(nested_node, base_offset);
            }
        }
        // Add other inline node types as needed
        _ => {}
    }
}

/// Create a location that maps back to original source coordinates when content has been
/// modified by passthrough replacement.
///
/// When we replace passthrough placeholders with their content, we need to map the location
/// back to the original source text coordinates rather than the preprocessed coordinates.
pub(crate) fn create_original_source_location(
    plain_content: &str,
    plain_location: &Location,
    processed: &ProcessedContent,
    base_location: &Location,
) -> Location {
    // Check if this PlainText content actually contains passthrough placeholders
    let contains_passthroughs = !processed.passthroughs.is_empty()
        && processed.passthroughs.iter().enumerate().any(|(index, _)| {
            let placeholder = format!("���{index}���");
            plain_content.contains(&placeholder)
        });

    if contains_passthroughs {
        // For a PlainText that contains passthrough placeholders and spans the entire content,
        // we should map back to the original source location
        if plain_location.absolute_start == 0 {
            // Use the base location which represents the original source coordinates
            return base_location.clone();
        }
    }

    // For other cases, use the existing location mapping
    // This is a fallback - in practice we might need more sophisticated logic here
    plain_location.clone()
}

/// Map inline node locations from preprocessed coordinates to original document coordinates.
///
/// This is the main entry point for location mapping during inline processing. It handles
/// the complex coordinate transformations needed to map parsed inline content back to
/// original document positions while accounting for preprocessing changes like attribute
/// substitutions.
///
/// See the module-level documentation for a detailed explanation of the coordinate
/// transformation pipeline and special cases.
#[tracing::instrument(skip_all, fields(location=?location, processed=?processed, content=?content))]
pub(crate) fn map_inline_locations(
    state: &ParserState,
    processed: &ProcessedContent,
    content: &Vec<InlineNode>,
    location: &Location,
) -> Vec<InlineNode> {
    tracing::info!(?location, "mapping inline locations");

    let map_loc = create_location_mapper(state, processed, location, None);

    content
        .iter()
        .flat_map(|inline| match inline {
            InlineNode::PlainText(plain) => {
                let original_content = &plain.content;

                // Check if this PlainText contains passthrough placeholders
                let contains_passthroughs = !processed.passthroughs.is_empty()
                    && processed.passthroughs.iter().enumerate().any(|(index, _)| {
                        let placeholder = format!("���{index}���");
                        original_content.contains(&placeholder)
                    });

                if contains_passthroughs {
                    // Use multi-pass processing for passthroughs with quote substitutions
                    let base_location = create_original_source_location(
                        original_content,
                        &plain.location,
                        processed,
                        location,
                    );

                    tracing::debug!(content = ?original_content, "Processing passthrough placeholders in PlainText");
                    super::passthrough_processing::process_passthrough_placeholders(original_content, processed, state, &base_location)
                } else {
                    // No passthroughs, handle normally
                    let mut mapped_location = map_loc(&plain.location);

                    // For single-character content, ensure start and end columns are the same
                    if original_content.chars().count() == 1 {
                        mapped_location.end.column = mapped_location.start.column;
                    }


                    vec![InlineNode::PlainText(Plain {
                        content: original_content.clone(),
                        location: mapped_location,
                    })]
                }
            }
            marked_text @ (InlineNode::ItalicText(_)
            | InlineNode::BoldText(_)
            | InlineNode::MonospaceText(_)
            | InlineNode::HighlightText(_)
            | InlineNode::SubscriptText(_)
            | InlineNode::SuperscriptText(_)
            | InlineNode::CurvedQuotationText(_)
            | InlineNode::CurvedApostropheText(_)) => {
                let mapping_ctx = LocationMappingContext {
                    state,
                    processed,
                    base_location: location,
                };
                vec![marked_text.clone().map_formatted_inline_locations(&mapping_ctx)]
            }
            InlineNode::StandaloneCurvedApostrophe(standalone) => {
                let mut mapped_standalone = standalone.clone();
                mapped_standalone.location = map_loc(&standalone.location);
                vec![InlineNode::StandaloneCurvedApostrophe(mapped_standalone)]
            }
            other => vec![other.clone()],
        })
        .collect::<Vec<_>>()
}
