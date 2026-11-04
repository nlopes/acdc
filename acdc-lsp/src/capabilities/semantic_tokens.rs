//! Semantic Tokens: rich syntax highlighting based on AST
//!
//! Provides semantic tokens for:
//! - Section titles (`namespace`)
//! - Macros (xref, link, image, include) (`function`)
//! - Attribute names (`property`)
//! - Attribute values (`string`)
//! - Inline formatting (bold, italic, etc.) (`variable`)
//! - Comments (`comment`)

use acdc_parser::ast::{
    BlockKind, ContentRef, DocumentRef, IdOrigin, InlineKind, LocationRef, SectionRef,
    StructuralRef,
};
use tower_lsp_server::ls_types::{
    Range, SemanticToken, SemanticTokenModifier, SemanticTokenType, SemanticTokens,
    SemanticTokensFullOptions, SemanticTokensLegend, SemanticTokensOptions,
    WorkDoneProgressOptions,
};

use crate::{
    convert::{LineIndex, byte_range_to_utf16_range, is_primary_location, to_lsp_u32},
    state::ConditionalBlock,
};

/// Semantic token types used by this LSP
pub(crate) const TOKEN_TYPES: &[SemanticTokenType] = &[
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
pub(crate) const TOKEN_MODIFIERS: &[SemanticTokenModifier] = &[
    SemanticTokenModifier::DECLARATION,     // 0 - anchor definitions
    SemanticTokenModifier::DEFINITION,      // 1 - section with ID
    SemanticTokenModifier::new("disabled"), // 2 - inactive conditional content
];

/// Create the semantic tokens legend for capability registration
#[must_use]
pub(crate) fn create_legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: TOKEN_TYPES.to_vec(),
        token_modifiers: TOKEN_MODIFIERS.to_vec(),
    }
}

/// Create semantic tokens options for capability registration
#[must_use]
pub(crate) fn create_options() -> SemanticTokensOptions {
    SemanticTokensOptions {
        legend: create_legend(),
        full: Some(SemanticTokensFullOptions::Bool(true)),
        range: None,
        work_done_progress_options: WorkDoneProgressOptions::default(),
    }
}

/// A single-line UTF-16 token before delta encoding.
#[derive(PartialEq, Eq)]
struct RawToken {
    line: u32,
    start_char: u32,
    length: u32,
    token_type: u32,
    token_modifiers: u32,
}

/// Compute semantic tokens for a document, including conditional directive awareness.
#[must_use]
pub(crate) fn compute_semantic_tokens(
    doc: DocumentRef<'_>,
    conditionals: &[ConditionalBlock],
    text: &str,
) -> SemanticTokens {
    let mut tokens: Vec<RawToken> = Vec::new();
    let source = LineIndex::new(text);

    for block in doc.blocks() {
        collect_tokens_from_structure(block.into(), &source, &mut tokens);
    }
    collect_conditional_tokens(conditionals, &source, &mut tokens);

    // Sort by position for delta encoding
    tokens.sort_by_key(|token| {
        (
            token.line,
            token.start_char,
            token.length,
            token.token_type,
            token.token_modifiers,
        )
    });
    tokens.dedup();

    // Delta encode
    let encoded = delta_encode(tokens);

    SemanticTokens {
        result_id: None,
        data: encoded,
    }
}

/// Emit semantic tokens for conditional directives and inactive content.
fn collect_conditional_tokens(
    conditionals: &[ConditionalBlock],
    source: &LineIndex<'_>,
    tokens: &mut Vec<RawToken>,
) {
    for cond in conditionals {
        // Directive line as KEYWORD
        if let Some(length) = source.line_width(to_lsp_u32(cond.start_line))
            && length > 0
        {
            tokens.push(RawToken {
                line: to_lsp_u32(cond.start_line),
                start_char: 0,
                length,
                token_type: 6, // KEYWORD
                token_modifiers: 0,
            });
        }

        // Endif line as KEYWORD
        if let Some(endif_line) = cond.end_line
            && let Some(length) = source.line_width(to_lsp_u32(endif_line))
            && length > 0
        {
            tokens.push(RawToken {
                line: to_lsp_u32(endif_line),
                start_char: 0,
                length,
                token_type: 6, // KEYWORD
                token_modifiers: 0,
            });
        }

        // For inactive blocks, emit COMMENT + disabled modifier for content lines
        if !cond.is_active
            && let Some(end_line) = cond.end_line
        {
            for line_idx in (cond.start_line + 1)..end_line {
                if let Some(length) = source.line_width(to_lsp_u32(line_idx))
                    && length > 0
                {
                    tokens.push(RawToken {
                        line: to_lsp_u32(line_idx),
                        start_char: 0,
                        length,
                        token_type: 5,      // COMMENT
                        token_modifiers: 4, // bit 2 = disabled
                    });
                }
            }
        }
    }
}

fn collect_tokens_from_structure(
    node: StructuralRef<'_>,
    source: &LineIndex<'_>,
    tokens: &mut Vec<RawToken>,
) {
    match node {
        StructuralRef::Block(block) => match block.kind() {
            BlockKind::Section(section) => add_section_token(section, source, tokens),
            BlockKind::Paragraph(paragraph) => {
                collect_tokens_from_inlines(paragraph.content(), source, tokens);
            }
            BlockKind::Listing(block) => {
                collect_tokens_from_inlines(block.content(), source, tokens);
            }
            BlockKind::Literal(block) => {
                collect_tokens_from_inlines(block.content(), source, tokens);
            }
            BlockKind::Verse(block) => collect_tokens_from_inlines(block.content(), source, tokens),
            BlockKind::DelimitedComment(comment) => {
                add_token_for_location(comment.location(), 5, 0, source, tokens);
            }
            BlockKind::Admonition(admonition) => {
                add_token_for_location(admonition.location(), 6, 0, source, tokens);
            }
            BlockKind::Comment(comment) => {
                add_token_for_location(comment.location(), 5, 0, source, tokens);
            }
            BlockKind::DocumentAttribute(attribute) => {
                if is_primary_location(attribute.location())
                    && source.range(attribute.location()).is_some()
                {
                    add_attribute_token(
                        attribute.name(),
                        attribute.location().start().line() - 1,
                        source,
                        tokens,
                    );
                }
            }
            BlockKind::Example(_)
            | BlockKind::Open(_)
            | BlockKind::Sidebar(_)
            | BlockKind::Quote(_)
            | BlockKind::UnorderedList(_)
            | BlockKind::OrderedList(_)
            | BlockKind::DescriptionList(_) => {}
            BlockKind::TableOfContents(_)
            | BlockKind::DiscreteHeader(_)
            | BlockKind::ThematicBreak(_)
            | BlockKind::PageBreak(_)
            | BlockKind::CalloutList(_)
            | BlockKind::Image(_)
            | BlockKind::Audio(_)
            | BlockKind::Video(_)
            | BlockKind::Table(_)
            | BlockKind::Passthrough(_)
            | BlockKind::Stem(_)
            | _ => return,
        },
        StructuralRef::ListItem(item) => {
            collect_tokens_from_inlines(item.principal(), source, tokens);
        }
        StructuralRef::DescriptionListItem(item) => {
            collect_tokens_from_inlines(item.principal(), source, tokens);
        }
        StructuralRef::Document(_)
        | StructuralRef::Header(_)
        | StructuralRef::CalloutListItem(_)
        | StructuralRef::TableRow(_)
        | StructuralRef::TableCell(_)
        | _ => return,
    }
    for child in node.children() {
        collect_tokens_from_structure(child, source, tokens);
    }
}

fn add_section_token(section: SectionRef<'_>, source: &LineIndex<'_>, tokens: &mut Vec<RawToken>) {
    let Some(title) = section.title() else { return };
    let mut inlines = title.inlines();
    let Some(first) = inlines.next() else { return };
    let last = inlines.next_back().unwrap_or(first);
    let start = first.location().start();
    let end = last.location().end();
    if [start, end].into_iter().all(|position| {
        position
            .include_chain()
            .is_none_or(|mut chain| chain.next().is_none())
    }) && let Some(range) = source.range_between(start, end)
    {
        let modifiers = if section
            .id()
            .is_some_and(|id| id.origin() == IdOrigin::Explicit)
        {
            2
        } else {
            0
        };
        add_token_for_range(range, 0, modifiers, source, tokens);
    }
}

fn collect_tokens_from_inlines(
    content: ContentRef<'_>,
    source: &LineIndex<'_>,
    tokens: &mut Vec<RawToken>,
) {
    for inline in content.inlines() {
        match inline.kind() {
            InlineKind::CrossReference(_)
            | InlineKind::Link(_)
            | InlineKind::Url(_)
            | InlineKind::Autolink(_)
            | InlineKind::Mailto(_)
            | InlineKind::Image(_)
            | InlineKind::Icon(_) => {
                add_token_for_location(inline.location(), 1, 0, source, tokens);
            }
            InlineKind::Anchor(_) => {
                add_token_for_location(inline.location(), 7, 1, source, tokens);
            }
            InlineKind::Bold(_)
            | InlineKind::Italic(_)
            | InlineKind::Monospace(_)
            | InlineKind::Highlight(_)
            | InlineKind::Subscript(_)
            | InlineKind::Superscript(_) => {
                add_token_for_location(inline.location(), 4, 0, source, tokens);
                for field in inline.content_fields() {
                    collect_tokens_from_inlines(field.content(), source, tokens);
                }
            }
            InlineKind::Plain(_)
            | InlineKind::Symbol(_)
            | InlineKind::CharacterReference(_)
            | InlineKind::Raw(_)
            | InlineKind::Verbatim(_)
            | InlineKind::CurvedQuotation(_)
            | InlineKind::CurvedApostrophe(_)
            | InlineKind::StandaloneCurvedApostrophe(_)
            | InlineKind::LineBreak(_)
            | InlineKind::Callout(_)
            | InlineKind::Footnote(_)
            | InlineKind::Keyboard(_)
            | InlineKind::Button(_)
            | InlineKind::Menu(_)
            | InlineKind::Pass(_)
            | InlineKind::Stem(_)
            | InlineKind::IndexTerm(_)
            | _ => {}
        }
    }
}

fn add_attribute_token(name: &str, line: u32, source: &LineIndex<'_>, tokens: &mut Vec<RawToken>) {
    let Some(text) = source.line(line) else {
        return;
    };
    let Some(rest) = text.trim_start().strip_prefix(':') else {
        return;
    };
    let rest = rest.strip_prefix('!').unwrap_or(rest);
    let Some(suffix) = rest.strip_prefix(name) else {
        return;
    };
    if !suffix.starts_with(':') && !suffix.starts_with("!:") {
        return;
    }
    let start = text.len() - rest.len();
    if let Some(range) = byte_range_to_utf16_range(text, start..start + name.len())
        && range.end.character > range.start.character
    {
        tokens.push(RawToken {
            line,
            start_char: range.start.character,
            length: range.end.character - range.start.character,
            token_type: 2, // PROPERTY
            token_modifiers: 0,
        });
    }
}

fn add_token_for_location<'doc>(
    loc: impl Into<LocationRef<'doc>>,
    token_type: u32,
    token_modifiers: u32,
    source: &LineIndex<'_>,
    tokens: &mut Vec<RawToken>,
) {
    let loc = loc.into();
    if !is_primary_location(loc) {
        return;
    }
    let Some(range) = source.range(loc) else {
        return;
    };
    add_token_for_range(range, token_type, token_modifiers, source, tokens);
}

fn add_token_for_range(
    range: Range,
    token_type: u32,
    token_modifiers: u32,
    source: &LineIndex<'_>,
    tokens: &mut Vec<RawToken>,
) {
    // Keep highlighting multiline nodes on their first physical line.
    let end = if range.start.line == range.end.line {
        range.end.character
    } else {
        let Some(width) = source.line_width(range.start.line) else {
            return;
        };
        width
    };
    if end > range.start.character {
        tokens.push(RawToken {
            line: range.start.line,
            start_char: range.start.character,
            length: end - range.start.character,
            token_type,
            token_modifiers,
        });
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
    use acdc_parser::{Location, Options, Position};

    #[test]
    fn tokens_use_utf16_columns_and_stop_at_line_end() -> Result<(), Box<dyn std::error::Error>> {
        let text = "😀 *bold*\n*multi\nline*\n";
        let parsed = acdc_parser::parse(text, &Options::default())?;
        let tokens = compute_semantic_tokens(parsed.document(), &[], text);
        let first = tokens.data.first().ok_or("missing token")?;
        assert_eq!(first.delta_start, 3);
        assert_eq!(first.length, 6);
        let mut line = 0;
        let mut column = 0;
        for token in tokens.data {
            line += token.delta_line;
            column = if token.delta_line == 0 {
                column + token.delta_start
            } else {
                token.delta_start
            };
            let width = text
                .lines()
                .nth(line as usize)
                .map_or(0, |line| line.encode_utf16().count());
            assert!(column + token.length <= to_lsp_u32(width));
        }
        Ok(())
    }

    #[test]
    fn cross_file_tokens_are_omitted() {
        use std::sync::Arc;

        let mut start = Position::new(2, 1);
        start.file = Some(Arc::new(vec!["a.adoc".to_string()]));
        let mut end = Position::new(8, 5);
        end.file = Some(Arc::new(vec!["b.adoc".to_string()]));
        let mut loc = Location::point(start);
        loc.end = end;
        loc.absolute_start = 10;
        loc.absolute_end = 400;

        let mut tokens = Vec::new();
        add_token_for_location(&loc, 0, 0, &LineIndex::new("a\nb\n"), &mut tokens);

        assert_eq!(tokens.len(), 0);
    }

    #[test]
    fn invalid_token_spans_are_omitted_instead_of_clipped() {
        let source = LineIndex::new("é😀\r\nnext\n");
        for (start, end) in [
            ((1, 1), (1, 20)),
            ((1, 1), (9, 1)),
            ((2, 1), (1, 1)),
            ((0, 0), (1, 1)),
        ] {
            let mut span = Location::point(Position::new(start.0, start.1));
            span.end = Position::new(end.0, end.1);
            let mut tokens = Vec::new();
            add_token_for_location(&span, 4, 0, &source, &mut tokens);
            assert!(tokens.is_empty(), "invalid span: {span:?}");
        }
    }

    #[test]
    fn attribute_unsetting_highlights_only_the_authored_name()
    -> Result<(), Box<dyn std::error::Error>> {
        let text = "Text.\r\n\r\n:!name:\r\n:name!:\r\n:other: value\r\n";
        let parsed = acdc_parser::parse(text, &Options::default())?;
        let tokens = compute_semantic_tokens(parsed.document(), &[], text);
        let actual: Vec<_> = tokens
            .data
            .iter()
            .map(|token| {
                (
                    token.delta_line,
                    token.delta_start,
                    token.length,
                    token.token_type,
                )
            })
            .collect();
        assert_eq!(actual, [(2, 2, 4, 2), (1, 1, 4, 2), (1, 1, 5, 2)]);
        Ok(())
    }

    #[test]
    fn test_section_tokens() -> Result<(), acdc_parser::Error> {
        let content = r"= Document Title

== First Section

Some content.
";
        let options = Options::default();
        let parsed = acdc_parser::parse(content, &options)?;

        let tokens = compute_semantic_tokens(parsed.document(), &[], content);
        // Should have at least tokens for section titles
        assert_ne!(tokens.data, []);
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
        let parsed = acdc_parser::parse(content, &options)?;

        let tokens = compute_semantic_tokens(parsed.document(), &[], content);
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
        assert_eq!(legend.token_modifiers.len(), 3);
        assert_eq!(
            legend.token_modifiers.get(2),
            Some(&SemanticTokenModifier::new("disabled"))
        );
    }

    #[test]
    fn test_conditional_inactive_tokens() -> Result<(), acdc_parser::Error> {
        use crate::state::{ConditionalBlock, ConditionalDirectiveKind};

        let content = "ifdef::missing[]\ninactive content\nendif::[]";
        let options = Options::default();
        let parsed = acdc_parser::parse(content, &options)?;
        let doc = parsed.document();

        let conditionals = vec![ConditionalBlock {
            kind: ConditionalDirectiveKind::Ifdef,
            attributes: vec!["missing".to_string()],
            operation: None,
            is_active: false,
            start_line: 0,
            end_line: Some(2),
        }];

        let tokens = compute_semantic_tokens(doc, &conditionals, content);
        // Should have tokens: ifdef line (KEYWORD), inactive content (COMMENT+disabled), endif (KEYWORD)
        assert!(tokens.data.len() >= 3);

        // Reconstruct absolute positions from delta encoding
        let mut abs_tokens = Vec::new();
        let mut prev_line = 0u32;
        let mut prev_start = 0u32;
        for t in &tokens.data {
            let line = prev_line + t.delta_line;
            let start = if t.delta_line == 0 {
                prev_start + t.delta_start
            } else {
                t.delta_start
            };
            abs_tokens.push((
                line,
                start,
                t.length,
                t.token_type,
                t.token_modifiers_bitset,
            ));
            prev_line = line;
            prev_start = start;
        }

        // Line 0: ifdef directive → KEYWORD (type 6)
        assert!(
            abs_tokens.iter().any(|t| t.0 == 0 && t.3 == 6),
            "expected KEYWORD token on line 0"
        );
        // Line 1: inactive content → COMMENT (type 5) + disabled (bit 2 = 4)
        assert!(
            abs_tokens.iter().any(|t| t.0 == 1 && t.3 == 5 && t.4 == 4),
            "expected COMMENT+disabled token on line 1"
        );
        // Line 2: endif → KEYWORD (type 6)
        assert!(
            abs_tokens.iter().any(|t| t.0 == 2 && t.3 == 6),
            "expected KEYWORD token on line 2"
        );

        Ok(())
    }

    #[test]
    fn test_conditional_active_no_disabled_tokens() -> Result<(), acdc_parser::Error> {
        use crate::state::{ConditionalBlock, ConditionalDirectiveKind};

        let content = "ifdef::present[]\nactive content\nendif::[]";
        let options = Options::default();
        let parsed = acdc_parser::parse(content, &options)?;
        let doc = parsed.document();

        let conditionals = vec![ConditionalBlock {
            kind: ConditionalDirectiveKind::Ifdef,
            attributes: vec!["present".to_string()],
            operation: None,
            is_active: true,
            start_line: 0,
            end_line: Some(2),
        }];

        let tokens = compute_semantic_tokens(doc, &conditionals, content);
        // Active block: should have KEYWORD tokens for directives but NO disabled tokens
        let mut abs_tokens = Vec::new();
        let mut prev_line = 0u32;
        let mut prev_start = 0u32;
        for t in &tokens.data {
            let line = prev_line + t.delta_line;
            let start = if t.delta_line == 0 {
                prev_start + t.delta_start
            } else {
                t.delta_start
            };
            abs_tokens.push((
                line,
                start,
                t.length,
                t.token_type,
                t.token_modifiers_bitset,
            ));
            prev_line = line;
            prev_start = start;
        }

        // No tokens should have the disabled modifier (bit 2 = 4)
        assert!(
            !abs_tokens.iter().any(|t| t.4 & 4 != 0),
            "active block should have no disabled tokens"
        );

        Ok(())
    }
}
