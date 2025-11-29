//! Table utilities shared between converters.

use acdc_parser::{ColumnFormat, ColumnWidth};

/// Calculate column widths as percentages from column format specifications.
///
/// Converts proportional widths (e.g., `1,3`) to percentages (e.g., `25%, 75%`).
/// Percentage widths are passed through directly. Auto widths return 0.0,
/// leaving the renderer to decide.
///
/// # Arguments
///
/// * `columns` - Slice of column format specifications from the table
///
/// # Returns
///
/// Vector of width percentages. Empty if input is empty.
///
/// # Examples
///
/// ```ignore
/// // [cols="1,3"] -> [25.0, 75.0]
/// // [cols="2,1,1,1,1"] -> [33.33, 16.67, 16.67, 16.67, 16.67]
/// // [cols="25%,75%"] -> [25.0, 75.0]
/// ```
#[must_use]
pub fn calculate_column_widths(columns: &[ColumnFormat]) -> Vec<f64> {
    if columns.is_empty() {
        return vec![];
    }

    // Sum all proportional widths
    let total_proportional: u32 = columns
        .iter()
        .filter_map(|c| match c.width {
            ColumnWidth::Proportional(w) => Some(w),
            ColumnWidth::Percentage(_) | ColumnWidth::Auto => None,
        })
        .sum();

    // Calculate percentage for each column
    columns
        .iter()
        .map(|c| match c.width {
            ColumnWidth::Proportional(w) if total_proportional > 0 => {
                (f64::from(w) / f64::from(total_proportional)) * 100.0
            }
            // No proportional context or auto width - let renderer decide
            ColumnWidth::Proportional(_) | ColumnWidth::Auto => 0.0,
            ColumnWidth::Percentage(p) => f64::from(p),
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;
    use acdc_parser::{ColumnStyle, HorizontalAlignment, VerticalAlignment};

    fn make_column(width: ColumnWidth) -> ColumnFormat {
        ColumnFormat {
            halign: HorizontalAlignment::Left,
            valign: VerticalAlignment::Top,
            width,
            style: ColumnStyle::Default,
        }
    }

    #[test]
    fn test_proportional_widths() {
        let columns = vec![
            make_column(ColumnWidth::Proportional(1)),
            make_column(ColumnWidth::Proportional(3)),
        ];
        let widths = calculate_column_widths(&columns);
        assert_eq!(widths.len(), 2);
        assert!((widths[0] - 25.0).abs() < 0.01);
        assert!((widths[1] - 75.0).abs() < 0.01);
    }

    #[test]
    fn test_equal_proportional_widths() {
        let columns = vec![
            make_column(ColumnWidth::Proportional(1)),
            make_column(ColumnWidth::Proportional(1)),
            make_column(ColumnWidth::Proportional(1)),
        ];
        let widths = calculate_column_widths(&columns);
        assert_eq!(widths.len(), 3);
        for w in &widths {
            assert!((*w - 33.333).abs() < 0.01);
        }
    }

    #[test]
    fn test_percentage_widths() {
        let columns = vec![
            make_column(ColumnWidth::Percentage(25)),
            make_column(ColumnWidth::Percentage(75)),
        ];
        let widths = calculate_column_widths(&columns);
        assert_eq!(widths, vec![25.0, 75.0]);
    }

    #[test]
    fn test_auto_widths() {
        let columns = vec![
            make_column(ColumnWidth::Auto),
            make_column(ColumnWidth::Auto),
        ];
        let widths = calculate_column_widths(&columns);
        assert_eq!(widths, vec![0.0, 0.0]);
    }

    #[test]
    fn test_empty_columns() {
        let widths = calculate_column_widths(&[]);
        assert!(widths.is_empty());
    }

    #[test]
    fn test_complex_proportional() {
        // [cols="2,1,1,1,1"] -> 33.33%, 16.67%, 16.67%, 16.67%, 16.67%
        let columns = vec![
            make_column(ColumnWidth::Proportional(2)),
            make_column(ColumnWidth::Proportional(1)),
            make_column(ColumnWidth::Proportional(1)),
            make_column(ColumnWidth::Proportional(1)),
            make_column(ColumnWidth::Proportional(1)),
        ];
        let widths = calculate_column_widths(&columns);
        assert_eq!(widths.len(), 5);
        // 2/(2+1+1+1+1) = 2/6 = 33.33%
        assert!((widths[0] - 33.333).abs() < 0.01);
        // 1/6 = 16.67%
        for w in &widths[1..] {
            assert!((*w - 16.667).abs() < 0.01);
        }
    }
}
