#[allow(unused_imports)]
use crate::{
    Bold, CurvedApostrophe, CurvedQuotation, Form, Highlight, InlineNode, Italic, Location,
    Monospace, Plain, ProcessedContent, StandaloneCurvedApostrophe, Subscript, Superscript,
};

use super::{ParserState, marked_text::WithLocationMappingContext};

/// Context for location mapping operations
pub(crate) struct LocationMappingContext<'a> {
    pub state: &'a ParserState,
    pub processed: &'a ProcessedContent,
    pub base_location: &'a Location,
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
        let mapped_abs_start = processed
            .source_map
            .map_position(processed_abs_start)
            .expect("mapped start position is not valid");
        let mapped_abs_end = processed
            .source_map
            .map_position(processed_abs_end)
            .expect("mapped end position is not valid");

        // Compute human positions from the document's line map
        let start_pos = state
            .line_map
            .offset_to_position(mapped_abs_start, &state.input);
        let mut end_pos = state
            .line_map
            .offset_to_position(mapped_abs_end, &state.input);

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
                .offset_to_position(attr_replacement.absolute_start, &state.input);
            let end_pos = state
                .line_map
                .offset_to_position(attr_replacement.absolute_end, &state.input);
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
                marked_text.with_location_mapping_context(&mapping_ctx)
            }
            other => other,
        })
        .collect()
}

/// Helper macro to remap locations for simple nodes (`PlainText`, etc.)
macro_rules! remap_simple_location {
    ($node:expr, $base_offset:expr) => {{
        $node.location.absolute_start += $base_offset;
        $node.location.absolute_end += $base_offset;
        $node.location.start.column += $base_offset;
        $node.location.end.column += $base_offset;
    }};
}

/// Helper macro to remap locations for formatted nodes with content
macro_rules! remap_formatted_location {
    ($node:expr, $base_offset:expr) => {{
        remap_simple_location!($node, $base_offset);
        // Recursively remap nested content
        for nested_node in &mut $node.content {
            remap_inline_node_location(nested_node, $base_offset);
        }
    }};
}

/// Remap the location of an inline node to final document coordinates
pub(crate) fn remap_inline_node_location(node: &mut InlineNode, base_offset: usize) {
    match node {
        InlineNode::PlainText(plain) => remap_simple_location!(plain, base_offset),
        InlineNode::BoldText(bold) => remap_formatted_location!(bold, base_offset),
        InlineNode::ItalicText(italic) => remap_formatted_location!(italic, base_offset),
        InlineNode::SuperscriptText(superscript) => {
            remap_formatted_location!(superscript, base_offset);
        }
        InlineNode::SubscriptText(subscript) => remap_formatted_location!(subscript, base_offset),
        InlineNode::CurvedQuotationText(curved_quotation) => {
            remap_formatted_location!(curved_quotation, base_offset);
        }
        InlineNode::CurvedApostropheText(curved_apostrophe) => {
            remap_formatted_location!(curved_apostrophe, base_offset);
        }
        InlineNode::MonospaceText(monospace) => remap_formatted_location!(monospace, base_offset),
        InlineNode::HighlightText(highlight) => remap_formatted_location!(highlight, base_offset),
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
                vec![marked_text.clone().with_location_mapping_context(&mapping_ctx)]
            }
            InlineNode::StandaloneCurvedApostrophe(standalone) => {
                let mut mapped_standalone = standalone.clone();
                mapped_standalone.location = map_loc(&standalone.location);
                vec![InlineNode::StandaloneCurvedApostrophe(mapped_standalone)]
            }
            InlineNode::Macro(inline_macro) => {
                use crate::InlineMacro;
                let mut mapped_macro = inline_macro.clone();
                match &mut mapped_macro {
                    InlineMacro::Footnote(footnote) => {
                        footnote.location = map_loc(&footnote.location);
                        // Recursively map the content locations using the same mapping function
                        footnote.content = map_inline_locations(state, processed, &footnote.content, location);
                    }
                    InlineMacro::Url(url) => {
                        url.location = map_loc(&url.location);
                    }
                    InlineMacro::Link(link) => {
                        link.location = map_loc(&link.location);
                    }
                    InlineMacro::Icon(icon) => {
                        icon.location = map_loc(&icon.location);
                    }
                    InlineMacro::Button(button) => {
                        button.location = map_loc(&button.location);
                    }
                    InlineMacro::Image(image) => {
                        image.location = map_loc(&image.location);
                    }
                    InlineMacro::Menu(menu) => {
                        menu.location = map_loc(&menu.location);
                    }
                    InlineMacro::Keyboard(keyboard) => {
                        keyboard.location = map_loc(&keyboard.location);
                    }
                    InlineMacro::CrossReference(xref) => {
                        xref.location = map_loc(&xref.location);
                    }
                    InlineMacro::Autolink(autolink) => {
                        autolink.location = map_loc(&autolink.location);
                    }
                    InlineMacro::Stem(stem) => {
                        stem.location = map_loc(&stem.location);
                    }
                    InlineMacro::Pass(pass) => {
                        pass.location = map_loc(&pass.location);
                    }
                }
                vec![InlineNode::Macro(mapped_macro)]
            }
            other => vec![other.clone()],
        })
        .collect::<Vec<_>>()
}
