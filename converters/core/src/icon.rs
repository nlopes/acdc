//! Icon rendering mode configuration.
//!
//! `AsciiDoc` supports three icon rendering modes controlled by the `:icons:` attribute:
//!
//! - **Text mode** (default): Icons rendered as text labels `[NOTE]`, `[TIP]`, etc.
//! - **Image mode** (`:icons:` or any set value other than `font`): Icons rendered as images
//!   from `iconsdir`
//! - **Font mode** (`:icons: font`): Icons rendered using the backend's supported glyph set
//!
//! # Example
//!
//! ```ignore
//! use acdc_converters_core::icon::IconMode;
//! use acdc_parser::DocumentAttributes;
//!
//! let attrs = document.attributes;
//! let mode = IconMode::from(&attrs);
//! match mode {
//!     IconMode::Font => println!("Using font or built-in glyph icons"),
//!     IconMode::Image => println!("Using image icons"),
//!     IconMode::Text => println!("Using text labels"),
//! }
//! ```

use std::borrow::Cow;

use acdc_parser::{AttributeValue, DocumentAttributes, ElementAttributes, Source};

/// Resolve the text alternative for an icon.
///
/// An explicit `alt` attribute takes precedence. Otherwise, hyphens and
/// underscores in the icon target become spaces.
#[must_use]
pub fn alt<'a>(target: &Source<'a>, attributes: &ElementAttributes<'a>) -> Cow<'a, str> {
    attributes
        .get_string("alt")
        .unwrap_or_else(|| match target {
            Source::Name(name) if !name.contains(['-', '_']) => Cow::Borrowed(name),
            Source::Name(name) => Cow::Owned(name.replace(['-', '_'], " ")),
            Source::Path(_) | Source::Url(_) => {
                Cow::Owned(target.to_string().replace(['-', '_'], " "))
            }
        })
}

/// Build the logical image source for an icon.
///
/// `iconsdir` defaults to `./images/icons`. An explicit `icontype` takes
/// precedence over an image format supplied through `icons`, and the default
/// extension is `png`.
#[must_use]
pub fn image_source(attributes: &DocumentAttributes<'_>, target: &Source<'_>) -> String {
    let directory = attributes
        .get_string("iconsdir")
        .unwrap_or_else(|| "./images/icons".into());
    let extension = attributes
        .get_string("icontype")
        .or_else(|| {
            attributes.get_string("icons").filter(|value| {
                !value.is_empty() && value.as_ref() != "image" && value.as_ref() != "font"
            })
        })
        .unwrap_or_else(|| "png".into());
    let directory = directory.trim_end_matches(['/', '\\']);
    let extension = extension.trim_start_matches('.');

    if directory.is_empty() {
        format!("{target}.{extension}")
    } else {
        format!("{directory}/{target}.{extension}")
    }
}

/// Icon rendering mode.
///
/// Determined by the `:icons:` document attribute. Converters should use this
/// to decide how to render admonition icons and inline icon macros.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
#[non_exhaustive]
pub enum IconMode {
    /// Use the backend's supported font or built-in glyph set (`:icons: font`).
    Font,

    /// Use image files from `iconsdir` (`:icons:` or any set value other than `font`).
    ///
    /// Images are loaded from the directory specified by `:iconsdir:`.
    Image,

    /// Use text labels like `[NOTE]`, `[TIP]` (default, no `:icons:` attribute).
    #[default]
    Text,
}

impl From<&DocumentAttributes<'_>> for IconMode {
    fn from(attrs: &DocumentAttributes<'_>) -> Self {
        match attrs.get("icons") {
            Some(AttributeValue::String(value)) if value == "font" => Self::Font,
            Some(AttributeValue::String(_) | AttributeValue::Bool(true)) => Self::Image,
            Some(_) | None => Self::Text,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::*;

    #[test]
    fn mode_matches_asciidoctor_attribute_semantics() {
        let mut attributes = DocumentAttributes::default();
        assert_eq!(IconMode::from(&attributes), IconMode::Text);

        attributes.set("icons".into(), AttributeValue::Bool(true));
        assert_eq!(IconMode::from(&attributes), IconMode::Image);

        attributes.set(
            "icons".into(),
            AttributeValue::String(Cow::Borrowed("image")),
        );
        assert_eq!(IconMode::from(&attributes), IconMode::Image);

        attributes.set(
            "icons".into(),
            AttributeValue::String(Cow::Borrowed("custom")),
        );
        assert_eq!(IconMode::from(&attributes), IconMode::Image);

        attributes.set(
            "icons".into(),
            AttributeValue::String(Cow::Borrowed("font")),
        );
        assert_eq!(IconMode::from(&attributes), IconMode::Font);

        attributes.set("icons".into(), AttributeValue::Bool(false));
        assert_eq!(IconMode::from(&attributes), IconMode::Text);
    }

    #[test]
    fn alt_prefers_explicit_text_then_normalizes_the_target() {
        let target = Source::Name("arrow-left");
        let mut attributes = ElementAttributes::default();

        assert_eq!(alt(&target, &attributes), "arrow left");

        attributes.set("alt".into(), AttributeValue::String(Cow::Borrowed("Back")));
        assert_eq!(alt(&target, &attributes), "Back");
    }

    #[test]
    fn image_source_honors_directory_and_type_attributes() {
        let target = Source::Name("arrow-left");
        let mut attributes = DocumentAttributes::default();

        assert_eq!(
            image_source(&attributes, &target),
            "./images/icons/arrow-left.png"
        );

        attributes.set(
            "iconsdir".into(),
            AttributeValue::String(Cow::Borrowed("assets/icons/")),
        );
        attributes.set("icons".into(), AttributeValue::String(Cow::Borrowed("svg")));
        assert_eq!(
            image_source(&attributes, &target),
            "assets/icons/arrow-left.svg"
        );

        attributes.set(
            "icontype".into(),
            AttributeValue::String(Cow::Borrowed(".png")),
        );
        assert_eq!(
            image_source(&attributes, &target),
            "assets/icons/arrow-left.png"
        );
    }
}
