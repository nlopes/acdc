use acdc_converters_common::visitor::{WritableVisitor, WritableVisitorExt};
use acdc_parser::{AttributeValue, Image};

use crate::{
    Error, Processor,
    image_helpers::{alt_text_from_filename, write_dimension_attributes},
};

pub(crate) fn visit_image<V: WritableVisitor<Error = Error>>(
    img: &Image,
    visitor: &mut V,
    processor: &Processor,
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

    // Render title with figure caption if title exists
    if !img.title.is_empty() {
        let count = processor.figure_counter.get() + 1;
        processor.figure_counter.set(count);
        let caption = processor
            .document_attributes
            .get("figure-caption")
            .and_then(|v| match v {
                AttributeValue::String(s) => Some(s.as_str()),
                _ => None,
            })
            .unwrap_or("Figure");
        let _ = w;
        visitor.render_title_with_wrapper(
            &img.title,
            &format!("<div class=\"title\">{caption} {count}. "),
            "</div>",
        )?;
        w = visitor.writer_mut();
    }

    write!(w, "</div>")?; // close imageblock
    Ok(())
}
