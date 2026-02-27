//! Parser invariant tests using property-based testing
//!
//! These tests verify that certain properties hold for ANY input to the parser.
//! They're organized by priority:
//! - P0: Critical invariants (parser never panics, bounds checking)
//! - P1: Structural invariants (position monotonicity, serialization)
//! - P2: Behavioral invariants (`SafeMode`, preprocessing)

use proptest::prelude::*;

use crate::{
    Block, Document, InlineNode, Location, Options, model::Locateable, parse, parse_inline,
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
        if let Ok(doc) = parse(&input, &options) {
            verify_all_locations_bounded(&doc, input.len());
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
            let mut state = crate::grammar::ParserState::new(&result.text);
            state.document_attributes = options.document_attributes.clone();
            state.options = options.clone();
            state.leveloffset_ranges = result.leveloffset_ranges;
            if let Ok(Ok(doc)) = crate::grammar::document_parser::document(&result.text, &mut state) {
                // Verify UTF-8 boundaries against the preprocessed input, not the original
                verify_utf8_boundaries(&doc, &result.text);
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
        if let Ok(doc) = parse(&input, &options) {
            verify_monotonic_positions("Block", &doc.blocks);
        }
    }

}

// ====================================================================
// Helper functions for invariant verification
// ====================================================================

/// Recursively verify all Location structs have offsets ≤ input length
fn verify_all_locations_bounded(doc: &Document, input_len: usize) {
    // Check document location
    verify_location_bounded(&doc.location, input_len, "document");

    // Check all blocks
    for block in &doc.blocks {
        verify_block_locations_bounded(block, input_len);
    }

    // Check header blocks if present
    if let Some(header) = &doc.header {
        verify_location_bounded(&header.location, input_len, "header");
    }
}

fn verify_block_locations_bounded(block: &Block, input_len: usize) {
    // Check block's own location based on type
    match block {
        Block::Section(section) => {
            verify_location_bounded(&section.location, input_len, "section");
            for child in &section.content {
                verify_block_locations_bounded(child, input_len);
            }
        }
        Block::Paragraph(para) => {
            verify_location_bounded(&para.location, input_len, "paragraph");
            for inline in &para.content {
                verify_inline_locations_bounded(inline, input_len);
            }
        }
        Block::UnorderedList(list) => {
            verify_location_bounded(&list.location, input_len, "unordered list");
            // Check list items recursively
            for item in &list.items {
                verify_location_bounded(&item.location, input_len, "list item");
                for block in &item.blocks {
                    verify_block_locations_bounded(block, input_len);
                }
            }
        }
        Block::OrderedList(list) => {
            verify_location_bounded(&list.location, input_len, "ordered list");
            for item in &list.items {
                verify_location_bounded(&item.location, input_len, "list item");
                for block in &item.blocks {
                    verify_block_locations_bounded(block, input_len);
                }
            }
        }
        Block::DescriptionList(list) => {
            verify_location_bounded(&list.location, input_len, "description list");
            for item in &list.items {
                verify_location_bounded(&item.location, input_len, "description list item");
                // Check term
                for inline in &item.term {
                    verify_inline_locations_bounded(inline, input_len);
                }
                // Check description blocks
                for block in &item.description {
                    verify_block_locations_bounded(block, input_len);
                }
            }
        }
        Block::CalloutList(list) => {
            verify_location_bounded(&list.location, input_len, "callout list");
            for item in &list.items {
                verify_location_bounded(&item.location, input_len, "callout item");
                for block in &item.blocks {
                    verify_block_locations_bounded(block, input_len);
                }
            }
        }
        Block::DelimitedBlock(delimited) => {
            verify_location_bounded(&delimited.location, input_len, "delimited block");
            // DelimitedBlock contains inner blocks in its type
        }
        Block::Admonition(admonition) => {
            verify_location_bounded(&admonition.location, input_len, "admonition");
            for block in &admonition.blocks {
                verify_block_locations_bounded(block, input_len);
            }
        }
        Block::TableOfContents(toc) => {
            verify_location_bounded(&toc.location, input_len, "table of contents");
        }
        Block::DiscreteHeader(header) => {
            verify_location_bounded(&header.location, input_len, "discrete header");
            for inline in &header.title {
                verify_inline_locations_bounded(inline, input_len);
            }
        }
        Block::DocumentAttribute(attr) => {
            verify_location_bounded(&attr.location, input_len, "document attribute");
        }
        Block::ThematicBreak(br) => {
            verify_location_bounded(&br.location, input_len, "thematic break");
        }
        Block::PageBreak(pb) => {
            verify_location_bounded(&pb.location, input_len, "page break");
        }
        Block::Image(img) => {
            verify_location_bounded(&img.location, input_len, "image");
        }
        Block::Audio(audio) => {
            verify_location_bounded(&audio.location, input_len, "audio");
        }
        Block::Video(video) => {
            verify_location_bounded(&video.location, input_len, "video");
        }
        Block::Comment(comment) => {
            verify_location_bounded(&comment.location, input_len, "comment");
        }
    }
}

#[allow(clippy::too_many_lines)]
fn verify_inline_locations_bounded(inline: &InlineNode, input_len: usize) {
    // Check inline's own location based on type
    match inline {
        InlineNode::PlainText(text) => {
            verify_location_bounded(&text.location, input_len, "plain text");
        }
        InlineNode::RawText(text) => {
            verify_location_bounded(&text.location, input_len, "raw text");
        }
        InlineNode::VerbatimText(text) => {
            verify_location_bounded(&text.location, input_len, "verbatim text");
        }
        InlineNode::BoldText(text) => {
            verify_location_bounded(&text.location, input_len, "bold text");
            for child in &text.content {
                verify_inline_locations_bounded(child, input_len);
            }
        }
        InlineNode::ItalicText(text) => {
            verify_location_bounded(&text.location, input_len, "italic text");
            for child in &text.content {
                verify_inline_locations_bounded(child, input_len);
            }
        }
        InlineNode::MonospaceText(text) => {
            verify_location_bounded(&text.location, input_len, "monospace text");
            for child in &text.content {
                verify_inline_locations_bounded(child, input_len);
            }
        }
        InlineNode::HighlightText(text) => {
            verify_location_bounded(&text.location, input_len, "highlight text");
            for child in &text.content {
                verify_inline_locations_bounded(child, input_len);
            }
        }
        InlineNode::SubscriptText(text) => {
            verify_location_bounded(&text.location, input_len, "subscript text");
            for child in &text.content {
                verify_inline_locations_bounded(child, input_len);
            }
        }
        InlineNode::SuperscriptText(text) => {
            verify_location_bounded(&text.location, input_len, "superscript text");
            for child in &text.content {
                verify_inline_locations_bounded(child, input_len);
            }
        }
        InlineNode::CurvedQuotationText(text) => {
            verify_location_bounded(&text.location, input_len, "curved quotation");
            for child in &text.content {
                verify_inline_locations_bounded(child, input_len);
            }
        }
        InlineNode::CurvedApostropheText(text) => {
            verify_location_bounded(&text.location, input_len, "curved apostrophe");
            for child in &text.content {
                verify_inline_locations_bounded(child, input_len);
            }
        }
        InlineNode::StandaloneCurvedApostrophe(text) => {
            verify_location_bounded(&text.location, input_len, "standalone curved apostrophe");
        }
        InlineNode::LineBreak(lb) => {
            verify_location_bounded(&lb.location, input_len, "line break");
        }
        InlineNode::InlineAnchor(anchor) => {
            verify_location_bounded(&anchor.location, input_len, "inline anchor");
        }
        InlineNode::Macro(m) => {
            // Macros have various subtypes, check their locations
            match m {
                crate::InlineMacro::Footnote(f) => {
                    verify_location_bounded(&f.location, input_len, "footnote");
                }
                crate::InlineMacro::Icon(i) => {
                    verify_location_bounded(&i.location, input_len, "icon");
                }
                crate::InlineMacro::Image(img) => {
                    verify_location_bounded(&img.location, input_len, "inline image");
                }
                crate::InlineMacro::Keyboard(k) => {
                    verify_location_bounded(&k.location, input_len, "keyboard");
                }
                crate::InlineMacro::Button(b) => {
                    verify_location_bounded(&b.location, input_len, "button");
                }
                crate::InlineMacro::Menu(m) => {
                    verify_location_bounded(&m.location, input_len, "menu");
                }
                crate::InlineMacro::Url(u) => {
                    verify_location_bounded(&u.location, input_len, "url");
                }
                crate::InlineMacro::Mailto(m) => {
                    verify_location_bounded(&m.location, input_len, "mailto");
                }
                crate::InlineMacro::Link(l) => {
                    verify_location_bounded(&l.location, input_len, "link");
                }
                crate::InlineMacro::Autolink(a) => {
                    verify_location_bounded(&a.location, input_len, "autolink");
                }
                crate::InlineMacro::CrossReference(x) => {
                    verify_location_bounded(&x.location, input_len, "cross reference");
                }
                crate::InlineMacro::Pass(p) => {
                    verify_location_bounded(&p.location, input_len, "pass");
                }
                crate::InlineMacro::Stem(s) => {
                    verify_location_bounded(&s.location, input_len, "stem");
                }
                crate::InlineMacro::IndexTerm(i) => {
                    verify_location_bounded(&i.location, input_len, "index term");
                }
            }
        }
        InlineNode::CalloutRef(callout) => {
            verify_location_bounded(&callout.location, input_len, "callout ref");
        }
    }
}

fn verify_location_bounded(loc: &Location, input_len: usize, context: &str) {
    let start_byte = loc.absolute_start;
    let end_byte = loc.absolute_end;

    assert!(
        start_byte <= input_len,
        "{context} location start {start_byte} exceeds input length {input_len}"
    );
    assert!(
        end_byte <= input_len,
        "{context} location end {end_byte} exceeds input length {input_len}"
    );
    assert!(
        start_byte <= end_byte,
        "{context} location has start {start_byte} > end {end_byte}"
    );
}

/// Verify all byte offsets fall on UTF-8 character boundaries
fn verify_utf8_boundaries(doc: &Document, input: &str) {
    // Check document location
    verify_location_utf8(&doc.location, input, "document");

    // Check all blocks
    for block in &doc.blocks {
        verify_block_utf8_boundaries(block, input);
    }

    // Check header if present
    if let Some(header) = &doc.header {
        verify_location_utf8(&header.location, input, "header");
    }
}

fn verify_block_utf8_boundaries(block: &Block, input: &str) {
    match block {
        Block::Section(section) => {
            verify_location_utf8(&section.location, input, "section");
            for child in &section.content {
                verify_block_utf8_boundaries(child, input);
            }
        }
        Block::Paragraph(para) => {
            verify_location_utf8(&para.location, input, "paragraph");
            for inline in &para.content {
                verify_inline_utf8_boundaries(inline, input);
            }
        }
        Block::UnorderedList(list) => {
            verify_location_utf8(&list.location, input, "unordered list");
            for item in &list.items {
                verify_location_utf8(&item.location, input, "list item");
                for block in &item.blocks {
                    verify_block_utf8_boundaries(block, input);
                }
            }
        }
        Block::OrderedList(list) => {
            verify_location_utf8(&list.location, input, "ordered list");
            for item in &list.items {
                verify_location_utf8(&item.location, input, "list item");
                for block in &item.blocks {
                    verify_block_utf8_boundaries(block, input);
                }
            }
        }
        Block::DescriptionList(list) => {
            verify_location_utf8(&list.location, input, "description list");
            for item in &list.items {
                verify_location_utf8(&item.location, input, "description list item");
                for block in &item.description {
                    verify_block_utf8_boundaries(block, input);
                }
            }
        }
        Block::CalloutList(list) => {
            verify_location_utf8(&list.location, input, "callout list");
            for item in &list.items {
                verify_location_utf8(&item.location, input, "callout item");
                for block in &item.blocks {
                    verify_block_utf8_boundaries(block, input);
                }
            }
        }
        Block::DelimitedBlock(delimited) => {
            verify_location_utf8(&delimited.location, input, "delimited block");
        }
        Block::Admonition(admonition) => {
            verify_location_utf8(&admonition.location, input, "admonition");
            for block in &admonition.blocks {
                verify_block_utf8_boundaries(block, input);
            }
        }
        Block::TableOfContents(toc) => {
            verify_location_utf8(&toc.location, input, "table of contents");
        }
        Block::DiscreteHeader(header) => {
            verify_location_utf8(&header.location, input, "discrete header");
            for inline in &header.title {
                verify_inline_utf8_boundaries(inline, input);
            }
        }
        Block::DocumentAttribute(attr) => {
            verify_location_utf8(&attr.location, input, "document attribute");
        }
        Block::ThematicBreak(br) => {
            verify_location_utf8(&br.location, input, "thematic break");
        }
        Block::PageBreak(pb) => {
            verify_location_utf8(&pb.location, input, "page break");
        }
        Block::Image(img) => {
            verify_location_utf8(&img.location, input, "image");
        }
        Block::Audio(audio) => {
            verify_location_utf8(&audio.location, input, "audio");
        }
        Block::Video(video) => {
            verify_location_utf8(&video.location, input, "video");
        }
        Block::Comment(comment) => {
            verify_location_utf8(&comment.location, input, "comment");
        }
    }
}

#[allow(clippy::too_many_lines)]
fn verify_inline_utf8_boundaries(inline: &InlineNode, input: &str) {
    match inline {
        InlineNode::PlainText(text) => {
            verify_location_utf8(&text.location, input, "plain text");
        }
        InlineNode::RawText(text) => {
            verify_location_utf8(&text.location, input, "raw text");
        }
        InlineNode::VerbatimText(text) => {
            verify_location_utf8(&text.location, input, "verbatim text");
        }
        InlineNode::BoldText(text) => {
            verify_location_utf8(&text.location, input, "bold text");
            for child in &text.content {
                verify_inline_utf8_boundaries(child, input);
            }
        }
        InlineNode::ItalicText(text) => {
            verify_location_utf8(&text.location, input, "italic text");
            for child in &text.content {
                verify_inline_utf8_boundaries(child, input);
            }
        }
        InlineNode::MonospaceText(text) => {
            verify_location_utf8(&text.location, input, "monospace text");
            for child in &text.content {
                verify_inline_utf8_boundaries(child, input);
            }
        }
        InlineNode::HighlightText(text) => {
            verify_location_utf8(&text.location, input, "highlight text");
            for child in &text.content {
                verify_inline_utf8_boundaries(child, input);
            }
        }
        InlineNode::SubscriptText(text) => {
            verify_location_utf8(&text.location, input, "subscript text");
            for child in &text.content {
                verify_inline_utf8_boundaries(child, input);
            }
        }
        InlineNode::SuperscriptText(text) => {
            verify_location_utf8(&text.location, input, "superscript text");
            for child in &text.content {
                verify_inline_utf8_boundaries(child, input);
            }
        }
        InlineNode::CurvedQuotationText(text) => {
            verify_location_utf8(&text.location, input, "curved quotation");
            for child in &text.content {
                verify_inline_utf8_boundaries(child, input);
            }
        }
        InlineNode::CurvedApostropheText(text) => {
            verify_location_utf8(&text.location, input, "curved apostrophe");
            for child in &text.content {
                verify_inline_utf8_boundaries(child, input);
            }
        }
        InlineNode::StandaloneCurvedApostrophe(text) => {
            verify_location_utf8(&text.location, input, "standalone curved apostrophe");
        }
        InlineNode::LineBreak(lb) => {
            verify_location_utf8(&lb.location, input, "line break");
        }
        InlineNode::InlineAnchor(anchor) => {
            verify_location_utf8(&anchor.location, input, "inline anchor");
        }
        InlineNode::Macro(m) => match m {
            crate::InlineMacro::Footnote(f) => {
                verify_location_utf8(&f.location, input, "footnote");
            }
            crate::InlineMacro::Icon(i) => {
                verify_location_utf8(&i.location, input, "icon");
            }
            crate::InlineMacro::Image(img) => {
                verify_location_utf8(&img.location, input, "inline image");
            }
            crate::InlineMacro::Keyboard(k) => {
                verify_location_utf8(&k.location, input, "keyboard");
            }
            crate::InlineMacro::Button(b) => {
                verify_location_utf8(&b.location, input, "button");
            }
            crate::InlineMacro::Menu(m) => {
                verify_location_utf8(&m.location, input, "menu");
            }
            crate::InlineMacro::Url(u) => {
                verify_location_utf8(&u.location, input, "url");
            }
            crate::InlineMacro::Mailto(m) => {
                verify_location_utf8(&m.location, input, "mailto");
            }
            crate::InlineMacro::Link(l) => {
                verify_location_utf8(&l.location, input, "link");
            }
            crate::InlineMacro::Autolink(a) => {
                verify_location_utf8(&a.location, input, "autolink");
            }
            crate::InlineMacro::CrossReference(x) => {
                verify_location_utf8(&x.location, input, "cross reference");
            }
            crate::InlineMacro::Pass(p) => {
                verify_location_utf8(&p.location, input, "pass");
            }
            crate::InlineMacro::Stem(s) => {
                verify_location_utf8(&s.location, input, "stem");
            }
            crate::InlineMacro::IndexTerm(i) => {
                verify_location_utf8(&i.location, input, "index term");
            }
        },
        InlineNode::CalloutRef(callout) => {
            verify_location_utf8(&callout.location, input, "callout ref");
        }
    }
}

fn verify_location_utf8(loc: &Location, input: &str, context: &str) {
    let start_byte = loc.absolute_start;
    let end_byte = loc.absolute_end;

    assert!(
        input.is_char_boundary(start_byte),
        "{context} location start {start_byte} is not on a UTF-8 boundary"
    );
    assert!(
        input.is_char_boundary(end_byte),
        "{context} location end {end_byte} is not on a UTF-8 boundary"
    );
}

/// Verify positions are monotonically increasing within scopes
fn verify_monotonic_positions(prefix: &str, blocks: &[Block]) {
    // Check blocks are in order
    let mut last_end = 0;
    for block in blocks {
        let location = block.location();
        let start = location.absolute_start;
        assert!(
            start >= last_end,
            "{prefix} starts at {start} but previous ended at {last_end}"
        );
        last_end = location.absolute_end;

        // Recursively check within blocks
        verify_block_monotonic(block);
    }
}

fn verify_block_monotonic(block: &Block) {
    match block {
        Block::Section(section) => {
            verify_monotonic_positions("Section child", &section.content);
        }
        Block::Paragraph(para) => {
            let mut last_end = 0;
            for inline in &para.content {
                let location = inline.location();
                let start = location.absolute_start;
                assert!(
                    start >= last_end,
                    "Inline starts at {start} but previous ended at {last_end}"
                );
                last_end = location.absolute_end;
            }
        }
        Block::Admonition(_)
        | Block::DelimitedBlock(_)
        | Block::TableOfContents(_)
        | Block::DiscreteHeader(_)
        | Block::DocumentAttribute(_)
        | Block::ThematicBreak(_)
        | Block::PageBreak(_)
        | Block::Image(_)
        | Block::Audio(_)
        | Block::Video(_)
        | Block::UnorderedList(_)
        | Block::OrderedList(_)
        | Block::DescriptionList(_)
        | Block::CalloutList(_)
        | Block::Comment(_) => {
            // Add other block types as needed
        }
    }
}
