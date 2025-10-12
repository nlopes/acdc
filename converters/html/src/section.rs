use std::io::Write;

use acdc_parser::{DiscreteHeader, Section};

use crate::{Processor, Render, RenderOptions};

impl Render for Section {
    type Error = crate::Error;

    fn render<W: Write>(
        &self,
        w: &mut W,
        processor: &Processor,
        options: &RenderOptions,
    ) -> Result<(), Self::Error> {
        let level = self.level + 1; // Level 1 = h2
        let id = Section::generate_id(&self.metadata, &self.title);

        writeln!(w, "<div class=\"sect{}\">", self.level)?;
        write!(w, "<h{level} id=\"{id}\">")?;
        crate::inlines::render_inlines(&self.title, w, processor, options)?;
        writeln!(w, "</h{level}>")?;
        writeln!(w, "<div class=\"sectionbody\">")?;
        for block in &self.content {
            block.render(w, processor, options)?;
        }
        writeln!(w, "</div>")?;
        writeln!(w, "</div>")?;
        Ok(())
    }
}

impl Render for DiscreteHeader {
    type Error = crate::Error;

    fn render<W: Write>(
        &self,
        w: &mut W,
        processor: &Processor,
        options: &RenderOptions,
    ) -> Result<(), Self::Error> {
        let level = self.level + 1; // Level 1 = h2
        let id = Section::generate_id(&self.metadata, &self.title);

        write!(w, "<h{level} id=\"{id}\" class=\"discrete\">")?;
        crate::inlines::render_inlines(&self.title, w, processor, options)?;
        writeln!(w, "</h{level}>")?;
        Ok(())
    }
}
