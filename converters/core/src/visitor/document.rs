use acdc_parser::ast::{BlockKind, BlockRef, DocumentRef, HeaderRef};

use crate::TraversalContext;

use super::BlockVisitor;

/// Document framing hooks over typed reads.
///
/// The default traversal shares the existing [`super::Visitor`] hook order and
/// preamble rules. Blocks go through [`TraversalContext::visit_block`]; callbacks
/// select their children and content. Framing and header hooks default to no-ops.
pub trait DocumentVisitor<'doc>: BlockVisitor<'doc> {
    /// Visit the frame, header, top-level blocks, and supplements.
    ///
    /// A preamble is wrapped only when a header and section are present and a
    /// preceding block is neither a comment nor a document attribute assignment.
    ///
    /// # Errors
    ///
    /// Returns the first callback error without running later hooks.
    fn visit_document(
        &mut self,
        context: &mut TraversalContext<'doc>,
        document: DocumentRef<'doc>,
    ) -> Result<(), Self::Error> {
        walk(
            document.header(),
            || document.blocks(),
            |event| match event {
                Event::Start => self.visit_document_start(context, document),
                Event::Header(header) => self.visit_header(context, header),
                Event::BodyStart => self.visit_body_content_start(context, document),
                Event::PreambleStart => self.visit_preamble_start(context, document),
                Event::PreambleEnd => self.visit_preamble_end(context, document),
                Event::Block(block) => context.visit_block(self, block),
                Event::Supplements => self.visit_document_supplements(context, document),
                Event::End => self.visit_document_end(context, document),
            },
        )
    }

    /// Start the document frame before visiting its header or body.
    ///
    /// # Errors
    ///
    /// Return an error to stop traversal.
    fn visit_document_start(
        &mut self,
        _context: &mut TraversalContext<'doc>,
        _document: DocumentRef<'doc>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Handle the header, selecting any title, subtitle, or author content.
    ///
    /// # Errors
    ///
    /// Return an error to stop traversal.
    fn visit_header(
        &mut self,
        _context: &mut TraversalContext<'doc>,
        _header: HeaderRef<'doc>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Start the body after the optional header and before any blocks.
    ///
    /// # Errors
    ///
    /// Return an error to stop traversal.
    fn visit_body_content_start(
        &mut self,
        _context: &mut TraversalContext<'doc>,
        _document: DocumentRef<'doc>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Open a preamble wrapper before its first block.
    ///
    /// # Errors
    ///
    /// Return an error to stop traversal.
    fn visit_preamble_start(
        &mut self,
        _context: &mut TraversalContext<'doc>,
        _document: DocumentRef<'doc>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Close the preamble wrapper before dispatching the first section.
    ///
    /// # Errors
    ///
    /// Return an error to stop traversal.
    fn visit_preamble_end(
        &mut self,
        _context: &mut TraversalContext<'doc>,
        _document: DocumentRef<'doc>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Handle supplements after the body and before closing the frame.
    ///
    /// # Errors
    ///
    /// Return an error to stop traversal.
    fn visit_document_supplements(
        &mut self,
        _context: &mut TraversalContext<'doc>,
        _document: DocumentRef<'doc>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Close the document frame after its supplements.
    ///
    /// # Errors
    ///
    /// Return an error to stop traversal.
    fn visit_document_end(
        &mut self,
        _context: &mut TraversalContext<'doc>,
        _document: DocumentRef<'doc>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
}

pub(super) enum Event<B, H> {
    Start,
    Header(H),
    BodyStart,
    PreambleStart,
    PreambleEnd,
    Block(B),
    Supplements,
    End,
}

pub(super) fn walk<'doc, B, H, I, E>(
    header: Option<H>,
    blocks: impl Fn() -> I,
    mut visit: impl FnMut(Event<B, H>) -> Result<(), E>,
) -> Result<(), E>
where
    B: Copy + Into<BlockRef<'doc>>,
    H: Copy,
    I: Iterator<Item = B>,
{
    visit(Event::Start)?;
    if let Some(header) = header {
        visit(Event::Header(header))?;
    }
    visit(Event::BodyStart)?;

    let first_section =
        blocks().position(|block| matches!(block.into().kind(), BlockKind::Section(_)));
    let emit_preamble = header.is_some()
        && first_section.is_some_and(|index| {
            blocks().take(index).any(|block| {
                !matches!(
                    block.into().kind(),
                    BlockKind::Comment(_) | BlockKind::DocumentAttribute(_)
                )
            })
        });

    if emit_preamble {
        visit(Event::PreambleStart)?;
    }
    for (index, block) in blocks().enumerate() {
        if emit_preamble && first_section == Some(index) {
            visit(Event::PreambleEnd)?;
        }
        visit(Event::Block(block))?;
    }

    visit(Event::Supplements)?;
    visit(Event::End)
}
