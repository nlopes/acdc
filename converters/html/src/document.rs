use std::io::Write;

use acdc_converters_core::visitor::WritableVisitor;
use acdc_parser::{Author, Header};

use crate::{Error, Processor, RenderOptions};

/// Render header metadata for HTML head (<title> and <meta> tags)
///
/// This generates HTML-specific metadata tags for the `<head>` element.
/// This is separate from the body header rendered by `visit_header()` in the visitor trait.
pub(crate) fn render_header_metadata<V: WritableVisitor<Error = Error>>(
    header: &Header,
    visitor: &mut V,
    _processor: &Processor,
    _options: &RenderOptions,
) -> Result<(), Error> {
    let mut w = visitor.writer_mut();
    for author in &header.authors {
        render_author(author, w)?;
    }
    write!(w, "<title>")?;
    let _ = w;
    visitor.visit_inline_nodes(&header.title)?;
    if let Some(subtitle) = &header.subtitle {
        w = visitor.writer_mut();
        write!(w, ": ")?;
        let _ = w;
        visitor.visit_inline_nodes(subtitle)?;
    }
    w = visitor.writer_mut();
    writeln!(w, "</title>")?;
    Ok(())
}

fn render_author<W: Write + ?Sized>(author: &Author, w: &mut W) -> Result<(), Error> {
    write!(w, "<meta name=\"author\" content=\"")?;
    write!(w, "{} ", author.first_name)?;
    if let Some(middle_name) = &author.middle_name {
        write!(w, "{middle_name} ")?;
    }
    write!(w, "{}", author.last_name)?;
    if let Some(email) = &author.email {
        write!(w, " <{email}>")?;
    }
    writeln!(w, "\">")?;
    Ok(())
}
