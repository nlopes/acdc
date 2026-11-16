//! Host services for explicitly selected native block renderers.

use std::io::Write;

use acdc_converters_core::{
    Diagnostics, Options, TraversalContext,
    render::{RenderError, RenderHost},
};
use acdc_parser::{CaptionKind, DocumentAttributes, ast::BlockRef};

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
        let mut session = session.try_borrow_mut().map_err(|_| {
            Error::Extension(std::io::Error::other("recursive block renderer dispatch").into())
        })?;
        Ok(session.render(
            block,
            &mut Host {
                visitor: self,
                traversal,
            },
        )?)
    }
}

struct Host<'host, 'doc, 'diagnostics, W: Write> {
    visitor: &'host mut HtmlVisitor<'doc, 'diagnostics, W>,
    traversal: &'host mut TraversalContext<'doc>,
}

impl<W: Write> RenderHost for Host<'_, '_, '_, W> {
    fn variant(&self) -> &str {
        self.visitor.converter.variant.backend_profile().backend()
    }
    fn document_attributes(&self) -> &DocumentAttributes<'_> {
        self.visitor.converter.document_attributes()
    }
    fn options(&self) -> &Options {
        &self.visitor.converter.options
    }

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

    fn output(
        &mut self,
        operation: &mut dyn FnMut(&mut dyn Write, &mut Diagnostics<'_>) -> Result<(), RenderError>,
    ) -> Result<(), RenderError> {
        operation(&mut self.visitor.writer, &mut self.visitor.diagnostics)
    }
}
