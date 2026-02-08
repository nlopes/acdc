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
    let has_title = !img.title.is_empty();
    let mut w = visitor.writer_mut();

    // Build class and style for wrapper
    let mut classes = vec!["image-block".to_string()];
    for role in &img.metadata.roles {
        classes.push(role.clone());
    }

    let mut styles = Vec::new();
    if let Some(align) = img.metadata.attributes.get_string("align") {
        styles.push(format!("text-align: {align}"));
    }
    if let Some(float) = img.metadata.attributes.get_string("float") {
        styles.push(format!("float: {float}"));
    }

    // Wrapper: figure for titled, div for untitled
    let tag = if has_title { "figure" } else { "div" };
    write!(w, "<{tag} class=\"{}\"", classes.join(" "))?;
    if let Some(id) = &img.metadata.id {
        write!(w, " id=\"{}\"", id.id)?;
    } else if let Some(anchor) = img.metadata.anchors.first() {
        write!(w, " id=\"{}\"", anchor.id)?;
    }
    if !styles.is_empty() {
        write!(w, " style=\"{}\"", styles.join("; "))?;
    }
    writeln!(w, ">")?;

    let alt_text = img
        .metadata
        .attributes
        .get_string("alt")
        .unwrap_or(alt_text_from_filename(&img.source));

    // Check for link=self, link=none, or html5s-image-default-link=self
    let link = img.metadata.attributes.get("link");
    let link_str = link.as_ref().map(ToString::to_string);
    let is_link_none = link_str.as_deref() == Some("none");
    let is_link_self = link_str.as_deref() == Some("self");

    let use_self_link = is_link_self
        || (!is_link_none
            && link.is_none()
            && processor
                .document_attributes()
                .get("html5s-image-default-link")
                .is_some_and(|v| v.to_string() == "self"));

    // Check if default-link=self but explicit link=none should suppress
    let suppress_default_self = is_link_none
        && processor
            .document_attributes()
            .get("html5s-image-default-link")
            .is_some_and(|v| v.to_string() == "self");

    if use_self_link && !suppress_default_self {
        let label = processor
            .document_attributes()
            .get("html5s-image-self-link-label")
            .map_or_else(
                || "Open the image in full size".to_string(),
                ToString::to_string,
            );
        write!(
            w,
            "<a class=\"image bare\" href=\"{}\" title=\"{label}\" aria-label=\"{label}\">",
            img.source
        )?;
    } else if !is_link_none
        && !is_link_self
        && let Some(ref link_str) = link_str
    {
        write!(w, "<a class=\"image\" href=\"{}\">", escape_href(link_str))?;
    }

    write!(w, "<img src=\"{}\" alt=\"{alt_text}\"", img.source)?;
    write_dimension_attributes(w, &img.metadata)?;

    // Add loading attribute if present
    if let Some(loading) = img.metadata.attributes.get_string("loading") {
        write!(w, " loading=\"{loading}\"")?;
    }

    write!(w, ">")?;

    // Close link tag if we opened one
    let has_link = (use_self_link && !suppress_default_self)
        || (!is_link_none && !is_link_self && link_str.is_some());
    if has_link {
        write!(w, "</a>")?;
    }

    if has_title {
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

    writeln!(w, "</{tag}>")?;
    Ok(())
}
