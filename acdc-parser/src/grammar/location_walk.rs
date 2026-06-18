//! One mutable structural walk over every [`Location`] in the AST.
//!
//! This centralizes the "which fields/children carry a location" knowledge so that
//! location-rewriting passes (the post-parse source remap, inline bounds clamping)
//! supply only a per-location closure and never re-implement the traversal. The walk
//! visits a node's own location first, then recurses into every inline/block-bearing
//! child exactly once.
//!
//! Functions are generic over `F: FnMut(&mut Location)` and pass `&mut F` down, so the
//! whole traversal monomorphizes to a single, allocation-free closure with no dynamic
//! dispatch.

use crate::model::{
    Block, BlockMetadata, DelimitedBlock, DelimitedBlockType, Document, InlineMacro, InlineNode,
    Location,
};

/// Visit every location in a whole document.
pub(crate) fn walk_document_locations_mut<F: FnMut(&mut Location)>(
    doc: &mut Document<'_>,
    visit: &mut F,
) {
    visit(&mut doc.location);
    if let Some(header) = &mut doc.header {
        visit(&mut header.location);
        walk_metadata(&mut header.metadata, visit);
        walk_inlines(header.title.inlines_mut(), visit);
        if let Some(subtitle) = &mut header.subtitle {
            walk_inlines(subtitle.inlines_mut(), visit);
        }
    }
    walk_blocks(&mut doc.blocks, visit);

    // `footnotes`, `toc_entries`, and `references` are independent owned copies (not
    // aliases of block-tree nodes), so each location is visited exactly once here. They
    // are `#[serde(skip)]` (absent from the ASG) but drive consumers — notably LSP
    // go-to-definition on `references` — which must land in the originating file. (The
    // footnote tracker finalizes its entries to document-absolute coordinates during
    // parsing, so they are safe to remap here like the rest.)
    for footnote in &mut doc.footnotes {
        visit(&mut footnote.location);
        walk_inlines(&mut footnote.content, visit);
    }
    for entry in &mut doc.toc_entries {
        visit(&mut entry.location);
        walk_inlines(entry.title.inlines_mut(), visit);
    }
    for reference in doc.references.values_mut() {
        visit(&mut reference.location);
        if let Some(title) = &mut reference.title {
            walk_inlines(title.inlines_mut(), visit);
        }
    }
}

/// Visit every location in a slice of blocks.
pub(crate) fn walk_blocks<F: FnMut(&mut Location)>(blocks: &mut [Block<'_>], visit: &mut F) {
    for block in blocks {
        walk_block_locations_mut(block, visit);
    }
}

/// Visit every location in a single block (its own location, metadata, title, and
/// any inline/block children).
pub(crate) fn walk_block_locations_mut<F: FnMut(&mut Location)>(
    block: &mut Block<'_>,
    visit: &mut F,
) {
    visit(block.location_mut());
    match block {
        Block::Section(s) => {
            walk_metadata(&mut s.metadata, visit);
            walk_inlines(s.title.inlines_mut(), visit);
            walk_blocks(&mut s.content, visit);
        }
        Block::Paragraph(p) => {
            walk_metadata(&mut p.metadata, visit);
            walk_inlines(p.title.inlines_mut(), visit);
            walk_inlines(&mut p.content, visit);
        }
        Block::UnorderedList(l) => {
            walk_metadata(&mut l.metadata, visit);
            walk_inlines(l.title.inlines_mut(), visit);
            for item in &mut l.items {
                visit(&mut item.location);
                walk_inlines(&mut item.principal, visit);
                walk_blocks(&mut item.blocks, visit);
            }
        }
        Block::OrderedList(l) => {
            walk_metadata(&mut l.metadata, visit);
            walk_inlines(l.title.inlines_mut(), visit);
            for item in &mut l.items {
                visit(&mut item.location);
                walk_inlines(&mut item.principal, visit);
                walk_blocks(&mut item.blocks, visit);
            }
        }
        Block::DescriptionList(l) => {
            walk_metadata(&mut l.metadata, visit);
            walk_inlines(l.title.inlines_mut(), visit);
            for item in &mut l.items {
                visit(&mut item.location);
                opt(&mut item.delimiter_location, visit);
                for anchor in &mut item.anchors {
                    visit(&mut anchor.location);
                }
                walk_inlines(&mut item.term, visit);
                walk_inlines(&mut item.principal_text, visit);
                walk_blocks(&mut item.description, visit);
            }
        }
        Block::CalloutList(l) => {
            walk_metadata(&mut l.metadata, visit);
            walk_inlines(l.title.inlines_mut(), visit);
            for item in &mut l.items {
                visit(&mut item.location);
                walk_inlines(&mut item.principal, visit);
                walk_blocks(&mut item.blocks, visit);
            }
        }
        Block::Admonition(a) => {
            walk_metadata(&mut a.metadata, visit);
            walk_inlines(a.title.inlines_mut(), visit);
            walk_blocks(&mut a.blocks, visit);
        }
        Block::DiscreteHeader(h) => {
            walk_metadata(&mut h.metadata, visit);
            walk_inlines(h.title.inlines_mut(), visit);
        }
        Block::DelimitedBlock(d) => walk_delimited_block(d, visit),
        Block::ThematicBreak(tb) => {
            walk_inlines(tb.title.inlines_mut(), visit);
            for anchor in &mut tb.anchors {
                visit(&mut anchor.location);
            }
        }
        Block::Image(i) => {
            walk_metadata(&mut i.metadata, visit);
            walk_inlines(i.title.inlines_mut(), visit);
        }
        Block::Audio(a) => walk_metadata(&mut a.metadata, visit),
        Block::Video(v) => walk_metadata(&mut v.metadata, visit),
        // Own location already visited; no inline/block children carrying locations.
        Block::TableOfContents(_)
        | Block::DocumentAttribute(_)
        | Block::PageBreak(_)
        | Block::Comment(_) => {}
    }
}

/// Visit every location in a slice of inline nodes.
pub(crate) fn walk_inlines<F: FnMut(&mut Location)>(nodes: &mut [InlineNode<'_>], visit: &mut F) {
    for node in nodes {
        walk_inline_locations_mut(node, visit);
    }
}

/// Visit a single inline node's location and recurse into any inline children.
pub(crate) fn walk_inline_locations_mut<F: FnMut(&mut Location)>(
    node: &mut InlineNode<'_>,
    visit: &mut F,
) {
    visit(node.location_mut());
    // The formatted-text variants are distinct struct types, so each needs its own
    // arm (they can't share an or-pattern binding).
    match node {
        InlineNode::BoldText(t) => walk_inlines(&mut t.content, visit),
        InlineNode::ItalicText(t) => walk_inlines(&mut t.content, visit),
        InlineNode::MonospaceText(t) => walk_inlines(&mut t.content, visit),
        InlineNode::HighlightText(t) => walk_inlines(&mut t.content, visit),
        InlineNode::SubscriptText(t) => walk_inlines(&mut t.content, visit),
        InlineNode::SuperscriptText(t) => walk_inlines(&mut t.content, visit),
        InlineNode::CurvedQuotationText(t) => walk_inlines(&mut t.content, visit),
        InlineNode::CurvedApostropheText(t) => walk_inlines(&mut t.content, visit),
        InlineNode::Macro(m) => walk_inline_macro(m, visit),
        InlineNode::PlainText(_)
        | InlineNode::RawText(_)
        | InlineNode::VerbatimText(_)
        | InlineNode::StandaloneCurvedApostrophe(_)
        | InlineNode::LineBreak(_)
        | InlineNode::InlineAnchor(_)
        | InlineNode::CalloutRef(_) => {}
    }
}

fn walk_inline_macro<F: FnMut(&mut Location)>(m: &mut InlineMacro<'_>, visit: &mut F) {
    // Own location already visited via `node.location_mut()`. Recurse into the
    // inline-bearing macro variants.
    match m {
        InlineMacro::Footnote(f) => walk_inlines(&mut f.content, visit),
        InlineMacro::Link(l) => walk_inlines(&mut l.text, visit),
        InlineMacro::Url(u) => walk_inlines(&mut u.text, visit),
        InlineMacro::Mailto(m) => walk_inlines(&mut m.text, visit),
        InlineMacro::CrossReference(x) => walk_inlines(&mut x.text, visit),
        InlineMacro::Icon(_)
        | InlineMacro::Image(_)
        | InlineMacro::Keyboard(_)
        | InlineMacro::Button(_)
        | InlineMacro::Menu(_)
        | InlineMacro::Autolink(_)
        | InlineMacro::Pass(_)
        | InlineMacro::Stem(_)
        | InlineMacro::IndexTerm(_) => {}
    }
}

fn walk_metadata<F: FnMut(&mut Location)>(metadata: &mut BlockMetadata<'_>, visit: &mut F) {
    opt(&mut metadata.location, visit);
    if let Some(anchor) = &mut metadata.id {
        visit(&mut anchor.location);
    }
    for anchor in &mut metadata.anchors {
        visit(&mut anchor.location);
    }
    // Quote/verse attribution and citetitle are inline-bearing and ASG-serialized,
    // so their locations need remapping like any other inline content.
    if let Some(attribution) = &mut metadata.attribution {
        walk_inlines(attribution.inlines_mut(), visit);
    }
    if let Some(citetitle) = &mut metadata.citetitle {
        walk_inlines(citetitle.inlines_mut(), visit);
    }
}

fn walk_delimited_block<F: FnMut(&mut Location)>(d: &mut DelimitedBlock<'_>, visit: &mut F) {
    walk_metadata(&mut d.metadata, visit);
    walk_inlines(d.title.inlines_mut(), visit);
    opt(&mut d.open_delimiter_location, visit);
    opt(&mut d.close_delimiter_location, visit);
    match &mut d.inner {
        DelimitedBlockType::DelimitedExample(blocks)
        | DelimitedBlockType::DelimitedOpen(blocks)
        | DelimitedBlockType::DelimitedSidebar(blocks)
        | DelimitedBlockType::DelimitedQuote(blocks) => walk_blocks(blocks, visit),
        DelimitedBlockType::DelimitedComment(inlines)
        | DelimitedBlockType::DelimitedListing(inlines)
        | DelimitedBlockType::DelimitedLiteral(inlines)
        | DelimitedBlockType::DelimitedPass(inlines)
        | DelimitedBlockType::DelimitedVerse(inlines) => walk_inlines(inlines, visit),
        DelimitedBlockType::DelimitedTable(table) => {
            visit(&mut table.location);
            let rows = table
                .header
                .iter_mut()
                .chain(table.rows.iter_mut())
                .chain(table.footer.iter_mut());
            for row in rows {
                for col in &mut row.columns {
                    walk_blocks(&mut col.content, visit);
                }
            }
        }
        DelimitedBlockType::DelimitedStem(_) => {}
    }
}

fn opt<F: FnMut(&mut Location)>(loc: &mut Option<Location>, visit: &mut F) {
    if let Some(loc) = loc {
        visit(loc);
    }
}
