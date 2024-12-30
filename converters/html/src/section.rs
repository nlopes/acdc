use std::io::Write;

use acdc_parser::Section;

use crate::{Processor, Render, RenderOptions};

impl Render for Section {
    type Error = crate::Error;

    fn render<W: Write>(
        &self,
        w: &mut W,
        processor: &Processor,
        options: &RenderOptions,
    ) -> Result<(), Self::Error> {
        writeln!(w, "<div class=\"sect{}\">", self.level)?;
        write!(w, "<h{}>", self.level + 1)?;
        crate::inlines::render_inlines(&self.title, w, processor, options)?;
        writeln!(w, "</h{}>", self.level)?;
        writeln!(w, "<div class=\"sectionbody\">")?;
        for block in &self.content {
            block.render(w, processor, options)?;
        }
        writeln!(w, "</div>")?;
        writeln!(w, "</div>")?;
        Ok(())
    }
}
