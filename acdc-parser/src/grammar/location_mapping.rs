#[allow(unused_imports)]
use crate::{
    Bold, CurvedApostrophe, CurvedQuotation, Form, Highlight, InlineNode, Italic, Location,
    Monospace, Plain, ProcessedContent, StandaloneCurvedApostrophe, Subscript, Superscript,
};

use super::{
    ParserState,
    marked_text::WithLocationMappingContext,
    utf8_utils::{self, RoundDirection, snap_to_boundary},
};

/// Offset to skip the opening delimiter in constrained formatting.
/// For `*s*`, position 1 is where 's' starts.
const CONSTRAINED_CONTENT_START_OFFSET: usize = 1;

/// End offset for single character content after opening delimiter.
/// For `*s*`, position 2 is where 's' ends.
const CONSTRAINED_CONTENT_END_OFFSET: usize = 2;

/// Clamp a Location's byte offsets to valid bounds within the input string
/// and ensure they fall on UTF-8 character boundaries.
///
/// This only operates on `absolute_start/end` - the canonical byte offsets.
/// `Position` fields (line/column) are not modified.
pub(crate) fn clamp_location_bounds(location: &mut Location, input: &str) {
    let input_len = input.len();

    // Clamp to input bounds
    location.absolute_start = location.absolute_start.min(input_len);
    location.absolute_end = location.absolute_end.min(input_len);

    // Ensure start is on a valid UTF-8 boundary (round backward)
    location.absolute_start = utf8_utils::ensure_char_boundary(input, location.absolute_start);

    // Ensure end is on a valid UTF-8 boundary (round forward)
    location.absolute_end = utf8_utils::ensure_char_boundary_forward(input, location.absolute_end);

    // Ensure start <= end
    if location.absolute_start > location.absolute_end {
        location.absolute_end = location.absolute_start;
    }
}

/// Recursively clamp all locations in an `InlineNode` to valid bounds
pub(crate) fn clamp_inline_node_locations(node: &mut InlineNode, input: &str) {
    match node {
        InlineNode::PlainText(plain) => clamp_location_bounds(&mut plain.location, input),
        InlineNode::RawText(raw) => clamp_location_bounds(&mut raw.location, input),
        InlineNode::VerbatimText(verbatim) => clamp_location_bounds(&mut verbatim.location, input),
        InlineNode::BoldText(bold) => {
            clamp_location_bounds(&mut bold.location, input);
            for child in &mut bold.content {
                clamp_inline_node_locations(child, input);
            }
        }
        InlineNode::ItalicText(italic) => {
            clamp_location_bounds(&mut italic.location, input);
            for child in &mut italic.content {
                clamp_inline_node_locations(child, input);
            }
        }
        InlineNode::MonospaceText(mono) => {
            clamp_location_bounds(&mut mono.location, input);
            for child in &mut mono.content {
                clamp_inline_node_locations(child, input);
            }
        }
        InlineNode::HighlightText(highlight) => {
            clamp_location_bounds(&mut highlight.location, input);
            for child in &mut highlight.content {
                clamp_inline_node_locations(child, input);
            }
        }
        InlineNode::SubscriptText(sub) => {
            clamp_location_bounds(&mut sub.location, input);
            for child in &mut sub.content {
                clamp_inline_node_locations(child, input);
            }
        }
        InlineNode::SuperscriptText(sup) => {
            clamp_location_bounds(&mut sup.location, input);
            for child in &mut sup.content {
                clamp_inline_node_locations(child, input);
            }
        }
        InlineNode::CurvedQuotationText(cq) => {
            clamp_location_bounds(&mut cq.location, input);
            for child in &mut cq.content {
                clamp_inline_node_locations(child, input);
            }
        }
        InlineNode::CurvedApostropheText(ca) => {
            clamp_location_bounds(&mut ca.location, input);
            for child in &mut ca.content {
                clamp_inline_node_locations(child, input);
            }
        }
        InlineNode::StandaloneCurvedApostrophe(sca) => {
            clamp_location_bounds(&mut sca.location, input);
        }
        InlineNode::LineBreak(lb) => clamp_location_bounds(&mut lb.location, input),
        InlineNode::InlineAnchor(anchor) => clamp_location_bounds(&mut anchor.location, input),
        InlineNode::CalloutRef(callout) => clamp_location_bounds(&mut callout.location, input),
        InlineNode::Macro(m) => match m {
            crate::InlineMacro::Footnote(f) => {
                clamp_location_bounds(&mut f.location, input);
                for child in &mut f.content {
                    clamp_inline_node_locations(child, input);
                }
            }
            crate::InlineMacro::Icon(i) => clamp_location_bounds(&mut i.location, input),
            crate::InlineMacro::Image(img) => clamp_location_bounds(&mut img.location, input),
            crate::InlineMacro::Keyboard(k) => clamp_location_bounds(&mut k.location, input),
            crate::InlineMacro::Button(b) => clamp_location_bounds(&mut b.location, input),
            crate::InlineMacro::Menu(menu) => clamp_location_bounds(&mut menu.location, input),
            crate::InlineMacro::Url(u) => clamp_location_bounds(&mut u.location, input),
            crate::InlineMacro::Link(l) => clamp_location_bounds(&mut l.location, input),
            crate::InlineMacro::Mailto(m) => clamp_location_bounds(&mut m.location, input),
            crate::InlineMacro::Autolink(a) => clamp_location_bounds(&mut a.location, input),
            crate::InlineMacro::CrossReference(x) => {
                clamp_location_bounds(&mut x.location, input);
            }
            crate::InlineMacro::Pass(p) => clamp_location_bounds(&mut p.location, input),
            crate::InlineMacro::Stem(s) => clamp_location_bounds(&mut s.location, input),
            crate::InlineMacro::IndexTerm(it) => clamp_location_bounds(&mut it.location, input),
        },
    }
}

/// Context for location mapping operations
pub(crate) struct LocationMappingContext<'a, 'b> {
    pub state: &'a ParserState<'b>,
    pub processed: &'a ProcessedContent<'b>,
    pub base_location: &'a Location,
}

/// Type alias for location mapping closures
pub(crate) type LocationMapper<'a> = dyn Fn(&Location) -> Result<Location, crate::Error> + 'a;

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
    state: &'a ParserState<'_>,
    processed: &'a ProcessedContent<'_>,
    base_location: &'a Location,
    form: Option<&'a Form>,
) -> Box<LocationMapper<'a>> {
    Box::new(move |loc: &Location| -> Result<Location, crate::Error> {
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
                    // No enclosing formatting context: constrained heuristic never applies
                    false
                };

                if is_constrained_single_char {
                    // Constrained formatting: skip single-char delimiter to point at content
                    processed_abs_start =
                        base_location.absolute_start + CONSTRAINED_CONTENT_START_OFFSET;
                    processed_abs_end =
                        base_location.absolute_start + CONSTRAINED_CONTENT_END_OFFSET;
                } else {
                    // General case: expand collapsed locations to the next UTF-8 boundary
                    processed_abs_end = crate::grammar::utf8_utils::safe_increment_offset(
                        state.input,
                        processed_abs_end,
                    );
                }
            } else {
                // General case: expand collapsed locations to the next UTF-8 boundary
                processed_abs_end = crate::grammar::utf8_utils::safe_increment_offset(
                    state.input,
                    processed_abs_end,
                );
            }
        }

        // Map those through the preprocessor source map back to original source
        let mut mapped_abs_start = processed.source_map.map_position(processed_abs_start)?;
        let mut mapped_abs_end = processed.source_map.map_position(processed_abs_end)?;

        // Clamp to input bounds - preprocessor expansion can produce positions beyond input length
        let input_len = state.input.len();
        mapped_abs_start = mapped_abs_start.min(input_len);
        mapped_abs_end = mapped_abs_end.min(input_len);

        // Ensure mapped positions are on valid UTF-8 boundaries
        mapped_abs_start =
            snap_to_boundary(state.input, mapped_abs_start, RoundDirection::Backward);
        mapped_abs_end = snap_to_boundary(state.input, mapped_abs_end, RoundDirection::Forward);

        // Compute human positions from the document's line map
        let start_pos = state
            .line_map
            .offset_to_position(mapped_abs_start, state.input);
        let mut end_pos = state
            .line_map
            .offset_to_position(mapped_abs_end, state.input);

        // For single-character content inside constrained formatting, ensure both start and end column point to the same character
        let is_single_char_fix = mapped_abs_end == mapped_abs_start + 1
            && loc.absolute_start == 0
            && base_location.absolute_start < base_location.absolute_end;
        if is_single_char_fix {
            // Check if this is constrained formatting (which has single-character delimiters)
            let is_constrained_single_char = if let Some(form) = form {
                matches!(form, Form::Constrained)
            } else {
                // No enclosing formatting context: constrained heuristic never applies
                false
            };

            if is_constrained_single_char {
                end_pos.column = start_pos.column;
            }
        }

        Ok(Location {
            absolute_start: mapped_abs_start,
            absolute_end: mapped_abs_end,
            start: start_pos,
            end: end_pos,
        })
    })
}

/// Apply attribute substitution location extension if needed.
///
/// When attribute substitutions collapse locations to a single point (e.g., `{attr}` -> `value`),
/// we need to extend the location back to cover the original attribute span for better UX.
pub(crate) fn extend_attribute_location_if_needed(
    state: &ParserState<'_>,
    processed: &ProcessedContent<'_>,
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
                .offset_to_position(attr_replacement.absolute_start, state.input);
            let end_pos = state
                .line_map
                .offset_to_position(attr_replacement.absolute_end, state.input);
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
pub(crate) fn map_inner_content_locations<'a>(
    content: Vec<InlineNode<'a>>,
    map_loc: &LocationMapper<'_>,
    state: &ParserState<'a>,
    processed: &ProcessedContent<'a>,
    base_location: &Location,
) -> Result<Vec<InlineNode<'a>>, crate::Error> {
    content
        .into_iter()
        .map(|node| -> Result<InlineNode, crate::Error> {
            match node {
                InlineNode::PlainText(mut inner_plain) => {
                    // Replace passthrough placeholders in the content
                    let content = super::passthrough_processing::replace_passthrough_placeholders(
                        inner_plain.content,
                        processed,
                    );
                    inner_plain.content = state.intern_str(&content);

                    // Map to document coordinates first (use normal location mapping for inner content)
                    let mut mapped = map_loc(&inner_plain.location)?;

                    // For single-character content, ensure start and end columns are the same
                    if inner_plain.content.chars().count() == 1 {
                        mapped.end.column = mapped.start.column;
                    }

                    // Apply attribute location extension if needed
                    inner_plain.location =
                        extend_attribute_location_if_needed(state, processed, mapped);
                    Ok(InlineNode::PlainText(inner_plain))
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
                other @ (InlineNode::RawText(_)
                | InlineNode::VerbatimText(_)
                | InlineNode::StandaloneCurvedApostrophe(_)
                | InlineNode::LineBreak(_)
                | InlineNode::InlineAnchor(_)
                | InlineNode::CalloutRef(_)
                | InlineNode::Macro(_)) => Ok(other),
            }
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
        InlineNode::RawText(_)
        | InlineNode::VerbatimText(_)
        | InlineNode::StandaloneCurvedApostrophe(_)
        | InlineNode::LineBreak(_)
        | InlineNode::InlineAnchor(_)
        | InlineNode::CalloutRef(_)
        | InlineNode::Macro(_) => {
            // No location remapping needed for these types
        }
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
    processed: &ProcessedContent<'_>,
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
pub(crate) fn map_inline_locations<'a>(
    state: &ParserState<'a>,
    processed: &'a ProcessedContent<'a>,
    content: &[InlineNode<'a>],
    location: &Location,
) -> Result<Vec<InlineNode<'a>>, crate::Error> {
    tracing::info!(?location, "mapping inline locations");

    let map_loc = create_location_mapper(state, processed, location, None);

    // `PlainText` is the only variant that can expand to multiple output nodes
    // (passthrough placeholder substitution). Every other variant maps 1:1, so
    // sizing `acc` at `content.len()` is a lower bound; growth beyond that is
    // rare and cheap.
    let mut acc: Vec<InlineNode<'a>> = Vec::with_capacity(content.len());
    for inline in content {
        match inline {
            InlineNode::PlainText(plain) => {
                // May expand to N nodes (placeholder splits); push in place.
                let nodes =
                    map_plain_text_inline_locations(plain, state, processed, location, &map_loc)?;
                acc.extend(nodes);
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
                acc.push(
                    marked_text
                        .clone()
                        .with_location_mapping_context(&mapping_ctx)?,
                );
            }
            InlineNode::StandaloneCurvedApostrophe(standalone) => {
                let mut mapped_standalone = standalone.clone();
                mapped_standalone.location = map_loc(&standalone.location)?;
                acc.push(InlineNode::StandaloneCurvedApostrophe(mapped_standalone));
            }
            InlineNode::Macro(inline_macro) => {
                acc.push(map_inline_macro(
                    inline_macro,
                    state,
                    processed,
                    location,
                    &map_loc,
                )?);
            }
            InlineNode::LineBreak(lb) => {
                let mut mapped_lb = lb.clone();
                mapped_lb.location = map_loc(&lb.location)?;
                acc.push(InlineNode::LineBreak(mapped_lb));
            }
            InlineNode::RawText(raw) => {
                let mut mapped = raw.clone();
                mapped.location = map_loc(&raw.location)?;
                acc.push(InlineNode::RawText(mapped));
            }
            InlineNode::VerbatimText(verbatim) => {
                let mut mapped = verbatim.clone();
                mapped.location = map_loc(&verbatim.location)?;
                acc.push(InlineNode::VerbatimText(mapped));
            }
            InlineNode::InlineAnchor(anchor) => {
                let mut mapped = anchor.clone();
                mapped.location = map_loc(&anchor.location)?;
                acc.push(InlineNode::InlineAnchor(mapped));
            }
            InlineNode::CalloutRef(callout) => {
                let mut mapped = callout.clone();
                mapped.location = map_loc(&callout.location)?;
                acc.push(InlineNode::CalloutRef(mapped));
            }
        }
    }
    Ok(acc)
}

fn map_inline_macro<'a>(
    inline_macro: &crate::InlineMacro<'a>,
    state: &ParserState<'a>,
    processed: &'a ProcessedContent<'a>,
    location: &Location,
    map_loc: &LocationMapper<'_>,
) -> Result<InlineNode<'a>, crate::Error> {
    use crate::InlineMacro;
    let mut mapped_macro = inline_macro.clone();
    match &mut mapped_macro {
        InlineMacro::Footnote(footnote) => {
            footnote.location = map_loc(&footnote.location)?;
            footnote.content = map_inline_locations(state, processed, &footnote.content, location)?;
        }
        InlineMacro::Url(url) => {
            url.location = map_loc(&url.location)?;
            url.text = map_inline_locations(state, processed, &url.text, location)?;
        }
        InlineMacro::Mailto(mailto) => {
            mailto.location = map_loc(&mailto.location)?;
            mailto.text = map_inline_locations(state, processed, &mailto.text, location)?;
        }
        InlineMacro::Link(link) => link.location = map_loc(&link.location)?,
        InlineMacro::Icon(icon) => icon.location = map_loc(&icon.location)?,
        InlineMacro::Button(button) => button.location = map_loc(&button.location)?,
        InlineMacro::Image(image) => image.location = map_loc(&image.location)?,
        InlineMacro::Menu(menu) => menu.location = map_loc(&menu.location)?,
        InlineMacro::Keyboard(keyboard) => keyboard.location = map_loc(&keyboard.location)?,
        InlineMacro::CrossReference(xref) => {
            xref.location = map_loc(&xref.location)?;
            xref.text = map_inline_locations(state, processed, &xref.text, location)?;
        }
        InlineMacro::Autolink(autolink) => autolink.location = map_loc(&autolink.location)?,
        InlineMacro::Stem(stem) => stem.location = map_loc(&stem.location)?,
        InlineMacro::Pass(pass) => pass.location = map_loc(&pass.location)?,
        InlineMacro::IndexTerm(index_term) => index_term.location = map_loc(&index_term.location)?,
    }
    Ok(InlineNode::Macro(mapped_macro))
}

fn map_plain_text_inline_locations<'a>(
    plain: &Plain<'a>,
    state: &ParserState<'a>,
    processed: &'a ProcessedContent<'a>,
    location: &Location,
    map_loc: &LocationMapper<'_>,
) -> Result<Vec<InlineNode<'a>>, crate::Error> {
    // Extract plain text at `'a` so the passthrough-processing path can hand
    // back `InlineNode<'a>`. The content is already arena-allocated `&'a str`.
    let original_content: &'a str = plain.content;

    // Check if this PlainText contains passthrough placeholders
    let contains_passthroughs = !processed.passthroughs.is_empty()
        && processed.passthroughs.iter().enumerate().any(|(index, _)| {
            let placeholder = format!("���{index}���");
            original_content.contains(&placeholder)
        });

    if contains_passthroughs {
        // Use multi-pass processing for passthroughs with quote substitutions
        let base_location =
            create_original_source_location(original_content, &plain.location, processed, location);

        tracing::debug!(content = ?original_content, "Processing passthrough placeholders in PlainText");
        Ok(
            super::passthrough_processing::process_passthrough_placeholders(
                original_content,
                processed,
                state,
                &base_location,
            ),
        )
    } else {
        // No passthroughs, handle normally
        let mut mapped_location = map_loc(&plain.location)?;

        // For single-character content, ensure start and end columns are the same
        if original_content.chars().count() == 1 {
            mapped_location.end.column = mapped_location.start.column;
        }

        Ok(vec![InlineNode::PlainText(Plain {
            content: original_content,
            location: mapped_location,
            escaped: plain.escaped,
        })])
    }
}

#[cfg(test)]
mod tests {
    use crate::{Block, Options, parse};

    fn check_monotonic(input: &str) -> Result<(), String> {
        let parsed = parse(input, &Options::default()).map_err(|e| format!("parse failed: {e}"))?;
        let first_block = parsed
            .document()
            .blocks
            .first()
            .ok_or("expected at least one block")?;
        let Block::Paragraph(para) = first_block else {
            return Err(format!("Expected paragraph, got {first_block:?}"));
        };

        assert!(para.content.len() >= 2, "Expected at least 2 inline nodes");

        let first = para
            .content
            .first()
            .ok_or("missing first inline")?
            .location();
        let second = para
            .content
            .get(1)
            .ok_or("missing second inline")?
            .location();
        assert!(
            second.absolute_start >= first.absolute_end,
            "Non-monotonic: first ends at {}, second starts at {}",
            first.absolute_end,
            second.absolute_start,
        );
        Ok(())
    }

    #[test]
    fn subscript_after_single_char_has_monotonic_positions() -> Result<(), String> {
        check_monotonic("?~sub~")
    }

    #[test]
    fn superscript_after_single_char_has_monotonic_positions() -> Result<(), String> {
        check_monotonic("?^sup^")
    }

    #[test]
    fn short_text_before_subscript_no_constrained_heuristic() -> Result<(), String> {
        check_monotonic("a~sub~")
    }

    #[test]
    fn longer_text_before_subscript_unaffected() -> Result<(), String> {
        check_monotonic("ab~sub~")
    }

    #[test]
    fn subscript_before_single_char_has_monotonic_positions() -> Result<(), String> {
        check_monotonic("~sub~?")
    }

    #[test]
    fn superscript_before_single_char_has_monotonic_positions() -> Result<(), String> {
        check_monotonic("^sup^?")
    }
}
