use serde::Deserialize;

use crate::Error;

/// Horizontal alignment of page-header content.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeaderAlignment {
    /// Align content with the left edge.
    #[default]
    Left,
    /// Centre content across the page width.
    Center,
    /// Align content with the right edge.
    Right,
}

/// Horizontal footer slot used for the page number.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PageNumberPosition {
    /// Use the left footer slot.
    Left,
    /// Use the centre footer slot.
    #[default]
    Center,
    /// Use the right footer slot.
    Right,
}

/// Styling and placement of the page header.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Header {
    /// Horizontal placement of the logo and title.
    pub align: HeaderAlignment,
    /// Header title size, in points.
    pub font_size_pt: f64,
    /// Header title font weight from 100 through 900.
    pub font_weight: u16,
    /// Logo height, in points.
    pub logo_height_pt: f64,
    /// Whether to show the header when the current page counter is 1.
    pub show_on_page_one: bool,
}

impl Default for Header {
    fn default() -> Self {
        Self {
            align: HeaderAlignment::Left,
            font_size_pt: 11.0,
            font_weight: 500,
            logo_height_pt: 22.0,
            show_on_page_one: false,
        }
    }
}

impl Header {
    pub(super) fn validate(&self) -> Result<(), Error> {
        positive("header.font_size_pt", self.font_size_pt)?;
        positive("header.logo_height_pt", self.logo_height_pt)?;
        font_weight("header.font_weight", self.font_weight)
    }
}

/// Styling and placement of the page footer.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Footer {
    /// Footer text size, in points.
    pub font_size_pt: f64,
    /// Horizontal slot used for the page number.
    pub page_number_position: PageNumberPosition,
}

impl Default for Footer {
    fn default() -> Self {
        Self {
            font_size_pt: 9.0,
            page_number_position: PageNumberPosition::Center,
        }
    }
}

impl Footer {
    pub(super) fn validate(&self) -> Result<(), Error> {
        positive("footer.font_size_pt", self.font_size_pt)
    }
}

fn positive(field: &'static str, value: f64) -> Result<(), Error> {
    if !value.is_finite() || value <= 0.0 {
        return Err(Error::validation(
            field,
            format!("expected a finite positive number, got {value}"),
        ));
    }
    Ok(())
}

fn font_weight(field: &'static str, value: u16) -> Result<(), Error> {
    if !(100..=900).contains(&value) {
        return Err(Error::validation(
            field,
            format!("expected a Typst font weight from 100 through 900, got {value}"),
        ));
    }
    Ok(())
}
