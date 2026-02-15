//! Semantic Tokens: rich syntax highlighting based on AST
//!
//! Provides semantic tokens for:
//! - Section titles (`namespace`)
//! - Macros (xref, link, image, include) (`function`)
//! - Attribute names (`property`)
//! - Attribute values (`string`)
//! - Inline formatting (bold, italic, etc.) (`variable`)
//! - Comments (`comment`)

use acdc_parser::{
    Block, DelimitedBlock, DelimitedBlockType, Document, InlineMacro, InlineNode, Location,
};
use tower_lsp::lsp_types::{
    SemanticToken, SemanticTokenModifier, SemanticTokenType, SemanticTokens,
    SemanticTokensFullOptions, SemanticTokensLegend, SemanticTokensOptions,
    WorkDoneProgressOptions,
};

/// Convert usize to u32 for LSP types, saturating at `u32::MAX`.
fn to_lsp_u32(val: usize) -> u32 {
    val.try_into().unwrap_or(u32::MAX)
}

/// Semantic token types used by this LSP
pub const TOKEN_TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::NAMESPACE, // 0 - section titles
    SemanticTokenType::FUNCTION,  // 1 - macros (xref, link, image, include)
    SemanticTokenType::PROPERTY,  // 2 - attribute names
    SemanticTokenType::STRING,    // 3 - attribute values, link text
    SemanticTokenType::VARIABLE,  // 4 - formatted text (bold, italic)
    SemanticTokenType::COMMENT,   // 5 - comments
    SemanticTokenType::KEYWORD,   // 6 - admonition labels
    SemanticTokenType::DECORATOR, // 7 - anchors
    SemanticTokenType::OPERATOR,  // 8 - formatting markers (**, __, etc.)
];

/// Semantic token modifiers
pub const TOKEN_MODIFIERS: &[SemanticTokenModifier] = &[
    SemanticTokenModifier::DECLARATION, // anchor definitions
    SemanticTokenModifier::DEFINITION,  // section with ID
];

/// Create the semantic tokens legend for capability registration
#[must_use]
pub fn create_legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: TOKEN_TYPES.to_vec(),
        token_modifiers: TOKEN_MODIFIERS.to_vec(),
    }
}

/// Create semantic tokens options for capability registration
#[must_use]
pub fn create_options() -> SemanticTokensOptions {
    SemanticTokensOptions {
        legend: create_legend(),
        full: Some(SemanticTokensFullOptions::Bool(true)),
        range: None,
        work_done_progress_options: WorkDoneProgressOptions::default(),
    }
}

/// Token being collected before encoding
struct RawToken {
    line: u32,
    start_char: u32,
    length: u32,
    token_type: u32,
    token_modifiers: u32,
}

/// Compute semantic tokens for a document
#[must_use]
pub fn compute_semantic_tokens(doc: &Document) -> SemanticTokens {
    let mut tokens: Vec<RawToken> = Vec::new();

    collect_tokens_from_blocks(&doc.blocks, &mut tokens);

    // Sort by position for delta encoding
    tokens.sort_by(|a, b| a.line.cmp(&b.line).then(a.start_char.cmp(&b.start_char)));

    // Delta encode
    let encoded = delta_encode(tokens);

    SemanticTokens {
        result_id: None,
        data: encoded,
    }
}

fn collect_tokens_from_blocks(blocks: &[Block], tokens: &mut Vec<RawToken>) {
    for block in blocks {
        collect_tokens_from_block(block, tokens);
    }
}

fn collect_tokens_from_block(block: &Block, tokens: &mut Vec<RawToken>) {
    match block {
        Block::Section(section) => {
            // Section title as namespace
            // The title is at the section location start
            let title_len = section.title.iter().map(inline_text_len).sum::<usize>();

            if title_len > 0 {
                tokens.push(RawToken {
                    line: to_lsp_u32(section.location.start.line.saturating_sub(1)),
                    // Skip the = markers and space
                    start_char: u32::from(section.level) + 2, // Skip = markers and space
                    length: to_lsp_u32(title_len),
                    token_type: 0, // NAMESPACE
                    token_modifiers: if section.metadata.id.is_some() { 2 } else { 0 },
                });
            }

            // Process section content
            collect_tokens_from_blocks(&section.content, tokens);
        }
        Block::Paragraph(para) => {
            collect_tokens_from_inlines(&para.content, tokens);
        }
        Block::DelimitedBlock(delimited) => {
            collect_tokens_from_delimited(delimited, tokens);
        }
        Block::UnorderedList(list) => {
            for item in &list.items {
                collect_tokens_from_inlines(&item.principal, tokens);
                collect_tokens_from_blocks(&item.blocks, tokens);
            }
        }
        Block::OrderedList(list) => {
            for item in &list.items {
                collect_tokens_from_inlines(&item.principal, tokens);
                collect_tokens_from_blocks(&item.blocks, tokens);
            }
        }
        Block::DescriptionList(list) => {
            for item in &list.items {
                collect_tokens_from_inlines(&item.principal_text, tokens);
                collect_tokens_from_blocks(&item.description, tokens);
            }
        }
        Block::Admonition(adm) => {
            // Admonition label as keyword
            add_token_for_location(&adm.location, 6, 0, tokens); // KEYWORD
            collect_tokens_from_blocks(&adm.blocks, tokens);
        }
        Block::Comment(comment) => {
            add_token_for_location(&comment.location, 5, 0, tokens); // COMMENT
        }
        Block::DocumentAttribute(attr) => {
            // Attribute name as property
            tokens.push(RawToken {
                line: to_lsp_u32(attr.location.start.line.saturating_sub(1)),
                start_char: 1, // Skip leading :
                length: to_lsp_u32(attr.name.len()),
                token_type: 2, // PROPERTY
                token_modifiers: 0,
            });
        }
        Block::TableOfContents(_)
        | Block::DiscreteHeader(_)
        | Block::ThematicBreak(_)
        | Block::PageBreak(_)
        | Block::CalloutList(_)
        | Block::Image(_)
        | Block::Audio(_)
        | Block::Video(_)
        // non_exhaustive
        | _ => {}
    }
}

fn collect_tokens_from_delimited(delimited: &DelimitedBlock, tokens: &mut Vec<RawToken>) {
    match &delimited.inner {
        DelimitedBlockType::DelimitedComment(_) => {
            add_token_for_location(&delimited.location, 5, 0, tokens); // COMMENT
        }
        DelimitedBlockType::DelimitedExample(blocks)
        | DelimitedBlockType::DelimitedOpen(blocks)
        | DelimitedBlockType::DelimitedSidebar(blocks)
        | DelimitedBlockType::DelimitedQuote(blocks) => {
            collect_tokens_from_blocks(blocks, tokens);
        }
        DelimitedBlockType::DelimitedListing(inlines)
        | DelimitedBlockType::DelimitedLiteral(inlines)
        | DelimitedBlockType::DelimitedVerse(inlines) => {
            collect_tokens_from_inlines(inlines, tokens);
        }
        DelimitedBlockType::DelimitedPass(_)
        | DelimitedBlockType::DelimitedTable(_)
        | DelimitedBlockType::DelimitedStem(_)
        // non_exhaustive
        | _ => {}
    }
}

fn collect_tokens_from_inlines(inlines: &[InlineNode], tokens: &mut Vec<RawToken>) {
    for inline in inlines {
        collect_tokens_from_inline(inline, tokens);
    }
}

fn collect_tokens_from_inline(inline: &InlineNode, tokens: &mut Vec<RawToken>) {
    match inline {
        InlineNode::Macro(InlineMacro::CrossReference(xref)) => {
            add_token_for_location(&xref.location, 1, 0, tokens); // FUNCTION (macro)
        }
        InlineNode::Macro(InlineMacro::Link(link)) => {
            add_token_for_location(&link.location, 1, 0, tokens); // FUNCTION
        }
        InlineNode::Macro(InlineMacro::Url(url)) => {
            add_token_for_location(&url.location, 1, 0, tokens); // FUNCTION
        }
        InlineNode::Macro(InlineMacro::Autolink(autolink)) => {
            add_token_for_location(&autolink.location, 1, 0, tokens); // FUNCTION
        }
        InlineNode::Macro(InlineMacro::Mailto(mailto)) => {
            add_token_for_location(&mailto.location, 1, 0, tokens); // FUNCTION
        }
        InlineNode::Macro(InlineMacro::Image(image)) => {
            add_token_for_location(&image.location, 1, 0, tokens); // FUNCTION
        }
        InlineNode::Macro(InlineMacro::Icon(icon)) => {
            add_token_for_location(&icon.location, 1, 0, tokens); // FUNCTION
        }
        InlineNode::InlineAnchor(anchor) => {
            add_token_for_location(&anchor.location, 7, 1, tokens); // DECORATOR + DECLARATION
        }
        InlineNode::BoldText(b) => {
            add_token_for_location(&b.location, 4, 0, tokens); // VARIABLE
            collect_tokens_from_inlines(&b.content, tokens);
        }
        InlineNode::ItalicText(i) => {
            add_token_for_location(&i.location, 4, 0, tokens); // VARIABLE
            collect_tokens_from_inlines(&i.content, tokens);
        }
        InlineNode::MonospaceText(m) => {
            add_token_for_location(&m.location, 4, 0, tokens); // VARIABLE
            collect_tokens_from_inlines(&m.content, tokens);
        }
        InlineNode::HighlightText(h) => {
            add_token_for_location(&h.location, 4, 0, tokens); // VARIABLE
            collect_tokens_from_inlines(&h.content, tokens);
        }
        InlineNode::SubscriptText(s) => {
            add_token_for_location(&s.location, 4, 0, tokens); // VARIABLE
            collect_tokens_from_inlines(&s.content, tokens);
        }
        InlineNode::SuperscriptText(s) => {
            add_token_for_location(&s.location, 4, 0, tokens); // VARIABLE
            collect_tokens_from_inlines(&s.content, tokens);
        }
        InlineNode::PlainText(_)
        | InlineNode::RawText(_)
        | InlineNode::VerbatimText(_)
        | InlineNode::CurvedQuotationText(_)
        | InlineNode::CurvedApostropheText(_)
        | InlineNode::StandaloneCurvedApostrophe(_)
        | InlineNode::LineBreak(_)
        | InlineNode::Macro(_)
        | InlineNode::CalloutRef(_)
        // non_exhaustive
        | _ => {}
    }
}

/// Helper to add a token for a location
fn add_token_for_location(
    loc: &Location,
    token_type: u32,
    token_modifiers: u32,
    tokens: &mut Vec<RawToken>,
) {
    // Calculate length from location
    let length = if loc.start.line == loc.end.line {
        to_lsp_u32(loc.end.column.saturating_sub(loc.start.column))
    } else {
        // For multi-line, just use first line length (simplified)
        to_lsp_u32(loc.absolute_end.saturating_sub(loc.absolute_start))
    };

    if length > 0 {
        tokens.push(RawToken {
            line: to_lsp_u32(loc.start.line.saturating_sub(1)),
            start_char: to_lsp_u32(loc.start.column.saturating_sub(1)),
            length,
            token_type,
            token_modifiers,
        });
    }
}

/// Get approximate text length of an inline node
fn inline_text_len(inline: &InlineNode) -> usize {
    match inline {
        InlineNode::PlainText(p) => p.content.len(),
        InlineNode::BoldText(b) => b.content.iter().map(inline_text_len).sum(),
        InlineNode::ItalicText(i) => i.content.iter().map(inline_text_len).sum(),
        InlineNode::MonospaceText(m) => m.content.iter().map(inline_text_len).sum(),
        InlineNode::HighlightText(h) => h.content.iter().map(inline_text_len).sum(),
        InlineNode::SubscriptText(s) => s.content.iter().map(inline_text_len).sum(),
        InlineNode::SuperscriptText(s) => s.content.iter().map(inline_text_len).sum(),
        InlineNode::RawText(_)
        | InlineNode::VerbatimText(_)
        | InlineNode::CurvedQuotationText(_)
        | InlineNode::CurvedApostropheText(_)
        | InlineNode::StandaloneCurvedApostrophe(_)
        | InlineNode::LineBreak(_)
        | InlineNode::InlineAnchor(_)
        | InlineNode::Macro(_)
        | InlineNode::CalloutRef(_)
        // non_exhaustive
        | _ => 0,
    }
}

/// Delta encode tokens for LSP format
fn delta_encode(tokens: Vec<RawToken>) -> Vec<SemanticToken> {
    let mut result = Vec::with_capacity(tokens.len());
    let mut prev_line = 0u32;
    let mut prev_start = 0u32;

    for token in tokens {
        let delta_line = token.line - prev_line;
        let delta_start = if delta_line == 0 {
            token.start_char - prev_start
        } else {
            token.start_char
        };

        result.push(SemanticToken {
            delta_line,
            delta_start,
            length: token.length,
            token_type: token.token_type,
            token_modifiers_bitset: token.token_modifiers,
        });

        prev_line = token.line;
        prev_start = token.start_char;
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use acdc_parser::Options;

    #[test]
    fn test_section_tokens() -> Result<(), acdc_parser::Error> {
        let content = r"= Document Title

== First Section

Some content.
";
        let options = Options::default();
        let doc = acdc_parser::parse(content, &options)?;

        let tokens = compute_semantic_tokens(&doc);
        // Should have at least tokens for section titles
        assert!(!tokens.data.is_empty());
        Ok(())
    }

    #[test]
    fn test_xref_tokens() -> Result<(), acdc_parser::Error> {
        let content = r"= Document

[[target]]
== Target Section

See <<target>> for more.
";
        let options = Options::default();
        let doc = acdc_parser::parse(content, &options)?;

        let tokens = compute_semantic_tokens(&doc);
        // Should have tokens for section, anchor, and xref
        assert!(tokens.data.len() >= 2);
        Ok(())
    }

    #[test]
    fn test_legend_has_expected_types() {
        let legend = create_legend();
        assert!(legend.token_types.contains(&SemanticTokenType::NAMESPACE));
        assert!(legend.token_types.contains(&SemanticTokenType::FUNCTION));
        assert!(legend.token_types.contains(&SemanticTokenType::COMMENT));
    }
}
