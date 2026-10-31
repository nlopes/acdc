use acdc_parser::{Block, ast::BlockRef};

use crate::TraversalContext;

use super::Visitor;

/// Block callbacks with source-ordered document attributes.
///
/// The default input is `BlockRef`. Existing [`Visitor`] implementations accept
/// owner inputs through the same interface. Callbacks choose which children and
/// content to visit; dispatch does not recurse on its own.
/// Invoke callbacks through [`TraversalContext::visit_block`] or
/// [`TraversalContext::visit_blocks`] to apply document attributes.
pub trait BlockVisitor<'doc, Input = BlockRef<'doc>> {
    /// The error returned by a callback.
    type Error;

    /// Prepare a block before its attribute assignment is applied.
    ///
    /// # Errors
    ///
    /// An error stops dispatch without applying the block's assignment.
    fn before_block(
        &mut self,
        _context: &mut TraversalContext<'doc>,
        _block: Input,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Handle a block after any accepted attribute assignment is applied.
    ///
    /// Dispatch children through [`TraversalContext::visit_blocks`] and use
    /// [`TraversalContext::with_table_cell`] for `AsciiDoc` cell scopes.
    ///
    /// # Errors
    ///
    /// An error stops traversal. The assignment remains active in this scope.
    fn visit_block(
        &mut self,
        context: &mut TraversalContext<'doc>,
        block: Input,
    ) -> Result<(), Self::Error>;
}

impl<'doc, V: Visitor<'doc> + ?Sized> BlockVisitor<'doc, &'doc Block<'doc>> for V {
    type Error = V::Error;

    fn before_block(
        &mut self,
        context: &mut TraversalContext<'doc>,
        block: &'doc Block<'doc>,
    ) -> Result<(), Self::Error> {
        Visitor::before_block(self, context, block)
    }

    fn visit_block(
        &mut self,
        context: &mut TraversalContext<'doc>,
        block: &'doc Block<'doc>,
    ) -> Result<(), Self::Error> {
        match block {
            Block::DocumentAttribute(event) => self.visit_document_attribute(context, event),
            Block::Section(value) => self.visit_section(context, value),
            Block::Paragraph(value) => self.visit_paragraph(context, value),
            Block::DelimitedBlock(value) => self.visit_delimited_block(context, value),
            Block::OrderedList(value) => self.visit_ordered_list(context, value),
            Block::UnorderedList(value) => self.visit_unordered_list(context, value),
            Block::DescriptionList(value) => self.visit_description_list(context, value),
            Block::CalloutList(value) => self.visit_callout_list(context, value),
            Block::Admonition(value) => self.visit_admonition(context, value),
            Block::Image(value) => self.visit_image(context, value),
            Block::Video(value) => self.visit_video(context, value),
            Block::Audio(value) => self.visit_audio(context, value),
            Block::ThematicBreak(value) => self.visit_thematic_break(context, value),
            Block::PageBreak(value) => self.visit_page_break(context, value),
            Block::TableOfContents(value) => self.visit_table_of_contents(context, value),
            Block::DiscreteHeader(value) => self.visit_discrete_header(context, value),
            Block::Comment(_) => Ok(()),
            _ => self.visit_unhandled_block(context, block),
        }
    }
}
