//! Shared mutable AST traversal for location and inline-node passes.
//!
//! The common walk keeps the child set and visit order consistent. Each callback
//! runs before the callbacks for a node's children.

use std::marker::PhantomData;

use crate::model::{
    Block, BlockMetadata, DelimitedBlock, DelimitedBlockType, Document, InlineMacro, InlineNode,
    Location,
};

/// Visit every location in a whole document.
pub(crate) fn walk_document_locations_mut<F: FnMut(&mut Location)>(
    doc: &mut Document<'_>,
    visit: &mut F,
) {
    AstWalker::new(visit, |_| {}).document(doc);
}

/// Visit every inline node in a whole document.
pub(crate) fn walk_document_inline_nodes_mut<'a, F: FnMut(&mut InlineNode<'a>)>(
    doc: &mut Document<'a>,
    visit: &mut F,
) {
    AstWalker::new(|_| {}, visit).document(doc);
}

/// Visit a single inline node's location and recurse into any inline children.
pub(crate) fn walk_inline_locations_mut<F: FnMut(&mut Location)>(
    node: &mut InlineNode<'_>,
    visit: &mut F,
) {
    AstWalker::new(visit, |_| {}).inline(node);
}

struct AstWalker<'a, FL, FI> {
    visit_location: FL,
    visit_inline: FI,
    inline_lifetime: PhantomData<fn(&mut InlineNode<'a>)>,
}

impl<'a, FL, FI> AstWalker<'a, FL, FI>
where
    FL: FnMut(&mut Location),
    FI: FnMut(&mut InlineNode<'a>),
{
    fn new(visit_location: FL, visit_inline: FI) -> Self {
        Self {
            visit_location,
            visit_inline,
            inline_lifetime: PhantomData,
        }
    }

    fn document(&mut self, doc: &mut Document<'a>) {
        (self.visit_location)(&mut doc.location);
        if let Some(header) = &mut doc.header {
            (self.visit_location)(&mut header.location);
            self.metadata(&mut header.metadata);
            self.inlines(header.title.inlines_mut());
            if let Some(subtitle) = &mut header.subtitle {
                self.inlines(subtitle.inlines_mut());
            }
        }
        self.blocks(&mut doc.blocks);

        // `footnotes`, `toc_entries`, and `references` are independent owned copies (not
        // aliases of block-tree nodes), so each location is visited exactly once here. They
        // are `#[serde(skip)]` (absent from the ASG) but drive consumers — notably LSP
        // go-to-definition on `references` — which must land in the originating file. (The
        // footnote tracker finalizes its entries to document-absolute coordinates during
        // parsing, so they are safe to remap here like the rest.)
        for footnote in &mut doc.footnotes {
            (self.visit_location)(&mut footnote.location);
            self.inlines(&mut footnote.content);
        }
        for entry in &mut doc.toc_entries {
            (self.visit_location)(&mut entry.location);
            self.inlines(entry.title.inlines_mut());
        }
        for reference in doc.references.values_mut() {
            (self.visit_location)(&mut reference.location);
            if let Some(title) = &mut reference.title {
                self.inlines(title.inlines_mut());
            }
        }
    }

    fn blocks(&mut self, blocks: &mut [Block<'a>]) {
        for block in blocks {
            self.block(block);
        }
    }

    fn block(&mut self, block: &mut Block<'a>) {
        (self.visit_location)(block.location_mut());
        match block {
            Block::Section(s) => {
                self.metadata(&mut s.metadata);
                self.inlines(s.title.inlines_mut());
                self.blocks(&mut s.content);
            }
            Block::Paragraph(p) => {
                self.metadata(&mut p.metadata);
                self.inlines(p.title.inlines_mut());
                self.inlines(&mut p.content);
            }
            Block::UnorderedList(l) => {
                self.metadata(&mut l.metadata);
                self.inlines(l.title.inlines_mut());
                for item in &mut l.items {
                    (self.visit_location)(&mut item.location);
                    self.inlines(&mut item.principal);
                    self.blocks(&mut item.blocks);
                }
            }
            Block::OrderedList(l) => {
                self.metadata(&mut l.metadata);
                self.inlines(l.title.inlines_mut());
                for item in &mut l.items {
                    (self.visit_location)(&mut item.location);
                    self.inlines(&mut item.principal);
                    self.blocks(&mut item.blocks);
                }
            }
            Block::DescriptionList(l) => {
                self.metadata(&mut l.metadata);
                self.inlines(l.title.inlines_mut());
                for item in &mut l.items {
                    (self.visit_location)(&mut item.location);
                    self.optional_location(&mut item.delimiter_location);
                    for anchor in &mut item.anchors {
                        (self.visit_location)(&mut anchor.location);
                    }
                    self.inlines(&mut item.term);
                    self.inlines(&mut item.principal_text);
                    self.blocks(&mut item.description);
                }
            }
            Block::CalloutList(l) => {
                self.metadata(&mut l.metadata);
                self.inlines(l.title.inlines_mut());
                for item in &mut l.items {
                    (self.visit_location)(&mut item.location);
                    self.inlines(&mut item.principal);
                    self.blocks(&mut item.blocks);
                }
            }
            Block::Admonition(a) => {
                self.metadata(&mut a.metadata);
                self.inlines(a.title.inlines_mut());
                self.blocks(&mut a.blocks);
            }
            Block::DiscreteHeader(h) => {
                self.metadata(&mut h.metadata);
                self.inlines(h.title.inlines_mut());
            }
            Block::DelimitedBlock(d) => self.delimited_block(d),
            Block::ThematicBreak(tb) => {
                self.inlines(tb.title.inlines_mut());
                for anchor in &mut tb.anchors {
                    (self.visit_location)(&mut anchor.location);
                }
            }
            Block::Image(i) => {
                self.metadata(&mut i.metadata);
                self.inlines(i.title.inlines_mut());
            }
            Block::Audio(a) => self.metadata(&mut a.metadata),
            Block::Video(v) => self.metadata(&mut v.metadata),
            // Own location already visited; no inline or block children.
            Block::TableOfContents(_)
            | Block::DocumentAttribute(_)
            | Block::PageBreak(_)
            | Block::Comment(_) => {}
        }
    }

    fn inlines(&mut self, nodes: &mut [InlineNode<'a>]) {
        for node in nodes {
            self.inline(node);
        }
    }

    fn inline(&mut self, node: &mut InlineNode<'a>) {
        (self.visit_location)(node.location_mut());
        (self.visit_inline)(node);
        // The formatted-text variants are distinct struct types, so each needs its own
        // arm (they can't share an or-pattern binding).
        match node {
            InlineNode::BoldText(t) => self.inlines(&mut t.content),
            InlineNode::ItalicText(t) => self.inlines(&mut t.content),
            InlineNode::MonospaceText(t) => self.inlines(&mut t.content),
            InlineNode::HighlightText(t) => self.inlines(&mut t.content),
            InlineNode::SubscriptText(t) => self.inlines(&mut t.content),
            InlineNode::SuperscriptText(t) => self.inlines(&mut t.content),
            InlineNode::CurvedQuotationText(t) => self.inlines(&mut t.content),
            InlineNode::CurvedApostropheText(t) => self.inlines(&mut t.content),
            InlineNode::Macro(m) => self.inline_macro(m),
            InlineNode::PlainText(_)
            | InlineNode::RawText(_)
            | InlineNode::VerbatimText(_)
            | InlineNode::StandaloneCurvedApostrophe(_)
            | InlineNode::LineBreak(_)
            | InlineNode::InlineAnchor(_)
            | InlineNode::CalloutRef(_) => {}
        }
    }

    fn inline_macro(&mut self, m: &mut InlineMacro<'a>) {
        // Own location already visited via `node.location_mut()`. Recurse into the
        // inline-bearing macro variants.
        match m {
            InlineMacro::Footnote(f) => self.inlines(&mut f.content),
            InlineMacro::Link(l) => self.inlines(&mut l.text),
            InlineMacro::Url(u) => self.inlines(&mut u.text),
            InlineMacro::Mailto(m) => self.inlines(&mut m.text),
            InlineMacro::CrossReference(x) => self.inlines(&mut x.text),
            InlineMacro::IndexTerm(term) => {
                match &mut term.kind {
                    crate::IndexTermKind::Flow(term) => self.inlines(term),
                    crate::IndexTermKind::Concealed {
                        term,
                        secondary,
                        tertiary,
                    } => {
                        self.inlines(term);
                        if let Some(secondary) = secondary {
                            self.inlines(secondary);
                        }
                        if let Some(tertiary) = tertiary {
                            self.inlines(tertiary);
                        }
                    }
                }
                if let Some(relationship) = &mut term.relationship {
                    match relationship {
                        crate::IndexTermRelationship::See { target } => self.inlines(target),
                        crate::IndexTermRelationship::SeeAlso { targets } => {
                            for target in targets {
                                self.inlines(target);
                            }
                        }
                    }
                }
            }
            InlineMacro::Icon(_)
            | InlineMacro::Image(_)
            | InlineMacro::Keyboard(_)
            | InlineMacro::Button(_)
            | InlineMacro::Menu(_)
            | InlineMacro::Autolink(_)
            | InlineMacro::Pass(_)
            | InlineMacro::Stem(_) => {}
        }
    }

    fn metadata(&mut self, metadata: &mut BlockMetadata<'a>) {
        self.optional_location(&mut metadata.location);
        if let Some(anchor) = &mut metadata.id {
            (self.visit_location)(&mut anchor.location);
        }
        for anchor in &mut metadata.anchors {
            (self.visit_location)(&mut anchor.location);
        }
        // Quote/verse attribution and citetitle are inline-bearing and ASG-serialized,
        // so their locations need remapping like any other inline content.
        if let Some(attribution) = &mut metadata.attribution {
            self.inlines(attribution.inlines_mut());
        }
        if let Some(citetitle) = &mut metadata.citetitle {
            self.inlines(citetitle.inlines_mut());
        }
    }

    fn delimited_block(&mut self, d: &mut DelimitedBlock<'a>) {
        self.metadata(&mut d.metadata);
        self.inlines(d.title.inlines_mut());
        self.optional_location(&mut d.open_delimiter_location);
        self.optional_location(&mut d.close_delimiter_location);
        match &mut d.inner {
            DelimitedBlockType::DelimitedExample(blocks)
            | DelimitedBlockType::DelimitedOpen(blocks)
            | DelimitedBlockType::DelimitedSidebar(blocks)
            | DelimitedBlockType::DelimitedQuote(blocks) => self.blocks(blocks),
            DelimitedBlockType::DelimitedComment(nodes)
            | DelimitedBlockType::DelimitedListing(nodes)
            | DelimitedBlockType::DelimitedLiteral(nodes)
            | DelimitedBlockType::DelimitedPass(nodes)
            | DelimitedBlockType::DelimitedVerse(nodes) => self.inlines(nodes),
            DelimitedBlockType::DelimitedTable(table) => {
                (self.visit_location)(&mut table.location);
                let rows = table
                    .header
                    .iter_mut()
                    .chain(table.rows.iter_mut())
                    .chain(table.footer.iter_mut());
                for row in rows {
                    for col in &mut row.columns {
                        self.blocks(&mut col.content);
                    }
                }
            }
            DelimitedBlockType::DelimitedStem(_) => {}
        }
    }

    fn optional_location(&mut self, location: &mut Option<Location>) {
        if let Some(location) = location {
            (self.visit_location)(location);
        }
    }
}
