use std::io::Write;

use acdc_parser::{DelimitedBlock, DelimitedBlockType};

use crate::{Processor, Render, RenderOptions};

impl Render for DelimitedBlock {
    type Error = crate::Error;

    fn render<W: Write>(
        &self,
        w: &mut W,
        processor: &Processor,
        options: &RenderOptions,
    ) -> Result<(), Self::Error> {
        writeln!(w, "<div>")?;
        match &self.inner {
            DelimitedBlockType::DelimitedTable(t) => t.render(w, processor, options)?,
            DelimitedBlockType::DelimitedPass(inlines) => {
                crate::inlines::render_inlines(inlines, w, processor, options)?;
            }
            DelimitedBlockType::DelimitedListing(inlines)
            | DelimitedBlockType::DelimitedLiteral(inlines) => {
                if let Some(style) = &self.metadata.style {
                    writeln!(w, "<div class=\"{style}block\">")?;
                } else {
                    writeln!(w, "<div class=\"literalblock\">")?;
                }
                write!(w, "<div class=\"title\">")?;
                crate::inlines::render_inlines(&self.title, w, processor, options)?;
                writeln!(w, "</div>")?;
                writeln!(w, "<div class=\"content\">")?;
                writeln!(w, "<pre>")?;
                crate::inlines::render_inlines(
                    inlines,
                    w,
                    processor,
                    &RenderOptions {
                        inlines_substitutions: true,
                        ..*options
                    },
                )?;
                writeln!(w, "</pre>")?;
                writeln!(w, "</div>")?;
                writeln!(w, "</div>")?;
            }
            DelimitedBlockType::DelimitedQuote(blocks) => {
                if let Some(style) = &self.metadata.style {
                    writeln!(w, "<div class=\"{style}block\">")?;
                } else {
                    writeln!(w, "<div class=\"quoteblock\">")?;
                }
                writeln!(w, "<blockquote>")?;
                for block in blocks {
                    block.render(w, processor, options)?;
                }
                writeln!(w, "</blockquote>")?;
                writeln!(w, "</div>")?;
            }
            unknown => todo!("Unknown delimited block type: {:?}", unknown),
        }
        writeln!(w, "</div>")?;
        Ok(())
    }
}
