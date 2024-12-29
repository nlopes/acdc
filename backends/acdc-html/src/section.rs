use std::io::Write;

use acdc_parser::Section;

use crate::{Processor, Render, RenderOptions};

impl Render for Section {
    fn render<W: Write>(
        &self,
        w: &mut W,
        processor: &Processor,
        options: &RenderOptions,
    ) -> std::io::Result<()> {
        write!(w, "<h{}>", self.level)?;
        for inline in &self.title {
            inline.render(w, processor, options)?;
        }
        writeln!(w, "</h{}>", self.level)?;
        for block in &self.content {
            block.render(w, processor, options)?;
        }
        Ok(())
    }
}
