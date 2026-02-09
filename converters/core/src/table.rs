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
            ColumnWidth::Percentage(_) | ColumnWidth::Auto | _ => None,
        })
        .sum();

    // Calculate percentage for each column
    let mut widths: Vec<f64> = columns
        .iter()
        .map(|c| match c.width {
            ColumnWidth::Proportional(w) if total_proportional > 0 => {
                (f64::from(w) / f64::from(total_proportional)) * 100.0
            }
            ColumnWidth::Percentage(p) => f64::from(p),
            // No proportional context, auto, or unknown width - let renderer decide
            ColumnWidth::Proportional(_) | ColumnWidth::Auto | _ => 0.0,
        })
        .collect();

    // Normalize percentage widths to sum to 100% (like asciidoctor does).
    // Only non-zero (non-auto) widths participate in normalization.
    let pct_sum: f64 = widths.iter().filter(|w| **w > 0.0).sum();
    if pct_sum > 0.0 && (pct_sum - 100.0).abs() > f64::EPSILON {
        let scale = 100.0 / pct_sum;
        for w in &mut widths {
            if *w > 0.0 {
                *w *= scale;
            }
        }
    }

    widths
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn make_column(width: ColumnWidth) -> ColumnFormat {
        ColumnFormat::new().with_width(width)
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
    fn test_percentage_over_100_normalized() {
        // [cols="34%,36%,31%"] sums to 101% → normalize to 100%
        let columns = vec![
            make_column(ColumnWidth::Percentage(34)),
            make_column(ColumnWidth::Percentage(36)),
            make_column(ColumnWidth::Percentage(31)),
        ];
        let widths = calculate_column_widths(&columns);
        let sum: f64 = widths.iter().sum();
        assert!((sum - 100.0).abs() < 0.01, "sum was {sum}");
        // Each scaled by 100/101
        assert!((widths[0] - 34.0 * 100.0 / 101.0).abs() < 0.01);
        assert!((widths[1] - 36.0 * 100.0 / 101.0).abs() < 0.01);
        assert!((widths[2] - 31.0 * 100.0 / 101.0).abs() < 0.01);
    }

    #[test]
    fn test_percentage_under_100_normalized() {
        // [cols="20%,30%,40%"] sums to 90% → normalize to 100%
        let columns = vec![
            make_column(ColumnWidth::Percentage(20)),
            make_column(ColumnWidth::Percentage(30)),
            make_column(ColumnWidth::Percentage(40)),
        ];
        let widths = calculate_column_widths(&columns);
        let sum: f64 = widths.iter().sum();
        assert!((sum - 100.0).abs() < 0.01, "sum was {sum}");
        assert!((widths[0] - 20.0 * 100.0 / 90.0).abs() < 0.01);
        assert!((widths[1] - 30.0 * 100.0 / 90.0).abs() < 0.01);
        assert!((widths[2] - 40.0 * 100.0 / 90.0).abs() < 0.01);
    }

    #[test]
    fn test_percentage_exact_100_unchanged() {
        let columns = vec![
            make_column(ColumnWidth::Percentage(25)),
            make_column(ColumnWidth::Percentage(25)),
            make_column(ColumnWidth::Percentage(50)),
        ];
        let widths = calculate_column_widths(&columns);
        assert_eq!(widths, vec![25.0, 25.0, 50.0]);
    }

    #[test]
    fn test_auto_with_percentage_over_100() {
        // [cols="~,60%,50%"] non-auto sum=110% → normalize percentages, auto stays 0.0
        let columns = vec![
            make_column(ColumnWidth::Auto),
            make_column(ColumnWidth::Percentage(60)),
            make_column(ColumnWidth::Percentage(50)),
        ];
        let widths = calculate_column_widths(&columns);
        assert!((widths[0] - 0.0).abs() < f64::EPSILON);
        let pct_sum: f64 = widths[1] + widths[2];
        assert!((pct_sum - 100.0).abs() < 0.01, "pct sum was {pct_sum}");
        assert!((widths[1] - 60.0 * 100.0 / 110.0).abs() < 0.01);
        assert!((widths[2] - 50.0 * 100.0 / 110.0).abs() < 0.01);
    }

    #[test]
    fn test_all_auto_no_normalization() {
        let columns = vec![
            make_column(ColumnWidth::Auto),
            make_column(ColumnWidth::Auto),
        ];
        let widths = calculate_column_widths(&columns);
        assert_eq!(widths, vec![0.0, 0.0]);
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
