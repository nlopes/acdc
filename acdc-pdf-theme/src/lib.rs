//! Design tokens plus bundled fonts and syntax highlighting for acdc's PDF converter.
#![forbid(unsafe_code)]

mod color;
mod error;
mod fonts;
mod spacing;
mod syntax;
mod typography;

use std::sync::LazyLock;

use serde::Deserialize;

pub use color::Palette;
pub use error::Error;
pub use fonts::{EMOJI_FONT_FAMILY, embedded_fonts};
pub use spacing::Spacing;
pub use syntax::{HIGHLIGHT_THEME_PATH, highlight_theme};
pub use typography::{FontStack, Typography};

const DEFAULT_THEME_YAML: &str = include_str!("../assets/theme/default.yaml");

/// A complete set of PDF design tokens.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Theme {
    pub palette: Palette,
    pub typography: Typography,
    pub spacing: Spacing,
}

impl Theme {
    /// Parse and validate one YAML theme document.
    ///
    /// `serde-saphyr` supplies bounded parsing and alias protection; this
    /// method adds token-specific checks.
    ///
    /// # Errors
    /// Returns [`Error`] when the YAML or a theme value is invalid.
    pub fn from_yaml_str(yaml: &str) -> Result<Self, Error> {
        let mut theme: Self = serde_saphyr::from_str(yaml)?;
        theme.palette.normalize()?;
        theme.validate()?;
        Ok(theme)
    }

    fn validate(&self) -> Result<(), Error> {
        self.palette.validate()?;
        self.typography.validate()?;
        self.spacing.validate()
    }
}

static DEFAULT_THEME: LazyLock<Theme> = LazyLock::new(|| {
    #[expect(
        clippy::expect_used,
        reason = "a unit test validates this compile-time-embedded asset"
    )]
    Theme::from_yaml_str(DEFAULT_THEME_YAML).expect("bundled default theme is valid YAML")
});

impl Default for Theme {
    fn default() -> Self {
        DEFAULT_THEME.clone()
    }
}

#[cfg(test)]
mod tests {
    use proptest::{collection::vec, prelude::*};

    use super::*;

    #[test]
    fn bundled_default_is_valid_and_stable() -> Result<(), Box<dyn std::error::Error>> {
        let theme = Theme::from_yaml_str(DEFAULT_THEME_YAML)?;
        assert_eq!(theme.palette.page_bg, "#ffffff");
        assert_eq!(theme.typography.body_font.fallback, ["IBM Plex Serif"]);
        assert_eq!(Theme::default(), theme);
        Ok(())
    }

    #[test]
    fn accepts_brand_fonts_and_normalizes_short_colours() -> Result<(), Box<dyn std::error::Error>>
    {
        let yaml = DEFAULT_THEME_YAML
            .replace("brand: null", "brand: Brand Sans")
            .replace("#ffffff", "#AbC");
        let theme = Theme::from_yaml_str(&yaml)?;

        assert_eq!(
            theme.typography.body_font.brand.as_deref(),
            Some("Brand Sans")
        );
        assert_eq!(theme.palette.page_bg, "#aabbcc");
        Ok(())
    }

    #[test]
    fn rejects_unknown_fields() {
        assert!(Theme::from_yaml_str(&format!("{DEFAULT_THEME_YAML}\nunknown: true")).is_err());
    }

    #[test]
    fn parser_has_no_artificial_document_size_limit() -> Result<(), Error> {
        let yaml = format!("{DEFAULT_THEME_YAML}\n# {}", "x".repeat(128 * 1024));
        Theme::from_yaml_str(&yaml)?;
        Ok(())
    }

    #[test]
    fn rejects_invalid_colours() {
        for invalid in ["red", "#12", "#abcd", "#12345g", " #fff"] {
            let result = Theme::from_yaml_str(&DEFAULT_THEME_YAML.replace("#ffffff", invalid));
            assert!(
                matches!(&result, Err(Error::Validation { field, .. }) if field == "palette.page_bg"),
                "unexpected result for {invalid:?}: {result:?}"
            );
        }
    }

    #[test]
    fn rejects_invalid_measurements_and_weights() {
        for (original, replacement, field) in [
            (
                "body_size_pt: 11.0",
                "body_size_pt: .nan",
                "typography.body_size_pt",
            ),
            (
                "body_size_pt: 11.0",
                "body_size_pt: 0",
                "typography.body_size_pt",
            ),
            (
                "tracking_em: 0.0",
                "tracking_em: .inf",
                "typography.tracking_em",
            ),
            (
                "margin_x_cm: 2.5",
                "margin_x_cm: -0.1",
                "spacing.margin_x_cm",
            ),
            (
                "body_weight: 400",
                "body_weight: 99",
                "typography.body_weight",
            ),
        ] {
            let result = Theme::from_yaml_str(&DEFAULT_THEME_YAML.replace(original, replacement));
            assert!(
                matches!(&result, Err(Error::Validation { field: actual, .. }) if actual == field),
                "unexpected result for {field}: {result:?}"
            );
        }
    }

    #[test]
    fn rejects_invalid_font_stacks() {
        for fallback in ["[]", "[\" IBM Plex Serif\"]", "[\"IBM\\nPlex Serif\"]"] {
            let yaml = DEFAULT_THEME_YAML.replacen(
                "fallback: [\"IBM Plex Serif\"]",
                &format!("fallback: {fallback}"),
                1,
            );
            assert!(matches!(
                Theme::from_yaml_str(&yaml),
                Err(Error::Validation { field, .. })
                    if field.starts_with("typography.body_font")
            ));
        }
    }

    #[test]
    fn malformed_yaml_never_panics() {
        for yaml in [
            "",
            ":",
            "palette: [",
            "---\n---\n",
            "palette: {page_bg: \"unterminated}",
        ] {
            assert!(std::panic::catch_unwind(|| Theme::from_yaml_str(yaml)).is_ok());
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        #[test]
        fn arbitrary_unicode_yaml_never_panics(characters in vec(any::<char>(), 0..=2_048)) {
            let yaml = characters.into_iter().collect::<String>();
            drop(Theme::from_yaml_str(&yaml));
        }
    }
}
