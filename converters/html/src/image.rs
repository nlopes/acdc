use std::string::ToString;

use acdc_converters_core::visitor::{WritableVisitor, WritableVisitorExt};
use acdc_parser::Image;

use crate::{
    Error, HtmlVariant, Processor,
    image_helpers::{alt_text_from_filename, write_dimension_attributes},
    inlines::escape_href,
};

pub(crate) fn visit_image<V: WritableVisitor<Error = Error>>(
    img: &Image,
    visitor: &mut V,
    processor: &Processor,
) -> Result<(), Error> {
    if processor.variant() == HtmlVariant::Semantic {
        return visit_image_semantic(img, visitor, processor);
    }

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
        write!(
            w,
            "<a class=\"image\" href=\"{}\">",
            escape_href(&link.to_string())
        )?;
    }

    write!(w, "<img src=\"{}\" alt=\"{alt_text}\"", img.source)?;
    write_dimension_attributes(w, &img.metadata)?;
    write!(w, ">")?;

    if link.is_some() {
        write!(w, "</a>")?;
    }
    write!(w, "</div>")?; // close content

    // Render title with figure caption if title exists
    // Caption can be disabled with :figure-caption!:
    if !img.title.is_empty() {
        let prefix =
            processor.caption_prefix("figure-caption", &processor.figure_counter, "Figure");
        let _ = w;
        visitor.render_title_with_wrapper(
            &img.title,
            &format!("<div class=\"title\">{prefix}"),
            "</div>",
        )?;
        w = visitor.writer_mut();
    }

    write!(w, "</div>")?; // close imageblock
    Ok(())
}

fn visit_image_semantic<V: WritableVisitor<Error = Error>>(
    img: &Image,
    visitor: &mut V,
    processor: &Processor,
) -> Result<(), Error> {
    let mut w = visitor.writer_mut();
    writeln!(w, "<figure class=\"image-block\">")?;

    let alt_text = img
        .metadata
        .attributes
        .get_string("alt")
        .unwrap_or(alt_text_from_filename(&img.source));

    // Check for link=self or html5s-image-default-link=self
    let link = img.metadata.attributes.get("link");
    let use_self_link = link.as_ref().is_some_and(|v| v.to_string() == "self")
        || (link.is_none()
            && processor
                .document_attributes()
                .get("html5s-image-default-link")
                .is_some_and(|v| v.to_string() == "self"));

    if use_self_link {
        let label = processor
            .document_attributes()
            .get("html5s-image-self-link-label")
            .map_or_else(
                || "Open the image in full size".to_string(),
                ToString::to_string,
            );
        write!(
            w,
            "<a class=\"image\" href=\"{}\" title=\"{label}\" aria-label=\"{label}\">",
            img.source
        )?;
    } else if let Some(link) = link {
        let link_str = link.to_string();
        if link_str != "self" {
            write!(w, "<a class=\"image\" href=\"{}\">", escape_href(&link_str))?;
        }
    }

    write!(w, "<img src=\"{}\" alt=\"{alt_text}\"", img.source)?;
    write_dimension_attributes(w, &img.metadata)?;

    // Add loading attribute if present
    if let Some(loading) = img.metadata.attributes.get_string("loading") {
        write!(w, " loading=\"{loading}\"")?;
    }

    write!(w, ">")?;

    if use_self_link || link.as_ref().is_some_and(|v| v.to_string() != "self") {
        write!(w, "</a>")?;
    }

    if !img.title.is_empty() {
        let prefix =
            processor.caption_prefix("figure-caption", &processor.figure_counter, "Figure");
        let _ = w;
        visitor.render_title_with_wrapper(
            &img.title,
            &format!("<figcaption>{prefix}"),
            "</figcaption>\n",
        )?;
        w = visitor.writer_mut();
    }

    writeln!(w, "</figure>")?;
    Ok(())
}
