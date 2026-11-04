use acdc_parser::ast::{BlockKind, DocumentRef, HeaderRef};

use crate::TraversalContext;

use super::BlockVisitor;

/// Document framing hooks over typed reads.
///
/// The default traversal visits the frame, header, body, and supplements in that
/// order. Blocks go through [`TraversalContext::visit_block`]; callbacks select
/// their children and content. Framing and header hooks default to no-ops.
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
        self.visit_document_start(context, document)?;
        if let Some(header) = document.header() {
            self.visit_header(context, header)?;
        }
        self.visit_body_content_start(context, document)?;

        let first_section = document
            .blocks()
            .position(|block| matches!(block.kind(), BlockKind::Section(_)));
        let emit_preamble = document.header().is_some()
            && first_section.is_some_and(|index| {
                document.blocks().take(index).any(|block| {
                    !matches!(
                        block.kind(),
                        BlockKind::Comment(_) | BlockKind::DocumentAttribute(_)
                    )
                })
            });
        if emit_preamble {
            self.visit_preamble_start(context, document)?;
        }
        for (index, block) in document.blocks().enumerate() {
            if emit_preamble && first_section == Some(index) {
                self.visit_preamble_end(context, document)?;
            }
            context.visit_block(self, block)?;
        }
        self.visit_document_supplements(context, document)?;
        self.visit_document_end(context, document)
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
