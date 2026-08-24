//! Design tokens plus bundled fonts and syntax highlighting for acdc's PDF converter.
#![forbid(unsafe_code)]

mod caption;
mod color;
mod error;
mod fonts;
mod heading;
mod index;
mod spacing;
mod syntax;
mod table;
mod typography;

use std::sync::LazyLock;

use serde::Deserialize;

pub use caption::{Caption, CaptionAlignment, CaptionFontStyle};
pub use color::Palette;
pub use error::Error;
pub use fonts::{EMOJI_FONT_FAMILY, embedded_fonts};
pub use heading::{ChapterHeading, Heading, PageBreakBefore, PartBreakAfter, PartHeading};
pub use index::Index;
pub use spacing::Spacing;
pub use syntax::{HIGHLIGHT_THEME_PATH, highlight_theme};
pub use table::{Table, TableAlignment};
pub use typography::{FontStack, Typography};

const DEFAULT_THEME_YAML: &str = include_str!("../assets/theme/default.yaml");

/// A complete set of PDF design tokens.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Theme {
    pub palette: Palette,
    pub typography: Typography,
    pub spacing: Spacing,
    #[serde(default)]
    pub caption: Caption,
    #[serde(default)]
    pub heading: Heading,
    #[serde(default)]
    pub index: Index,
    #[serde(default)]
    pub table: Table,
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
        theme.caption.normalize()?;
        theme.table.normalize()?;
        theme.validate()?;
        Ok(theme)
    }

    fn validate(&self) -> Result<(), Error> {
        self.palette.validate()?;
        self.caption.validate()?;
        self.typography.validate()?;
        self.spacing.validate()?;
        self.index.validate()?;
        self.table.validate()
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
        assert!((theme.typography.code_size_em - 0.8).abs() < f64::EPSILON);
        assert!((theme.typography.code_min_size_em - 0.6).abs() < f64::EPSILON);
        assert_eq!(theme.caption, Caption::default());
        assert_eq!(theme.heading, Heading::default());
        assert_eq!(theme.index.columns, 2);
        assert_eq!(theme.index.column_gap_pt, Some(12.0));
        assert_eq!(theme.table, Table::default());
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
    fn accepts_heading_page_break_overrides() -> Result<(), Box<dyn std::error::Error>> {
        let yaml = DEFAULT_THEME_YAML
            .replace("break_after: auto", "break_after: avoid")
            .replace(
                "chapter:\n    break_before: always",
                "chapter:\n    break_before: auto",
            );
        let theme = Theme::from_yaml_str(&yaml)?;

        assert_eq!(theme.heading.part.break_after, PartBreakAfter::Avoid);
        assert_eq!(theme.heading.chapter.break_before, PageBreakBefore::Auto);
        Ok(())
    }

    #[test]
    fn defaults_heading_page_breaks_when_omitted() -> Result<(), Box<dyn std::error::Error>> {
        let yaml = DEFAULT_THEME_YAML.replace(
            "heading:\n  part:\n    break_before: always\n    break_after: auto\n  chapter:\n    break_before: always\n",
            "",
        );
        let theme = Theme::from_yaml_str(&yaml)?;

        assert_eq!(theme.heading, Heading::default());
        Ok(())
    }

    #[test]
    fn defaults_caption_style_when_omitted() -> Result<(), Box<dyn std::error::Error>> {
        let yaml = DEFAULT_THEME_YAML.replace(
            "caption:\n  align: left\n  font_color: \"#333333\"\n  font_size_em: 0.91\n  font_weight: 400\n  font_style: italic\n  margin_inside_pt: 8.0\n  margin_outside_pt: 0.0\n",
            "",
        );

        let theme = Theme::from_yaml_str(&yaml)?;

        assert_eq!(theme.caption, Caption::default());
        Ok(())
    }

    #[test]
    fn defaults_table_style_when_omitted() -> Result<(), Box<dyn std::error::Error>> {
        let yaml = DEFAULT_THEME_YAML.replace(
            "table:\n  align: left\n  border_color: \"#dddddd\"\n  border_width_pt: 0.5\n  header_divider_width_pt: 1.25\n  header_background: null\n  stripe_background: \"#f9f9f9\"\n  footer_background: \"#f0f0f0\"\n",
            "",
        );

        let theme = Theme::from_yaml_str(&yaml)?;

        assert_eq!(theme.table, Table::default());
        Ok(())
    }

    #[test]
    fn accepts_index_column_layout() -> Result<(), Box<dyn std::error::Error>> {
        let yaml = DEFAULT_THEME_YAML
            .replace("  columns: 2", "  columns: 3")
            .replace("  column_gap_pt: 12.0", "  column_gap_pt: 18.5");

        let theme = Theme::from_yaml_str(&yaml)?;

        assert_eq!(theme.index.columns, 3);
        assert_eq!(theme.index.column_gap_pt, Some(18.5));
        Ok(())
    }

    #[test]
    fn defaults_index_layout_when_omitted() -> Result<(), Box<dyn std::error::Error>> {
        let yaml = DEFAULT_THEME_YAML.replace("index:\n  columns: 2\n  column_gap_pt: 12.0\n", "");

        let theme = Theme::from_yaml_str(&yaml)?;

        assert_eq!(theme.index, Index::default());
        Ok(())
    }

    #[test]
    fn rejects_invalid_index_layout() {
        for (original, replacement, field) in [
            ("columns: 2", "columns: 0", "index.columns"),
            (
                "column_gap_pt: 12.0",
                "column_gap_pt: -.inf",
                "index.column_gap_pt",
            ),
        ] {
            let result = Theme::from_yaml_str(&DEFAULT_THEME_YAML.replace(original, replacement));
            assert!(
                matches!(&result, Err(Error::Validation { field: actual, .. }) if actual == field),
                "unexpected result for {field}: {result:?}",
            );
        }
    }

    #[test]
    fn rejects_invalid_table_alignment() {
        let yaml = DEFAULT_THEME_YAML.replace("table:\n  align: left", "table:\n  align: diagonal");

        assert!(Theme::from_yaml_str(&yaml).is_err());
    }

    #[test]
    fn defaults_block_margin_when_omitted() -> Result<(), Box<dyn std::error::Error>> {
        let yaml = DEFAULT_THEME_YAML.replace("  block_margin_bottom_pt: 12.0\n", "");

        let theme = Theme::from_yaml_str(&yaml)?;

        assert!((theme.spacing.block_margin_bottom_pt - 12.0).abs() < f64::EPSILON);
        Ok(())
    }

    #[test]
    fn defaults_code_sizes_when_omitted() -> Result<(), Box<dyn std::error::Error>> {
        let yaml = DEFAULT_THEME_YAML
            .replace("  code_size_em: 0.8\n", "")
            .replace("  code_min_size_em: 0.6\n", "");

        let theme = Theme::from_yaml_str(&yaml)?;

        assert!((theme.typography.code_size_em - 0.8).abs() < f64::EPSILON);
        assert!((theme.typography.code_min_size_em - 0.6).abs() < f64::EPSILON);
        Ok(())
    }

    #[test]
    fn validates_and_normalizes_caption_style() -> Result<(), Box<dyn std::error::Error>> {
        let yaml = DEFAULT_THEME_YAML
            .replace("font_color: \"#333333\"", "font_color: '#AbC'")
            .replace("font_size_em: 0.91", "font_size_em: 1.2")
            .replace("font_weight: 400", "font_weight: 600")
            .replace("font_style: italic", "font_style: normal")
            .replace("align: left", "align: center");

        let theme = Theme::from_yaml_str(&yaml)?;

        assert_eq!(theme.caption.align, CaptionAlignment::Center);
        assert_eq!(theme.caption.font_color.as_deref(), Some("#aabbcc"));
        assert!((theme.caption.font_size_em - 1.2).abs() < f64::EPSILON);
        assert_eq!(theme.caption.font_weight, 600);
        assert_eq!(theme.caption.font_style, CaptionFontStyle::Normal);
        Ok(())
    }

    #[test]
    fn validates_and_normalizes_table_style() -> Result<(), Box<dyn std::error::Error>> {
        let yaml = DEFAULT_THEME_YAML
            .replace("table:\n  align: left", "table:\n  align: right")
            .replace("border_color: \"#dddddd\"", "border_color: '#AbC'")
            .replace("border_width_pt: 0.5", "border_width_pt: 0.75")
            .replace(
                "header_divider_width_pt: 1.25",
                "header_divider_width_pt: 1.5",
            )
            .replace("header_background: null", "header_background: '#123'")
            .replace(
                "stripe_background: \"#f9f9f9\"",
                "stripe_background: '#456'",
            )
            .replace(
                "footer_background: \"#f0f0f0\"",
                "footer_background: '#789'",
            );

        let theme = Theme::from_yaml_str(&yaml)?;

        assert_eq!(theme.table.align, TableAlignment::Right);
        assert_eq!(theme.table.border_color, "#aabbcc");
        assert!((theme.table.border_width_pt - 0.75).abs() < f64::EPSILON);
        assert!((theme.table.header_divider_width_pt - 1.5).abs() < f64::EPSILON);
        assert_eq!(theme.table.header_background.as_deref(), Some("#112233"));
        assert_eq!(theme.table.stripe_background, "#445566");
        assert_eq!(theme.table.footer_background.as_deref(), Some("#778899"));
        Ok(())
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
        let result = Theme::from_yaml_str(
            &DEFAULT_THEME_YAML.replace("font_color: \"#333333\"", "font_color: red"),
        );
        assert!(matches!(
            result,
            Err(Error::Validation { field, .. }) if field == "caption.font_color"
        ));
        for (original, replacement, field) in [
            (
                "border_color: \"#dddddd\"",
                "border_color: red",
                "table.border_color",
            ),
            (
                "stripe_background: \"#f9f9f9\"",
                "stripe_background: red",
                "table.stripe_background",
            ),
            (
                "footer_background: \"#f0f0f0\"",
                "footer_background: red",
                "table.footer_background",
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
                "code_size_em: 0.8",
                "code_size_em: .nan",
                "typography.code_size_em",
            ),
            (
                "code_min_size_em: 0.6",
                "code_min_size_em: 0",
                "typography.code_min_size_em",
            ),
            (
                "code_min_size_em: 0.6",
                "code_min_size_em: 0.9",
                "typography.code_min_size_em",
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
                "block_margin_bottom_pt: 12.0",
                "block_margin_bottom_pt: -0.1",
                "spacing.block_margin_bottom_pt",
            ),
            (
                "body_weight: 400",
                "body_weight: 99",
                "typography.body_weight",
            ),
            (
                "font_size_em: 0.91",
                "font_size_em: 0",
                "caption.font_size_em",
            ),
            (
                "font_weight: 400",
                "font_weight: 901",
                "caption.font_weight",
            ),
            (
                "margin_inside_pt: 8.0",
                "margin_inside_pt: -0.1",
                "caption.margin_inside_pt",
            ),
            (
                "border_width_pt: 0.5",
                "border_width_pt: -0.1",
                "table.border_width_pt",
            ),
            (
                "header_divider_width_pt: 1.25",
                "header_divider_width_pt: .nan",
                "table.header_divider_width_pt",
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
