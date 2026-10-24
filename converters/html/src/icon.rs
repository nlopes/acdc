use std::fmt::Write as _;

use acdc_converters_core::{
    TraversalContext,
    icon::{IconMode, alt as icon_alt, image_source},
};
use acdc_parser::{BlockMetadata, ElementAttributes, Icon};

use crate::{
    Error, HtmlVisitor,
    image::image_link_control_attributes,
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
fn get_fa_family(attrs: &ElementAttributes, doc_attrs: &TraversalContext<'_>) -> String {
    // 1. Icon macro attribute: set= or pack= (aliases)
    let macro_value = attrs.get_string("set").or_else(|| attrs.get_string("pack"));

    if let Some(ref val) = macro_value
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
        icon: &Icon<'_>,
    ) -> Result<String, Error> {
        let attrs = &icon.attributes;
        let mut class = String::from("icon");
        for name in ["float", "role"] {
            if let Some(value) = attrs.get_string(name) {
                write!(class, " {value}")?;
            }
        }
        let tooltip = tooltip_attribute(icon.tooltip.as_ref());
        let mut inner = match IconMode::from_attributes(traversal) {
            IconMode::Font => {
                let mut classes = format!("{} fa-{}", get_fa_family(attrs, traversal), icon.target);
                if let Some(size) = attrs.get_string("size") {
                    write!(classes, " fa-{size}")?;
                }
                if let Some(flip) = attrs.get_string("flip") {
                    write!(classes, " fa-flip-{flip}")?;
                } else if let Some(rotate) = attrs.get_string("rotate") {
                    write!(classes, " fa-rotate-{rotate}")?;
                }
                format!("<i class=\"{}\"{tooltip}></i>", escape_attribute(&classes))
            }
            IconMode::Image => {
                let target = attrs.get_string("icon").map_or_else(
                    || image_source(traversal, &icon.target),
                    |value| acdc_converters_core::icon::custom_image_source(traversal, &value),
                );
                let source = self.image_uri(&target, traversal);
                let alt = icon_alt(&icon.target, icon.alt.as_ref());
                let mut dimensions = String::new();
                for name in ["width", "height"] {
                    if let Some(value) = attrs.get_string(name) {
                        write!(dimensions, " {name}=\"{}\"", escape_attribute(&value))?;
                    }
                }
                format!(
                    "<img src=\"{}\" alt=\"{}\"{dimensions}{tooltip}>",
                    escape_attribute(&source),
                    escape_attribute(&alt)
                )
            }
            IconMode::Text | _ => {
                let alt = match &icon.alt {
                    Some(content) => self.content_html(traversal, content)?,
                    None => escape_attribute(&icon_alt(&icon.target, None)),
                };
                format!("[{alt}]")
            }
        };
        if let Some(link) = attrs.get_string("link") {
            let mut metadata = BlockMetadata::new().with_attributes(attrs.clone());
            if let Some(options) = attrs.get_string("options") {
                metadata.options = options.split(',').map(str::trim).collect();
                let controls = image_link_control_attributes(&metadata);
                inner = format!(
                    "<a class=\"image\" href=\"{}\"{controls}>{inner}</a>",
                    escape_attribute(&link)
                );
            } else {
                let controls = image_link_control_attributes(&metadata);
                inner = format!(
                    "<a class=\"image\" href=\"{}\"{controls}>{inner}</a>",
                    escape_attribute(&link)
                );
            }
        }
        Ok(format!(
            "<span class=\"{}\">{inner}</span>",
            escape_attribute(&class)
        ))
    }
}
