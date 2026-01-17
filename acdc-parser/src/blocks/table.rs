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

/// Split a CSV line, respecting quoted fields (RFC 4180).
///
/// - Fields enclosed in double quotes can contain commas
/// - Double-double-quotes (`""`) inside quoted fields become a single quote
fn split_csv(line: &str) -> Vec<CellPart> {
    let mut parts = Vec::new();
    let mut current_content = String::new();
    let mut part_start = 0;
    let mut in_quotes = false;
    let mut chars = line.char_indices().peekable();

    while let Some((byte_idx, ch)) = chars.next() {
        if in_quotes {
            if ch == '"' {
                // Check for escaped quote ("")
                if let Some(&(_, next_ch)) = chars.peek() {
                    if next_ch == '"' {
                        // Escaped quote - add one quote, skip both
                        current_content.push('"');
                        chars.next(); // consume the second quote
                        continue;
                    }
                }
                // End of quoted field
                in_quotes = false;
            } else {
                current_content.push(ch);
            }
        } else if ch == '"' {
            // Start of quoted field
            in_quotes = true;
        } else if ch == ',' {
            // Field separator
            parts.push(CellPart {
                content: std::mem::take(&mut current_content),
                start: part_start,
            });
            part_start = byte_idx + 1; // comma is always 1 byte
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

/// Determine if this is a CSV format table.
fn is_csv_format(separator: &str) -> bool {
    separator == ","
}

/// Split a line into cell parts using the appropriate method for the separator.
fn split_line(line: &str, separator: &str) -> Vec<CellPart> {
    if is_csv_format(separator) {
        split_csv(line)
    } else if let Some(sep_char) = separator.chars().next() {
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

            // Split the line by separator, handling escapes appropriately
            let parts = split_line(line, separator);

            // Handle span specifier at the start of line (before first separator)
            // e.g., "2+| content" -> part 0 is "2+", applies to part 1
            let mut pending_spec: Option<CellSpecifier> = None;

            // Determine if first part should be treated as content (CSV) or specifier/skip (PSV/DSV)
            // For CSV: first part is actual content
            // For PSV/DSV: first part is either empty, whitespace, or a cell specifier
            let is_csv = is_csv_format(separator);

            for (i, part) in parts.iter().enumerate() {
                if i == 0 && !is_csv {
                    // First part is before first separator (PSV/DSV format)
                    let trimmed = part.content.trim();
                    if !trimmed.is_empty() {
                        // Check if this looks like a specifier (e.g., "2+", "3*", "^.>")
                        let (spec, spec_len) = CellSpecifier::parse(trimmed);
                        if spec_len > 0 && spec_len == trimmed.len() {
                            // Entire first part is a specifier, apply to next cell
                            pending_spec = Some(spec);
                        }
                        // If not a complete specifier, it's just content before first separator
                        // which we skip for PSV/DSV
                    }
                    continue;
                }

                let cell_content_trimmed = part.content.trim();

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
