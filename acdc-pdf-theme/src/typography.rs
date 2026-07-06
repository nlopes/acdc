use serde::Deserialize;

use crate::Error;

/// A font family with an optional brand face and one or more fallbacks.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FontStack {
    /// Preferred brand family, used only when brand fonts are available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brand: Option<String>,
    /// Fallback families, in priority order.
    pub fallback: Vec<String>,
}

impl FontStack {
    fn validate(&self, field: &'static str) -> Result<(), Error> {
        if self.fallback.is_empty() {
            return Err(Error::validation(
                format!("{field}.fallback"),
                "expected at least one fallback family",
            ));
        }

        for (suffix, family) in self
            .brand
            .iter()
            .map(|family| ("brand".to_owned(), family))
            .chain(
                self.fallback
                    .iter()
                    .enumerate()
                    .map(|(index, family)| (format!("fallback[{index}]"), family)),
            )
        {
            if family.trim().is_empty() || family.trim() != family {
                return Err(Error::validation(
                    format!("{field}.{suffix}"),
                    "font family must be non-empty with no surrounding whitespace",
                ));
            }
            if family.chars().any(char::is_control) {
                return Err(Error::validation(
                    format!("{field}.{suffix}"),
                    "font family must not contain control characters",
                ));
            }
        }
        Ok(())
    }
}

/// Font families and the type scale. Sizes are in points.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Typography {
    pub body_font: FontStack,
    pub heading_font: FontStack,
    pub mono_font: FontStack,
    /// Base body size, in points.
    pub body_size_pt: f64,
    /// Heading sizes H1 through H6, in points.
    pub heading_pt: [f64; 6],
    /// Letter spacing, in em.
    pub tracking_em: f64,
    /// Body line spacing, in em.
    pub body_leading_em: f64,
    /// Regular body weight.
    pub body_weight: u16,
    /// Weight for strong/bold text.
    pub strong_weight: u16,
    /// Weight for headings.
    pub heading_weight: u16,
    /// Weight for table header cells.
    pub table_header_weight: u16,
}

impl Typography {
    pub(super) fn validate(&self) -> Result<(), Error> {
        self.body_font.validate("typography.body_font")?;
        self.heading_font.validate("typography.heading_font")?;
        self.mono_font.validate("typography.mono_font")?;

        positive("typography.body_size_pt", self.body_size_pt)?;
        for (index, value) in self.heading_pt.iter().copied().enumerate() {
            positive(format!("typography.heading_pt[{index}]"), value)?;
        }
        finite("typography.tracking_em", self.tracking_em)?;
        non_negative("typography.body_leading_em", self.body_leading_em)?;

        for (field, value) in [
            ("typography.body_weight", self.body_weight),
            ("typography.strong_weight", self.strong_weight),
            ("typography.heading_weight", self.heading_weight),
            ("typography.table_header_weight", self.table_header_weight),
        ] {
            if !(100..=900).contains(&value) {
                return Err(Error::validation(
                    field,
                    format!("expected a Typst font weight from 100 through 900, got {value}"),
                ));
            }
        }
        Ok(())
    }
}

fn positive(field: impl Into<String>, value: f64) -> Result<(), Error> {
    if !value.is_finite() || value <= 0.0 {
        return Err(Error::validation(
            field,
            format!("expected a finite positive number, got {value}"),
        ));
    }
    Ok(())
}

fn non_negative(field: impl Into<String>, value: f64) -> Result<(), Error> {
    if !value.is_finite() || value < 0.0 {
        return Err(Error::validation(
            field,
            format!("expected a finite non-negative number, got {value}"),
        ));
    }
    Ok(())
}

fn finite(field: impl Into<String>, value: f64) -> Result<(), Error> {
    if !value.is_finite() {
        return Err(Error::validation(
            field,
            format!("expected a finite number, got {value}"),
        ));
    }
    Ok(())
}
