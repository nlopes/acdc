use crate::{InlineNode, Location, Pass, Plain, ProcessedContent, Raw, Substitution};

use super::{
    ParserState,
    location_mapping::{clamp_inline_node_locations, remap_inline_node_location},
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

/// Markup type for passthrough inline content parsing.
#[derive(Debug, Clone, Copy)]
enum MarkupType {
    UnconstrainedBold,
    UnconstrainedItalic,
    ConstrainedBold,
    ConstrainedItalic,
    Superscript,
    Subscript,
    CurvedQuotation,
    CurvedApostrophe,
    UnconstrainedMonospace,
    ConstrainedMonospace,
    UnconstrainedHighlight,
    ConstrainedHighlight,
}

impl MarkupType {
    /// Returns the delimiter length for this markup type.
    const fn delimiter_len(self) -> usize {
        match self {
            Self::UnconstrainedBold
            | Self::UnconstrainedItalic
            | Self::CurvedQuotation
            | Self::CurvedApostrophe
            | Self::UnconstrainedMonospace
            | Self::UnconstrainedHighlight => 2,
            Self::ConstrainedBold
            | Self::ConstrainedItalic
            | Self::Superscript
            | Self::Subscript
            | Self::ConstrainedMonospace
            | Self::ConstrainedHighlight => 1,
        }
    }

    /// Returns the Form for this markup type.
    const fn form(self) -> Form {
        match self {
            Self::UnconstrainedBold
            | Self::UnconstrainedItalic
            | Self::Superscript
            | Self::Subscript
            | Self::CurvedQuotation
            | Self::CurvedApostrophe
            | Self::UnconstrainedMonospace
            | Self::UnconstrainedHighlight => Form::Unconstrained,
            Self::ConstrainedBold
            | Self::ConstrainedItalic
            | Self::ConstrainedMonospace
            | Self::ConstrainedHighlight => Form::Constrained,
        }
    }

    /// Whether this pattern uses <= priority (curved quotes take precedence at same position).
    const fn uses_lte_priority(self) -> bool {
        matches!(self, Self::CurvedQuotation | Self::CurvedApostrophe)
    }

    /// Find this pattern in the input.
    fn find(self, input: &str) -> Option<MarkupMatch> {
        match self {
            Self::UnconstrainedBold => find_unconstrained_bold_pattern(input),
            Self::UnconstrainedItalic => find_unconstrained_italic_pattern(input),
            Self::ConstrainedBold => find_constrained_bold_pattern(input),
            Self::ConstrainedItalic => find_italic_pattern(input),
            Self::Superscript => find_superscript_pattern(input),
            Self::Subscript => find_subscript_pattern(input),
            Self::CurvedQuotation => find_curved_quotation_pattern(input),
            Self::CurvedApostrophe => find_curved_apostrophe_pattern(input),
            Self::UnconstrainedMonospace => find_monospace_unconstrained_pattern(input),
            Self::ConstrainedMonospace => find_monospace_constrained_pattern(input),
            Self::UnconstrainedHighlight => find_highlight_unconstrained_pattern(input),
            Self::ConstrainedHighlight => find_highlight_constrained_pattern(input),
        }
    }

    /// Create an `InlineNode` for this markup type.
    fn create_node(self, inner_content: InlineNode, outer_location: Location) -> InlineNode {
        let form = self.form();
        match self {
            Self::UnconstrainedBold | Self::ConstrainedBold => InlineNode::BoldText(Bold {
                content: vec![inner_content],
                form,
                role: None,
                id: None,
                location: outer_location,
            }),
            Self::UnconstrainedItalic | Self::ConstrainedItalic => InlineNode::ItalicText(Italic {
                content: vec![inner_content],
                form,
                role: None,
                id: None,
                location: outer_location,
            }),
            Self::Superscript => InlineNode::SuperscriptText(Superscript {
                content: vec![inner_content],
                form,
                role: None,
                id: None,
                location: outer_location,
            }),
            Self::Subscript => InlineNode::SubscriptText(Subscript {
                content: vec![inner_content],
                form,
                role: None,
                id: None,
                location: outer_location,
            }),
            Self::CurvedQuotation => InlineNode::CurvedQuotationText(CurvedQuotation {
                content: vec![inner_content],
                form,
                role: None,
                id: None,
                location: outer_location,
            }),
            Self::CurvedApostrophe => InlineNode::CurvedApostropheText(CurvedApostrophe {
                content: vec![inner_content],
                form,
                role: None,
                id: None,
                location: outer_location,
            }),
            Self::UnconstrainedMonospace | Self::ConstrainedMonospace => {
                InlineNode::MonospaceText(Monospace {
                    content: vec![inner_content],
                    form,
                    role: None,
                    id: None,
                    location: outer_location,
                })
            }
            Self::UnconstrainedHighlight | Self::ConstrainedHighlight => {
                InlineNode::HighlightText(Highlight {
                    content: vec![inner_content],
                    form,
                    role: None,
                    id: None,
                    location: outer_location,
                })
            }
        }
    }
}

/// All markup types to check, in priority order.
const MARKUP_TYPES: &[MarkupType] = &[
    MarkupType::UnconstrainedBold,
    MarkupType::UnconstrainedItalic,
    MarkupType::ConstrainedBold,
    MarkupType::ConstrainedItalic,
    MarkupType::Superscript,
    MarkupType::Subscript,
    // Curved quotes checked before monospace since they start with backticks
    MarkupType::CurvedQuotation,
    MarkupType::CurvedApostrophe,
    MarkupType::UnconstrainedMonospace,
    MarkupType::ConstrainedMonospace,
    MarkupType::UnconstrainedHighlight,
    MarkupType::ConstrainedHighlight,
];

/// Process passthrough content that contains quote substitutions, parsing nested markup
pub(crate) fn process_passthrough_with_quotes(
    content: &str,
    passthrough: &Pass,
) -> Vec<InlineNode> {
    let has_quotes = passthrough.substitutions.contains(&Substitution::Quotes);

    // If no quotes processing needed
    if !has_quotes {
        // If SpecialChars substitution is enabled, escape HTML (return PlainText)
        // This applies to: +text+ (Single), ++text++ (Double), pass:c[] (Macro with SpecialChars)
        // Otherwise output raw HTML (return RawText)
        // This applies to: +++text+++ (Triple), pass:[] (Macro without SpecialChars)
        // Use RawText for all passthroughs without Quotes to avoid merging with
        // adjacent PlainText nodes (which would lose the passthrough's substitution info).
        // Carry the passthrough's own subs (minus Quotes, already handled) so the
        // converter applies exactly those instead of the block's subs.
        return vec![InlineNode::RawText(Raw {
            content: content.to_string(),
            location: passthrough.location.clone(),
            subs: passthrough
                .substitutions
                .iter()
                .filter(|s| **s != Substitution::Quotes)
                .cloned()
                .collect(),
        })];
    }

    tracing::debug!(content = ?content, "Parsing passthrough content with quotes");

    // Manual parsing for bold and italic patterns in passthrough content
    // This is a simpler approach than trying to use the full PEG parser
    parse_text_for_quotes(content)
}

/// Parse text for inline formatting markup (bold, italic, monospace, etc.).
///
/// This function scans the input text for `AsciiDoc` formatting patterns and returns
/// a vector of `InlineNode`s representing the parsed content. Used for applying
/// "quotes" substitution to verbatim block content.
///
/// # Supported Patterns
///
/// - `*bold*` and `**bold**` (constrained/unconstrained)
/// - `_italic_` and `__italic__`
/// - `` `monospace` `` and ``` ``monospace`` ```
/// - `^superscript^` and `~subscript~`
/// - `#highlight#` and `##highlight##`
/// - `` "`curved quotes`" `` and `` '`curved apostrophe`' ``
///
/// # Example
///
/// ```
/// use acdc_parser::parse_text_for_quotes;
///
/// let nodes = parse_text_for_quotes("This has *bold* text.");
/// assert_eq!(nodes.len(), 3); // "This has ", Bold("bold"), " text."
/// ```
#[must_use]
pub fn parse_text_for_quotes(content: &str) -> Vec<InlineNode> {
    let mut result = Vec::new();
    let mut remaining = content;
    let mut current_offset = 0;

    while !remaining.is_empty() {
        // Find the earliest pattern in the remaining text
        let earliest = find_earliest_pattern(remaining);

        if let Some((markup_match, markup_type)) = earliest {
            // Add any content before the markup as plain text
            if markup_match.start > 0 {
                let before_content = &remaining[..markup_match.start];
                result.push(InlineNode::PlainText(Plain {
                    content: before_content.to_string(),
                    location: create_relative_location(
                        current_offset,
                        current_offset + before_content.len(),
                    ),
                    escaped: false,
                }));
                current_offset += before_content.len();
            }

            // Create inner content location
            let delim_len = markup_type.delimiter_len();
            let inner_location = create_relative_location(
                current_offset + delim_len,
                current_offset + delim_len + markup_match.content.len(),
            );
            let inner_content = InlineNode::PlainText(Plain {
                content: markup_match.content.clone(),
                location: inner_location,
                escaped: false,
            });

            // Create outer location
            let outer_location = create_relative_location(
                current_offset,
                current_offset + markup_match.end - markup_match.start,
            );

            // Create the appropriate node
            result.push(markup_type.create_node(inner_content, outer_location));

            // Move past the markup pattern
            remaining = &remaining[markup_match.end..];
            current_offset += markup_match.end - markup_match.start;
        } else {
            // No patterns found, add remaining content as plain text and exit
            if !remaining.is_empty() {
                if let Some(InlineNode::PlainText(last_plain)) = result.last_mut() {
                    // Merge with the last plain text node
                    last_plain.content.push_str(remaining);
                    last_plain.location.absolute_end = current_offset + remaining.len();
                    last_plain.location.end.column = current_offset + remaining.len() + 1;
                } else {
                    result.push(InlineNode::PlainText(Plain {
                        content: remaining.to_string(),
                        location: create_relative_location(
                            current_offset,
                            current_offset + remaining.len(),
                        ),
                        escaped: false,
                    }));
                }
            }
            break;
        }
    }

    result
}

/// Find the earliest matching pattern in the input.
fn find_earliest_pattern(input: &str) -> Option<(MarkupMatch, MarkupType)> {
    let mut earliest: Option<(MarkupMatch, MarkupType)> = None;

    for &markup_type in MARKUP_TYPES {
        if let Some(markup_match) = markup_type.find(input) {
            let dominated = earliest.as_ref().is_some_and(|(e, _)| {
                if markup_type.uses_lte_priority() {
                    markup_match.start > e.start
                } else {
                    markup_match.start >= e.start
                }
            });

            if !dominated {
                earliest = Some((markup_match, markup_type));
            }
        }
    }

    earliest
}

/// Create a location for relative positions within passthrough content.
/// These positions will be remapped later during final location mapping.
fn create_relative_location(start: usize, end: usize) -> Location {
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
                    escaped: false,
                }));
                processed_offset += before.len();
            }

            // Process the passthrough content using original string positions from passthrough.location
            if let Some(passthrough_content) = &passthrough.text {
                let processed_nodes =
                    process_passthrough_with_quotes(passthrough_content, passthrough);

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
                escaped: false,
            }));
        }
    }

    // If no placeholders were found, return the original content as plain text
    if result.is_empty() {
        result.push(InlineNode::PlainText(Plain {
            content: content.to_string(),
            location: base_location.clone(),
            escaped: false,
        }));
    }

    // Clamp all locations to valid bounds within the input string
    for node in &mut result {
        clamp_inline_node_locations(node, &state.input);
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

#[cfg(test)]
#[allow(clippy::indexing_slicing)] // Tests verify length before indexing
mod tests {
    use super::*;

    // === Divergence Prevention Tests ===
    //
    // These tests verify that parse_text_for_quotes produces the same structural
    // output as the main PEG parser for common inline formatting patterns.
    // If these tests fail after grammar changes, update parse_text_for_quotes.

    #[test]
    fn test_constrained_bold_pattern() {
        let nodes = parse_text_for_quotes("This is *bold* text.");
        assert_eq!(nodes.len(), 3);
        assert!(matches!(nodes[0], InlineNode::PlainText(_)));
        assert!(
            matches!(&nodes[1], InlineNode::BoldText(b) if matches!(b.content.first(), Some(InlineNode::PlainText(p)) if p.content == "bold"))
        );
        assert!(matches!(nodes[2], InlineNode::PlainText(_)));
    }

    #[test]
    fn test_unconstrained_bold_pattern() {
        let nodes = parse_text_for_quotes("This**bold**word");
        assert_eq!(nodes.len(), 3);
        assert!(
            matches!(&nodes[1], InlineNode::BoldText(b) if matches!(b.content.first(), Some(InlineNode::PlainText(p)) if p.content == "bold"))
        );
    }

    #[test]
    fn test_constrained_italic_pattern() {
        let nodes = parse_text_for_quotes("This is _italic_ text.");
        assert_eq!(nodes.len(), 3);
        assert!(
            matches!(&nodes[1], InlineNode::ItalicText(i) if matches!(i.content.first(), Some(InlineNode::PlainText(p)) if p.content == "italic"))
        );
    }

    #[test]
    fn test_unconstrained_italic_pattern() {
        let nodes = parse_text_for_quotes("This__italic__word");
        assert_eq!(nodes.len(), 3);
        assert!(
            matches!(&nodes[1], InlineNode::ItalicText(i) if matches!(i.content.first(), Some(InlineNode::PlainText(p)) if p.content == "italic"))
        );
    }

    #[test]
    fn test_constrained_monospace_pattern() {
        let nodes = parse_text_for_quotes("Use `code` here.");
        assert_eq!(nodes.len(), 3);
        assert!(
            matches!(&nodes[1], InlineNode::MonospaceText(m) if matches!(m.content.first(), Some(InlineNode::PlainText(p)) if p.content == "code"))
        );
    }

    #[test]
    fn test_superscript_pattern() {
        let nodes = parse_text_for_quotes("E=mc^2^");
        assert_eq!(nodes.len(), 2);
        assert!(
            matches!(&nodes[1], InlineNode::SuperscriptText(s) if matches!(s.content.first(), Some(InlineNode::PlainText(p)) if p.content == "2"))
        );
    }

    #[test]
    fn test_subscript_pattern() {
        let nodes = parse_text_for_quotes("H~2~O");
        assert_eq!(nodes.len(), 3);
        assert!(
            matches!(&nodes[1], InlineNode::SubscriptText(s) if matches!(s.content.first(), Some(InlineNode::PlainText(p)) if p.content == "2"))
        );
    }

    #[test]
    fn test_highlight_pattern() {
        let nodes = parse_text_for_quotes("This is #highlighted# text.");
        assert_eq!(nodes.len(), 3);
        assert!(
            matches!(&nodes[1], InlineNode::HighlightText(h) if matches!(h.content.first(), Some(InlineNode::PlainText(p)) if p.content == "highlighted"))
        );
    }

    #[test]
    fn test_escaped_superscript_not_parsed() {
        // Backslash-escaped markers should not be parsed as formatting
        let nodes = parse_text_for_quotes(r"E=mc\^2^");
        // Should remain as plain text (escape prevents parsing)
        assert!(
            nodes.iter().all(|n| matches!(n, InlineNode::PlainText(_))),
            "Escaped superscript should not be parsed"
        );
    }

    #[test]
    fn test_escaped_subscript_not_parsed() {
        let nodes = parse_text_for_quotes(r"H\~2~O");
        assert!(
            nodes.iter().all(|n| matches!(n, InlineNode::PlainText(_))),
            "Escaped subscript should not be parsed"
        );
    }

    #[test]
    fn test_multiple_formats_in_sequence() {
        let nodes = parse_text_for_quotes("*bold* and _italic_ and `code`");
        assert!(nodes.iter().any(|n| matches!(n, InlineNode::BoldText(_))));
        assert!(nodes.iter().any(|n| matches!(n, InlineNode::ItalicText(_))));
        assert!(
            nodes
                .iter()
                .any(|n| matches!(n, InlineNode::MonospaceText(_)))
        );
    }

    #[test]
    fn test_plain_text_only() {
        let nodes = parse_text_for_quotes("Just plain text here.");
        assert_eq!(nodes.len(), 1);
        assert!(matches!(nodes[0], InlineNode::PlainText(_)));
    }

    #[test]
    fn test_empty_input() {
        let nodes = parse_text_for_quotes("");
        assert!(nodes.is_empty());
    }
}
