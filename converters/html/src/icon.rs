use std::fmt::Write as _;

use acdc_converters_core::{
    TraversalContext,
    icon::{IconMode, alt_from_content as icon_alt, image_source},
};
use acdc_parser::ast::{AttributesRef, IconRef};

use crate::{
    Error, HtmlVisitor,
    image::image_link_control_attributes,
    image_helpers::attribute_text,
    inlines::{escape_attribute, tooltip_attribute},
};

/// Resolve a Font Awesome family value to a CSS class prefix.
///
/// Supports both shorthand (e.g., `fab`) and long-form (e.g., `brands`) values.
/// Returns `None` for unrecognized values, which means the caller should use
/// the default `fa-solid` prefix.
fn resolve_fa_family(value: &str) -> Option<&'static str> {
    match value {
        "fab" | "brands" => Some("fa-brands"),
        "far" | "regular" => Some("fa-regular"),
        "fas" | "solid" => Some("fa-solid"),
        "fal" | "light" => Some("fa-light"),
        "fat" | "thin" => Some("fa-thin"),
        "fad" | "duotone" => Some("fa-duotone"),
        "fass" | "sharp-solid" => Some("fa-sharp fa-solid"),
        _ => None,
    }
}

/// Get the FA family CSS class for an icon, checking macro attributes then document defaults.
fn get_fa_family(attrs: AttributesRef<'_>, doc_attrs: &TraversalContext<'_>) -> String {
    // 1. Icon macro attribute: set= or pack= (aliases)
    let macro_value = attribute_text(attrs, "set").or_else(|| attribute_text(attrs, "pack"));

    if let Some(val) = macro_value
        && let Some(css_class) = resolve_fa_family(val)
    {
        return css_class.to_string();
    }

    // 2. Document attribute: :icon-set: or :icon-pack: (aliases)
    let doc_value = acdc_converters_core::document_attribute_text((doc_attrs).get("icon-set"))
        .or_else(|| acdc_converters_core::document_attribute_text((doc_attrs).get("icon-pack")));

    if let Some(val) = doc_value
        && let Some(css_class) = resolve_fa_family(val)
    {
        return css_class.to_string();
    }

    // 3. Fallback: solid (FA v7 default)
    "fa-solid".to_string()
}

impl<'a, W: std::io::Write> HtmlVisitor<'a, '_, W> {
    pub(crate) fn icon_markup(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        icon: IconRef<'_>,
    ) -> Result<String, Error> {
        let attrs = icon.attributes();
        let mut class = String::from("icon");
        if let Some(value) = attribute_text(attrs, "float") {
            write!(class, " {value}")?;
        }
        for role in icon.roles().iter() {
            write!(class, " {role}")?;
        }
        let tooltip = tooltip_attribute(icon.tooltip());
        let mut inner = match IconMode::from_attributes(traversal) {
            IconMode::Font => {
                let mut classes =
                    format!("{} fa-{}", get_fa_family(attrs, traversal), icon.target());
                if let Some(size) = attribute_text(attrs, "size") {
                    write!(classes, " fa-{size}")?;
                }
                if let Some(flip) = attribute_text(attrs, "flip") {
                    write!(classes, " fa-flip-{flip}")?;
                } else if let Some(rotate) = attribute_text(attrs, "rotate") {
                    write!(classes, " fa-rotate-{rotate}")?;
                }
                format!(
                    "<i class=\"{}\"{tooltip}></i>",
                    crate::inlines::escape_role(&classes)
                )
            }
            IconMode::Image => {
                let target = attribute_text(attrs, "icon").map_or_else(
                    || image_source(traversal, icon.target()),
                    |value| acdc_converters_core::icon::custom_image_source(traversal, value),
                );
                let source = self.image_uri(&target, traversal);
                let alt = icon_alt(icon.target(), icon.alt());
                let mut dimensions = String::new();
                for name in ["width", "height"] {
                    if let Some(value) = attribute_text(attrs, name) {
                        write!(dimensions, " {name}=\"{}\"", escape_attribute(value))?;
                    }
                }
                format!(
                    "<img src=\"{}\" alt=\"{}\"{dimensions}{tooltip}>",
                    escape_attribute(&source),
                    escape_attribute(&alt)
                )
            }
            IconMode::Text | _ => {
                let alt = match icon.alt() {
                    Some(content) => self.content_html(traversal, content)?,
                    None => escape_attribute(&icon_alt(icon.target(), None)),
                };
                format!("[{alt}]")
            }
        };
        if let Some(link) = attribute_text(attrs, "link") {
            let controls = image_link_control_attributes(icon.attributes(), icon.options());
            inner = format!(
                "<a class=\"image\" href=\"{}\"{controls}>{inner}</a>",
                escape_attribute(link)
            );
        }
        Ok(format!(
            "<span class=\"{}\">{inner}</span>",
            crate::inlines::escape_role(&class)
        ))
    }
}
