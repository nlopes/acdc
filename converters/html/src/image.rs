use acdc_converters_core::visitor::{WritableVisitor, WritableVisitorExt};
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

    // Build class list: imageblock + alignment + float + roles
    let mut classes = vec!["imageblock".to_string()];

    // align=left|center|right → text-left|text-center|text-right
    if let Some(align) = img.metadata.attributes.get_string("align") {
        classes.push(format!("text-{align}"));
    }

    // float=left|right → left|right
    if let Some(float) = img.metadata.attributes.get_string("float") {
        classes.push(float.clone());
    }

    // roles → added as classes
    for role in &img.metadata.roles {
        classes.push(role.clone());
    }

    write!(w, "<div class=\"{}\">", classes.join(" "))?;
    write!(w, "<div class=\"content\">")?;
    // Get alt text from attribute or generate from filename
    let alt_text = img
        .metadata
        .attributes
        .get_string("alt")
        .unwrap_or(alt_text_from_filename(&img.source));

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
                AttributeValue::Bool(_) | AttributeValue::None | _ => None,
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
