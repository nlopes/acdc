use serde::Deserialize;

use crate::{Error, color::canonical_colour};

/// Styling for table rules and section backgrounds.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Table {
    /// Outer-frame and grid-rule colour.
    pub border_color: String,
    /// Outer-frame and grid-rule width, in points.
    pub border_width_pt: f64,
    /// Header-divider width, in points.
    pub header_divider_width_pt: f64,
    /// Header background, or the table background when omitted.
    pub header_background: Option<String>,
    /// Striped body-row background.
    pub stripe_background: String,
    /// Footer background, or the table background when omitted.
    pub footer_background: Option<String>,
}

impl Default for Table {
    fn default() -> Self {
        Self {
            border_color: "#dddddd".to_owned(),
            border_width_pt: 0.5,
            header_divider_width_pt: 1.25,
            header_background: None,
            stripe_background: "#f9f9f9".to_owned(),
            footer_background: Some("#f0f0f0".to_owned()),
        }
    }
}

impl Table {
    pub(super) fn normalize(&mut self) -> Result<(), Error> {
        self.border_color = canonical_colour("table.border_color", &self.border_color)?;
        self.stripe_background =
            canonical_colour("table.stripe_background", &self.stripe_background)?;
        if let Some(color) = &mut self.header_background {
            *color = canonical_colour("table.header_background", color)?;
        }
        if let Some(color) = &mut self.footer_background {
            *color = canonical_colour("table.footer_background", color)?;
        }
        Ok(())
    }

    pub(super) fn validate(&self) -> Result<(), Error> {
        canonical_colour("table.border_color", &self.border_color)?;
        canonical_colour("table.stripe_background", &self.stripe_background)?;
        if let Some(color) = &self.header_background {
            canonical_colour("table.header_background", color)?;
        }
        if let Some(color) = &self.footer_background {
            canonical_colour("table.footer_background", color)?;
        }
        for (field, value) in [
            ("table.border_width_pt", self.border_width_pt),
            (
                "table.header_divider_width_pt",
                self.header_divider_width_pt,
            ),
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
