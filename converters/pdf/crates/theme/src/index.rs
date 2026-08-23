use serde::Deserialize;

use crate::Error;

/// Column layout for generated index catalogs.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Index {
    /// Number of catalog columns.
    pub columns: usize,
    /// Space between columns in points, or the body font size when omitted.
    pub column_gap_pt: Option<f64>,
}

impl Default for Index {
    fn default() -> Self {
        Self {
            columns: 2,
            column_gap_pt: None,
        }
    }
}

impl Index {
    pub(super) fn validate(&self) -> Result<(), Error> {
        if self.columns == 0 {
            return Err(Error::validation(
                "index.columns",
                "expected a positive integer, got 0",
            ));
        }
        if let Some(value) = self.column_gap_pt
            && (!value.is_finite() || value < 0.0)
        {
            return Err(Error::validation(
                "index.column_gap_pt",
                format!("expected a finite non-negative number, got {value}"),
            ));
        }
        Ok(())
    }
}
