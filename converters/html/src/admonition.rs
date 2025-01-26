use std::io::Write;

use acdc_parser::Admonition;

use crate::{Processor, Render, RenderOptions};

impl Render for Admonition {
    type Error = crate::Error;

    fn render<W: Write>(
        &self,
        w: &mut W,
        processor: &Processor,
        options: &RenderOptions,
    ) -> Result<(), Self::Error> {
        writeln!(w, "<div class=\"admonitionblock {}\">", self.variant)?;
        writeln!(w, "<table>")?;
        writeln!(w, "<tr>")?;
        writeln!(w, "<td class=\"icon\">")?;
        writeln!(w, "<div class=\"title\">{}</div>", self.variant)?;
        writeln!(w, "</td>")?;
        writeln!(w, "<td class=\"content\">")?;
        write!(w, "<div class=\"title\">")?;
        crate::inlines::render_inlines(&self.title, w, processor, options)?;
        writeln!(w, "</div>")?;
        for block in &self.blocks {
            block.render(w, processor, options)?;
        }
        writeln!(w, "</td>")?;
        writeln!(w, "</tr>")?;
        writeln!(w, "</table>")?;
        writeln!(w, "</div>")?;
        Ok(())
    }
}
