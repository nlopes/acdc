use crate::{Form, InlineNode, Location, grammar::ProcessedContent};

use super::document::ParserState;
use super::location_mapping::LocationMappingContext;

/// Trait for types that represent marked text with location mapping capabilities.
///
/// This trait uses Generic Associated Types (GATs) to provide a unified interface for all
/// marked text node types while maintaining compile-time type safety and zero runtime cost.
pub trait MarkedText: Sized {
    /// The type of content this formatted node contains (typically Vec<InlineNode>)
    type Content: LocationMappable;

    /// Get an immutable reference to the location
    fn location(&self) -> &Location;

    /// Get a mutable reference to the location
    fn location_mut(&mut self) -> &mut Location;

    /// Get a mutable reference to the content
    fn content_mut(&mut self) -> &mut Self::Content;

    /// Get the form (constrained/unconstrained)
    fn form(&self) -> &Form;

    /// Generic location mapping that works for any `MarkedText`
    fn map_locations(mut self, mapping_ctx: &LocationMappingContext) -> Self {
        // Get the form first to avoid borrowing issues
        let form = self.form().clone();
        let location = self.location().clone();

        // Create a form-aware location mapper
        let map_loc = super::location_mapping::create_location_mapper(
            mapping_ctx.state,
            mapping_ctx.processed,
            mapping_ctx.base_location,
            Some(&form),
        );

        // Map outer location with attribute extension
        let mapped_outer = map_loc(&location);
        let extended_location = super::location_mapping::extend_attribute_location_if_needed(
            mapping_ctx.state,
            mapping_ctx.processed,
            mapped_outer,
        );
        *self.location_mut() = extended_location;

        // Map inner content locations
        self.content_mut().map_locations_with(
            &map_loc,
            mapping_ctx.state,
            mapping_ctx.processed,
            mapping_ctx.base_location,
        );

        self
    }
}

/// Trait for types that can have their locations recursively mapped.
///
/// This trait enables recursive location mapping for nested content structures.
pub trait LocationMappable: Clone {
    /// Map locations within this content using the provided location mapper
    fn map_locations_with(
        &mut self,
        map_loc: &dyn Fn(&Location) -> Location,
        state: &ParserState,
        processed: &ProcessedContent,
        base_location: &Location,
    );
}

/// Implementation for Vec<InlineNode> - the most common content type
impl LocationMappable for Vec<InlineNode> {
    fn map_locations_with(
        &mut self,
        map_loc: &dyn Fn(&Location) -> Location,
        state: &ParserState,
        processed: &ProcessedContent,
        base_location: &Location,
    ) {
        *self = super::location_mapping::map_inner_content_locations(
            std::mem::take(self),
            map_loc,
            state,
            processed,
            base_location,
        );
    }
}

/// Macro to implement `MarkedText` for all marked text inline types
macro_rules! impl_marked_text {
    ($($type:ty),+ $(,)?) => {
        $(
            impl MarkedText for $type {
                type Content = Vec<InlineNode>;

                fn location(&self) -> &Location {
                    &self.location
                }

                fn location_mut(&mut self) -> &mut Location {
                    &mut self.location
                }

                fn content_mut(&mut self) -> &mut Self::Content {
                    &mut self.content
                }

                fn form(&self) -> &Form {
                    &self.form
                }
            }
        )+
    };
}

// Apply the macro to all marked text inline types
impl_marked_text!(
    crate::Bold,
    crate::Italic,
    crate::Monospace,
    crate::Highlight,
    crate::Subscript,
    crate::Superscript,
    crate::CurvedQuotation,
    crate::CurvedApostrophe,
);

/// Trait for enum dispatch to `MarkedText` implementations
///
/// This allows us to call `MarkedText` methods on `InlineNode` enum variants
/// without repetitive match statements.
pub trait WithLocationMappingContext {
    /// Map inline node locations using the provided location mapping context
    fn with_location_mapping_context(self, mapping_ctx: &LocationMappingContext) -> Self;
}

impl WithLocationMappingContext for InlineNode {
    fn with_location_mapping_context(self, mapping_ctx: &LocationMappingContext) -> InlineNode {
        match self {
            InlineNode::BoldText(node) => InlineNode::BoldText(node.map_locations(mapping_ctx)),
            InlineNode::ItalicText(node) => InlineNode::ItalicText(node.map_locations(mapping_ctx)),
            InlineNode::MonospaceText(node) => {
                InlineNode::MonospaceText(node.map_locations(mapping_ctx))
            }
            InlineNode::HighlightText(node) => {
                InlineNode::HighlightText(node.map_locations(mapping_ctx))
            }
            InlineNode::SubscriptText(node) => {
                InlineNode::SubscriptText(node.map_locations(mapping_ctx))
            }
            InlineNode::SuperscriptText(node) => {
                InlineNode::SuperscriptText(node.map_locations(mapping_ctx))
            }
            InlineNode::CurvedQuotationText(node) => {
                InlineNode::CurvedQuotationText(node.map_locations(mapping_ctx))
            }
            InlineNode::CurvedApostropheText(node) => {
                InlineNode::CurvedApostropheText(node.map_locations(mapping_ctx))
            }
            _ => self,
        }
    }
}
