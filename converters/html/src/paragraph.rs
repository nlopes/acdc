use std::io::Write;

use acdc_parser::Paragraph;

use crate::{Processor, Render, RenderOptions};

impl Render for Paragraph {
    type Error = crate::Error;

    fn render<W: Write>(
        &self,
        w: &mut W,
        processor: &Processor,
        options: &RenderOptions,
    ) -> Result<(), Self::Error> {
        // Check if this paragraph should be rendered as a literal block
        if let Some(style) = &self.metadata.style {
            if style == "literal" {
                writeln!(w, "<div class=\"literalblock\">")?;
                if !self.title.is_empty() {
                    write!(w, "<div class=\"title\">")?;
                    crate::inlines::render_inlines(&self.title, w, processor, options)?;
                    writeln!(w, "</div>")?;
                }
                writeln!(w, "<div class=\"content\">")?;
                write!(w, "<pre>")?;
                crate::inlines::render_inlines(&self.content, w, processor, options)?;
                writeln!(w, "</pre>")?;
                writeln!(w, "</div>")?;
                writeln!(w, "</div>")?;
                return Ok(());
            }
        }

        // Regular paragraph rendering
        writeln!(w, "<div class=\"paragraph\">")?;
        if !self.title.is_empty() {
            write!(w, "<div class=\"title\">")?;
            crate::inlines::render_inlines(&self.title, w, processor, options)?;
            writeln!(w, "</div>")?;
        }
        write!(w, "<p>")?;
        crate::inlines::render_inlines(&self.content, w, processor, options)?;
        writeln!(w, "</p>")?;
        writeln!(w, "</div>")?;
        Ok(())
    }
}
