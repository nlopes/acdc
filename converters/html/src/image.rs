use acdc_converters_common::visitor::WritableVisitor;
use acdc_parser::Image;

use crate::{
    Error,
    image_helpers::{alt_text_from_filename, write_dimension_attributes},
};

pub(crate) fn visit_image<V: WritableVisitor<Error = Error>>(
    img: &Image,
    visitor: &mut V,
) -> Result<(), Error> {
    let mut w = visitor.writer_mut();
    write!(w, "<div class=\"imageblock\">")?;
    write!(w, "<div class=\"content\">")?;
    // Get alt text from attribute or generate from filename
    let alt_text = img.metadata.attributes.get("alt").map_or_else(
        || alt_text_from_filename(&img.source),
        std::string::ToString::to_string,
    );

    // Wrap in link if link attribute exists
    let link = img.metadata.attributes.get("link");
    if let Some(link) = link {
        write!(w, "<a class=\"image\" href=\"{link}\">")?;
    }

    write!(w, "<img src=\"{}\" alt=\"{alt_text}\"", img.source)?;
    write_dimension_attributes(w, &img.metadata)?;
    write!(w, " />")?;

    if link.is_some() {
        write!(w, "</a>")?;
    }
    write!(w, "</div>")?; // close content
    if !img.title.is_empty() {
        write!(w, "<div class=\"title\">")?;
        let _ = w;
        visitor.visit_inline_nodes(&img.title)?;
        w = visitor.writer_mut();
        write!(w, "</div>")?;
    }
    write!(w, "</div>")?; // close imageblock
    Ok(())
}
