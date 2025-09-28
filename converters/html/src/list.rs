use std::io::Write;

use acdc_parser::{DescriptionList, DescriptionListItem, ListItem, OrderedList, UnorderedList};

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

impl Render for OrderedList {
    type Error = crate::Error;

    fn render<W: Write>(
        &self,
        w: &mut W,
        processor: &Processor,
        options: &RenderOptions,
    ) -> Result<(), Self::Error> {
        writeln!(w, "<div class=\"olist arabic\">")?;
        writeln!(w, "<ol class=\"arabic\">")?;
        for item in &self.items {
            item.render(w, processor, options)?;
        }
        writeln!(w, "</ol>")?;
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

impl Render for DescriptionList {
    type Error = crate::Error;

    fn render<W: Write>(
        &self,
        w: &mut W,
        processor: &Processor,
        options: &RenderOptions,
    ) -> Result<(), Self::Error> {
        writeln!(w, "<div class=\"dlist\">")?;
        writeln!(w, "<dl>")?;
        for item in &self.items {
            item.render(w, processor, options)?;
        }
        writeln!(w, "</dl>")?;
        writeln!(w, "</div>")?;
        Ok(())
    }
}

impl Render for DescriptionListItem {
    type Error = crate::Error;

    fn render<W: Write>(
        &self,
        w: &mut W,
        processor: &Processor,
        options: &RenderOptions,
    ) -> Result<(), Self::Error> {
        writeln!(w, "<dt class=\"hdlist1\">")?;
        crate::inlines::render_inlines(&self.term, w, processor, options)?;
        writeln!(w, "</dt>")?;
        writeln!(w, "<dd>")?;
        if !self.principal_text.is_empty() {
            writeln!(w, "<p>")?;
            crate::inlines::render_inlines(&self.principal_text, w, processor, options)?;
            writeln!(w, "</p>")?;
        }
        for block in &self.description {
            block.render(w, processor, options)?;
        }
        writeln!(w, "</dd>")?;
        Ok(())
    }
}
