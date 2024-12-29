use std::io::Write;

use acdc_parser::Paragraph;

use crate::{Processor, Render, RenderOptions};

impl Render for Paragraph {
    fn render<W: Write>(
        &self,
        w: &mut W,
        processor: &Processor,
        options: &RenderOptions,
    ) -> std::io::Result<()> {
        writeln!(w, "<div class=\"paragraph\">")?;
        write!(w, "<p>")?;
        crate::inlines::render_inlines(&self.content, w, processor, options)?;
        writeln!(w, "</p>")?;
        writeln!(w, "</div>")?;
        Ok(())
    }
}
