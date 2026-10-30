//! Shared mutable AST traversal for location and inline-node passes.
//!
//! The common walk keeps the child set and visit order consistent. Each callback
//! runs before the callbacks for a node's children.

use std::{convert::Infallible, marker::PhantomData};

use crate::model::{
    Anchor, Block, BlockMetadata, ContentAttribute, DelimitedBlock, DelimitedBlockType, Document,
    Header, InlineMacro, InlineNode, Location,
};

/// Visit every location in a whole document.
pub(crate) fn walk_document_locations_mut<F: FnMut(&mut Location)>(
    doc: &mut Document<'_>,
    visit: &mut F,
) {
    AstWalker::new(
        |location| {
            visit(location);
            Ok(())
        },
        |_| Ok(()),
        |_, _| Ok(()),
    )
    .document(doc)
    .unwrap_or_else(|never: Infallible| match never {});
}

/// Visit every inline node in a whole document.
pub(crate) fn walk_document_inline_nodes_mut<'a, F: FnMut(&mut InlineNode<'a>)>(
    doc: &mut Document<'a>,
    visit: &mut F,
) {
    AstWalker::new(
        |_| Ok(()),
        |node| {
            visit(node);
            Ok(())
        },
        |_, _| Ok(()),
    )
    .document(doc)
    .unwrap_or_else(|never: Infallible| match never {});
}

/// Visit a single inline node's location and recurse into any inline children.
pub(crate) fn walk_inline_locations_mut<F: FnMut(&mut Location)>(
    node: &mut InlineNode<'_>,
    visit: &mut F,
) {
    try_walk_inline_locations_mut(node, &mut |location| {
        visit(location);
        Ok(())
    })
    .unwrap_or_else(|never: Infallible| match never {});
}

/// Visit inline locations in preorder, stopping at the first error.
pub(crate) fn try_walk_inline_locations_mut<E>(
    node: &mut InlineNode<'_>,
    visit: &mut impl FnMut(&mut Location) -> Result<(), E>,
) -> Result<(), E> {
    AstWalker::new(visit, |_| Ok(()), |_, _| Ok(())).inline(node)
}

/// Visit an inline node, then recurse into its children.
pub(crate) fn walk_inline_nodes_mut<'a, F: FnMut(&mut InlineNode<'a>)>(
    node: &mut InlineNode<'a>,
    visit: &mut F,
) {
    AstWalker::new(
        |_| Ok(()),
        |node| {
            visit(node);
            Ok(())
        },
        |_, _| Ok(()),
    )
    .inline(node)
    .unwrap_or_else(|never: Infallible| match never {});
}

/// Visit reference-label owners before building the derived catalog.
pub(super) fn walk_reference_texts_mut<'a, E>(
    header: &mut Option<Header<'a>>,
    blocks: &mut [Block<'a>],
    visit: &mut impl FnMut(&mut Option<ContentAttribute<'a>>, &mut Option<usize>) -> Result<(), E>,
) -> Result<(), E> {
    let mut walker = AstWalker::new(|_| Ok(()), |_| Ok(()), visit);
    if let Some(header) = header {
        walker.header(header)?;
    }
    walker.blocks(blocks)
}

pub(super) fn walk_document_reference_texts_mut<'a, E>(
    document: &mut Document<'a>,
    visit: &mut impl FnMut(&mut Option<ContentAttribute<'a>>, &mut Option<usize>) -> Result<(), E>,
) -> Result<(), E> {
    AstWalker::new(|_| Ok(()), |_| Ok(()), visit).document(document)
}

pub(crate) fn walk_inline_reference_texts_mut<'a, E>(
    inlines: &mut [InlineNode<'a>],
    visit: &mut impl FnMut(&mut Option<ContentAttribute<'a>>, &mut Option<usize>) -> Result<(), E>,
) -> Result<(), E> {
    AstWalker::new(|_| Ok(()), |_| Ok(()), visit).inlines(inlines)
}

struct AstWalker<'a, FL, FI, FR> {
    visit_location: FL,
    visit_inline: FI,
    visit_reference_text: FR,
    inline_lifetime: PhantomData<fn(&mut InlineNode<'a>)>,
}

impl<'a, FL, FI, FR, E> AstWalker<'a, FL, FI, FR>
where
    FL: FnMut(&mut Location) -> Result<(), E>,
    FI: FnMut(&mut InlineNode<'a>) -> Result<(), E>,
    FR: FnMut(&mut Option<ContentAttribute<'a>>, &mut Option<usize>) -> Result<(), E>,
{
    fn new(visit_location: FL, visit_inline: FI, visit_reference_text: FR) -> Self {
        Self {
            visit_location,
            visit_inline,
            visit_reference_text,
            inline_lifetime: PhantomData,
        }
    }

    fn document(&mut self, doc: &mut Document<'a>) -> Result<(), E> {
        (self.visit_location)(&mut doc.location)?;
        if let Some(header) = &mut doc.header {
            self.header(header)?;
        }
        self.blocks(&mut doc.blocks)?;

        // `footnotes`, `toc_entries`, and `references` are independent owned copies (not
        // aliases of block-tree nodes), so each location is visited exactly once here. They
        // are `#[serde(skip)]` (absent from the ASG) but drive consumers — notably LSP
        // go-to-definition on `references` — which must land in the originating file. (The
        // footnote tracker finalizes its entries to document-absolute coordinates during
        // parsing, so they are safe to remap here like the rest.)
        for footnote in &mut doc.footnotes {
            (self.visit_location)(&mut footnote.location)?;
            self.inlines(&mut footnote.content)?;
        }
        for entry in &mut doc.toc_entries {
            (self.visit_location)(&mut entry.location)?;
            self.inlines(entry.title.inlines_mut())?;
            if let Some(label) = &mut entry.xreflabel {
                self.inlines(label.inlines_mut())?;
            }
        }
        for reference in doc.references.values_mut() {
            (self.visit_location)(&mut reference.location)?;
            if let Some(label) = &mut reference.xreflabel {
                self.inlines(label.inlines_mut())?;
            }
            if let Some(title) = &mut reference.title {
                self.inlines(title.inlines_mut())?;
            }
        }
        Ok(())
    }

    fn header(&mut self, header: &mut Header<'a>) -> Result<(), E> {
        (self.visit_location)(&mut header.location)?;
        self.metadata(&mut header.metadata)?;
        self.inlines(header.title.inlines_mut())?;
        if let Some(subtitle) = &mut header.subtitle {
            self.inlines(subtitle.inlines_mut())?;
        }
        Ok(())
    }

    fn anchor(&mut self, anchor: &mut Anchor<'a>) -> Result<(), E> {
        (self.visit_location)(&mut anchor.location)?;
        self.reference_text(&mut anchor.xreflabel, &mut anchor.reftext_source)?;
        if let Some(label) = &mut anchor.bibliography_label {
            self.inlines(&mut label.content)?;
        }
        Ok(())
    }

    fn reference_text(
        &mut self,
        content: &mut Option<ContentAttribute<'a>>,
        source: &mut Option<usize>,
    ) -> Result<(), E> {
        (self.visit_reference_text)(content, source)?;
        if let Some(content) = content {
            self.inlines(content.inlines_mut())?;
        }
        Ok(())
    }

    fn blocks(&mut self, blocks: &mut [Block<'a>]) -> Result<(), E> {
        for block in blocks {
            self.block(block)?;
        }
        Ok(())
    }

    fn block(&mut self, block: &mut Block<'a>) -> Result<(), E> {
        (self.visit_location)(block.location_mut())?;
        match block {
            Block::Section(s) => {
                self.metadata(&mut s.metadata)?;
                self.inlines(s.title.inlines_mut())?;
                self.blocks(&mut s.content)?;
            }
            Block::Paragraph(p) => {
                self.metadata(&mut p.metadata)?;
                self.inlines(p.title.inlines_mut())?;
                self.inlines(&mut p.content)?;
            }
            Block::UnorderedList(l) => {
                self.metadata(&mut l.metadata)?;
                self.inlines(l.title.inlines_mut())?;
                for item in &mut l.items {
                    (self.visit_location)(&mut item.location)?;
                    self.inlines(&mut item.principal)?;
                    self.blocks(&mut item.blocks)?;
                }
            }
            Block::OrderedList(l) => {
                self.metadata(&mut l.metadata)?;
                self.inlines(l.title.inlines_mut())?;
                for item in &mut l.items {
                    (self.visit_location)(&mut item.location)?;
                    self.inlines(&mut item.principal)?;
                    self.blocks(&mut item.blocks)?;
                }
            }
            Block::DescriptionList(l) => {
                self.metadata(&mut l.metadata)?;
                self.inlines(l.title.inlines_mut())?;
                for item in &mut l.items {
                    (self.visit_location)(&mut item.location)?;
                    self.optional_location(&mut item.delimiter_location)?;
                    for anchor in &mut item.anchors {
                        self.anchor(anchor)?;
                    }
                    self.inlines(&mut item.term)?;
                    self.inlines(&mut item.principal_text)?;
                    self.blocks(&mut item.description)?;
                }
            }
            Block::CalloutList(l) => {
                self.metadata(&mut l.metadata)?;
                self.inlines(l.title.inlines_mut())?;
                for item in &mut l.items {
                    (self.visit_location)(&mut item.location)?;
                    self.inlines(&mut item.principal)?;
                    self.blocks(&mut item.blocks)?;
                }
            }
            Block::Admonition(a) => {
                self.metadata(&mut a.metadata)?;
                self.inlines(a.title.inlines_mut())?;
                self.blocks(&mut a.blocks)?;
            }
            Block::DiscreteHeader(h) => {
                self.metadata(&mut h.metadata)?;
                self.inlines(h.title.inlines_mut())?;
            }
            Block::DelimitedBlock(d) => self.delimited_block(d)?,
            Block::ThematicBreak(tb) => {
                self.inlines(tb.title.inlines_mut())?;
                for anchor in &mut tb.anchors {
                    self.anchor(anchor)?;
                }
            }
            Block::Image(i) => {
                self.metadata(&mut i.metadata)?;
                self.inlines(i.title.inlines_mut())?;
                if let Some(alt) = &mut i.alt {
                    self.inlines(alt.inlines_mut())?;
                }
            }
            Block::Audio(a) => {
                self.metadata(&mut a.metadata)?;
                self.inlines(a.title.inlines_mut())?;
            }
            Block::Video(v) => {
                self.metadata(&mut v.metadata)?;
                self.inlines(v.title.inlines_mut())?;
            }
            Block::PageBreak(block) => {
                self.metadata(&mut block.metadata)?;
                self.inlines(block.title.inlines_mut())?;
            }
            Block::TableOfContents(block) => self.metadata(&mut block.metadata)?,
            Block::DocumentAttribute(_) | Block::Comment(_) => {}
        }
        Ok(())
    }

    fn inlines(&mut self, nodes: &mut [InlineNode<'a>]) -> Result<(), E> {
        for node in nodes {
            self.inline(node)?;
        }
        Ok(())
    }

    fn inline(&mut self, node: &mut InlineNode<'a>) -> Result<(), E> {
        (self.visit_location)(node.location_mut())?;
        (self.visit_inline)(node)?;
        // The formatted-text variants are distinct struct types, so each needs its own
        // arm (they can't share an or-pattern binding).
        match node {
            InlineNode::BoldText(t) => self.inlines(&mut t.content)?,
            InlineNode::ItalicText(t) => self.inlines(&mut t.content)?,
            InlineNode::MonospaceText(t) => self.inlines(&mut t.content)?,
            InlineNode::HighlightText(t) => self.inlines(&mut t.content)?,
            InlineNode::SubscriptText(t) => self.inlines(&mut t.content)?,
            InlineNode::SuperscriptText(t) => self.inlines(&mut t.content)?,
            InlineNode::CurvedQuotationText(t) => self.inlines(&mut t.content)?,
            InlineNode::CurvedApostropheText(t) => self.inlines(&mut t.content)?,
            InlineNode::Macro(m) => self.inline_macro(m)?,
            InlineNode::InlineAnchor(anchor) => {
                self.reference_text(&mut anchor.xreflabel, &mut anchor.reftext_source)?;
                if let Some(label) = &mut anchor.bibliography_label {
                    self.inlines(&mut label.content)?;
                }
            }
            InlineNode::PlainText(_)
            | InlineNode::RawText(_)
            | InlineNode::VerbatimText(_)
            | InlineNode::StandaloneCurvedApostrophe(_)
            | InlineNode::LineBreak(_)
            | InlineNode::CalloutRef(_) => {}
        }
        Ok(())
    }

    fn inline_macro(&mut self, m: &mut InlineMacro<'a>) -> Result<(), E> {
        m.try_for_each_content_attribute_mut(|nodes| self.inlines(nodes))?;
        // Own location already visited via `node.location_mut()`. Recurse into the
        // inline-bearing macro variants.
        match m {
            InlineMacro::Footnote(f) => self.inlines(&mut f.content)?,
            InlineMacro::Link(l) => {
                if let Some(id) = &mut l.metadata.id {
                    (self.visit_location)(&mut id.location)?;
                }
                self.inlines(&mut l.text)?;
            }
            InlineMacro::Url(u) => {
                if let Some(id) = &mut u.metadata.id {
                    (self.visit_location)(&mut id.location)?;
                }
                self.inlines(&mut u.text)?;
            }
            InlineMacro::Mailto(m) => {
                if let Some(id) = &mut m.metadata.id {
                    (self.visit_location)(&mut id.location)?;
                }
                self.inlines(&mut m.text)?;
            }
            InlineMacro::CrossReference(x) => {
                if let Some(id) = &mut x.metadata.id {
                    (self.visit_location)(&mut id.location)?;
                }
                self.inlines(&mut x.text)?;
            }
            InlineMacro::IndexTerm(term) => self.index_term(term)?,
            InlineMacro::Icon(icon) => {
                if let Some(id) = &mut icon.metadata.id {
                    (self.visit_location)(&mut id.location)?;
                }
            }
            InlineMacro::Image(image) => {
                if let Some(id) = &mut image.metadata.id {
                    (self.visit_location)(&mut id.location)?;
                }
            }
            InlineMacro::Keyboard(_)
            | InlineMacro::Button(_)
            | InlineMacro::Menu(_)
            | InlineMacro::Autolink(_)
            | InlineMacro::Pass(_)
            | InlineMacro::Stem(_) => {}
        }
        Ok(())
    }

    fn index_term(&mut self, term: &mut crate::IndexTerm<'a>) -> Result<(), E> {
        if let Some(id) = &mut term.metadata.id {
            (self.visit_location)(&mut id.location)?;
        }
        match &mut term.kind {
            crate::IndexTermKind::Flow(term) => self.inlines(term)?,
            crate::IndexTermKind::Concealed {
                term,
                secondary,
                tertiary,
            } => {
                self.inlines(term)?;
                if let Some(secondary) = secondary {
                    self.inlines(secondary)?;
                }
                if let Some(tertiary) = tertiary {
                    self.inlines(tertiary)?;
                }
            }
        }
        if let Some(relationship) = &mut term.relationship {
            match relationship {
                crate::IndexTermRelationship::See { target } => self.inlines(target)?,
                crate::IndexTermRelationship::SeeAlso { targets } => {
                    for target in targets {
                        self.inlines(target)?;
                    }
                }
            }
        }
        if let Some(catalog) = &mut term.catalog {
            // Catalog labels share locations but are not new occurrences.
            (self.visit_location)(&mut catalog.location)?;
            self.index_term(catalog)?;
        }
        Ok(())
    }

    fn metadata(&mut self, metadata: &mut BlockMetadata<'a>) -> Result<(), E> {
        self.optional_location(&mut metadata.location)?;
        self.reference_text(&mut metadata.reftext, &mut metadata.reftext_source)?;
        if let Some(crate::Caption::CustomContent(caption)) = &mut metadata.caption {
            self.inlines(caption.inlines_mut())?;
        }
        if let Some(anchor) = &mut metadata.id {
            self.anchor(anchor)?;
        }
        for anchor in &mut metadata.anchors {
            self.anchor(anchor)?;
        }
        for attribute in &mut metadata.positional_attributes {
            self.optional_location(&mut attribute.location)?;
        }
        if let Some(attributes) = &mut metadata.retained_positional_attributes {
            for attribute in attributes {
                self.optional_location(&mut attribute.location)?;
            }
        }
        // Quote/verse attribution and citetitle are inline-bearing and ASG-serialized,
        // so their locations need remapping like any other inline content.
        if let Some(attribution) = &mut metadata.attribution {
            self.inlines(attribution.inlines_mut())?;
        }
        if let Some(citetitle) = &mut metadata.citetitle {
            self.inlines(citetitle.inlines_mut())?;
        }
        Ok(())
    }

    fn delimited_block(&mut self, d: &mut DelimitedBlock<'a>) -> Result<(), E> {
        self.metadata(&mut d.metadata)?;
        self.inlines(d.title.inlines_mut())?;
        self.optional_location(&mut d.open_delimiter_location)?;
        self.optional_location(&mut d.close_delimiter_location)?;
        match &mut d.inner {
            DelimitedBlockType::DelimitedExample(blocks)
            | DelimitedBlockType::DelimitedOpen(blocks)
            | DelimitedBlockType::DelimitedSidebar(blocks)
            | DelimitedBlockType::DelimitedQuote(blocks) => self.blocks(blocks)?,
            DelimitedBlockType::DelimitedComment(nodes)
            | DelimitedBlockType::DelimitedListing(nodes)
            | DelimitedBlockType::DelimitedLiteral(nodes)
            | DelimitedBlockType::DelimitedPass(nodes)
            | DelimitedBlockType::DelimitedVerse(nodes) => self.inlines(nodes)?,
            DelimitedBlockType::DelimitedTable(table) => {
                (self.visit_location)(&mut table.location)?;
                let rows = table
                    .header
                    .iter_mut()
                    .chain(table.rows.iter_mut())
                    .chain(table.footer.iter_mut());
                for row in rows {
                    for col in &mut row.columns {
                        self.blocks(&mut col.content)?;
                    }
                }
            }
            DelimitedBlockType::DelimitedStem(_) => {}
        }
        Ok(())
    }

    fn optional_location(&mut self, location: &mut Option<Location>) -> Result<(), E> {
        if let Some(location) = location {
            (self.visit_location)(location)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{try_walk_inline_locations_mut, walk_inline_locations_mut};
    use crate::{Block, Options, parse};

    #[rstest::rstest]
    #[case("*Outer _nested_ tail*")]
    #[case("image:photo.png[*Alt*,title='Tip']")]
    #[case("footnote:[Body *bold*]")]
    #[case("((Term *bold*))")]
    fn error_stops_before_remaining_descendants_and_siblings(
        #[case] source: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let parsed = parse(source, &Options::default())?;
        let Some(Block::Paragraph(paragraph)) = parsed.document().blocks.first() else {
            return Err("expected a paragraph".into());
        };
        let original = paragraph.content.first().ok_or("missing inline")?;
        let mut expected = Vec::new();
        walk_inline_locations_mut(&mut original.clone(), &mut |location| {
            expected.push(location.clone());
        });
        assert!(expected.len() > 1, "the input must exercise nested content");

        for fail_at in 0..expected.len() {
            let mut visited = Vec::new();
            let result = try_walk_inline_locations_mut(&mut original.clone(), &mut |location| {
                let index = visited.len();
                visited.push(location.clone());
                if index == fail_at { Err(index) } else { Ok(()) }
            });
            assert_eq!(result, Err(fail_at));
            assert_eq!(
                visited,
                expected.get(..=fail_at).ok_or("missing visited prefix")?
            );
        }
        Ok(())
    }
}
