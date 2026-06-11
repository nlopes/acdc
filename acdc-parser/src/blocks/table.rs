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

/// Split a single line into cell parts for line-per-row formats (DSV, TSV, or a
/// custom multi-character separator). Single-character separators honor `\`
/// escapes; multi-character separators are matched literally.
fn split_row_line(line: &str, separator: &str) -> Vec<CellPart> {
    match separator.chars().next() {
        Some(sep_char) if separator.len() == 1 => split_escaped(line, sep_char),
        Some(_) => split_multi_char(line, separator),
        None => vec![CellPart {
            content: line.to_string(),
            start: 0,
        }],
    }
}

/// Split by a multi-character separator (no escape handling).
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
    /// Returns the specifier and the offset where actual content begins. A
    /// specifier is only ever recognized as the line-leading token before a
    /// cell delimiter, so style-only specifiers (e.g. `s|` for strong) are
    /// always accepted here.
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

        // Parse optional alignment markers
        let (halign, valign, align_end) = Self::parse_alignments(bytes, pos);
        pos = align_end;

        // Parse optional `colspan` (digits)
        let (colspan, colspan_end) = Self::parse_number(content, bytes, pos);
        pos = colspan_end;

        // Parse optional `rowspan` (dot followed by digits)
        let (rowspan, rowspan_end) = Self::parse_rowspan(content, bytes, pos);
        pos = rowspan_end;

        // Check for operator and build result
        Self::build(bytes, pos, colspan, rowspan, halign, valign)
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

    /// Parse `rowspan` (dot followed by digits) at the current position.
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
    fn build(
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
            // Alignment without span operator - still valid.
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
        } else if let Some(style) = bytes.get(pos).and_then(|&b| parse_style_byte(b)) {
            // Style-only specifier (e.g., `s|` for strong).
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
            // No valid specifier found
            (Self::default(), 0)
        }
    }
}

/// A parsed table cell with position, span, alignment, and style information.
///
/// `start` marks the document offset of the cell as a whole (the byte just
/// past the cell specifier and separator). `content_start` marks the offset
/// of the first byte of `content` in the document — they diverge when the
/// cell's content begins on a continuation line, e.g.:
///
/// ```text
/// a|              <- start points here (end of `a|`)
/// !===            <- content_start points here
/// ```
///
/// Recursive parsers consuming the cell content (in particular the
/// `AsciiDoc`-style `a|` cell) must use `content_start` so that diagnostics
/// resolve to the line of the offending token, not the cell's style prefix.
#[derive(Debug, Clone)]
pub(crate) struct ParsedCell {
    pub content: String,
    pub start: usize,
    pub content_start: usize,
    pub end: usize,
    pub colspan: usize,
    pub rowspan: usize,
    pub halign: Option<HorizontalAlignment>,
    pub valign: Option<VerticalAlignment>,
    pub style: Option<ColumnStyle>,
    pub is_duplication: bool,
    pub duplication_count: usize,
}

/// Check if a blank line after the first row indicates a header.
/// A header is indicated only if the first non-empty line after the blank
/// contains a separator. If it's a continuation line (no separator), it's content
/// that attaches to the previous cell, not a header indicator.
fn detect_header_after_first_row(lines: &[&str], start_idx: usize, separator: &str) -> bool {
    for &line in lines.iter().skip(start_idx) {
        let trimmed = line.trim_end();
        if !trimmed.is_empty() {
            return trimmed.contains(separator);
        }
    }
    false
}

/// A raw cell split from the flat table-body stream, before grouping into rows.
///
/// Cell specifiers attach to the cell that *follows* their delimiter, so a
/// specifier discovered while emitting one cell is carried forward to the next.
struct RawCell {
    spec: CellSpecifier,
    content: String,
    start: usize,
    content_start: usize,
    end: usize,
}

/// Whether cell specifiers (`a|`, `2+|`, `.3+|`, `^|`, …) are recognized for a
/// separator. Only PSV (`|`) and the nested separator (`!`) carry specifiers;
/// DSV (`:`) cells are always plain content.
fn separator_uses_specifiers(separator: &str) -> bool {
    matches!(separator, "|" | "!")
}

/// Split a part's content into `(content_before_spec, trailing_specifier)`.
///
/// A cell specifier is only recognized when it is *line-leading*: the entire
/// trimmed text on the part's last physical line (i.e. after the last `\n`),
/// immediately before the delimiter that ends the part. This matches
/// asciidoctor, where `2+|` at the start of a line is a colspan but `|2+|`
/// mid-line keeps `2+` as literal cell content.
///
/// `allow_at_start` lets the leading part (before the table's first delimiter)
/// be treated as line-leading even without a preceding newline, so `2+|cell`
/// and `a|cell` at the very start of the body are recognized.
fn extract_trailing_spec(content: &str, allow_at_start: bool) -> (&str, Option<CellSpecifier>) {
    let line_start = match content.rfind('\n') {
        Some(n) => n + 1,
        None => {
            if allow_at_start {
                0
            } else {
                return (content, None);
            }
        }
    };
    let tail = content[line_start..].trim();
    if tail.is_empty() {
        return (content, None);
    }
    let (spec, spec_len) = CellSpecifier::parse(tail);
    if spec_len > 0 && spec_len == tail.len() {
        (&content[..line_start], Some(spec))
    } else {
        (content, None)
    }
}

/// Scan the table body into a flat stream of cells.
///
/// The whole body is split on the separator (respecting `\|` escapes), so
/// newlines and blank lines are insignificant for cell boundaries — a cell's
/// content runs until the next unescaped delimiter, spanning as many physical
/// lines as needed. This mirrors asciidoctor's PSV/DSV model and is what makes
/// multi-line cells and rows split across lines parse correctly.
fn scan_cells(text: &str, separator: &str, base_offset: usize) -> Vec<RawCell> {
    let Some(sep_char) = separator.chars().next() else {
        return Vec::new();
    };
    let uses_spec = separator_uses_specifiers(separator);
    let parts = split_escaped(text, sep_char);
    let last_idx = parts.len().saturating_sub(1);

    let mut cells = Vec::new();
    let mut carried_spec: Option<CellSpecifier> = None;

    for (i, part) in parts.iter().enumerate() {
        // A trailing specifier only exists if another delimiter follows this part.
        let (kept, trailing_spec) = if uses_spec && i < last_idx {
            extract_trailing_spec(&part.content, i == 0)
        } else {
            (part.content.as_str(), None)
        };

        // For PSV/`!`, the leading part (before the first delimiter) is not a
        // cell — only its trailing line-leading specifier matters.
        if uses_spec && i == 0 {
            carried_spec = trailing_spec;
            continue;
        }

        let leading_ws = kept.len() - kept.trim_start().len();
        let content = kept.trim().to_string();
        let cell_start = base_offset + part.start;
        let content_start = cell_start + leading_ws;
        let end = if content.is_empty() {
            content_start
        } else {
            content_start + content.len().saturating_sub(1)
        };

        cells.push(RawCell {
            spec: carried_spec.take().unwrap_or_default(),
            content,
            start: cell_start,
            content_start,
            end,
        });
        carried_spec = trailing_spec;
    }

    cells
}

/// Number of cells on the first non-empty physical line of the body.
///
/// asciidoctor derives the column count from the first line when no `cols`
/// attribute is present (e.g. one `|`-led cell per line with no `cols` yields a
/// single-column table).
fn first_line_column_count(text: &str, separator: &str) -> usize {
    let Some(sep_char) = separator.chars().next() else {
        return 0;
    };
    let first_line = text
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("");
    let parts = split_escaped(first_line, sep_char);
    if separator_uses_specifiers(separator) {
        // The leading part before the first delimiter is not a cell.
        parts.len().saturating_sub(1)
    } else {
        parts.len()
    }
}

/// Group a flat cell stream into rows of `ncols` columns.
///
/// Placement is colspan-, duplication-, and rowspan-aware: a cell spanning
/// columns occupies them, a `k*` cell occupies `k` columns (and is expanded
/// downstream), and a `.n+` rowspan reserves its column position in the next
/// `n - 1` rows so those rows need fewer new cells. A trailing partial row that
/// cannot be completed is dropped, matching asciidoctor; its inclusive
/// `(start, end)` document offsets are written to `dropped` so the caller can
/// warn about it.
fn group_cells_into_rows(
    cells: Vec<RawCell>,
    ncols: usize,
    dropped: &mut Option<(usize, usize)>,
) -> Vec<Vec<ParsedCell>> {
    let ncols = ncols.max(1);
    let mut rows = Vec::new();
    // Columns occupied by rowspans carried down from earlier rows.
    let mut active: Vec<(usize, usize, usize)> = Vec::new(); // (col, remaining_rows, width)
    let mut iter = cells.into_iter();
    let mut next = iter.next();

    while next.is_some() {
        let mut row = Vec::new();
        let mut new_spans: Vec<(usize, usize, usize)> = Vec::new();
        let mut col = 0;
        let mut filled = false;

        while col < ncols {
            // Skip columns occupied by an active rowspan from a previous row.
            if let Some(&(_, _, width)) = active
                .iter()
                .find(|(pos, _, w)| col >= *pos && col < pos + w)
            {
                col += width;
                continue;
            }
            let Some(cell) = next.take() else {
                break;
            };
            next = iter.next();

            let occupied = if cell.spec.is_duplication {
                cell.spec.duplication_count.max(1) * cell.spec.colspan.max(1)
            } else {
                cell.spec.colspan.max(1)
            };
            if cell.spec.rowspan > 1 {
                new_spans.push((col, cell.spec.rowspan - 1, occupied));
            }
            row.push(parsed_cell(cell));
            col += occupied;
            filled = col >= ncols;
        }

        if !filled && next.is_none() {
            // Incomplete final row — asciidoctor drops it and warns. Capture
            // the span of the dropped cells for the warning location.
            if let (Some(first), Some(last)) = (row.first(), row.last()) {
                *dropped = Some((first.start, last.end));
            }
            break;
        }
        rows.push(row);
        // Age existing rowspans, then add the ones introduced by this row.
        active.retain_mut(|(_, remaining, _)| {
            *remaining -= 1;
            *remaining > 0
        });
        active.extend(new_spans);
    }

    rows
}

/// Convert a raw cell into a `ParsedCell`, carrying span/alignment/style.
fn parsed_cell(cell: RawCell) -> ParsedCell {
    let spec = cell.spec;
    ParsedCell {
        content: cell.content,
        start: cell.start,
        content_start: cell.content_start,
        end: cell.end,
        colspan: spec.colspan,
        rowspan: spec.rowspan,
        halign: spec.halign,
        valign: spec.valign,
        style: spec.style,
        is_duplication: spec.is_duplication,
        duplication_count: spec.duplication_count,
    }
}

impl Table<'_> {
    pub(crate) fn parse_rows_with_positions(
        text: &str,
        separator: &str,
        has_header: &mut bool,
        base_offset: usize,
        ncols: Option<usize>,
        dropped: &mut Option<(usize, usize)>,
    ) -> Vec<Vec<ParsedCell>> {
        // CSV format needs special handling for multi-line quoted values
        if is_csv_format(separator) {
            return Self::parse_csv_rows_with_positions(text, has_header, base_offset);
        }

        let lines: Vec<&str> = text.lines().collect();

        // Implicit header: the first physical line is immediately followed by a
        // blank line (with a real row after it). A leading blank line means
        // there is no header. We only ever *set* the header here — an explicit
        // `options="header"` from the caller is left untouched.
        if lines.first().is_some_and(|l| l.trim().is_empty()) {
            *has_header = false;
        } else if lines.get(1).is_some_and(|l| l.trim().is_empty())
            && detect_header_after_first_row(&lines, 1, separator)
        {
            tracing::debug!("Detected table header via blank line after first row");
            *has_header = true;
        }

        // PSV (`|`) and the nested separator (`!`) follow a flat cell-stream
        // model: newlines are insignificant and cells flow into rows by column
        // count. DSV (`:`), TSV (`\t`), and any custom separator are line-per-row.
        if !separator_uses_specifiers(separator) {
            return Self::parse_delimited_rows(&lines, separator, base_offset);
        }

        // Column count: the `cols` attribute when given, otherwise the number of
        // cells on the first physical line (asciidoctor's implicit rule).
        let ncols = ncols
            .filter(|&n| n > 0)
            .unwrap_or_else(|| first_line_column_count(text, separator));

        tracing::debug!(?has_header, ncols, "Starting table parsing");

        let cells = scan_cells(text, separator, base_offset);
        group_cells_into_rows(cells, ncols, dropped)
    }

    /// Parse a line-per-row table body (DSV, TSV, or a custom separator).
    ///
    /// Each non-empty line is one row; the separator splits cells within the
    /// line. These formats do not use cell specifiers.
    fn parse_delimited_rows(
        lines: &[&str],
        separator: &str,
        base_offset: usize,
    ) -> Vec<Vec<ParsedCell>> {
        let mut rows = Vec::new();
        let mut offset = base_offset;
        for line in lines {
            if !line.trim().is_empty() {
                let cells: Vec<ParsedCell> = split_row_line(line, separator)
                    .into_iter()
                    .map(|part| {
                        let leading_ws = part.content.len() - part.content.trim_start().len();
                        let content = part.content.trim().to_string();
                        let start = offset + part.start + leading_ws;
                        let end = if content.is_empty() {
                            start
                        } else {
                            start + content.len().saturating_sub(1)
                        };
                        ParsedCell {
                            content,
                            start,
                            content_start: start,
                            end,
                            colspan: 1,
                            rowspan: 1,
                            halign: None,
                            valign: None,
                            style: None,
                            is_duplication: false,
                            duplication_count: 1,
                        }
                    })
                    .collect();
                if !cells.is_empty() {
                    rows.push(cells);
                }
            }
            offset += line.len() + 1; // +1 for newline
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
                    content_start: start,
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

    /// Trailing content after a completed row — no leading separator — is
    /// attached to the last cell as a continuation paragraph. The blank
    /// line between the row and the trailing text must survive into the
    /// cell's string content so downstream rendering produces a second
    /// paragraph (matching asciidoctor's `<p class="tableblock">...</p>`
    /// pair).
    #[test]
    fn trailing_text_becomes_continuation_paragraph_of_last_cell() {
        let input = "| A | B\n\nTrailing\n";
        let mut has_header = false;
        let rows =
            Table::parse_rows_with_positions(input, "|", &mut has_header, 0, None, &mut None);
        let [row] = rows.as_slice() else {
            panic!("expected 1 row, got {}", rows.len());
        };
        let [a, b] = row.as_slice() else {
            panic!("expected 2 cells, got {}", row.len());
        };
        assert_eq!(a.content, "A");
        // The blank line boundary must be preserved so the cell content,
        // when later parsed as blocks, yields a second paragraph rather
        // than a single joined line.
        assert_eq!(b.content, "B\n\nTrailing");
    }

    /// The outer parse must not collapse a blank line that lives *inside*
    /// an `a`-cell's content. If it did, a nested table's own trailing
    /// continuation paragraph would disappear when the cell is re-parsed
    /// as `AsciiDoc` blocks.
    #[test]
    fn a_cell_preserves_blank_line_inside_nested_table_content() {
        let input = "a|\n!===\n! Inner A ! Inner B\n\nTrailing in inner cell\n!===\n";
        let mut has_header = false;
        let rows =
            Table::parse_rows_with_positions(input, "|", &mut has_header, 0, Some(1), &mut None);
        let [row] = rows.as_slice() else {
            panic!("expected 1 row, got {}", rows.len());
        };
        let [cell] = row.as_slice() else {
            panic!("expected 1 cell, got {}", row.len());
        };
        assert_eq!(
            cell.content,
            "!===\n! Inner A ! Inner B\n\nTrailing in inner cell\n!===",
        );
    }

    /// A cell whose content continues on the next line (no separator on that
    /// line) belongs to the same cell — newlines do not delimit cells.
    #[test]
    fn multiline_cell_content_stays_in_one_cell() {
        let input = "| a | b\nstill b\n| c | d\n";
        let mut has_header = false;
        let rows =
            Table::parse_rows_with_positions(input, "|", &mut has_header, 0, Some(2), &mut None);
        let contents: Vec<Vec<&str>> = rows
            .iter()
            .map(|r| r.iter().map(|c| c.content.as_str()).collect())
            .collect();
        assert_eq!(contents, vec![vec!["a", "b\nstill b"], vec!["c", "d"]]);
    }

    /// A logical row split so later cells start with a delimiter on their own
    /// line still groups into one row by column count.
    #[test]
    fn row_split_across_lines_groups_by_column_count() {
        let input = "| a\n| b\n| c\n| d\n";
        let mut has_header = false;
        let rows =
            Table::parse_rows_with_positions(input, "|", &mut has_header, 0, Some(2), &mut None);
        let contents: Vec<Vec<&str>> = rows
            .iter()
            .map(|r| r.iter().map(|c| c.content.as_str()).collect())
            .collect();
        assert_eq!(contents, vec![vec!["a", "b"], vec!["c", "d"]]);
    }

    /// Rowspans reserve their column in following rows, so those rows need
    /// fewer new cells — the flat stream must group accordingly.
    #[test]
    fn rowspan_aware_grouping() {
        let input = ".2+| spans | b | c\n| e | f\n| g | h | i\n";
        let mut has_header = false;
        let rows =
            Table::parse_rows_with_positions(input, "|", &mut has_header, 0, Some(3), &mut None);
        let contents: Vec<Vec<&str>> = rows
            .iter()
            .map(|r| r.iter().map(|c| c.content.as_str()).collect())
            .collect();
        assert_eq!(
            contents,
            vec![vec!["spans", "b", "c"], vec!["e", "f"], vec!["g", "h", "i"]]
        );
        assert_eq!(rows[0][0].rowspan, 2);
    }
}
