use std::io::Write;

use acdc_parser::{Author, Document, Header};

use crate::{Processor, Render, RenderOptions};

impl Render for Document {
    fn render<W: Write>(
        &self,
        w: &mut W,
        processor: &Processor,
        options: &RenderOptions,
    ) -> std::io::Result<()> {
        writeln!(w, "<!DOCTYPE html>")?;
        writeln!(w, "<html>")?;
        writeln!(w, "<head>")?;
        writeln!(w, "<meta charset=\"utf-8\">")?;
        writeln!(
            w,
            "<meta name=\"generator\" content=\"{}\">",
            processor.config.generator_metadata
        )?;
        if let Some(header) = &self.header {
            header.render(
                w,
                processor,
                &RenderOptions {
                    inlines_basic: true,
                },
            )?;
        }
        writeln!(w, "</head>")?;
        writeln!(w, "<body class=\"{}\">", processor.config.doctype)?;
        writeln!(w, "<div id=\"header\">")?;
        if let Some(header) = &self.header {
            if !header.title.is_empty() {
                write!(w, "<h1>")?;
                for node in &header.title {
                    node.render(w, processor, options)?;
                }
                writeln!(w, "</h1>")?;
            }
        }
        writeln!(w, "</div>")?;
        for block in &self.blocks {
            block.render(w, processor, options)?;
        }
        writeln!(w, "<div id=\"footer\">")?;
        writeln!(w, "</div>")?;
        writeln!(w, "</body>")?;
        writeln!(w, "</html>")?;
        Ok(())
    }
}

impl Render for Header {
    fn render<W: Write>(
        &self,
        w: &mut W,
        processor: &Processor,
        options: &RenderOptions,
    ) -> std::io::Result<()> {
        self.authors.iter().try_for_each(|author| {
            author.render(w, processor, options)?;
            Ok::<(), std::io::Error>(())
        })?;
        write!(w, "<title>")?;
        for node in &self.title {
            node.render(w, processor, options)?;
        }
        writeln!(w, "</title>")?;
        Ok(())
    }
}

impl Render for Author {
    fn render<W: Write>(
        &self,
        w: &mut W,
        _processor: &Processor,
        _options: &RenderOptions,
    ) -> std::io::Result<()> {
        write!(w, "<meta name=\"author\" content=\"")?;
        write!(w, "{} ", self.first_name)?;
        if let Some(middle_name) = &self.middle_name {
            write!(w, "{middle_name} ")?;
        }
        write!(w, "{}", self.last_name)?;
        if let Some(email) = &self.email {
            write!(w, " <{email}>")?;
        }
        writeln!(w, "\">")?;
        Ok(())
    }
}
