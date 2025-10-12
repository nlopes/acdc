use std::io::Write;

use acdc_parser::Audio;

use crate::{Processor, Render, RenderOptions};

impl Render for Audio {
    type Error = crate::Error;

    fn render<W: Write>(
        &self,
        w: &mut W,
        processor: &Processor,
        options: &RenderOptions,
    ) -> Result<(), Self::Error> {
        writeln!(w, "<div class=\"audioblock\">")?;

        if !self.title.is_empty() {
            write!(w, "<div class=\"title\">")?;
            crate::inlines::render_inlines(&self.title, w, processor, options)?;
            writeln!(w, "</div>")?;
        }

        writeln!(w, "<div class=\"content\">")?;
        writeln!(w, "<audio src=\"{}\" controls>", self.source)?;
        writeln!(w, "Your browser does not support the audio tag.")?;
        writeln!(w, "</audio>")?;
        writeln!(w, "</div>")?;
        writeln!(w, "</div>")?;

        Ok(())
    }
}
