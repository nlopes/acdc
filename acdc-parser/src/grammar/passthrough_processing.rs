use crate::{
    InlineNode, Location, Pass, PassthroughKind, Plain, ProcessedContent, Raw, Substitution,
};

use super::{
    ParserState,
    document::{BlockParsingMetadata, document_parser},
    location_mapping::{clamp_inline_node_locations, remap_inline_node_location},
};

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
        // Compute content-only location by stripping the delimiter prefix/suffix
        // from the full passthrough macro location.
        let suffix_len = match passthrough.kind {
            PassthroughKind::Macro | PassthroughKind::Single => 1, // ] or +
            PassthroughKind::Double => 2,                          // ++
            PassthroughKind::Triple => 3,                          // +++
        };
        let total_span = passthrough.location.absolute_end - passthrough.location.absolute_start;
        let prefix_len = total_span - content.len() - suffix_len;

        let content_abs_start = passthrough.location.absolute_start + prefix_len;
        let content_col_start = passthrough.location.start.column + prefix_len;

        let content_location = Location {
            absolute_start: content_abs_start,
            absolute_end: content_abs_start + content.len(),
            start: crate::Position {
                line: passthrough.location.start.line,
                column: content_col_start,
            },
            end: crate::Position {
                line: passthrough.location.start.line,
                column: content_col_start + content.len(),
            },
        };

        return vec![InlineNode::RawText(Raw {
            content: content.to_string(),
            location: content_location,
            subs: passthrough
                .substitutions
                .iter()
                .filter(|s| **s != Substitution::Quotes)
                .cloned()
                .collect(),
        })];
    }

    tracing::debug!(content = ?content, "Parsing passthrough content with quotes");

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
    if content.is_empty() {
        return Vec::new();
    }

    let mut state = ParserState::new(content);
    state.quotes_only = true;
    let block_metadata = BlockParsingMetadata::default();

    match document_parser::quotes_only_inlines(content, &mut state, 0, &block_metadata) {
        Ok(nodes) => nodes,
        Err(err) => {
            tracing::warn!(
                ?err,
                ?content,
                "quotes-only PEG parse failed, falling back to plain text"
            );
            vec![InlineNode::PlainText(Plain {
                content: content.to_string(),
                location: Location::default(),
                escaped: false,
            })]
        }
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
        let placeholder = crate::grammar::inline_preprocessor::passthrough_placeholder(index);

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
                let has_quotes = passthrough.substitutions.contains(&Substitution::Quotes);
                let remaining_subs: Vec<Substitution> = passthrough
                    .substitutions
                    .iter()
                    .filter(|s| **s != Substitution::Quotes)
                    .cloned()
                    .collect();
                for mut node in processed_nodes {
                    remap_inline_node_location(
                        &mut node,
                        passthrough.location.absolute_start + macro_prefix_len,
                    );
                    // For passthroughs with quotes, convert PlainText to RawText so
                    // HTML content passes through unescaped. Must happen AFTER
                    // remapping since remap_inline_node_location handles PlainText
                    // but not RawText (RawText from non-quotes path already has
                    // correct locations from passthrough.location).
                    if has_quotes {
                        if let InlineNode::PlainText(p) = node {
                            node = InlineNode::RawText(Raw {
                                content: p.content,
                                location: p.location,
                                subs: remaining_subs.clone(),
                            });
                        }
                    }
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
        let placeholder = crate::grammar::inline_preprocessor::passthrough_placeholder(index);
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
