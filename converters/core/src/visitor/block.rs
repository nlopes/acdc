use acdc_parser::ast::BlockRef;

use crate::TraversalContext;

/// Block callbacks with source-ordered document attributes.
///
/// Callbacks choose which children and content to visit; dispatch does not recurse
/// on its own.
/// Invoke callbacks through [`TraversalContext::visit_block`] or
/// [`TraversalContext::visit_blocks`] to apply document attributes.
pub trait BlockVisitor<'doc> {
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
        _block: BlockRef<'doc>,
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
        block: BlockRef<'doc>,
    ) -> Result<(), Self::Error>;
}
