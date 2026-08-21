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

use acdc_parser::{AttributeValue, DocumentAttributes};

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
}
