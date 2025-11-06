use crate::Table;

impl Table {
    pub(crate) fn parse_rows_with_positions(
        text: &str,
        separator: &str,
        has_header: &mut bool,
        base_offset: usize,
    ) -> Vec<Vec<(String, usize, usize)>> {
        let mut rows = Vec::new();
        let mut current_offset = base_offset;
        let lines: Vec<&str> = text.lines().collect();
        let mut i = 0;

        tracing::debug!(
            ?has_header,
            total_lines = lines.len(),
            "Starting table parsing"
        );

        while i < lines.len() {
            let line = lines[i].trim_end();
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
            let first_line = lines[i].trim_end();
            let is_single_line_row =
                first_line.starts_with(separator) && first_line.matches(separator).count() > 1;

            if is_single_line_row {
                // Single-line row format: each line is a complete row
                row_lines.push(first_line);
                current_offset += lines[i].len() + 1;
                i += 1;
            } else {
                // Multi-line row format: collect lines until empty line
                while i < lines.len() && !lines[i].trim_end().is_empty() {
                    row_lines.push(lines[i].trim_end());
                    current_offset += lines[i].len() + 1; // +1 for newline
                    i += 1;
                }
            }

            if !row_lines.is_empty() {
                let columns =
                    Self::parse_row_with_positions(&row_lines, separator, row_start_offset);
                rows.push(columns);
            }

            // After processing the first row, check if the next line is blank (indicates header)
            if rows.len() == 1 && i < lines.len() && lines[i].trim_end().is_empty() {
                tracing::debug!("Detected table header via blank line after first row");
                *has_header = true;
            }

            // Skip empty lines
            while i < lines.len() && lines[i].trim_end().is_empty() {
                current_offset += lines[i].len() + 1;
                i += 1;
            }
        }

        rows
    }

    fn parse_row_with_positions(
        row_lines: &[&str],
        separator: &str,
        row_start_offset: usize,
    ) -> Vec<(String, usize, usize)> {
        let mut columns = Vec::new();
        let mut current_offset = row_start_offset;

        for line in row_lines {
            // Skip lines that don't start with the separator
            if !line.starts_with(separator) {
                current_offset += line.len() + 1; // +1 for newline
                continue;
            }

            // Split the line by separator to get all cells
            let parts: Vec<&str> = line.split(separator).collect();

            // Track position within the line
            let mut line_offset = current_offset;

            // Skip the first empty part (before the first |)
            for (i, part) in parts.iter().enumerate() {
                if i == 0 {
                    // First part is always empty (before first |)
                    line_offset += separator.len();
                    continue;
                }

                let cell_content_with_spaces = part;
                let cell_content = cell_content_with_spaces.trim();

                // Find where the actual content starts (after leading spaces)
                let leading_spaces =
                    cell_content_with_spaces.len() - cell_content_with_spaces.trim_start().len();
                let cell_start = line_offset + leading_spaces;
                let cell_end = if cell_content.is_empty() {
                    cell_start
                } else {
                    cell_start + cell_content.len() - 1 // -1 for inclusive end
                };

                columns.push((cell_content.to_string(), cell_start, cell_end));

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
