use crate::{ColumnStyle, HorizontalAlignment, Table, VerticalAlignment};

/// Represents a parsed cell specifier with span, alignment, and style information.
///
/// In `AsciiDoc`, cell specifiers appear before the cell separator with format:
/// `[halign][valign][colspan][.rowspan][op][style]|`
///
/// Examples:
/// - `2+|content` → colspan=2
/// - `.3+|content` → rowspan=3
/// - `2.3+|content` → colspan=2, rowspan=3
/// - `^.>2+s|content` → center, bottom, colspan=2, strong style
/// - `3*|content` → duplicate cell 3 times
#[derive(Debug, Clone, Copy)]
pub(crate) struct CellSpecifier {
    pub colspan: usize,
    pub rowspan: usize,
    pub halign: Option<HorizontalAlignment>,
    pub valign: Option<VerticalAlignment>,
    pub style: Option<ColumnStyle>,
    /// If true, this is a duplication specifier (`*`) rather than a span (`+`).
    pub is_duplication: bool,
    /// For duplication, this is the count (e.g., `3*` means 3 copies).
    pub duplication_count: usize,
}

impl Default for CellSpecifier {
    fn default() -> Self {
        Self {
            colspan: 1,
            rowspan: 1,
            halign: None,
            valign: None,
            style: None,
            is_duplication: false,
            duplication_count: 1,
        }
    }
}

/// Parse a single style letter into a `ColumnStyle`.
fn parse_style_byte(byte: u8) -> Option<ColumnStyle> {
    match byte {
        b'a' => Some(ColumnStyle::AsciiDoc),
        b'd' => Some(ColumnStyle::Default),
        b'e' => Some(ColumnStyle::Emphasis),
        b'h' => Some(ColumnStyle::Header),
        b'l' => Some(ColumnStyle::Literal),
        b'm' => Some(ColumnStyle::Monospace),
        b's' => Some(ColumnStyle::Strong),
        _ => None,
    }
}

impl CellSpecifier {
    /// Parse a cell specifier from the beginning of cell content.
    ///
    /// Returns the specifier and the offset where actual content begins.
    /// Full pattern: `[halign][valign][colspan][.rowspan][+|*][style]`
    ///
    /// Examples:
    /// - `"2+rest"` → colspan=2
    /// - `".3+rest"` → rowspan=3
    /// - `"2.3+rest"` → colspan=2, rowspan=3
    /// - `"^.>2+srest"` → center, bottom, colspan=2, strong style
    /// - `"3*rest"` → `duplication_count`=3
    /// - `"plain"` → defaults (no specifier found)
    #[must_use]
    pub fn parse(content: &str) -> (Self, usize) {
        let bytes = content.as_bytes();
        let mut pos = 0;

        // Phase 1: Parse optional alignment markers
        let (halign, valign, align_end) = Self::parse_alignments(bytes, pos);
        pos = align_end;

        // Phase 2: Parse optional colspan (digits)
        let (colspan, colspan_end) = Self::parse_number(content, bytes, pos);
        pos = colspan_end;

        // Phase 3: Parse optional rowspan (dot followed by digits)
        let (rowspan, rowspan_end) = Self::parse_rowspan(content, bytes, pos);
        pos = rowspan_end;

        // Phase 4: Check for operator and build result
        Self::build_result(bytes, pos, colspan, rowspan, halign, valign)
    }

    /// Parse alignment markers at the current position.
    /// Returns `(halign, valign, new_position)`.
    fn parse_alignments(
        bytes: &[u8],
        mut pos: usize,
    ) -> (
        Option<HorizontalAlignment>,
        Option<VerticalAlignment>,
        usize,
    ) {
        let mut halign: Option<HorizontalAlignment> = None;
        let mut valign: Option<VerticalAlignment> = None;

        loop {
            match bytes.get(pos) {
                Some(b'<') => {
                    halign = Some(HorizontalAlignment::Left);
                    pos += 1;
                }
                Some(b'^') => {
                    halign = Some(HorizontalAlignment::Center);
                    pos += 1;
                }
                Some(b'>') => {
                    halign = Some(HorizontalAlignment::Right);
                    pos += 1;
                }
                Some(b'.') => {
                    // Could be vertical alignment (.< .^ .>) or rowspan (.N)
                    match bytes.get(pos + 1) {
                        Some(b'<') => {
                            valign = Some(VerticalAlignment::Top);
                            pos += 2;
                        }
                        Some(b'^') => {
                            valign = Some(VerticalAlignment::Middle);
                            pos += 2;
                        }
                        Some(b'>') => {
                            valign = Some(VerticalAlignment::Bottom);
                            pos += 2;
                        }
                        _ => break, // Not vertical alignment, might be rowspan
                    }
                }
                _ => break,
            }
        }

        (halign, valign, pos)
    }

    /// Parse a number (for colspan) at the current position.
    /// Returns `(parsed_value, new_position)`.
    fn parse_number(content: &str, bytes: &[u8], mut pos: usize) -> (Option<usize>, usize) {
        let start = pos;
        while bytes.get(pos).is_some_and(u8::is_ascii_digit) {
            pos += 1;
        }
        let value = if pos > start {
            content
                .get(start..pos)
                .and_then(|s| s.parse::<usize>().ok())
        } else {
            None
        };
        (value, pos)
    }

    /// Parse rowspan (dot followed by digits) at the current position.
    /// Returns `(parsed_value, new_position)`.
    fn parse_rowspan(content: &str, bytes: &[u8], mut pos: usize) -> (Option<usize>, usize) {
        if bytes.get(pos) != Some(&b'.') {
            return (None, pos);
        }

        let dot_pos = pos;
        pos += 1;
        let start = pos;
        while bytes.get(pos).is_some_and(u8::is_ascii_digit) {
            pos += 1;
        }

        if pos > start {
            let value = content
                .get(start..pos)
                .and_then(|s| s.parse::<usize>().ok());
            (value, pos)
        } else {
            // Dot without following digits - not a rowspan specifier
            (None, dot_pos)
        }
    }

    /// Build the final result based on parsed components.
    fn build_result(
        bytes: &[u8],
        mut pos: usize,
        colspan: Option<usize>,
        rowspan: Option<usize>,
        halign: Option<HorizontalAlignment>,
        valign: Option<VerticalAlignment>,
    ) -> (Self, usize) {
        let has_span_or_dup = colspan.is_some() || rowspan.is_some();
        let is_duplication = bytes.get(pos) == Some(&b'*');
        let is_span = bytes.get(pos) == Some(&b'+');

        if (is_span || is_duplication) && has_span_or_dup {
            pos += 1;

            // Parse optional style letter after operator
            let style = bytes.get(pos).and_then(|&b| parse_style_byte(b));
            if style.is_some() {
                pos += 1;
            }

            let spec = if is_duplication {
                Self {
                    colspan: 1,
                    rowspan: 1,
                    halign,
                    valign,
                    style,
                    is_duplication: true,
                    duplication_count: colspan.unwrap_or(1),
                }
            } else {
                Self {
                    colspan: colspan.unwrap_or(1),
                    rowspan: rowspan.unwrap_or(1),
                    halign,
                    valign,
                    style,
                    is_duplication: false,
                    duplication_count: 1,
                }
            };
            (spec, pos)
        } else if halign.is_some() || valign.is_some() {
            // Alignment without span operator - still valid
            let style = bytes.get(pos).and_then(|&b| parse_style_byte(b));
            if style.is_some() {
                pos += 1;
            }
            (
                Self {
                    colspan: 1,
                    rowspan: 1,
                    halign,
                    valign,
                    style,
                    is_duplication: false,
                    duplication_count: 1,
                },
                pos,
            )
        } else {
            // No valid specifier found
            (Self::default(), 0)
        }
    }
}

/// A parsed table cell with position, span, alignment, and style information.
#[derive(Debug, Clone)]
pub(crate) struct ParsedCell {
    pub content: String,
    pub start: usize,
    pub end: usize,
    pub colspan: usize,
    pub rowspan: usize,
    pub halign: Option<HorizontalAlignment>,
    pub valign: Option<VerticalAlignment>,
    pub style: Option<ColumnStyle>,
    pub is_duplication: bool,
    pub duplication_count: usize,
}

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

                columns.push(ParsedCell {
                    content: cell_content.to_string(),
                    start: cell_start,
                    end: cell_end,
                    colspan: spec.colspan,
                    rowspan: spec.rowspan,
                    halign: spec.halign,
                    valign: spec.valign,
                    style: spec.style,
                    is_duplication: spec.is_duplication,
                    duplication_count: spec.duplication_count,
                });

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
