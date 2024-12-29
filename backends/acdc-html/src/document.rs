use std::io::Write;

use acdc_parser::{Author, Block, Document, Header};

use crate::{Processor, Render, RenderOptions};

impl Render for Document {
    type Error = crate::Error;

    fn render<W: Write>(
        &self,
        w: &mut W,
        processor: &Processor,
        options: &RenderOptions,
    ) -> Result<(), Self::Error> {
        writeln!(w, "<!DOCTYPE html>")?;
        writeln!(w, "<html>")?;
        writeln!(w, "<head>")?;
        writeln!(w, "<meta charset=\"utf-8\">")?;
        writeln!(
            w,
            "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">"
        )?;
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
                    ..*options
                },
            )?;
        }
        writeln!(w, "<link rel=\"stylesheet\" href=\"https://fonts.googleapis.com/css?family=Open+Sans:300,300italic,400,400italic,600,600italic%7CNoto+Serif:400,400italic,700,700italic%7CDroid+Sans+Mono:400,700\">")?;
        writeln!(w, "<style>")?;
        writeln!(w, "{}", include_str!("../static/asciidoctor.css"))?;
        writeln!(w, "</style>")?;
        writeln!(w, "</head>")?;
        writeln!(w, "<body class=\"{}\">", processor.config.doctype)?;
        writeln!(w, "<div id=\"header\">")?;
        if let Some(header) = &self.header {
            if !header.title.is_empty() {
                write!(w, "<h1>")?;
                crate::inlines::render_inlines(&header.title, w, processor, options)?;
                writeln!(w, "</h1>")?;
                writeln!(w, "<div class=\"details\">")?;
                if !header.authors.is_empty() {
                    for (i, author) in header.authors.iter().enumerate() {
                        writeln!(
                            w,
                            "<span id=\"author{}\" class=\"author\">",
                            if i > 0 {
                                format!("{}", i + 1)
                            } else {
                                String::new()
                            }
                        )?;
                        write!(w, "{} ", author.first_name)?;
                        if let Some(middle_name) = &author.middle_name {
                            write!(w, "{middle_name} ")?;
                        }
                        write!(w, "{}", author.last_name)?;
                        writeln!(w, "</span>")?;
                        writeln!(w, "<br>")?;
                        if let Some(email) = &author.email {
                            writeln!(
                                w,
                                "<span id=\"email{}\" class=\"email\">",
                                if i > 0 {
                                    format!("{}", i + 1)
                                } else {
                                    String::new()
                                }
                            )?;

                            writeln!(w, "<a href=\"mailto:{email}\">{email}</a>")?;
                            writeln!(w, "</span>")?;
                            writeln!(w, "<br>")?;
                        }
                    }
                }
                writeln!(w, "</div>")?;
            }
        }
        writeln!(w, "</div>")?;
        writeln!(w, "<div id=\"content\">")?;
        let mut blocks = self.blocks.clone();
        let preamble = find_preamble(&mut blocks);
        if let Some(preamble) = preamble {
            writeln!(w, "<div id=\"preamble\">")?;
            writeln!(w, "<div class=\"sectionbody\">")?;
            for block in &preamble {
                block.render(w, processor, options)?;
            }
            writeln!(w, "</div>")?;
            writeln!(w, "</div>")?;
        }
        for block in &blocks {
            block.render(w, processor, options)?;
        }
        writeln!(w, "</div>")?;
        writeln!(w, "<div id=\"footer\">")?;
        writeln!(w, "<div id=\"footer-text\">")?;
        if let Some(last_updated) = options.last_updated {
            writeln!(w, "Last updated {}", last_updated.format("%F %T %Z"))?;
        }
        writeln!(w, "</div>")?;
        writeln!(w, "</div>")?;
        writeln!(w, "</body>")?;
        writeln!(w, "</html>")?;
        Ok(())
    }
}

fn find_preamble(blocks: &mut Vec<Block>) -> Option<Vec<Block>> {
    let mut first_section_index = 0;
    for (index, block) in blocks.iter().enumerate() {
        if let Block::Section(_) = block {
            first_section_index = index;
            break;
        }
    }
    if first_section_index > 0 {
        Some(blocks.drain(..first_section_index).collect::<Vec<_>>())
    } else {
        None
    }
}

impl Render for Header {
    type Error = crate::Error;

    fn render<W: Write>(
        &self,
        w: &mut W,
        processor: &Processor,
        options: &RenderOptions,
    ) -> Result<(), Self::Error> {
        self.authors.iter().try_for_each(|author| {
            author.render(w, processor, options)?;
            Ok::<(), Self::Error>(())
        })?;
        write!(w, "<title>")?;
        crate::inlines::render_inlines(&self.title, w, processor, options)?;
        writeln!(w, "</title>")?;
        Ok(())
    }
}

impl Render for Author {
    type Error = crate::Error;

    fn render<W: Write>(
        &self,
        w: &mut W,
        _processor: &Processor,
        _options: &RenderOptions,
    ) -> Result<(), Self::Error> {
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
