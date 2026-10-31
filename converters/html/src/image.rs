use std::{io::Write, string::ToString};

use acdc_converters_core::{TraversalContext, media::resolve_target};
use acdc_parser::{
    CaptionKind, Image,
    ast::{AttributeValueRef, AttributesRef, ImageRef, TokensRef},
    strip_quotes,
};

use crate::{Error, HtmlVariant, HtmlVisitor, inlines::escape_attribute};

impl<'a, W: Write> HtmlVisitor<'a, '_, W> {
    pub(crate) fn render_image(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        img: &Image,
    ) -> Result<(), Error> {
        let processor = self.processor.clone();
        if processor.variant() == HtmlVariant::Semantic {
            return self.render_image_semantic(traversal, img);
        }

        // Build class list: imageblock + alignment + float + roles
        let mut classes = vec!["imageblock".to_string()];

        // align=left|center|right → text-left|text-center|text-right
        if let Some(align) = img.metadata.attributes.get_string("align") {
            classes.push(format!("text-{align}"));
        }

        // float=left|right → left|right
        if let Some(float) = img.metadata.attributes.get_string("float") {
            classes.push(float.into_owned());
        }

        // roles → added as classes
        for role in &img.metadata.roles {
            classes.push(role.to_string());
        }

        write!(self.writer, "<div")?;
        crate::write_id(&mut self.writer, &img.metadata)?;
        write!(
            self.writer,
            " class=\"{}\">",
            escape_attribute(&classes.join(" "))
        )?;
        write!(self.writer, "<div class=\"content\">")?;
        // Get alt text from attribute or generate from filename

        // Wrap in link if link attribute exists
        let link = img.metadata.attributes.get("link");
        let image = ImageRef::from(img);
        let link_controls = image_link_control_attributes(image.attributes(), image.options());
        if let Some(link) = link {
            write!(
                self.writer,
                "<a class=\"image\" href=\"{}\"{link_controls}>",
                escape_attribute(&link.to_string()),
            )?;
        }

        let markup = self.image_markup(
            traversal,
            &img.source,
            &img.metadata.attributes,
            &img.metadata.options,
            img.alt.as_ref(),
            None,
        )?;
        write!(self.writer, "{markup}")?;

        if link.is_some() {
            write!(self.writer, "</a>")?;
        }
        write!(self.writer, "</div>")?; // close content

        // Render title with figure caption if title exists
        // Caption can be disabled with :figure-caption!:
        if img.title.is_present() {
            self.render_captioned_title_with_wrapper(
                traversal,
                &img.title,
                &img.metadata,
                Some(CaptionKind::Figure),
                "<div class=\"title\">",
                "</div>",
            )?;
        }

        write!(self.writer, "</div>")?; // close imageblock
        Ok(())
    }

    fn render_image_semantic(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        img: &Image,
    ) -> Result<(), Error> {
        let has_title = img.title.is_present();

        // Build class and style for wrapper
        let mut classes = vec!["image-block".to_string()];
        for role in &img.metadata.roles {
            classes.push(role.to_string());
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
        write!(
            self.writer,
            "<{tag} class=\"{}\"",
            escape_attribute(&classes.join(" "))
        )?;
        if let Some(id) = &img.metadata.id {
            write!(self.writer, " id=\"{}\"", escape_attribute(id.id))?;
        } else if let Some(anchor) = img.metadata.anchors.first() {
            write!(self.writer, " id=\"{}\"", escape_attribute(anchor.id))?;
        }
        if !styles.is_empty() {
            write!(
                self.writer,
                " style=\"{}\"",
                escape_attribute(&styles.join("; "))
            )?;
        }
        writeln!(self.writer, ">")?;

        let source = escape_attribute(&resolve_target(&img.source.to_string(), traversal));

        // Check for link=self, link=none, or html5s-image-default-link=self
        let link = img.metadata.attributes.get("link");
        let image = ImageRef::from(img);
        let link_controls = image_link_control_attributes(image.attributes(), image.options());
        let link_str = link.as_ref().map(ToString::to_string);
        let is_link_none = link_str.as_deref() == Some("none");
        let is_link_self = link_str.as_deref() == Some("self");

        let use_self_link = is_link_self
            || (!is_link_none
                && link.is_none()
                && traversal
                    .get("html5s-image-default-link")
                    .and_then(|value| value.text())
                    == Some("self"));

        // Check if default-link=self but explicit link=none should suppress
        let suppress_default_self = is_link_none
            && traversal
                .get("html5s-image-default-link")
                .and_then(|value| value.text())
                == Some("self");

        if use_self_link && !suppress_default_self {
            let label = traversal
                .get("html5s-image-self-link-label")
                .and_then(|value| value.text())
                .map_or_else(
                    || "Open the image in full size".to_string(),
                    escape_attribute,
                );
            write!(
                self.writer,
                "<a class=\"image bare\" href=\"{source}\"{link_controls} title=\"{label}\" aria-label=\"{label}\">",
            )?;
        } else if !is_link_none
            && !is_link_self
            && let Some(ref link_str) = link_str
        {
            write!(
                self.writer,
                "<a class=\"image\" href=\"{}\"{link_controls}>",
                escape_attribute(link_str),
            )?;
        }

        let markup = self.image_markup(
            traversal,
            &img.source,
            &img.metadata.attributes,
            &img.metadata.options,
            img.alt.as_ref(),
            None,
        )?;
        write!(self.writer, "{markup}")?;

        // Close link tag if we opened one
        let has_link = (use_self_link && !suppress_default_self)
            || (!is_link_none && !is_link_self && link_str.is_some());
        if has_link {
            write!(self.writer, "</a>")?;
        }

        if has_title {
            self.render_captioned_title_with_wrapper(
                traversal,
                &img.title,
                &img.metadata,
                Some(CaptionKind::Figure),
                "<figcaption>",
                "</figcaption>\n",
            )?;
        }

        writeln!(self.writer, "</{tag}>")?;
        Ok(())
    }
}

pub(crate) fn image_link_control_attributes(
    attributes: AttributesRef<'_>,
    options: TokensRef<'_>,
) -> String {
    let window = attributes
        .get("window")
        .and_then(|value| {
            if let AttributeValueRef::Text(text) = value {
                Some(strip_quotes(text))
            } else {
                None
            }
        })
        .filter(|window| !window.is_empty());
    let nofollow = options.contains("nofollow");
    let noopener = window.is_some_and(|window| window == "_blank" || options.contains("noopener"));

    let mut attributes = window.map_or_else(String::new, |window| {
        format!(" target=\"{}\"", escape_attribute(window))
    });
    match (nofollow, noopener) {
        (true, true) => attributes.push_str(" rel=\"nofollow noopener\""),
        (true, false) => attributes.push_str(" rel=\"nofollow\""),
        (false, true) => attributes.push_str(" rel=\"noopener\""),
        (false, false) => {}
    }
    attributes
}
