use crate::Table;

/// Represents a parsed cell specifier with span information.
///
/// In `AsciiDoc`, cell specifiers appear before the cell separator:
/// - `2+|content` → colspan=2
/// - `.3+|content` → rowspan=3
/// - `2.3+|content` → colspan=2, rowspan=3
#[derive(Debug, Clone, Copy)]
pub(crate) struct CellSpecifier {
    pub colspan: usize,
    pub rowspan: usize,
}

impl Default for CellSpecifier {
    fn default() -> Self {
        Self {
            colspan: 1,
            rowspan: 1,
        }
    }
}

impl CellSpecifier {
    /// Parse a cell specifier from the beginning of cell content.
    ///
    /// Returns the specifier and the offset where actual content begins.
    /// Pattern: `(\d+)?(\.\d+)?\+`
    ///
    /// Examples:
    /// - `"2+rest"` → `(CellSpecifier { colspan: 2, rowspan: 1 }, 2)`
    /// - `".3+rest"` → `(CellSpecifier { colspan: 1, rowspan: 3 }, 3)`
    /// - `"2.3+rest"` → `(CellSpecifier { colspan: 2, rowspan: 3 }, 4)`
    /// - `"plain"` → `(CellSpecifier { colspan: 1, rowspan: 1 }, 0)`
    #[must_use]
    pub fn parse(content: &str) -> (Self, usize) {
        let bytes = content.as_bytes();
        let mut pos = 0;
        let mut colspan: Option<usize> = None;
        let mut rowspan: Option<usize> = None;

        // Parse optional colspan (digits before optional dot)
        let colspan_start = pos;
        while bytes.get(pos).is_some_and(u8::is_ascii_digit) {
            pos += 1;
        }
        if pos > colspan_start {
            if let Some(n) = content
                .get(colspan_start..pos)
                .and_then(|s| s.parse::<usize>().ok())
            {
                colspan = Some(n);
            }
        }

        // Parse optional rowspan (dot followed by digits)
        if bytes.get(pos) == Some(&b'.') {
            let dot_pos = pos;
            pos += 1;
            let rowspan_start = pos;
            while bytes.get(pos).is_some_and(u8::is_ascii_digit) {
                pos += 1;
            }
            if pos > rowspan_start {
                if let Some(n) = content
                    .get(rowspan_start..pos)
                    .and_then(|s| s.parse::<usize>().ok())
                {
                    rowspan = Some(n);
                }
            } else {
                // Dot without following digits - not a span specifier
                pos = dot_pos;
            }
        }

        // Must end with '+' to be a valid span specifier
        if bytes.get(pos) == Some(&b'+') && (colspan.is_some() || rowspan.is_some()) {
            pos += 1;
            (
                Self {
                    colspan: colspan.unwrap_or(1),
                    rowspan: rowspan.unwrap_or(1),
                },
                pos,
            )
        } else {
            // No valid span specifier found
            (Self::default(), 0)
        }
    }
}

/// A parsed table cell with position and span information.
pub(crate) type ParsedCell = (String, usize, usize, usize, usize); // (content, start, end, colspan, rowspan)

impl Table {
    pub(crate) fn parse_rows_with_positions(
        text: &str,
        separator: &str,
        has_header: &mut bool,
        base_offset: usize,
    ) -> Vec<Vec<ParsedCell>> {
        let mut rows = Vec::new();
        let mut current_offset = base_offset;
        let lines: Vec<&str> = text.lines().collect();
        let mut i = 0;

        tracing::debug!(
            ?has_header,
            total_lines = lines.len(),
            "Starting table parsing"
        );

        while let Some(&line_ref) = lines.get(i) {
            let line = line_ref.trim_end();
            tracing::trace!(i, ?line, is_empty = line.is_empty(), "Processing line");

            // If we are in the first row and it is empty, we should not have a header
            if i == 0 && line.is_empty() {
                *has_header = false;
                current_offset += line.len() + 1;
                i += 1;
                continue;
            }

            // Collect lines for this row (until we hit an empty line or end)
            let mut row_lines = Vec::new();
            let row_start_offset = current_offset;

            // Check if this is a single-line-per-row table (line has multiple separators)
            // vs multi-line-per-row table (one cell per line, rows separated by empty lines)
            // A line is single-line row if it has multiple separators (handles both `| a | b`
            // and `2+| a | b` formats)
            let first_line = line_ref.trim_end();
            let is_single_line_row = first_line.matches(separator).count() > 1;

            if is_single_line_row {
                // Single-line row format: each line is a complete row
                row_lines.push(first_line);
                current_offset += line_ref.len() + 1;
                i += 1;
            } else {
                // Multi-line row format: collect lines until empty line
                while let Some(&current_line) = lines.get(i) {
                    if current_line.trim_end().is_empty() {
                        break;
                    }
                    row_lines.push(current_line.trim_end());
                    current_offset += current_line.len() + 1; // +1 for newline
                    i += 1;
                }
            }

            if !row_lines.is_empty() {
                let columns =
                    Self::parse_row_with_positions(&row_lines, separator, row_start_offset);
                rows.push(columns);
            }

            // After processing the first row, check if the next line is blank (indicates header)
            if rows.len() == 1
                && let Some(&next_line) = lines.get(i)
                && next_line.trim_end().is_empty()
            {
                tracing::debug!("Detected table header via blank line after first row");
                *has_header = true;
            }

            // Skip empty lines
            while let Some(&empty_line) = lines.get(i) {
                if !empty_line.trim_end().is_empty() {
                    break;
                }
                current_offset += empty_line.len() + 1;
                i += 1;
            }
        }

        rows
    }

    fn parse_row_with_positions(
        row_lines: &[&str],
        separator: &str,
        row_start_offset: usize,
    ) -> Vec<ParsedCell> {
        let mut columns = Vec::new();
        let mut current_offset = row_start_offset;

        for line in row_lines {
            // Check if line contains the separator at all
            if !line.contains(separator) {
                current_offset += line.len() + 1; // +1 for newline
                continue;
            }

            // Split the line by separator to get all cells
            let parts: Vec<&str> = line.split(separator).collect();

            // Track position within the line
            let mut line_offset = current_offset;

            // Handle span specifier at the start of line (before first separator)
            // e.g., "2+| content" -> part 0 is "2+", applies to part 1
            let mut pending_spec: Option<CellSpecifier> = None;

            for (i, part) in parts.iter().enumerate() {
                if i == 0 {
                    // First part is before first separator
                    let trimmed = part.trim();
                    if trimmed.is_empty() {
                        // Normal case: line starts with separator
                        line_offset += separator.len();
                    } else {
                        // Span specifier before first separator: "2+| content"
                        let (spec, spec_len) = CellSpecifier::parse(trimmed);
                        if spec_len > 0 {
                            pending_spec = Some(spec);
                        }
                        // Move past the specifier and the separator
                        line_offset += part.len() + separator.len();
                    }
                    continue;
                }

                let cell_content_with_spaces = part;
                let cell_content_trimmed = cell_content_with_spaces.trim();

                // Use pending specifier if we have one, otherwise parse from content
                let (spec, spec_offset) = if let Some(pending) = pending_spec.take() {
                    (pending, 0)
                } else {
                    CellSpecifier::parse(cell_content_trimmed)
                };

                // The actual cell content starts after the specifier
                let cell_content = if spec_offset > 0 {
                    cell_content_trimmed
                        .get(spec_offset..)
                        .unwrap_or("")
                        .trim_start()
                } else {
                    cell_content_trimmed
                };

                // Find where the actual content starts (after leading spaces and specifier)
                let leading_spaces =
                    cell_content_with_spaces.len() - cell_content_with_spaces.trim_start().len();
                let cell_start = line_offset + leading_spaces + spec_offset;
                let cell_end = if cell_content.is_empty() {
                    cell_start
                } else {
                    cell_start + cell_content.len() - 1 // -1 for inclusive end
                };

                columns.push((
                    cell_content.to_string(),
                    cell_start,
                    cell_end,
                    spec.colspan,
                    spec.rowspan,
                ));

                // Move offset past this cell and its separator
                line_offset += part.len();
                if i < parts.len() - 1 {
                    line_offset += separator.len();
                }
            }

            current_offset += line.len() + 1; // +1 for newline
        }

        columns
    }
}
