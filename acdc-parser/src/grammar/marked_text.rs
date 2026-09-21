use crate::{Error, Form, InlineNode, Location};

use super::location_mapping::LocationMappingContext;

/// Shared access and source-location mapping for formatted inline nodes.
pub(crate) trait MarkedText<'a>: Sized {
    /// The type of content this formatted node contains (typically Vec<`InlineNode`<'a>>)
    type Content: LocationMappable<'a>;

    /// Get the optional cross-reference ID.
    fn id(&self) -> Option<&'a str>;

    /// Get an immutable reference to the location
    fn location(&self) -> &Location;

    /// Get an immutable reference to the content.
    fn content(&self) -> &Self::Content;

    /// Get a mutable reference to the location
    fn location_mut(&mut self) -> &mut Location;

    /// Get a mutable reference to the content
    fn content_mut(&mut self) -> &mut Self::Content;

    /// Get the form (constrained/unconstrained)
    fn form(&self) -> &Form;

    /// Map the node and its content to source coordinates with its delimiter form.
    fn map_locations(&mut self, ctx: &LocationMappingContext<'_, 'a>) -> Result<(), Error> {
        // Get the form first to avoid borrowing issues
        let form = self.form().clone();
        let mapped_outer = ctx.map_location(self.location(), Some(&form))?;
        let extended_location = super::location_mapping::extend_attribute_location_if_needed(
            ctx.state,
            ctx.processed,
            mapped_outer,
        );
        *self.location_mut() = extended_location;

        self.content_mut().map_locations_with(ctx, Some(&form))?;

        Ok(())
    }
}

/// Formatted content whose locations can be mapped back to source.
pub trait LocationMappable<'a> {
    /// Map this content using the source context and enclosing delimiter form.
    fn map_locations_with(
        &mut self,
        ctx: &LocationMappingContext<'_, 'a>,
        form: Option<&Form>,
    ) -> Result<(), Error>;
}

/// Implementation for Vec<`InlineNode`<'a>> - the most common content type
impl<'a> LocationMappable<'a> for Vec<InlineNode<'a>> {
    fn map_locations_with(
        &mut self,
        ctx: &LocationMappingContext<'_, 'a>,
        form: Option<&Form>,
    ) -> Result<(), Error> {
        *self =
            super::location_mapping::map_inner_content_locations(std::mem::take(self), ctx, form)?;
        Ok(())
    }
}

/// Macro to implement `MarkedText` for all marked text inline types
macro_rules! impl_marked_text {
    ($($type:ident),+ $(,)?) => {
        $(
            impl<'a> MarkedText<'a> for crate::$type<'a> {
                type Content = Vec<InlineNode<'a>>;

                fn id(&self) -> Option<&'a str> {
                    self.id
                }

                fn location(&self) -> &Location {
                    &self.location
                }

                fn content(&self) -> &Self::Content {
                    &self.content
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
    Bold,
    Italic,
    Monospace,
    Highlight,
    Subscript,
    Superscript,
    CurvedQuotation,
    CurvedApostrophe,
);

pub(crate) fn map_marked_text_locations<'a>(
    inline: &mut InlineNode<'a>,
    ctx: &LocationMappingContext<'_, 'a>,
) -> Result<(), Error> {
    match inline {
        InlineNode::BoldText(node) => node.map_locations(ctx),
        InlineNode::ItalicText(node) => node.map_locations(ctx),
        InlineNode::MonospaceText(node) => node.map_locations(ctx),
        InlineNode::HighlightText(node) => node.map_locations(ctx),
        InlineNode::SubscriptText(node) => node.map_locations(ctx),
        InlineNode::SuperscriptText(node) => node.map_locations(ctx),
        InlineNode::CurvedQuotationText(node) => node.map_locations(ctx),
        InlineNode::CurvedApostropheText(node) => node.map_locations(ctx),
        InlineNode::RawText(_)
        | InlineNode::PlainText(_)
        | InlineNode::VerbatimText(_)
        | InlineNode::LineBreak(_)
        | InlineNode::InlineAnchor(_)
        | InlineNode::CalloutRef(_)
        | InlineNode::Macro(_)
        | InlineNode::StandaloneCurvedApostrophe(_) => Ok(()),
    }
}
