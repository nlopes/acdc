//! Output normalization utilities for test comparisons.
//!
//! This module provides functions to normalize converter output for reliable
//! test comparisons, handling whitespace differences that don't affect semantics.

/// Normalizes output for comparison by removing trailing whitespace from each line.
///
/// This is useful for test assertions where trailing whitespace differences
/// between expected and actual output should be ignored.
///
/// # Example
///
/// ```
/// use acdc_converters_dev::output::remove_lines_trailing_whitespace;
///
/// let input = "line1   \nline2\t\nline3";
/// let normalized = remove_lines_trailing_whitespace(input);
/// assert_eq!(normalized, "line1\nline2\nline3");
/// ```
#[must_use]
pub fn remove_lines_trailing_whitespace(output: &str) -> String {
    output
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
}
