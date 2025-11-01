use crate::{InlineNode, Location, Pass, Plain, ProcessedContent, Substitution};

use super::{
    ParserState,
    location_mapping::remap_inline_node_location,
    markup_patterns::{
        MarkupMatch, find_constrained_bold_pattern, find_curved_apostrophe_pattern,
        find_curved_quotation_pattern, find_highlight_constrained_pattern,
        find_highlight_unconstrained_pattern, find_italic_pattern,
        find_monospace_constrained_pattern, find_monospace_unconstrained_pattern,
        find_subscript_pattern, find_superscript_pattern, find_unconstrained_bold_pattern,
        find_unconstrained_italic_pattern,
    },
};
use crate::{
    Bold, CurvedApostrophe, CurvedQuotation, Form, Highlight, Italic, Monospace, Subscript,
    Superscript,
};

/// Process passthrough content that contains quote substitutions, parsing nested markup
pub(crate) fn process_passthrough_with_quotes(
    content: &str,
    passthrough: &Pass,
    state: &ParserState,
) -> Vec<InlineNode> {
    // Only process if this passthrough has quote substitutions
    if !passthrough.substitutions.contains(&Substitution::Quotes) {
        // No quote processing needed, return as plain text
        return vec![InlineNode::PlainText(Plain {
            content: content.to_string(),
            location: passthrough.location.clone(),
        })];
    }

    tracing::debug!(content = ?content, "Parsing passthrough content with quotes");

    // Manual parsing for bold and italic patterns in passthrough content
    // This is a simpler approach than trying to use the full PEG parser
    parse_inline_markup_in_passthrough(content, passthrough, state)
}

/// Parse inline markup (bold, italic) within passthrough content manually
#[allow(clippy::too_many_lines)]
pub(crate) fn parse_inline_markup_in_passthrough(
    content: &str,
    _passthrough: &Pass,
    _state: &ParserState,
) -> Vec<InlineNode> {
    let mut result = Vec::new();
    let mut remaining = content;
    let mut current_offset = 0;

    // Helper function to create location - this will be properly set during the final location mapping phase
    // For now, we use relative positions within the passthrough content
    let create_location = |start: usize, end: usize| -> Location {
        Location {
            absolute_start: start,
            absolute_end: end,
            start: crate::Position {
                line: 1,
                column: start + 1,
            },
            end: crate::Position {
                line: 1,
                column: end + 1,
            },
        }
    };

    while !remaining.is_empty() {
        // Find the earliest pattern in the remaining text
        let mut earliest_pattern: Option<(MarkupMatch, &str)> = None;

        // Check all pattern types and find the one that starts earliest
        if let Some(markup_match) = find_unconstrained_bold_pattern(remaining)
            && earliest_pattern
                .as_ref()
                .is_none_or(|(earliest, _)| markup_match.start < earliest.start)
        {
            earliest_pattern = Some((markup_match, "unconstrained_bold"));
        }
        if let Some(markup_match) = find_unconstrained_italic_pattern(remaining)
            && earliest_pattern
                .as_ref()
                .is_none_or(|(earliest, _)| markup_match.start < earliest.start)
        {
            earliest_pattern = Some((markup_match, "unconstrained_italic"));
        }
        if let Some(markup_match) = find_constrained_bold_pattern(remaining)
            && earliest_pattern
                .as_ref()
                .is_none_or(|(earliest, _)| markup_match.start < earliest.start)
        {
            earliest_pattern = Some((markup_match, "constrained_bold"));
        }
        if let Some(markup_match) = find_italic_pattern(remaining)
            && earliest_pattern
                .as_ref()
                .is_none_or(|(earliest, _)| markup_match.start < earliest.start)
        {
            earliest_pattern = Some((markup_match, "italic"));
        }
        if let Some(markup_match) = find_superscript_pattern(remaining)
            && earliest_pattern
                .as_ref()
                .is_none_or(|(earliest, _)| markup_match.start < earliest.start)
        {
            earliest_pattern = Some((markup_match, "superscript"));
        }
        if let Some(markup_match) = find_subscript_pattern(remaining)
            && earliest_pattern
                .as_ref()
                .is_none_or(|(earliest, _)| markup_match.start < earliest.start)
        {
            earliest_pattern = Some((markup_match, "subscript"));
        }
        // Check curved quotes BEFORE monospace patterns since they start with backticks
        if let Some(markup_match) = find_curved_quotation_pattern(remaining)
            && earliest_pattern
                .as_ref()
                .is_none_or(|(earliest, _)| markup_match.start <= earliest.start)
        {
            earliest_pattern = Some((markup_match, "curved_quotation"));
        }
        if let Some(markup_match) = find_curved_apostrophe_pattern(remaining)
            && earliest_pattern
                .as_ref()
                .is_none_or(|(earliest, _)| markup_match.start <= earliest.start)
        {
            earliest_pattern = Some((markup_match, "curved_apostrophe"));
        }
        if let Some(markup_match) = find_monospace_unconstrained_pattern(remaining)
            && earliest_pattern
                .as_ref()
                .is_none_or(|(earliest, _)| markup_match.start < earliest.start)
        {
            earliest_pattern = Some((markup_match, "monospace_unconstrained"));
        }
        if let Some(markup_match) = find_monospace_constrained_pattern(remaining)
            && earliest_pattern
                .as_ref()
                .is_none_or(|(earliest, _)| markup_match.start < earliest.start)
        {
            earliest_pattern = Some((markup_match, "monospace_constrained"));
        }
        if let Some(markup_match) = find_highlight_unconstrained_pattern(remaining)
            && earliest_pattern
                .as_ref()
                .is_none_or(|(earliest, _)| markup_match.start < earliest.start)
        {
            earliest_pattern = Some((markup_match, "highlight_unconstrained"));
        }
        if let Some(markup_match) = find_highlight_constrained_pattern(remaining)
            && earliest_pattern
                .as_ref()
                .is_none_or(|(earliest, _)| markup_match.start < earliest.start)
        {
            earliest_pattern = Some((markup_match, "highlight_constrained"));
        }

        if let Some((markup_match, pattern_type)) = earliest_pattern {
            // Add any content before the markup as plain text
            if markup_match.start > 0 {
                let before_content = &remaining[..markup_match.start];
                result.push(InlineNode::PlainText(Plain {
                    content: before_content.to_string(),
                    location: create_location(
                        current_offset,
                        current_offset + before_content.len(),
                    ),
                }));
                current_offset += before_content.len();
            }

            // Add the appropriate markup node based on pattern type
            match pattern_type {
                "unconstrained_bold" => {
                    result.push(InlineNode::BoldText(Bold {
                        content: vec![InlineNode::PlainText(Plain {
                            content: markup_match.content.clone(),
                            location: create_location(
                                current_offset + 2,
                                current_offset + 2 + markup_match.content.len(),
                            ),
                        })],
                        form: Form::Unconstrained,
                        role: None,
                        id: None,
                        location: create_location(
                            current_offset,
                            current_offset + markup_match.end - markup_match.start,
                        ),
                    }));
                }
                "unconstrained_italic" => {
                    result.push(InlineNode::ItalicText(Italic {
                        content: vec![InlineNode::PlainText(Plain {
                            content: markup_match.content.clone(),
                            location: create_location(
                                current_offset + 2,
                                current_offset + 2 + markup_match.content.len(),
                            ),
                        })],
                        form: Form::Unconstrained,
                        role: None,
                        id: None,
                        location: create_location(
                            current_offset,
                            current_offset + markup_match.end - markup_match.start,
                        ),
                    }));
                }
                "constrained_bold" => {
                    result.push(InlineNode::BoldText(Bold {
                        content: vec![InlineNode::PlainText(Plain {
                            content: markup_match.content.clone(),
                            location: create_location(
                                current_offset + 1,
                                current_offset + 1 + markup_match.content.len(),
                            ),
                        })],
                        form: Form::Constrained,
                        role: None,
                        id: None,
                        location: create_location(
                            current_offset,
                            current_offset + markup_match.end - markup_match.start,
                        ),
                    }));
                }
                "italic" => {
                    result.push(InlineNode::ItalicText(Italic {
                        content: vec![InlineNode::PlainText(Plain {
                            content: markup_match.content.clone(),
                            location: create_location(
                                current_offset + 1,
                                current_offset + 1 + markup_match.content.len(),
                            ),
                        })],
                        form: Form::Constrained,
                        role: None,
                        id: None,
                        location: create_location(
                            current_offset,
                            current_offset + markup_match.end - markup_match.start,
                        ),
                    }));
                }
                "superscript" => {
                    result.push(InlineNode::SuperscriptText(Superscript {
                        content: vec![InlineNode::PlainText(Plain {
                            content: markup_match.content.clone(),
                            location: create_location(
                                current_offset + 1,
                                current_offset + 1 + markup_match.content.len(),
                            ),
                        })],
                        form: Form::Unconstrained,
                        role: None,
                        id: None,
                        location: create_location(
                            current_offset,
                            current_offset + markup_match.end - markup_match.start,
                        ),
                    }));
                }
                "subscript" => {
                    result.push(InlineNode::SubscriptText(Subscript {
                        content: vec![InlineNode::PlainText(Plain {
                            content: markup_match.content.clone(),
                            location: create_location(
                                current_offset + 1,
                                current_offset + 1 + markup_match.content.len(),
                            ),
                        })],
                        form: Form::Unconstrained,
                        role: None,
                        id: None,
                        location: create_location(
                            current_offset,
                            current_offset + markup_match.end - markup_match.start,
                        ),
                    }));
                }
                "curved_quotation" => {
                    result.push(InlineNode::CurvedQuotationText(CurvedQuotation {
                        content: vec![InlineNode::PlainText(Plain {
                            content: markup_match.content.clone(),
                            location: create_location(
                                current_offset + 2,
                                current_offset + 2 + markup_match.content.len(),
                            ),
                        })],
                        form: Form::Unconstrained,
                        role: None,
                        id: None,
                        location: create_location(
                            current_offset,
                            current_offset + markup_match.end - markup_match.start,
                        ),
                    }));
                }
                "curved_apostrophe" => {
                    result.push(InlineNode::CurvedApostropheText(CurvedApostrophe {
                        content: vec![InlineNode::PlainText(Plain {
                            content: markup_match.content.clone(),
                            location: create_location(
                                current_offset + 2,
                                current_offset + 2 + markup_match.content.len(),
                            ),
                        })],
                        form: Form::Unconstrained,
                        role: None,
                        id: None,
                        location: create_location(
                            current_offset,
                            current_offset + markup_match.end - markup_match.start,
                        ),
                    }));
                }
                "monospace_unconstrained" => {
                    result.push(InlineNode::MonospaceText(Monospace {
                        content: vec![InlineNode::PlainText(Plain {
                            content: markup_match.content.clone(),
                            location: create_location(
                                current_offset + 2,
                                current_offset + 2 + markup_match.content.len(),
                            ),
                        })],
                        form: Form::Unconstrained,
                        role: None,
                        id: None,
                        location: create_location(
                            current_offset,
                            current_offset + markup_match.end - markup_match.start,
                        ),
                    }));
                }
                "monospace_constrained" => {
                    result.push(InlineNode::MonospaceText(Monospace {
                        content: vec![InlineNode::PlainText(Plain {
                            content: markup_match.content.clone(),
                            location: create_location(
                                current_offset + 1,
                                current_offset + 1 + markup_match.content.len(),
                            ),
                        })],
                        form: Form::Constrained,
                        role: None,
                        id: None,
                        location: create_location(
                            current_offset,
                            current_offset + markup_match.end - markup_match.start,
                        ),
                    }));
                }
                "highlight_unconstrained" => {
                    result.push(InlineNode::HighlightText(Highlight {
                        content: vec![InlineNode::PlainText(Plain {
                            content: markup_match.content.clone(),
                            location: create_location(
                                current_offset + 2,
                                current_offset + 2 + markup_match.content.len(),
                            ),
                        })],
                        form: Form::Unconstrained,
                        role: None,
                        id: None,
                        location: create_location(
                            current_offset,
                            current_offset + markup_match.end - markup_match.start,
                        ),
                    }));
                }
                "highlight_constrained" => {
                    result.push(InlineNode::HighlightText(Highlight {
                        content: vec![InlineNode::PlainText(Plain {
                            content: markup_match.content.clone(),
                            location: create_location(
                                current_offset + 1,
                                current_offset + 1 + markup_match.content.len(),
                            ),
                        })],
                        form: Form::Constrained,
                        role: None,
                        id: None,
                        location: create_location(
                            current_offset,
                            current_offset + markup_match.end - markup_match.start,
                        ),
                    }));
                }
                _ => {
                    // This shouldn't happen but handle it gracefully
                    result.push(InlineNode::PlainText(Plain {
                        content: markup_match.content.clone(),
                        location: create_location(
                            current_offset + markup_match.start,
                            current_offset + markup_match.end,
                        ),
                    }));
                }
            }

            // Move past the markup pattern (markup_match.end is exclusive)
            remaining = &remaining[markup_match.end..];
            current_offset += markup_match.end - markup_match.start;
        } else {
            // No patterns found, add remaining content as plain text and exit
            if !remaining.is_empty() {
                if let Some(InlineNode::PlainText(last_plain)) = result.last_mut() {
                    // Merge with the last plain text node if it exists
                    last_plain.content.push_str(remaining);
                    last_plain.location.absolute_end = current_offset + remaining.len();
                    last_plain.location.end.column = current_offset + remaining.len() + 1;
                } else {
                    // Create a new plain text node
                    result.push(InlineNode::PlainText(Plain {
                        content: remaining.to_string(),
                        location: create_location(current_offset, current_offset + remaining.len()),
                    }));
                }
            }
            break;
        }
    }

    result
}

/// Process passthrough placeholders in content, returning expanded `InlineNode`s.
///
/// This function handles the multi-pass parsing needed for passthroughs with quote substitutions.
/// It splits the content around placeholders and processes each passthrough according to its
/// substitution settings.
pub(crate) fn process_passthrough_placeholders(
    content: &str,
    processed: &ProcessedContent,
    state: &ParserState,
    base_location: &Location,
) -> Vec<InlineNode> {
    let mut result = Vec::new();
    let mut remaining = content;
    let mut processed_offset = 0; // Position in the processed content (with placeholders)

    // Process each passthrough placeholder in order
    for (index, passthrough) in processed.passthroughs.iter().enumerate() {
        let placeholder = format!("���{index}���");

        if let Some(placeholder_pos) = remaining.find(&placeholder) {
            let before_content = if placeholder_pos > 0 {
                Some(&remaining[..placeholder_pos])
            } else {
                None
            };

            // Add content before the placeholder if any, using original string positions
            if let Some(before) = before_content
                && !before.is_empty()
            {
                result.push(InlineNode::PlainText(Plain {
                    content: before.to_string(),
                    location: Location {
                        // Use original string positions
                        absolute_start: base_location.absolute_start + processed_offset,
                        absolute_end: base_location.absolute_start
                            + processed_offset
                            + before.len(),
                        start: crate::Position {
                            line: base_location.start.line,
                            column: base_location.start.column + processed_offset,
                        },
                        end: crate::Position {
                            line: base_location.start.line,
                            column: base_location.start.column + processed_offset + before.len(),
                        },
                    },
                }));
                processed_offset += before.len();
            }

            // Process the passthrough content using original string positions from passthrough.location
            if let Some(passthrough_content) = &passthrough.text {
                let processed_nodes =
                    process_passthrough_with_quotes(passthrough_content, passthrough, state);

                // Remap locations of processed nodes to use original string coordinates
                // The passthrough content starts after "pass:q[" so we need to account for that offset
                let macro_prefix_len = "pass:q[".len(); // 7 characters
                for mut node in processed_nodes {
                    remap_inline_node_location(
                        &mut node,
                        passthrough.location.absolute_start + macro_prefix_len,
                    );
                    result.push(node);
                }
            }

            // Move past the placeholder in the processed content
            let skip_len = placeholder_pos + placeholder.len();
            remaining = &remaining[skip_len..];
            // Update processed_offset to account for the original passthrough macro length
            processed_offset +=
                passthrough.location.absolute_end - passthrough.location.absolute_start;
        }
    }

    // Add any remaining content as plain text
    if !remaining.is_empty() {
        // Check if the last node is PlainText and merge if so
        if let Some(InlineNode::PlainText(last_plain)) = result.last_mut() {
            // Merge remaining content with the last plain text node
            last_plain.content.push_str(remaining);
            // Extend the location to include the remaining content
            last_plain.location.absolute_end = base_location.absolute_end;
            last_plain.location.end = base_location.end.clone();
        } else {
            // Add as separate node if last node is not plain text
            result.push(InlineNode::PlainText(Plain {
                content: remaining.to_string(),
                location: Location {
                    absolute_start: base_location.absolute_start + processed_offset,
                    absolute_end: base_location.absolute_end,
                    start: crate::Position {
                        line: base_location.start.line,
                        column: base_location.start.column + processed_offset,
                    },
                    end: base_location.end.clone(),
                },
            }));
        }
    }

    // If no placeholders were found, return the original content as plain text
    if result.is_empty() {
        result.push(InlineNode::PlainText(Plain {
            content: content.to_string(),
            location: base_location.clone(),
        }));
    }

    // Merge adjacent plain text nodes
    merge_adjacent_plain_text_nodes(result)
}

/// Merge adjacent plain text nodes into single nodes to simplify the output
pub(crate) fn merge_adjacent_plain_text_nodes(nodes: Vec<InlineNode>) -> Vec<InlineNode> {
    let mut result = Vec::new();

    for node in nodes {
        match (result.last_mut(), node) {
            (Some(InlineNode::PlainText(last_plain)), InlineNode::PlainText(current_plain)) => {
                // Merge current plain text with the last one
                last_plain.content.push_str(&current_plain.content);
                // Extend the location to cover both nodes
                last_plain.location.absolute_end = current_plain.location.absolute_end;
                last_plain.location.end = current_plain.location.end;
            }
            (_, node) => {
                // Not adjacent plain text nodes, add as separate node
                result.push(node);
            }
        }
    }

    result
}

pub(crate) fn replace_passthrough_placeholders(
    content: &str,
    processed: &ProcessedContent,
) -> String {
    let mut result = content.to_string();

    // Replace each passthrough placeholder with its content
    for (index, passthrough) in processed.passthroughs.iter().enumerate() {
        let placeholder = format!("���{index}���");
        if let Some(text) = &passthrough.text {
            result = result.replace(&placeholder, text);
        }
    }

    result
}
