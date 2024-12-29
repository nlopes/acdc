use std::io::Write;

use acdc_parser::{ListItem, UnorderedList};

use crate::{Processor, Render, RenderOptions};

impl Render for UnorderedList {
    fn render<W: Write>(
        &self,
        w: &mut W,
        processor: &Processor,
        options: &RenderOptions,
    ) -> std::io::Result<()> {
        writeln!(w, "<ul>")?;
        for item in &self.items {
            item.render(w, processor, options)?;
        }
        writeln!(w, "</ul>")?;
        Ok(())
    }
}

impl Render for ListItem {
    fn render<W: Write>(
        &self,
        w: &mut W,
        processor: &Processor,
        options: &RenderOptions,
    ) -> std::io::Result<()> {
        write!(w, "<li>")?;
        for (i, inline) in self.content.iter().enumerate() {
            if i != 0 {
                write!(w, " ")?;
            }

            inline.render(w, processor, options)?;
        }
        writeln!(w, "</li>")?;
        Ok(())
    }
}
