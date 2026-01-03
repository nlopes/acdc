//! Icon rendering mode configuration.
//!
//! `AsciiDoc` supports three icon rendering modes controlled by the `:icons:` attribute:
//!
//! - **Text mode** (default): Icons rendered as text labels `[NOTE]`, `[TIP]`, etc.
//! - **Image mode** (`:icons:` or `:icons: image`): Icons rendered as images from `iconsdir`
//! - **Font mode** (`:icons: font`): Icons rendered using Font Awesome icon font
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
//!     IconMode::Font => println!("Using Font Awesome"),
//!     IconMode::Image => println!("Using image icons"),
//!     IconMode::Text => println!("Using text labels"),
//! }
//! ```

use acdc_parser::{AttributeValue, DocumentAttributes};

/// Icon rendering mode.
///
/// Determined by the `:icons:` document attribute. Converters should use this
/// to decide how to render admonition icons and inline icon macros.
#[derive(Clone, Debug, Eq, PartialEq, Default)]
#[non_exhaustive]
pub enum IconMode {
    /// Use Font Awesome icon font (`:icons: font`).
    ///
    /// Requires the Font Awesome CSS to be loaded in the output.
    Font,

    /// Use image files from `iconsdir` (`:icons:` or `:icons: image`).
    ///
    /// Images are loaded from the directory specified by `:iconsdir:`.
    Image,

    /// Use text labels like `[NOTE]`, `[TIP]` (default, no `:icons:` attribute).
    #[default]
    Text,
}

impl From<&DocumentAttributes> for IconMode {
    fn from(attrs: &DocumentAttributes) -> Self {
        if let Some(icons_value) = attrs.get("icons") {
            match icons_value {
                AttributeValue::String(s) if s == "image" => Self::Image,
                AttributeValue::Bool(true) => Self::Image,
                AttributeValue::String(s) if s == "font" => Self::Font,
                AttributeValue::String(_) | AttributeValue::Bool(_) | AttributeValue::None | _ => {
                    Self::Text
                }
            }
        } else {
            Self::Text
        }
    }
}
