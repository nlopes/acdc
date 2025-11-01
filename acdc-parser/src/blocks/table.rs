use crate::{Location, Table};

impl Table {
    pub(crate) fn parse_rows(
        text: &str,
        separator: &str,
        has_header: &mut bool,
    ) -> Vec<Vec<String>> {
        let mut location = Location::default();

        let mut rows = Vec::new();
        let mut row_string = String::new();
        for (i, row) in text.lines().enumerate() {
            let row = row.trim();
            // If we are in the first row and it is empty, we should not have a header,
            // set it to false and move on.
            if i == 0 && row.is_empty() {
                *has_header = false;
                continue;
            }

            // If we're in the first row and it is empty, and we've already added
            // something to the rows, then we should have a header
            if i == 1 && row.is_empty() {
                *has_header = true;
            }

            if row.is_empty() && !row_string.is_empty() {
                let columns = row_string
                    .split(separator)
                    .map(str::trim)
                    .map(str::to_string)
                    .collect();
                row_string.clear();
                rows.push(columns);
            }

            // Adjust the location
            if row_string.is_empty() {
                location.start.line = i + 1;
                location.start.column = 1;
            }
            location.end.line = i + 1;
            location.end.column = row.len() + 1;

            // Add the row to the row string
            row_string.push_str(row);
        }
        if !row_string.is_empty() {
            let columns = row_string
                .split(separator)
                .map(str::trim)
                .map(str::to_string)
                .collect();
            rows.push(columns);
        }
        rows
    }

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

        while i < lines.len() {
            let line = lines[i].trim_end();

            // If we are in the first row and it is empty, we should not have a header
            if i == 0 && line.is_empty() {
                *has_header = false;
                current_offset += line.len() + 1;
                i += 1;
                continue;
            }

            // If we're at index 1 and it's empty, then we have a header
            if i == 1 && line.is_empty() {
                *has_header = true;
            }

            // Collect lines for this row (until we hit an empty line or end)
            let mut row_lines = Vec::new();
            let row_start_offset = current_offset;

            while i < lines.len() && !lines[i].trim_end().is_empty() {
                row_lines.push(lines[i].trim_end());
                current_offset += lines[i].len() + 1; // +1 for newline
                i += 1;
            }

            if !row_lines.is_empty() {
                let columns =
                    Self::parse_row_with_positions(&row_lines, separator, row_start_offset);
                rows.push(columns);
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
