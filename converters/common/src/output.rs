/// Normalizes output for comparison.
///
/// This removes trailing whitespace and normalizes line endings.
pub fn remove_lines_trailing_whitespace(output: &str) -> String {
    output
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
}
