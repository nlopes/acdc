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

use acdc_parser::{DocumentAttributes, ElementAttributes, Source, strip_quotes};

use crate::TraversalContext;

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
pub fn image_source(attributes: &TraversalContext<'_>, target: &Source<'_>) -> String {
    let directory = attributes
        .get("iconsdir")
        .and_then(|value| value.text())
        .map_or("./images/icons", strip_quotes);
    let extension = attributes
        .get("icontype")
        .and_then(|value| value.text())
        .map(strip_quotes)
        .or_else(|| {
            attributes
                .get("icons")
                .and_then(|value| value.text())
                .map(strip_quotes)
                .filter(|value| !value.is_empty() && *value != "image" && *value != "font")
        })
        .unwrap_or("png");
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
        Self::from_attributes(&TraversalContext::new(attrs))
    }
}

impl IconMode {
    /// Select the icon mode from an active document-attribute view.
    #[must_use]
    pub fn from_attributes(attrs: &TraversalContext<'_>) -> Self {
        match attrs.get("icons") {
            Some(value) if value.as_str() == Some("font") => Self::Font,
            Some(value) if value.as_str().is_some() || value.is_presence() => Self::Image,
            Some(_) | None => Self::Text,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use acdc_parser::{AttributeValue, Options};

    use super::*;

    #[test]
    fn mode_matches_asciidoctor_attribute_semantics() -> Result<(), Box<dyn std::error::Error>> {
        let mut attributes = std::collections::HashMap::<
            std::borrow::Cow<'_, str>,
            acdc_parser::AttributeValue<'_>,
        >::new();
        assert_eq!(
            IconMode::from(
                &Options::builder()
                    .with_defaults(attributes.clone())
                    .build()?
                    .into_document_attributes()
            ),
            IconMode::Text
        );

        attributes.insert("icons".into(), AttributeValue::Bool(true));
        assert_eq!(
            IconMode::from(
                &Options::builder()
                    .with_defaults(attributes.clone())
                    .build()?
                    .into_document_attributes()
            ),
            IconMode::Image
        );

        attributes.insert(
            "icons".into(),
            AttributeValue::String(Cow::Borrowed("image")),
        );
        assert_eq!(
            IconMode::from(
                &Options::builder()
                    .with_defaults(attributes.clone())
                    .build()?
                    .into_document_attributes()
            ),
            IconMode::Image
        );

        attributes.insert(
            "icons".into(),
            AttributeValue::String(Cow::Borrowed("custom")),
        );
        assert_eq!(
            IconMode::from(
                &Options::builder()
                    .with_defaults(attributes.clone())
                    .build()?
                    .into_document_attributes()
            ),
            IconMode::Image
        );

        attributes.insert(
            "icons".into(),
            AttributeValue::String(Cow::Borrowed("font")),
        );
        assert_eq!(
            IconMode::from(
                &Options::builder()
                    .with_defaults(attributes.clone())
                    .build()?
                    .into_document_attributes()
            ),
            IconMode::Font
        );

        attributes.insert("icons".into(), AttributeValue::Bool(false));
        assert_eq!(
            IconMode::from(
                &Options::builder()
                    .with_defaults(attributes.clone())
                    .build()?
                    .into_document_attributes()
            ),
            IconMode::Text
        );
        Ok(())
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
    fn image_source_honors_directory_and_type_attributes() -> Result<(), Box<dyn std::error::Error>>
    {
        let target = Source::Name("arrow-left");
        let mut attributes = std::collections::HashMap::<
            std::borrow::Cow<'_, str>,
            acdc_parser::AttributeValue<'_>,
        >::new();

        assert_eq!(
            image_source(
                &TraversalContext::new(
                    &Options::builder()
                        .with_defaults(attributes.clone())
                        .build()?
                        .into_document_attributes()
                ),
                &target
            ),
            "./images/icons/arrow-left.png"
        );

        attributes.insert(
            "iconsdir".into(),
            AttributeValue::String(Cow::Borrowed("assets/icons/")),
        );
        attributes.insert("icons".into(), AttributeValue::String(Cow::Borrowed("svg")));
        assert_eq!(
            image_source(
                &TraversalContext::new(
                    &Options::builder()
                        .with_defaults(attributes.clone())
                        .build()?
                        .into_document_attributes()
                ),
                &target
            ),
            "assets/icons/arrow-left.svg"
        );

        attributes.insert(
            "icontype".into(),
            AttributeValue::String(Cow::Borrowed(".png")),
        );
        assert_eq!(
            image_source(
                &TraversalContext::new(
                    &Options::builder()
                        .with_defaults(attributes.clone())
                        .build()?
                        .into_document_attributes()
                ),
                &target
            ),
            "assets/icons/arrow-left.png"
        );
        Ok(())
    }
}
