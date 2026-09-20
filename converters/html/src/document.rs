use std::io::Write;

use acdc_converters_core::{inlines_to_string, visitor::WritableVisitor};
use acdc_parser::{Author, Header};

use crate::{Error, HtmlVisitor, inlines::escape_attribute};

impl<W: Write> HtmlVisitor<'_, '_, W> {
    pub(crate) fn render_document_metadata(&mut self) -> Result<(), Error> {
        for name in ["description", "keywords"] {
            let mut value = String::new();
            let value = self
                .processor
                .document_attributes()
                .get(name)
                .is_some_and(|attribute| attribute.write_text(&mut value).is_ok())
                .then(|| escape_attribute(&value));
            if let Some(value) = value {
                writeln!(
                    self.writer_mut(),
                    "<meta name=\"{name}\" content=\"{value}\">"
                )?;
            }
        }
        Ok(())
    }

    /// Render header metadata for HTML head (<title> and <meta> tags)
    ///
    /// This generates HTML-specific metadata tags for the `<head>` element.
    /// This is separate from the body header rendered by `visit_header()` in the visitor trait.
    pub(crate) fn render_header_metadata(&mut self, header: &Header) -> Result<(), Error> {
        let w = self.writer_mut();
        for author in &header.authors {
            render_author(author, w)?;
        }
        let title_text = inlines_to_string(&header.title);
        if let Some(subtitle) = &header.subtitle {
            let subtitle_text = inlines_to_string(subtitle);
            writeln!(w, "<title>{title_text}: {subtitle_text}</title>")?;
        } else {
            writeln!(w, "<title>{title_text}</title>")?;
        }
        Ok(())
    }
}

fn render_author<W: Write + ?Sized>(author: &Author, w: &mut W) -> Result<(), Error> {
    write!(w, "<meta name=\"author\" content=\"")?;
    write!(w, "{} ", author.first_name)?;
    if let Some(middle_name) = &author.middle_name {
        write!(w, "{middle_name} ")?;
    }
    write!(w, "{}", author.last_name)?;
    writeln!(w, "\">")?;
    Ok(())
}
