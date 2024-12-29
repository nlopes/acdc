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
        writeln!(w, "<div class=\"paragraph\">")?;
        write!(w, "<p>")?;
        crate::inlines::render_inlines(&self.content, w, processor, options)?;
        writeln!(w, "</p>")?;
        writeln!(w, "</div>")?;
        Ok(())
    }
}
