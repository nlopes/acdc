//! Parser invariant tests using property-based testing
//!
//! These tests verify that certain properties hold for ANY input to the parser.
//! They're organized by priority:
//! - P0: Critical invariants (parser never panics, bounds checking)
//! - P1: Structural invariants (position monotonicity, serialization)
//! - P2: Behavioral invariants (`SafeMode`, preprocessing)

use proptest::prelude::*;
use std::rc::Rc;

use crate::{
    Block, Document, InlineNode, Location, Options,
    model::{Delimited, TableContent},
    parse, parse_inline,
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
        if let Ok(parsed) = parse(&input, &options) { let doc = parsed.document_storage();
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
        if let Ok(result) = crate::Preprocessor::process(&input, &options, Rc::default()) {
            // Parse the preprocessed input
            let arena = bumpalo::Bump::new();
            let mut state = crate::grammar::ParserState::new(&result.text, &arena, Rc::new(options.clone()));
            state.leveloffset_ranges = result.leveloffset_ranges;
            if let Ok(Ok(doc)) = crate::grammar::parse_document(&result.text, &mut state) {
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
        if let Ok(parsed) = parse(&input, &options) { let doc = parsed.document_storage();
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
        if let Ok(parsed) = parse(&input, &options) { let doc = parsed.document_storage();
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
        if let Ok(parsed) = parse(&input, &options) { let doc = parsed.document_storage();
            verify_monotonic_positions("DescriptionList", &doc.blocks);
        }
    }

    /// Rich document positions must be monotonically increasing.
    /// Tests cross-construct interaction with sections, lists, and blocks.
    #[test]
    fn rich_document_positions_monotonic(input in rich_document()) {
        let options = Options::default();
        if let Ok(parsed) = parse(&input, &options) { let doc = parsed.document_storage();
            verify_monotonic_positions("RichDocument", &doc.blocks);
        }
    }

    /// Nested list locations must be within input bounds.
    /// List nesting is inherently difficult with PEG parsers.
    #[test]
    fn nested_list_locations_in_bounds(input in nested_list_document()) {
        let options = Options::default();
        if let Ok(parsed) = parse(&input, &options) { let doc = parsed.document_storage();
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
        if let Ok(parsed) = parse(&input, &options) { let doc = parsed.document_storage();
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
        Block::DescriptionList(list) => walk_description_list_locations(list, visitor),
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
        Block::DiscreteHeading(header) => {
            for inline in header.title.as_inlines() {
                walk_inline_locations(inline, visitor);
            }
        }
        Block::DelimitedExample(crate::model::Example(delimited))
        | Block::DelimitedOpen(crate::model::Open(delimited))
        | Block::DelimitedSidebar(crate::model::Sidebar(delimited))
        | Block::DelimitedQuote(crate::model::Quote(delimited)) => {
            walk_delimited_frame_locations(delimited, visitor);
            for block in &delimited.body {
                walk_block_locations(block, visitor);
            }
        }
        Block::DelimitedComment(crate::model::DelimitedComment(delimited))
        | Block::DelimitedListing(crate::model::Listing(delimited))
        | Block::DelimitedLiteral(crate::model::Literal(delimited))
        | Block::DelimitedPassthrough(crate::model::Passthrough(delimited))
        | Block::DelimitedVerse(crate::model::Verse(delimited)) => {
            walk_delimited_frame_locations(delimited, visitor);
            for inline in &delimited.body {
                walk_inline_locations(inline, visitor);
            }
        }
        Block::DelimitedTable(crate::model::Table(delimited)) => {
            walk_delimited_frame_locations(delimited, visitor);
            walk_table_locations(&delimited.body, visitor);
        }
        Block::DelimitedStem(crate::model::StemBlock(delimited)) => {
            walk_delimited_frame_locations(delimited, visitor);
        }
        // Leaf blocks: no children to recurse into
        Block::TableOfContents(_)
        | Block::DocumentAttribute(_)
        | Block::ThematicBreak(_)
        | Block::PageBreak(_)
        | Block::Image(_)
        | Block::Audio(_)
        | Block::Video(_)
        | Block::Extension(_)
        | Block::Comment(_) => {}
    }
}

/// Walk all locations in a description list, calling `visitor` for each one.
fn walk_description_list_locations(
    list: &crate::DescriptionList,
    visitor: &mut impl FnMut(&Location, &str),
) {
    for item in &list.items {
        visitor(&item.location, "description list item");
        for term in &item.terms {
            visitor(&term.location, "description term");
            for inline in &term.content {
                walk_inline_locations(inline, visitor);
            }
        }
        if let Some(definition) = &item.definition {
            visitor(&definition.location, "description definition");
            for inline in definition.principal.iter().flatten() {
                walk_inline_locations(inline, visitor);
            }
            for child in &definition.blocks {
                walk_block_locations(child, visitor);
            }
        }
    }
}

/// Walk the delimiter and title locations shared by every delimited block.
fn walk_delimited_frame_locations<B>(
    delimited: &Delimited<'_, B>,
    visitor: &mut impl FnMut(&Location, &str),
) {
    if let Some(loc) = &delimited.open_delimiter_location {
        visitor(loc, "delimited block open delimiter");
    }
    if let Some(loc) = &delimited.close_delimiter_location {
        visitor(loc, "delimited block close delimiter");
    }
    for inline in delimited.title.as_inlines() {
        walk_inline_locations(inline, visitor);
    }
}

/// Walk all locations in a table body, calling `visitor` for each one.
fn walk_table_locations(table: &TableContent, visitor: &mut impl FnMut(&Location, &str)) {
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

/// Walk all locations in an inline tree, calling `visitor` for each one.
fn walk_inline_locations(inline: &InlineNode, visitor: &mut impl FnMut(&Location, &str)) {
    visitor(inline.location(), "inline");

    match inline {
        InlineNode::Bold(t) => {
            for child in &t.content {
                walk_inline_locations(child, visitor);
            }
        }
        InlineNode::Italic(t) => {
            for child in &t.content {
                walk_inline_locations(child, visitor);
            }
        }
        InlineNode::Monospace(t) => {
            for child in &t.content {
                walk_inline_locations(child, visitor);
            }
        }
        InlineNode::Highlight(t) => {
            for child in &t.content {
                walk_inline_locations(child, visitor);
            }
        }
        InlineNode::Subscript(t) => {
            for child in &t.content {
                walk_inline_locations(child, visitor);
            }
        }
        InlineNode::Superscript(t) => {
            for child in &t.content {
                walk_inline_locations(child, visitor);
            }
        }
        InlineNode::DoubleQuotedSpan(t) => {
            for child in &t.content {
                walk_inline_locations(child, visitor);
            }
        }
        InlineNode::SingleQuotedSpan(t) => {
            for child in &t.content {
                walk_inline_locations(child, visitor);
            }
        }
        // Leaf inlines: no children to recurse into
        InlineNode::Autolink(_)
        | InlineNode::Button(_)
        | InlineNode::CrossReference(_)
        | InlineNode::Footnote(_)
        | InlineNode::Icon(_)
        | InlineNode::Image(_)
        | InlineNode::IndexTerm(_)
        | InlineNode::KeyboardShortcut(_)
        | InlineNode::Link(_)
        | InlineNode::Mailto(_)
        | InlineNode::MenuPath(_)
        | InlineNode::Passthrough(_)
        | InlineNode::Stem(_)
        | InlineNode::UrlMacro(_)
        | InlineNode::Extension(_)
        | InlineNode::Plain(_)
        | InlineNode::Raw(_)
        | InlineNode::Verbatim(_)
        | InlineNode::StandaloneCurvedApostrophe(_)
        | InlineNode::Symbol(_)
        | InlineNode::CharacterReference(_)
        | InlineNode::LineBreak(_)
        | InlineNode::Anchor(_)
        | InlineNode::Callout(_) => {}
    }
}

/// Verify positions are monotonically increasing within scopes
fn verify_monotonic_positions(prefix: &str, blocks: &[Block]) {
    let mut last_end = 0;
    let mut previous: Option<&Block<'_>> = None;
    for block in blocks {
        let location = block.location();
        let start = location.absolute_start;
        // Metadata-line events precede the block whose source range contains them.
        let contained_metadata_event = previous.is_some_and(|previous| {
            let prefix_event = matches!(
                previous,
                Block::DocumentAttribute(_) | Block::Comment(_) | Block::DelimitedComment(_)
            );
            let previous_location = previous.location();
            prefix_event
                && location.absolute_start <= previous_location.absolute_start
                && location.absolute_end >= previous_location.absolute_end
        });
        assert!(
            start >= last_end || contained_metadata_event,
            "{prefix} starts at {start} but previous ended at {last_end}"
        );
        last_end = location.absolute_end;
        previous = Some(block);

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
        Block::DelimitedExample(crate::model::Example(delimited))
        | Block::DelimitedOpen(crate::model::Open(delimited))
        | Block::DelimitedSidebar(crate::model::Sidebar(delimited))
        | Block::DelimitedQuote(crate::model::Quote(delimited)) => {
            let blocks = &delimited.body;
            verify_monotonic_positions("DelimitedBlock child", blocks);
        }
        Block::DelimitedComment(crate::model::DelimitedComment(delimited))
        | Block::DelimitedListing(crate::model::Listing(delimited))
        | Block::DelimitedLiteral(crate::model::Literal(delimited))
        | Block::DelimitedPassthrough(crate::model::Passthrough(delimited))
        | Block::DelimitedVerse(crate::model::Verse(delimited)) => {
            let inlines = &delimited.body;
            verify_inline_monotonic("DelimitedBlock", inlines);
        }
        Block::DelimitedTable(crate::model::Table(delimited)) => {
            let table = &delimited.body;
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
                for term in &item.terms {
                    verify_inline_monotonic("DescriptionList term", &term.content);
                }
                if let Some(definition) = &item.definition {
                    if let Some(principal) = &definition.principal {
                        verify_inline_monotonic("DescriptionList principal", principal);
                    }
                    verify_monotonic_positions("DescriptionList description", &definition.blocks);
                }
            }
        }
        Block::CalloutList(list) => {
            for item in &list.items {
                verify_inline_monotonic("CalloutList item principal", &item.principal);
                verify_monotonic_positions("CalloutList item child", &item.blocks);
            }
        }
        Block::DiscreteHeading(header) => {
            verify_inline_monotonic("DiscreteHeader title", header.title.as_inlines());
        }
        Block::DelimitedStem(_)
        | Block::TableOfContents(_)
        | Block::DocumentAttribute(_)
        | Block::ThematicBreak(_)
        | Block::PageBreak(_)
        | Block::Image(_)
        | Block::Audio(_)
        | Block::Video(_)
        | Block::Extension(_)
        | Block::Comment(_) => {}
    }
}
