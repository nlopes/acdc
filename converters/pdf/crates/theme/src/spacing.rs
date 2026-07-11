use serde::Deserialize;

use crate::Error;

/// Spacing, page geometry, and corner radii, in points unless noted.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spacing {
    /// Horizontal page margin, in centimetres.
    pub margin_x_cm: f64,
    /// Vertical page margin, in centimetres.
    pub margin_y_cm: f64,
    /// Corner radius for embedded images.
    pub image_radius_pt: f64,
    /// Corner radius for code blocks.
    pub code_radius_pt: f64,
    /// Corner radius for alert/callout cards.
    pub callout_radius_pt: f64,
    /// Left indent of callout cards from the text column.
    pub callout_indent_pt: f64,
    /// Horizontal inset inside callout cards.
    pub callout_pad_x_pt: f64,
    /// Vertical inset inside callout cards.
    pub callout_pad_y_pt: f64,
    /// Padding inside code blocks.
    pub code_pad_pt: f64,
    /// Thickness of the blockquote left rule.
    pub quote_rule_pt: f64,
    /// Indent of blockquote content past the rule.
    pub quote_indent_pt: f64,
    /// Thickness of rules and table borders.
    pub border_pt: f64,
}

impl Spacing {
    pub(super) fn validate(&self) -> Result<(), Error> {
        for (field, value) in [
            ("spacing.margin_x_cm", self.margin_x_cm),
            ("spacing.margin_y_cm", self.margin_y_cm),
            ("spacing.image_radius_pt", self.image_radius_pt),
            ("spacing.code_radius_pt", self.code_radius_pt),
            ("spacing.callout_radius_pt", self.callout_radius_pt),
            ("spacing.callout_indent_pt", self.callout_indent_pt),
            ("spacing.callout_pad_x_pt", self.callout_pad_x_pt),
            ("spacing.callout_pad_y_pt", self.callout_pad_y_pt),
            ("spacing.code_pad_pt", self.code_pad_pt),
            ("spacing.quote_rule_pt", self.quote_rule_pt),
            ("spacing.quote_indent_pt", self.quote_indent_pt),
            ("spacing.border_pt", self.border_pt),
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
