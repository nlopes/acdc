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

use acdc_parser::{
    ContentAttribute, DocumentAttributes, Source,
    ast::{ContentRef, SourceKind, SourceRef},
};

use crate::TraversalContext;

/// Resolve the text alternative for an icon.
///
/// An explicit `alt` attribute takes precedence. Otherwise, hyphens and
/// underscores in the icon target become spaces.
#[must_use]
pub fn alt<'a>(target: &Source<'a>, content: Option<&ContentAttribute<'_>>) -> Cow<'a, str> {
    if let Some(content) = content {
        return Cow::Owned(alt_text(ContentRef::from(content)));
    }
    match target {
        Source::Name(name) if !name.contains(['-', '_']) => Cow::Borrowed(*name),
        Source::Name(name) => Cow::Owned(name.replace(['-', '_'], " ")),
        Source::Path(_) | Source::Url(_) => Cow::Owned(target.to_string().replace(['-', '_'], " ")),
    }
}

pub(crate) fn alt_from_content<'a>(
    target: SourceRef<'a>,
    content: Option<ContentRef<'_>>,
) -> Cow<'a, str> {
    if let Some(content) = content {
        return Cow::Owned(alt_text(content));
    }
    match target.kind() {
        SourceKind::Name(name) if !name.contains(['-', '_']) => Cow::Borrowed(name),
        SourceKind::Name(name) => Cow::Owned(name.replace(['-', '_'], " ")),
        SourceKind::Path(_) | SourceKind::Url(_) | _ => {
            Cow::Owned(target.to_string().replace(['-', '_'], " "))
        }
    }
}

fn alt_text(content: ContentRef<'_>) -> String {
    crate::inline_text::InlineTextTransform::default()
        .rendered_replacements(true)
        .decode_char_refs(true)
        .content_to_string(content)
}

/// Build the logical image source for an icon.
///
/// `iconsdir` defaults to `./images/icons`. An explicit `icontype` takes
/// precedence over an image format supplied through `icons`, and the default
/// extension is `png`.
/// Accepts typed sources and existing source references.
#[must_use]
pub fn image_source<'doc>(
    attributes: &TraversalContext<'_>,
    target: impl Into<SourceRef<'doc>>,
) -> String {
    custom_image_source(attributes, &target.into().to_string())
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

/// Resolve an `icon=` override relative to `iconsdir`, adding the configured
/// extension when the override has none. Absolute targets retain their directory.
#[must_use]
pub fn custom_image_source(attributes: &TraversalContext<'_>, target: &str) -> String {
    let target = if std::path::Path::new(target.split(['?', '#']).next().unwrap_or(target))
        .extension()
        .is_some()
    {
        Cow::Borrowed(target)
    } else {
        let extension = crate::document_attribute_text((attributes).get("icontype"))
            .or_else(|| {
                crate::document_attribute_text((attributes).get("icons"))
                    .filter(|value| !value.is_empty() && *value != "image" && *value != "font")
            })
            .unwrap_or("png");
        Cow::Owned(format!("{target}.{}", extension.trim_start_matches('.')))
    };
    if target.starts_with('/') || target.contains("://") {
        return target.into_owned();
    }
    let directory = crate::document_attribute_text((attributes).get("iconsdir")).map_or_else(
        || {
            crate::document_attribute_text((attributes).get("imagesdir")).map_or_else(
                || "./images/icons".to_string(),
                |imagesdir| format!("{}/icons", imagesdir.trim_end_matches(['/', '\\'])),
            )
        },
        |iconsdir| iconsdir.trim_end_matches(['/', '\\']).to_string(),
    );
    if directory.is_empty() {
        target.into_owned()
    } else {
        format!("{directory}/{target}")
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use acdc_parser::{AttributeValue, Options};

    use super::*;

    #[test]
    fn default_alt_can_outlive_the_source_wrapper() {
        let text = {
            let target = Source::Name("arrow");
            alt(&target, None)
        };
        assert!(matches!(text, Cow::Borrowed("arrow")));
    }

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
        assert_eq!(alt(&target, None), "arrow left");
        let content = ContentAttribute::new(vec![acdc_parser::InlineNode::PlainText(
            acdc_parser::Plain {
                content: "Back",
                location: acdc_parser::Location::default(),
                escaped: false,
            },
        )]);
        assert_eq!(alt(&target, Some(&content)), "Back");
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
