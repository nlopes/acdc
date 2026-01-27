use crate::{ColumnStyle, HorizontalAlignment, Table, VerticalAlignment};

/// A cell part with its unescaped content and original start position.
struct CellPart {
    /// Unescaped content (e.g., `\|` becomes `|`)
    content: String,
    /// Start position in the original line
    start: usize,
}

/// Split a line by separator, respecting backslash escapes.
///
/// For PSV (`|`) and DSV (`:`), a backslash before the separator escapes it.
/// Returns parts with their original byte positions for accurate source mapping.
fn split_escaped(line: &str, separator: char) -> Vec<CellPart> {
    let mut parts = Vec::new();
    let mut current_content = String::new();
    let mut part_start = 0;
    let mut chars = line.char_indices().peekable();

    while let Some((byte_idx, ch)) = chars.next() {
        if ch == '\\' {
            // Check if next char is the separator
            if let Some(&(_, next_ch)) = chars.peek() {
                if next_ch == separator {
                    // Escaped separator - add literal separator, skip the backslash
                    current_content.push(separator);
                    chars.next(); // consume the separator
                    continue;
                }
            }
            // Not an escape - add backslash literally
            current_content.push(ch);
        } else if ch == separator {
            // Unescaped separator - end current part
            parts.push(CellPart {
                content: std::mem::take(&mut current_content),
                start: part_start,
            });
            part_start = byte_idx + ch.len_utf8();
        } else {
            current_content.push(ch);
        }
    }

    // Add final part
    parts.push(CellPart {
        content: current_content,
        start: part_start,
    });

    parts
}

/// Parse a CSV table body using the `csv` crate for full RFC 4180 compliance.
///
/// This handles multi-line quoted values, escaped quotes, and all CSV edge cases.
/// Returns rows with cells containing their content and accurate byte positions.
fn parse_csv_table(text: &str, base_offset: usize) -> Vec<Vec<CellPart>> {
    let text_bytes = text.as_bytes();
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true) // allow variable column counts
        .from_reader(text_bytes);

    let mut rows = Vec::new();

    for result in reader.records() {
        let Ok(record) = result else {
            continue;
        };

        // Get the byte position where this record starts in the input
        let record_start = record
            .position()
            .map_or(0, |p| usize::try_from(p.byte()).unwrap_or(0));

        let mut cells = Vec::new();
        let mut scan_pos = record_start;

        for field in &record {
            // Find actual field position by scanning the original text
            let (field_content_start, next_pos) =
                find_csv_field_position(text_bytes, scan_pos, field);

            cells.push(CellPart {
                content: field.to_string(),
                start: base_offset + field_content_start,
            });

            scan_pos = next_pos;
        }

        rows.push(cells);
    }

    rows
}

/// Find the actual byte position of a CSV field's content in the original text.
///
/// Returns `(content_start, next_scan_position)` where:
/// - `content_start` is where the field's actual content begins (after opening quote if quoted)
/// - `next_scan_position` is where to start scanning for the next field
fn find_csv_field_position(text: &[u8], start: usize, expected_content: &str) -> (usize, usize) {
    let Some(&first_byte) = text.get(start) else {
        return (start, start);
    };

    if first_byte == b'"' {
        // Quoted field: content starts after the opening quote
        let content_start = start + 1;
        // Find the closing quote (handle escaped quotes "")
        let end_pos = find_closing_quote(text, start + 1);
        // Next field starts after closing quote and comma (or newline)
        let next_pos = skip_to_next_field(text, end_pos);
        (content_start, next_pos)
    } else {
        // Unquoted field: content starts at current position
        let content_start = start;
        // Find end of field (comma or newline)
        let end_pos = find_unquoted_field_end(text, start, expected_content.len());
        // Next field starts after the separator
        let next_pos = skip_to_next_field(text, end_pos);
        (content_start, next_pos)
    }
}

/// Find the closing quote of a quoted CSV field, handling escaped quotes (`""`).
fn find_closing_quote(text: &[u8], start: usize) -> usize {
    let mut pos = start;
    while let Some(&byte) = text.get(pos) {
        if byte == b'"' {
            // Check if this is an escaped quote ("")
            if text.get(pos + 1) == Some(&b'"') {
                // Escaped quote - skip both and continue
                pos += 2;
            } else {
                // Closing quote found
                return pos;
            }
        } else {
            pos += 1;
        }
    }
    // No closing quote found - return end of text
    text.len()
}

/// Find the end of an unquoted CSV field.
fn find_unquoted_field_end(text: &[u8], start: usize, content_len: usize) -> usize {
    // The field ends at comma, CR, LF, or content_len bytes (whichever comes first)
    let mut pos = start;
    let mut remaining = content_len;
    while let Some(&byte) = text.get(pos) {
        if byte == b',' || byte == b'\n' || byte == b'\r' {
            return pos;
        }
        if remaining == 0 {
            return pos;
        }
        remaining = remaining.saturating_sub(1);
        pos += 1;
    }
    text.len()
}

/// Skip past the current field separator to find the start of the next field.
fn skip_to_next_field(text: &[u8], pos: usize) -> usize {
    let mut pos = pos;
    // Skip closing quote if present
    if text.get(pos) == Some(&b'"') {
        pos += 1;
    }
    // Skip comma or newline characters
    while let Some(&byte) = text.get(pos) {
        if byte == b',' {
            return pos + 1;
        }
        if byte == b'\r' || byte == b'\n' {
            // Skip CRLF or just LF
            if byte == b'\r' && text.get(pos + 1) == Some(&b'\n') {
                return pos + 2;
            }
            return pos + 1;
        }
        pos += 1;
    }
    pos
}

/// Determine if this is a CSV format table.
fn is_csv_format(separator: &str) -> bool {
    separator == ","
}

/// Split a line into cell parts using the appropriate method for the separator.
///
/// Note: CSV format is handled separately via `parse_csv_table()` for multi-line support.
fn split_line(line: &str, separator: &str) -> Vec<CellPart> {
    if let Some(sep_char) = separator.chars().next() {
        if separator.len() == 1 {
            split_escaped(line, sep_char)
        } else {
            // Multi-char separator - no escape handling
            split_multi_char(line, separator)
        }
    } else {
        // Empty separator - return whole line as one part
        vec![CellPart {
            content: line.to_string(),
            start: 0,
        }]
    }
}

/// Split by multi-character separator (no escape handling).
fn split_multi_char(line: &str, separator: &str) -> Vec<CellPart> {
    let mut parts = Vec::new();
    let mut last_end = 0;
    for (idx, _) in line.match_indices(separator) {
        parts.push(CellPart {
            content: line.get(last_end..idx).unwrap_or("").to_string(),
            start: last_end,
        });
        last_end = idx + separator.len();
    }
    parts.push(CellPart {
        content: line.get(last_end..).unwrap_or("").to_string(),
        start: last_end,
    });
    parts
}

/// Context for parsing cell specifiers, controlling which specifier types are valid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParseContext {
    /// First part before separator in PSV tables - style-only specifiers allowed (e.g., `s|`)
    FirstPart,
    /// Inline cell content - style-only specifiers NOT allowed (prevents "another" → 'a' style)
    InlineContent,
}

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
    /// The `mode` parameter controls whether style-only specifiers
    /// (e.g., `s|` for strong without any alignment or span) are accepted:
    /// - `ParseContext::FirstPart`: Accept style-only specifiers (first part before separator)
    /// - `ParseContext::InlineContent`: Reject style-only (prevents "another" → 'a' style)
    ///
    /// Examples:
    /// - `"2+rest"` → colspan=2
    /// - `".3+rest"` → rowspan=3
    /// - `"2.3+rest"` → colspan=2, rowspan=3
    /// - `"^.>2+srest"` → center, bottom, colspan=2, strong style
    /// - `"3*rest"` → `duplication_count`=3
    /// - `"plain"` → defaults (no specifier found)
    #[must_use]
    pub fn parse(content: &str, mode: ParseContext) -> (Self, usize) {
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
        Self::build_result(bytes, pos, colspan, rowspan, halign, valign, mode)
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
        context: ParseContext,
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
        } else if (halign.is_some() || valign.is_some()) && context == ParseContext::FirstPart {
            // Alignment without span operator - still valid (only in FirstPart context)
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
        } else if context == ParseContext::FirstPart {
            // Check for style-only specifier (e.g., `s|` for strong)
            // Only accepted in FirstPart context (first-part in PSV tables)
            let style = bytes.get(pos).and_then(|&b| parse_style_byte(b));
            if let Some(style) = style {
                pos += 1;
                (
                    Self {
                        colspan: 1,
                        rowspan: 1,
                        halign: None,
                        valign: None,
                        style: Some(style),
                        is_duplication: false,
                        duplication_count: 1,
                    },
                    pos,
                )
            } else {
                (Self::default(), 0)
            }
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
        // CSV format needs special handling for multi-line quoted values
        if is_csv_format(separator) {
            return Self::parse_csv_rows_with_positions(text, has_header, base_offset);
        }

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

    /// Parse CSV table rows using the `csv` crate for RFC 4180 compliance.
    ///
    /// This handles multi-line quoted values correctly by processing the entire
    /// table body at once rather than line-by-line.
    fn parse_csv_rows_with_positions(
        text: &str,
        has_header: &mut bool,
        base_offset: usize,
    ) -> Vec<Vec<ParsedCell>> {
        // Check for header indicator: first row followed by blank line
        // For CSV, we need to detect this before parsing since the csv crate
        // consumes the text as a stream.
        let lines: Vec<&str> = text.lines().collect();
        if lines.len() >= 2 {
            // Find where first CSV record ends - look for first complete record
            // A simple heuristic: if line 1 (0-indexed) is empty, we have a header
            if let Some(&line) = lines.get(1) {
                if line.trim().is_empty() {
                    *has_header = true;
                }
            }
        }

        let csv_rows = parse_csv_table(text, base_offset);
        let mut rows = Vec::new();

        for csv_row in csv_rows {
            let mut cells = Vec::new();
            for part in csv_row {
                let content = part.content.trim();
                let start = part.start;
                let end = if content.is_empty() {
                    start
                } else {
                    start + content.len().saturating_sub(1)
                };

                cells.push(ParsedCell {
                    content: content.to_string(),
                    start,
                    end,
                    colspan: 1,
                    rowspan: 1,
                    halign: None,
                    valign: None,
                    style: None,
                    is_duplication: false,
                    duplication_count: 1,
                });
            }
            if !cells.is_empty() {
                rows.push(cells);
            }
        }

        rows
    }

    fn parse_row_with_positions(
        row_lines: &[&str],
        separator: &str,
        row_start_offset: usize,
    ) -> Vec<ParsedCell> {
        let mut columns: Vec<ParsedCell> = Vec::new();
        let mut current_offset = row_start_offset;

        for line in row_lines {
            // Check if line contains the separator at all
            if !line.contains(separator) {
                // Continuation line: append to last cell's content
                if let Some(last_cell) = columns.last_mut() {
                    if !last_cell.content.is_empty() {
                        last_cell.content.push('\n');
                    }
                    last_cell.content.push_str(line);
                    // Update end position to include this line
                    last_cell.end = current_offset + line.len().saturating_sub(1);
                }
                current_offset += line.len() + 1; // +1 for newline
                continue;
            }

            // Split the line by separator, handling escapes appropriately
            let parts = split_line(line, separator);

            // Handle span specifier at the start of line (before first separator)
            // e.g., "2+| content" -> part 0 is "2+", applies to part 1
            let mut pending_spec: Option<CellSpecifier> = None;

            // Determine if first part should be treated as content or specifier/skip
            // For PSV (|): first part is before the leading separator, skip it or treat as specifier
            // For CSV (,) and DSV (:): first part is actual cell content

            for (i, part) in parts.iter().enumerate() {
                if i == 0 && separator == "|" {
                    // First part is before first separator (PSV format only)
                    let trimmed = part.content.trim();
                    if !trimmed.is_empty() {
                        // Check if this looks like a specifier (e.g., "2+", "3*", "^.>", "s")
                        // Style-only specifiers (e.g., "s" for strong) are valid here
                        let (spec, spec_len) =
                            CellSpecifier::parse(trimmed, ParseContext::FirstPart);
                        if spec_len > 0 && spec_len == trimmed.len() {
                            // Entire first part is a specifier, apply to next cell
                            pending_spec = Some(spec);
                        }
                        // If not a complete specifier, it's just content before first separator
                        // which we skip for PSV
                    }
                    continue;
                }

                let cell_content_trimmed = part.content.trim();

                // Use pending specifier if we have one, otherwise parse from content.
                // Style-only specifiers are NOT valid from inline content parsing -
                // this prevents treating content like "another" as having an 'a' (AsciiDoc) style.
                let (spec, spec_offset) = if let Some(pending) = pending_spec.take() {
                    (pending, 0)
                } else {
                    CellSpecifier::parse(cell_content_trimmed, ParseContext::InlineContent)
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

                // Calculate where cell_content starts within part.content
                // Pattern: leading_ws + spec_offset + post_spec_ws
                let leading_ws = part.content.len() - part.content.trim_start().len();
                let post_spec_ws = if spec_offset > 0 {
                    let after_spec = cell_content_trimmed.get(spec_offset..).unwrap_or("");
                    after_spec.len() - after_spec.trim_start().len()
                } else {
                    0
                };
                let content_start_offset = leading_ws + spec_offset + post_spec_ws;

                // Calculate positions using actual content boundaries
                let cell_start = current_offset + part.start + content_start_offset;
                let cell_end = if cell_content.is_empty() {
                    cell_start
                } else {
                    // End is start + content length - 1 (inclusive end position)
                    cell_start + cell_content.len().saturating_sub(1)
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
            }

            current_offset += line.len() + 1; // +1 for newline
        }

        columns
    }
}

#[cfg(test)]
#[allow(clippy::panic, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn split_escaped_psv_no_escapes() {
        let parts = split_escaped("| cell1 | cell2 |", '|');
        let [p0, p1, p2, p3] = parts.as_slice() else {
            panic!("expected 4 parts, got {}", parts.len());
        };
        assert_eq!(p0.content, "");
        assert_eq!(p1.content, " cell1 ");
        assert_eq!(p2.content, " cell2 ");
        assert_eq!(p3.content, "");
    }

    #[test]
    fn split_escaped_psv_with_escape() {
        let parts = split_escaped(r"| cell with \| pipe | normal |", '|');
        let [p0, p1, p2, p3] = parts.as_slice() else {
            panic!("expected 4 parts, got {}", parts.len());
        };
        assert_eq!(p0.content, "");
        assert_eq!(p1.content, " cell with | pipe ");
        assert_eq!(p2.content, " normal ");
        assert_eq!(p3.content, "");
    }

    #[test]
    fn split_escaped_dsv_no_escapes() {
        let parts = split_escaped("cell1:cell2:cell3", ':');
        let [p0, p1, p2] = parts.as_slice() else {
            panic!("expected 3 parts, got {}", parts.len());
        };
        assert_eq!(p0.content, "cell1");
        assert_eq!(p1.content, "cell2");
        assert_eq!(p2.content, "cell3");
    }

    #[test]
    fn split_escaped_dsv_with_escape() {
        let parts = split_escaped(r"cell with \: colon:normal", ':');
        let [p0, p1] = parts.as_slice() else {
            panic!("expected 2 parts, got {}", parts.len());
        };
        assert_eq!(p0.content, "cell with : colon");
        assert_eq!(p1.content, "normal");
    }

    #[test]
    fn split_escaped_backslash_not_before_separator() {
        // Backslash before non-separator should be preserved
        let parts = split_escaped(r"cell\n with backslash|next", '|');
        let [p0, p1] = parts.as_slice() else {
            panic!("expected 2 parts, got {}", parts.len());
        };
        assert_eq!(p0.content, r"cell\n with backslash");
        assert_eq!(p1.content, "next");
    }

    #[test]
    fn split_escaped_multiple_escapes() {
        let parts = split_escaped(r"\|start\|middle\|end", '|');
        let [p0] = parts.as_slice() else {
            panic!("expected 1 part, got {}", parts.len());
        };
        assert_eq!(p0.content, "|start|middle|end");
    }

    #[test]
    fn split_escaped_positions_tracked() {
        let parts = split_escaped("ab|cd|ef", '|');
        let [p0, p1, p2] = parts.as_slice() else {
            panic!("expected 3 parts, got {}", parts.len());
        };
        assert_eq!(p0.start, 0);
        assert_eq!(p1.start, 3); // after "ab|"
        assert_eq!(p2.start, 6); // after "ab|cd|"
    }
}
