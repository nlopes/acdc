//! Host services for explicitly selected native block renderers.

use std::io::Write;

use acdc_converters_core::{
    Diagnostics, Options, TraversalContext,
    render::{HtmlRenderHost, RenderContext, RenderError},
};
use acdc_parser::{
    CaptionKind,
    ast::{BlockKind, BlockRef},
};

use crate::{
    Error, HtmlVariant, HtmlVisitor,
    delimited::{write_block_div_open, write_semantic_tag_open},
};

impl<'doc, W: Write> HtmlVisitor<'doc, '_, W> {
    pub(super) fn render_extension(
        &mut self,
        traversal: &mut TraversalContext<'doc>,
        block: BlockRef<'doc>,
    ) -> Result<bool, Error> {
        let Some(session) = self.converter.render_session.clone() else {
            return Ok(false);
        };
        if !matches!(
            block.kind(),
            BlockKind::Paragraph(_) | BlockKind::Listing(_) | BlockKind::Literal(_)
        ) {
            return Ok(false);
        }
        let mut session = session.try_borrow_mut().map_err(|_| {
            Error::Extension(std::io::Error::other("recursive block renderer dispatch").into())
        })?;
        session.render(
            block,
            &mut Host {
                visitor: self,
                traversal,
            },
            |host| Ok(host.visitor.render_builtin_block(host.traversal, block)?),
        )?;
        Ok(true)
    }
}

struct Host<'host, 'doc, 'diagnostics, W: Write> {
    visitor: &'host mut HtmlVisitor<'doc, 'diagnostics, W>,
    traversal: &'host mut TraversalContext<'doc>,
}

impl<W: Write> RenderContext for Host<'_, '_, '_, W> {
    fn variant(&self) -> &str {
        self.visitor.converter.variant.backend_profile().backend()
    }
    fn document_attributes(&self) -> &TraversalContext<'_> {
        self.traversal
    }
    fn options(&self) -> &Options {
        &self.visitor.converter.options
    }

    fn diagnostics(&mut self) -> Diagnostics<'_> {
        self.visitor.diagnostics.reborrow()
    }
}

impl<W: Write> HtmlRenderHost for Host<'_, '_, '_, W> {
    fn open_block(
        &mut self,
        block: BlockRef<'_>,
        element: &str,
        classes: &str,
    ) -> Result<(), RenderError> {
        if self.visitor.converter.variant == HtmlVariant::Semantic || element != "div" {
            write_semantic_tag_open(
                &mut self.visitor.writer,
                element,
                block.attributes(),
                classes,
            )?;
        } else {
            write_block_div_open(&mut self.visitor.writer, block.attributes(), classes)?;
        }
        Ok(())
    }

    fn title(
        &mut self,
        block: BlockRef<'_>,
        prefix: &str,
        suffix: &str,
    ) -> Result<(), RenderError> {
        self.visitor.render_captioned_title_with_wrapper(
            self.traversal,
            block.title(),
            block.attributes(),
            CaptionKind::for_block(block),
            prefix,
            suffix,
        )?;
        Ok(())
    }

    fn output(&mut self) -> (&mut dyn Write, Diagnostics<'_>, &TraversalContext<'_>) {
        (
            &mut self.visitor.writer,
            self.visitor.diagnostics.reborrow(),
            self.traversal,
        )
    }
}
