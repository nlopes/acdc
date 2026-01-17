use std::fmt::Write as _;
use std::io::{self, Write};

use acdc_parser::{AttributeValue, ElementAttributes, ICON_SIZES, Icon, Source};

use crate::Processor;

/// Check if a positional attribute exists (stored as key with `AttributeValue::None`).
fn has_positional_attr(attrs: &ElementAttributes, name: &str) -> bool {
    matches!(attrs.get(name), Some(AttributeValue::None))
}

/// Get the icon size from attributes (either named `size=...` or positional like `2x`).
fn get_icon_size(attrs: &ElementAttributes) -> Option<String> {
    // First check named attribute
    if let Some(size) = attrs.get_string("size") {
        return Some(size);
    }
    // Then check for positional size values
    for size in ICON_SIZES {
        if has_positional_attr(attrs, size) {
            return Some((*size).to_string());
        }
    }
    None
}

/// Write an icon macro to HTML output.
///
/// Handles three icon modes:
/// - Font mode (`icons=font`): Uses Font Awesome icons
/// - Image mode (`icons` set but not "font"): Uses image files
/// - Text mode (no `icons` attribute): Uses text placeholders
pub(crate) fn write_icon<W: Write + ?Sized>(
    w: &mut W,
    processor: &Processor,
    icon: &Icon,
) -> io::Result<()> {
    let target = &icon.target;
    let attrs = &icon.attributes;

    // Build span class with optional role
    let span_class = match attrs.get_string("role") {
        Some(role) => format!("icon {role}"),
        None => "icon".to_string(),
    };

    // Determine icon mode based on document attribute
    if let Some(icons_value) = processor.document_attributes.get("icons") {
        if icons_value.to_string() == "font" {
            write_font_icon(w, target, attrs, &span_class)?;
        } else {
            write_image_icon(w, processor, target, attrs, &span_class)?;
        }
    } else {
        // Text mode: [target]
        write!(w, "<span class=\"{span_class}\">[{target}]</span>")?;
    }

    Ok(())
}

/// Write a font-based icon (Font Awesome).
fn write_font_icon<W: Write + ?Sized>(
    w: &mut W,
    target: &Source,
    attrs: &ElementAttributes,
    span_class: &str,
) -> io::Result<()> {
    // Build icon classes: fa fa-{target} [fa-{size}] [fa-flip-{dir}|fa-rotate-{deg}] [fa-{modifier}]
    let mut classes = format!("fa fa-{target}");

    if let Some(size) = get_icon_size(attrs) {
        let _ = write!(classes, " fa-{size}");
    }

    // flip takes precedence over rotate (matches asciidoctor behavior)
    if let Some(flip) = attrs.get_string("flip") {
        let _ = write!(classes, " fa-flip-{flip}");
    } else if let Some(rotate) = attrs.get_string("rotate") {
        let _ = write!(classes, " fa-rotate-{rotate}");
    }

    // Build title attribute
    let title_attr = attrs
        .get_string("title")
        .map(|t| format!(" title=\"{t}\""))
        .unwrap_or_default();

    // Build the <i> element
    let i_elem = format!("<i class=\"{classes}\"{title_attr}></i>");

    // Wrap with link if present
    let inner = wrap_icon_with_link(&i_elem, attrs);

    write!(w, "<span class=\"{span_class}\">{inner}</span>")
}

/// Write an image-based icon.
fn write_image_icon<W: Write + ?Sized>(
    w: &mut W,
    processor: &Processor,
    target: &Source,
    attrs: &ElementAttributes,
    span_class: &str,
) -> io::Result<()> {
    // Get iconsdir (defaults to "./images/icons")
    let iconsdir = processor
        .document_attributes
        .get("iconsdir")
        .map_or_else(|| "./images/icons".to_string(), ToString::to_string);

    // Build alt attribute (use custom alt or target name)
    let alt = attrs
        .get_string("alt")
        .unwrap_or_else(|| target.to_string());

    // Build img attributes
    let mut img_attrs = format!("src=\"{iconsdir}/{target}.png\" alt=\"{alt}\"");

    if let Some(width) = attrs.get_string("width") {
        let _ = write!(img_attrs, " width=\"{width}\"");
    }

    if let Some(title) = attrs.get_string("title") {
        let _ = write!(img_attrs, " title=\"{title}\"");
    }

    // Build the <img> element
    let img_elem = format!("<img {img_attrs}>");

    // Wrap with link if present
    let inner = wrap_icon_with_link(&img_elem, attrs);

    write!(w, "<span class=\"{span_class}\">{inner}</span>")
}

/// Wrap icon content with a link if the `link` attribute is present.
fn wrap_icon_with_link(content: &str, attrs: &ElementAttributes) -> String {
    if let Some(link) = attrs.get_string("link") {
        // HTML-escape ampersands in URLs for valid HTML
        let escaped_link = link.replace('&', "&amp;");
        let window_attrs = attrs
            .get_string("window")
            .map(|w| format!(" target=\"{w}\" rel=\"noopener\""))
            .unwrap_or_default();
        format!("<a class=\"image\" href=\"{escaped_link}\"{window_attrs}>{content}</a>")
    } else {
        content.to_string()
    }
}
