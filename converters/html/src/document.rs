use std::io::Write;

use acdc_converters_core::InlineTextTransform;
use acdc_parser::ast::{AuthorRef, DocumentAttributesRef, HeaderRef};

use crate::{Error, HtmlVisitor, inlines::escape_attribute};

impl<W: Write> HtmlVisitor<'_, '_, W> {
    pub(crate) fn render_document_metadata(&mut self) -> Result<(), Error> {
        let attributes = DocumentAttributesRef::from(self.processor.document_attributes());
        for name in ["description", "keywords"] {
            let mut value = String::new();
            let value = attributes
                .get(name)
                .is_some_and(|attribute| attribute.write_text(&mut value).is_ok())
                .then(|| escape_attribute(&value));
            if let Some(value) = value {
                writeln!(self.writer, "<meta name=\"{name}\" content=\"{value}\">")?;
            }
        }
        Ok(())
    }

    /// Render the title and author metadata in the HTML head.
    pub(crate) fn render_header_metadata(&mut self, header: HeaderRef<'_>) -> Result<(), Error> {
        let w = &mut self.writer;
        for author in header.authors() {
            render_author(author, w)?;
        }
        let text = InlineTextTransform::default();
        let title_text = header
            .title()
            .map_or_else(String::new, |title| text.content_to_string(title));
        if let Some(subtitle) = header.subtitle() {
            let subtitle_text = text.content_to_string(subtitle);
            writeln!(w, "<title>{title_text}: {subtitle_text}</title>")?;
        } else {
            writeln!(w, "<title>{title_text}</title>")?;
        }
        Ok(())
    }
}

fn render_author<W: Write + ?Sized>(author: AuthorRef<'_>, w: &mut W) -> Result<(), Error> {
    write!(w, "<meta name=\"author\" content=\"")?;
    write!(w, "{} ", author.first_name())?;
    if let Some(middle_name) = author.middle_name() {
        write!(w, "{middle_name} ")?;
    }
    write!(w, "{}", author.last_name())?;
    writeln!(w, "\">")?;
    Ok(())
}
