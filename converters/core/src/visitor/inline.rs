use acdc_parser::ast::{ContentRef, InlineRef};

use crate::TraversalContext;

/// Inline rendering callbacks with inputs independent of the document lifetime.
///
/// Inputs may borrow temporary text or parsed fragments. Callbacks choose which
/// nested fields to visit; the default sequence visits only its direct inlines.
/// Override the sequence when rendering needs sibling context or cleanup.
///
/// Temporary inline text cannot be retained as a document borrow:
///
/// ```compile_fail
/// use std::convert::Infallible;
/// use acdc_converters_core::{TraversalContext, visitor::InlineVisitor};
/// use acdc_parser::ast::{InlineKind, InlineRef};
///
/// struct Retained<'doc>(Option<&'doc str>);
/// impl<'doc> InlineVisitor<'doc> for Retained<'doc> {
///     type Error = Infallible;
///     fn visit_inline_node(
///         &mut self,
///         _: &mut TraversalContext<'doc>,
///         node: InlineRef<'_>,
///     ) -> Result<(), Self::Error> {
///         if let InlineKind::Plain(text) = node.kind() {
///             self.0 = Some(text.text());
///         }
///         Ok(())
///     }
/// }
/// ```
pub trait InlineVisitor<'doc> {
    /// The error returned by a callback.
    type Error;

    /// Visit the direct inline nodes in content order.
    ///
    /// # Errors
    ///
    /// Returns the first callback error without visiting later nodes.
    fn visit_inline_nodes(
        &mut self,
        context: &mut TraversalContext<'doc>,
        content: ContentRef<'_>,
    ) -> Result<(), Self::Error> {
        content
            .inlines()
            .try_for_each(|node| self.visit_inline_node(context, node))
    }

    /// Handle one inline, selecting any children or displayable attributes.
    ///
    /// # Errors
    ///
    /// Returns a rendering error from the implementation.
    fn visit_inline_node(
        &mut self,
        context: &mut TraversalContext<'doc>,
        node: InlineRef<'_>,
    ) -> Result<(), Self::Error>;
}
