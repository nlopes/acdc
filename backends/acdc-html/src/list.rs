use std::io::Write;

use acdc_parser::{ListItem, UnorderedList};

use crate::{Processor, Render, RenderOptions};

impl Render for UnorderedList {
    type Error = crate::Error;

    fn render<W: Write>(
        &self,
        w: &mut W,
        processor: &Processor,
        options: &RenderOptions,
    ) -> Result<(), Self::Error> {
        writeln!(w, "<div class=\"ulist\">")?;
        writeln!(w, "<ul>")?;
        for item in &self.items {
            item.render(w, processor, options)?;
        }
        writeln!(w, "</ul>")?;
        writeln!(w, "</div>")?;
        Ok(())
    }
}

impl Render for ListItem {
    type Error = crate::Error;

    fn render<W: Write>(
        &self,
        w: &mut W,
        processor: &Processor,
        options: &RenderOptions,
    ) -> Result<(), Self::Error> {
        writeln!(w, "<li>")?;
        writeln!(w, "<p>")?;
        crate::inlines::render_inlines(&self.content, w, processor, options)?;
        writeln!(w, "</p>")?;
        writeln!(w, "</li>")?;
        Ok(())
    }
}
