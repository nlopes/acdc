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
        write!(w, "<p>")?;
        for (i, inline) in self.content.iter().enumerate() {
            if i != 0 {
                write!(w, " ")?;
            }
            inline.render(w, processor, options)?;
        }
        writeln!(w, "</p>")?;
        Ok(())
    }
}
