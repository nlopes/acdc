use serde::Deserialize;

use crate::{Error, color::canonical_colour};

/// Horizontal alignment of a block title or caption.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptionAlignment {
    /// Align with the left edge of the block.
    #[default]
    Left,
    /// Centre over the block.
    Center,
    /// Align with the right edge of the block.
    Right,
}

/// Font style of a block title or caption.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptionFontStyle {
    /// Use upright text.
    Normal,
    /// Use italic text.
    #[default]
    Italic,
}

/// Styling shared by block titles and captions.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Caption {
    /// Text alignment within the block width.
    pub align: CaptionAlignment,
    /// Text colour, or the body-text colour when omitted.
    pub font_color: Option<String>,
    /// Font size relative to the body size, in em.
    pub font_size_em: f64,
    /// Typst font weight from 100 through 900.
    pub font_weight: u16,
    /// Upright or italic font style.
    pub font_style: CaptionFontStyle,
    /// Space between the title and its block, in points.
    pub margin_inside_pt: f64,
    /// Space outside the title, in points.
    pub margin_outside_pt: f64,
}

impl Default for Caption {
    fn default() -> Self {
        Self {
            align: CaptionAlignment::Left,
            font_color: Some("#333333".to_owned()),
            font_size_em: 0.91,
            font_weight: 400,
            font_style: CaptionFontStyle::Italic,
            margin_inside_pt: 8.0,
            margin_outside_pt: 0.0,
        }
    }
}

impl Caption {
    pub(super) fn normalize(&mut self) -> Result<(), Error> {
        if let Some(color) = &mut self.font_color {
            *color = canonical_colour("caption.font_color", color)?;
        }
        Ok(())
    }

    pub(super) fn validate(&self) -> Result<(), Error> {
        if let Some(color) = &self.font_color {
            canonical_colour("caption.font_color", color)?;
        }
        if !self.font_size_em.is_finite() || self.font_size_em <= 0.0 {
            return Err(Error::validation(
                "caption.font_size_em",
                format!(
                    "expected a finite positive number, got {}",
                    self.font_size_em
                ),
            ));
        }
        if !(100..=900).contains(&self.font_weight) {
            return Err(Error::validation(
                "caption.font_weight",
                format!(
                    "expected a Typst font weight from 100 through 900, got {}",
                    self.font_weight
                ),
            ));
        }
        for (field, value) in [
            ("caption.margin_inside_pt", self.margin_inside_pt),
            ("caption.margin_outside_pt", self.margin_outside_pt),
        ] {
            if !value.is_finite() || value < 0.0 {
                return Err(Error::validation(
                    field,
                    format!("expected a finite non-negative number, got {value}"),
                ));
            }
        }
        Ok(())
    }
}
