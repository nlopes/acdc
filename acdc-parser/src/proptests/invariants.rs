//! Parser invariant tests using property-based testing
//!
//! These tests verify that certain properties hold for ANY input to the parser.
//! They're organized by priority:
//! - P0: Critical invariants (parser never panics, bounds checking)
//! - P1: Structural invariants (position monotonicity, serialization)
//! - P2: Behavioral invariants (`SafeMode`, preprocessing)

use proptest::prelude::*;

use crate::{
    Block, DelimitedBlock, DelimitedBlockType, Document, InlineNode, Location, Options,
    model::Locateable, parse, parse_inline,
};

use super::generators::*;

// Configuration for proptest - can be overridden with PROPTEST_CASES env var
proptest! {
    #![proptest_config(ProptestConfig {
        cases: 1000, // Default for local dev
        max_shrink_iters: 10000,
        .. ProptestConfig::default()
    })]

    // ====================================================================
    // P0: CRITICAL INVARIANTS - These must NEVER fail
    // ====================================================================

    /// The parser should never panic on any input, no matter how malformed.
    /// This is the most important invariant - the parser must always return
    /// a Result (Ok or Err), never panic.
    #[test]
    fn parser_never_panics(input in any_document_string()) {
        let options = Options::default();
        // We don't care about the result, just that it doesn't panic
        let _ = parse(&input, &options);
    }

    /// The inline parser should also never panic on any input.
    #[test]
    fn inline_parser_never_panics(input in any_document_string()) {
        let options = Options::default();
        // We don't care about the result, just that it doesn't panic
        let _ = parse_inline(&input, &options);
    }

    /// All location byte offsets in the AST must be within the input bounds.
    /// This prevents index-out-of-bounds panics when using locations.
    #[test]
    fn all_locations_in_bounds(input in ascii_document()) {
        let options = Options::default();
        if let Ok(parsed) = parse(&input, &options) { let doc = parsed.document();
            let input_len = input.len();
            walk_document_locations(doc, &mut |loc, ctx| {
                assert!(
                    loc.absolute_start <= input_len,
                    "{ctx} location start {} exceeds input length {input_len}",
                    loc.absolute_start
                );
                assert!(
                    loc.absolute_end <= input_len,
                    "{ctx} location end {} exceeds input length {input_len}",
                    loc.absolute_end
                );
                assert!(
                    loc.absolute_start <= loc.absolute_end,
                    "{ctx} location has start {} > end {}",
                    loc.absolute_start,
                    loc.absolute_end
                );
            });
        }
    }

    /// All byte offsets must fall on valid UTF-8 character boundaries.
    /// This prevents panics when slicing strings at these offsets.
    #[test]
    fn byte_offsets_utf8_safe(input in unicode_stress_test()) {
        let options = Options::default();
        // Preprocess the input first to get the actual string the parser works on
        if let Ok(result) = crate::Preprocessor.process(&input, &options) {
            // Parse the preprocessed input
            let arena = bumpalo::Bump::new();
            let mut state = crate::grammar::ParserState::new(&result.text, &arena);
            state.document_attributes = std::rc::Rc::new(options.document_attributes.clone());
            state.options = std::rc::Rc::new(options.clone());
            state.leveloffset_ranges = result.leveloffset_ranges;
            if let Ok(Ok(doc)) = crate::grammar::document_parser::document(&result.text, &mut state) {
                let text = &result.text;
                walk_document_locations(&doc, &mut |loc, ctx| {
                    assert!(
                        text.is_char_boundary(loc.absolute_start),
                        "{ctx} location start {} is not on a UTF-8 boundary",
                        loc.absolute_start
                    );
                    assert!(
                        text.is_char_boundary(loc.absolute_end),
                        "{ctx} location end {} is not on a UTF-8 boundary",
                        loc.absolute_end
                    );
                });
            }
        }
    }

    // ====================================================================
    // P1: STRUCTURAL INVARIANTS
    // ====================================================================

    /// Positions within a block should be monotonically increasing.
    /// This helps ensure the parser processes content in order.
    #[test]
    fn positions_are_monotonic(input in structured_document()) {
        let options = Options::default();
        if let Ok(parsed) = parse(&input, &options) { let doc = parsed.document();
            verify_monotonic_positions("Block", &doc.blocks);
        }
    }

    // ====================================================================
    // P0: TARGETED INVARIANTS - Complex construct specific
    // ====================================================================

    /// Table locations must be within input bounds.
    /// Tables have the most complex grammar rules with recursive cell parsing.
    #[test]
    fn table_locations_in_bounds(input in table_document()) {
        let options = Options::default();
        if let Ok(parsed) = parse(&input, &options) { let doc = parsed.document();
            let input_len = input.len();
            walk_document_locations(doc, &mut |loc, ctx| {
                assert!(
                    loc.absolute_start <= input_len,
                    "{ctx} location start {} exceeds input length {input_len}",
                    loc.absolute_start
                );
                assert!(
                    loc.absolute_end <= input_len,
                    "{ctx} location end {} exceeds input length {input_len}",
                    loc.absolute_end
                );
                assert!(
                    loc.absolute_start <= loc.absolute_end,
                    "{ctx} location has start {} > end {}",
                    loc.absolute_start,
                    loc.absolute_end
                );
            });
        }
    }

    /// Inline macros and formatting must never cause panics.
    /// Exercises the two-pass inline processing system.
    #[test]
    fn inline_macros_never_panic(input in inline_formatted_text()) {
        let options = Options::default();
        let _ = parse(&input, &options);
    }

    // ====================================================================
    // P1: TARGETED STRUCTURAL INVARIANTS
    // ====================================================================

    /// Description list positions must be monotonically increasing.
    #[test]
    fn description_list_positions_monotonic(input in description_list_document()) {
        let options = Options::default();
        if let Ok(parsed) = parse(&input, &options) { let doc = parsed.document();
            verify_monotonic_positions("DescriptionList", &doc.blocks);
        }
    }

    /// Rich document positions must be monotonically increasing.
    /// Tests cross-construct interaction with sections, lists, and blocks.
    #[test]
    fn rich_document_positions_monotonic(input in rich_document()) {
        let options = Options::default();
        if let Ok(parsed) = parse(&input, &options) { let doc = parsed.document();
            verify_monotonic_positions("RichDocument", &doc.blocks);
        }
    }

    /// Nested list locations must be within input bounds.
    /// List nesting is inherently difficult with PEG parsers.
    #[test]
    fn nested_list_locations_in_bounds(input in nested_list_document()) {
        let options = Options::default();
        if let Ok(parsed) = parse(&input, &options) { let doc = parsed.document();
            let input_len = input.len();
            walk_document_locations(doc, &mut |loc, ctx| {
                assert!(
                    loc.absolute_start <= input_len,
                    "{ctx} location start {} exceeds input length {input_len}",
                    loc.absolute_start
                );
                assert!(
                    loc.absolute_end <= input_len,
                    "{ctx} location end {} exceeds input length {input_len}",
                    loc.absolute_end
                );
            });
        }
    }

    /// Delimited block positions must be monotonically increasing.
    /// Tests delimiter matching and content recursion.
    #[test]
    fn delimited_block_positions_monotonic(input in delimited_block_document()) {
        let options = Options::default();
        if let Ok(parsed) = parse(&input, &options) { let doc = parsed.document();
            verify_monotonic_positions("DelimitedBlock", &doc.blocks);
        }
    }

}

// ====================================================================
// Helper functions for invariant verification
// ====================================================================

/// Walk all locations in a document, calling `visitor` for each one.
fn walk_document_locations(doc: &Document, visitor: &mut impl FnMut(&Location, &str)) {
    visitor(&doc.location, "document");
    for block in &doc.blocks {
        walk_block_locations(block, visitor);
    }
    if let Some(header) = &doc.header {
        visitor(&header.location, "header");
    }
}

/// Walk all locations in a block tree, calling `visitor` for each one.
fn walk_block_locations(block: &Block, visitor: &mut impl FnMut(&Location, &str)) {
    visitor(block.location(), "block");

    match block {
        Block::Section(section) => {
            for child in &section.content {
                walk_block_locations(child, visitor);
            }
        }
        Block::Paragraph(para) => {
            for inline in &para.content {
                walk_inline_locations(inline, visitor);
            }
        }
        Block::UnorderedList(list) => {
            for item in &list.items {
                visitor(&item.location, "list item");
                for child in &item.blocks {
                    walk_block_locations(child, visitor);
                }
            }
        }
        Block::OrderedList(list) => {
            for item in &list.items {
                visitor(&item.location, "list item");
                for child in &item.blocks {
                    walk_block_locations(child, visitor);
                }
            }
        }
        Block::DescriptionList(list) => {
            for item in &list.items {
                visitor(&item.location, "description list item");
                for inline in &item.term {
                    walk_inline_locations(inline, visitor);
                }
                for child in &item.description {
                    walk_block_locations(child, visitor);
                }
            }
        }
        Block::CalloutList(list) => {
            for item in &list.items {
                visitor(&item.location, "callout item");
                for child in &item.blocks {
                    walk_block_locations(child, visitor);
                }
            }
        }
        Block::Admonition(admonition) => {
            for child in &admonition.blocks {
                walk_block_locations(child, visitor);
            }
        }
        Block::DiscreteHeader(header) => {
            for inline in &header.title {
                walk_inline_locations(inline, visitor);
            }
        }
        Block::DelimitedBlock(delimited) => {
            walk_delimited_block_locations(delimited, visitor);
        }
        // Leaf blocks: no children to recurse into
        Block::TableOfContents(_)
        | Block::DocumentAttribute(_)
        | Block::ThematicBreak(_)
        | Block::PageBreak(_)
        | Block::Image(_)
        | Block::Audio(_)
        | Block::Video(_)
        | Block::Comment(_) => {}
    }
}

/// Walk all locations in a delimited block, calling `visitor` for each one.
fn walk_delimited_block_locations(
    delimited: &DelimitedBlock,
    visitor: &mut impl FnMut(&Location, &str),
) {
    if let Some(loc) = &delimited.open_delimiter_location {
        visitor(loc, "delimited block open delimiter");
    }
    if let Some(loc) = &delimited.close_delimiter_location {
        visitor(loc, "delimited block close delimiter");
    }
    for inline in &delimited.title {
        walk_inline_locations(inline, visitor);
    }
    match &delimited.inner {
        DelimitedBlockType::DelimitedExample(blocks)
        | DelimitedBlockType::DelimitedOpen(blocks)
        | DelimitedBlockType::DelimitedSidebar(blocks)
        | DelimitedBlockType::DelimitedQuote(blocks) => {
            for block in blocks {
                walk_block_locations(block, visitor);
            }
        }
        DelimitedBlockType::DelimitedComment(inlines)
        | DelimitedBlockType::DelimitedListing(inlines)
        | DelimitedBlockType::DelimitedLiteral(inlines)
        | DelimitedBlockType::DelimitedPass(inlines)
        | DelimitedBlockType::DelimitedVerse(inlines) => {
            for inline in inlines {
                walk_inline_locations(inline, visitor);
            }
        }
        DelimitedBlockType::DelimitedTable(table) => {
            visitor(&table.location, "table");
            let all_rows = table
                .header
                .iter()
                .chain(&table.rows)
                .chain(table.footer.iter());
            for row in all_rows {
                for col in &row.columns {
                    for block in &col.content {
                        walk_block_locations(block, visitor);
                    }
                }
            }
        }
        DelimitedBlockType::DelimitedStem(_) => {}
    }
}

/// Walk all locations in an inline tree, calling `visitor` for each one.
fn walk_inline_locations(inline: &InlineNode, visitor: &mut impl FnMut(&Location, &str)) {
    visitor(inline.location(), "inline");

    match inline {
        InlineNode::BoldText(t) => {
            for child in &t.content {
                walk_inline_locations(child, visitor);
            }
        }
        InlineNode::ItalicText(t) => {
            for child in &t.content {
                walk_inline_locations(child, visitor);
            }
        }
        InlineNode::MonospaceText(t) => {
            for child in &t.content {
                walk_inline_locations(child, visitor);
            }
        }
        InlineNode::HighlightText(t) => {
            for child in &t.content {
                walk_inline_locations(child, visitor);
            }
        }
        InlineNode::SubscriptText(t) => {
            for child in &t.content {
                walk_inline_locations(child, visitor);
            }
        }
        InlineNode::SuperscriptText(t) => {
            for child in &t.content {
                walk_inline_locations(child, visitor);
            }
        }
        InlineNode::CurvedQuotationText(t) => {
            for child in &t.content {
                walk_inline_locations(child, visitor);
            }
        }
        InlineNode::CurvedApostropheText(t) => {
            for child in &t.content {
                walk_inline_locations(child, visitor);
            }
        }
        // Leaf inlines: no children to recurse into
        InlineNode::PlainText(_)
        | InlineNode::RawText(_)
        | InlineNode::VerbatimText(_)
        | InlineNode::StandaloneCurvedApostrophe(_)
        | InlineNode::LineBreak(_)
        | InlineNode::InlineAnchor(_)
        | InlineNode::Macro(_)
        | InlineNode::CalloutRef(_) => {}
    }
}

/// Verify positions are monotonically increasing within scopes
fn verify_monotonic_positions(prefix: &str, blocks: &[Block]) {
    let mut last_end = 0;
    for block in blocks {
        let location = block.location();
        let start = location.absolute_start;
        assert!(
            start >= last_end,
            "{prefix} starts at {start} but previous ended at {last_end}"
        );
        last_end = location.absolute_end;

        verify_block_monotonic(block);
    }
}

fn verify_inline_monotonic(context: &str, inlines: &[InlineNode]) {
    let mut last_end = 0;
    for inline in inlines {
        let location = inline.location();
        let start = location.absolute_start;
        assert!(
            start >= last_end,
            "{context} inline starts at {start} but previous ended at {last_end}"
        );
        last_end = location.absolute_end;
    }
}

fn verify_block_monotonic(block: &Block) {
    match block {
        Block::Section(section) => {
            verify_monotonic_positions("Section child", &section.content);
        }
        Block::Paragraph(para) => {
            verify_inline_monotonic("Paragraph", &para.content);
        }
        Block::Admonition(admonition) => {
            verify_monotonic_positions("Admonition child", &admonition.blocks);
        }
        Block::DelimitedBlock(delimited) => match &delimited.inner {
            DelimitedBlockType::DelimitedExample(blocks)
            | DelimitedBlockType::DelimitedOpen(blocks)
            | DelimitedBlockType::DelimitedSidebar(blocks)
            | DelimitedBlockType::DelimitedQuote(blocks) => {
                verify_monotonic_positions("DelimitedBlock child", blocks);
            }
            DelimitedBlockType::DelimitedComment(inlines)
            | DelimitedBlockType::DelimitedListing(inlines)
            | DelimitedBlockType::DelimitedLiteral(inlines)
            | DelimitedBlockType::DelimitedPass(inlines)
            | DelimitedBlockType::DelimitedVerse(inlines) => {
                verify_inline_monotonic("DelimitedBlock", inlines);
            }
            DelimitedBlockType::DelimitedStem(_) => {}
            DelimitedBlockType::DelimitedTable(table) => {
                let all_rows = table
                    .header
                    .iter()
                    .chain(&table.rows)
                    .chain(table.footer.iter());
                for row in all_rows {
                    for col in &row.columns {
                        verify_monotonic_positions("Table cell", &col.content);
                    }
                }
            }
        },
        Block::UnorderedList(list) => {
            for item in &list.items {
                verify_inline_monotonic("UnorderedList item principal", &item.principal);
                verify_monotonic_positions("UnorderedList item child", &item.blocks);
            }
        }
        Block::OrderedList(list) => {
            for item in &list.items {
                verify_inline_monotonic("OrderedList item principal", &item.principal);
                verify_monotonic_positions("OrderedList item child", &item.blocks);
            }
        }
        Block::DescriptionList(list) => {
            for item in &list.items {
                verify_inline_monotonic("DescriptionList term", &item.term);
                verify_inline_monotonic("DescriptionList principal", &item.principal_text);
                verify_monotonic_positions("DescriptionList description", &item.description);
            }
        }
        Block::CalloutList(list) => {
            for item in &list.items {
                verify_inline_monotonic("CalloutList item principal", &item.principal);
                verify_monotonic_positions("CalloutList item child", &item.blocks);
            }
        }
        Block::DiscreteHeader(header) => {
            verify_inline_monotonic("DiscreteHeader title", &header.title);
        }
        Block::TableOfContents(_)
        | Block::DocumentAttribute(_)
        | Block::ThematicBreak(_)
        | Block::PageBreak(_)
        | Block::Image(_)
        | Block::Audio(_)
        | Block::Video(_)
        | Block::Comment(_) => {}
    }
}
