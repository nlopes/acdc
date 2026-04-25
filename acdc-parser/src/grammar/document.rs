// The `peg` macro adds 5 hidden parameters to every rule function, so even
// rules with just 3 explicit params exceed clippy's 7-argument threshold.
#![allow(clippy::too_many_arguments)]
use crate::{
    Admonition, AdmonitionVariant, Anchor, AttributeValue, Attribution, Audio, Author, Block,
    BlockMetadata, CalloutList, CalloutListItem, CalloutRef, CiteTitle, Comment, DelimitedBlock,
    DelimitedBlockType, DescriptionList, DescriptionListItem, DiscreteHeader, Document,
    DocumentAttribute, DocumentAttributes, Error, Header, Image, InlineNode, ListItem,
    ListItemCheckedStatus, Location, OrderedList, PageBreak, Paragraph, Plain, Raw, Section,
    Source, SourceLocation, StemContent, StemNotation, Subtitle, Table, TableOfContents, TableRow,
    ThematicBreak, Title, UnorderedList, Verbatim, Video,
    grammar::{
        ParserState,
        attributes::AttributeEntry,
        author::derive_author_attrs,
        doctype::{is_book_doctype, is_manpage_doctype},
        inline_preprocessing,
        inline_preprocessor::InlinePreprocessorParserState,
        inline_processing::{adjust_and_log_parse_error, process_inlines},
        manpage::{derive_manpage_header_attrs, derive_name_section_attrs, extract_plain_text},
        revision::{IgnoredRevisionFields, RevisionInfo, process_revision_info},
        table::parse_table_cell,
    },
    model::{
        LeveloffsetRange, ListLevel, Locateable, SectionLevel, UNNUMBERED_SECTION_STYLES,
        strip_quotes,
        substitution::{HEADER, parse_subs_attribute},
    },
};

use super::helpers::{
    AttributeProcessingMode, BlockMetadataLine, BlockParsingMetadata, HeaderMetadataLine,
    PositionWithOffset, RESERVED_NAMED_ATTRIBUTE_ID, RESERVED_NAMED_ATTRIBUTE_OPTIONS,
    RESERVED_NAMED_ATTRIBUTE_ROLE, RESERVED_NAMED_ATTRIBUTE_SUBS, Shorthand,
    process_attribute_list, strip_url_backslash_escapes, title_looks_like_description_list,
};
use super::setext;
use std::borrow::Cow;
use std::rc::Rc;

/// Helper to check delimiter matching and return error if mismatched
fn check_delimiters(
    open: &str,
    close: &str,
    block_type: &str,
    detail: SourceLocation,
) -> Result<(), Error> {
    if open == close {
        Ok(())
    } else {
        Err(Error::mismatched_delimiters(detail, block_type))
    }
}

fn get_literal_paragraph<'input>(
    state: &ParserState<'input>,
    content: &'input str,
    start: usize,
    end: usize,
    offset: usize,
    block_metadata: &BlockParsingMetadata<'input>,
) -> Block<'input> {
    tracing::debug!(
        content,
        "paragraph starts with a space - switching to literal block"
    );
    let mut metadata = block_metadata.metadata.clone();
    metadata.move_positional_attributes_to_attributes();
    metadata.style = Some("literal");
    let location = state.create_block_location(start, end, offset);

    // Strip leading space from each line ONLY if ALL lines consistently have leading space
    // This matches asciidoctor's behavior
    let all_lines_have_leading_space = content
        .lines()
        .all(|line| line.is_empty() || line.starts_with(' '));

    let content_ref: &'input str = if all_lines_have_leading_space {
        state.intern_join(
            content
                .lines()
                .map(|line| line.strip_prefix(' ').unwrap_or(line)),
            "\n",
        )
    } else {
        content
    };

    tracing::debug!(
        content = content_ref,
        all_lines_have_leading_space,
        "created literal paragraph"
    );
    Block::Paragraph(Paragraph {
        content: vec![InlineNode::PlainText(Plain {
            content: content_ref,
            location: location.clone(),
            escaped: false,
        })],
        metadata,
        title: block_metadata.title.clone(),
        location,
    })
}

/// Assembles principal text from first line and continuation lines.
/// Used by list item parsing rules to combine multi-line content.
/// Produce the principal text for a list item, interned into the arena.
///
/// When there are no continuation lines (the common case), this just returns
/// the borrowed `first_line` unchanged — zero allocation. Otherwise it writes
/// `first_line` followed by each continuation line (separated by `\n`) into a
/// fresh arena string.
fn assemble_principal_text<'a>(
    state: &ParserState<'a>,
    first_line: &'a str,
    continuation_lines: &[&str],
) -> &'a str {
    if continuation_lines.is_empty() {
        first_line
    } else {
        let mut s = bumpalo::collections::String::new_in(state.arena);
        s.push_str(first_line);
        for line in continuation_lines {
            s.push('\n');
            s.push_str(line);
        }
        s.into_bump_str()
    }
}

/// Calculates the end position for a list item based on its principal text.
/// Returns `start` if empty, otherwise one less than `first_line_end`.
const fn calculate_item_end(
    principal_text_is_empty: bool,
    start: usize,
    first_line_end: usize,
) -> usize {
    if principal_text_is_empty {
        start
    } else {
        first_line_end.saturating_sub(1)
    }
}

/// Apply leveloffset to a section level.
///
/// This function combines two sources of leveloffset:
/// 1. Range-based offsets from include directives with `leveloffset=` attribute
/// 2. Document attribute `:leveloffset:` set directly in the document
///
/// The range-based offsets are checked first (based on byte position), and any
/// offset from document attributes is added on top.
///
/// Used by both `section_level` and `section_level_at_line_start` rules.
fn apply_leveloffset(
    base_level: SectionLevel,
    byte_offset: usize,
    leveloffset_ranges: &[LeveloffsetRange],
    document_attributes: &DocumentAttributes,
) -> SectionLevel {
    // Calculate offset from ranges (include directives)
    let range_offset = crate::model::calculate_leveloffset_at(leveloffset_ranges, byte_offset);

    // Get offset from document attributes (inline :leveloffset: settings)
    let attr_offset = document_attributes
        .get_string("leveloffset")
        .and_then(|s| s.parse::<isize>().ok())
        .unwrap_or(0);

    // Combine both offsets
    let total_offset = range_offset + attr_offset;

    if total_offset != 0 {
        let adjusted = isize::from(base_level) + total_offset;
        // Clamp to valid section levels (0-5)
        let clamped = adjusted.clamp(0, 5);
        // Safely converting the clamp ensures the value is in u8 range
        SectionLevel::try_from(clamped)
            .inspect_err(|error| {
                tracing::error!(
                    clamped,
                    ?error,
                    "not a valid section after applying leveloffset"
                );
            })
            .unwrap_or(0)
    } else {
        base_level
    }
}

/// How the closing delimiter of a table block was resolved.
///
/// A `Terminated` variant carries the matched close delimiter and its start
/// position so callers can validate symmetry with the open delimiter and
/// record an accurate close-delimiter location. `Unterminated` means the
/// opening delimiter ran to end-of-input without a matching close — the
/// parser still assembles a table (matching asciidoctor's recovery) and
/// emits a warning.
#[derive(Clone, Copy)]
enum TableClosing<'a> {
    Terminated {
        close_delim: &'a str,
        close_start: usize,
    },
    Unterminated,
}

/// Parameters for parsing a table block, passed from delimiter-specific grammar rules
/// to the common parsing helper function.
struct TableParseParams<'a> {
    start: usize,
    offset: usize,
    table_start: usize,
    content_start: usize,
    content_end: usize,
    end: usize,
    open_delim: &'a str,
    content: &'a str,
    default_separator: &'a str,
    closing: TableClosing<'a>,
}

/// Parse a table block from pre-extracted positions and content.
///
/// This helper function contains the common table parsing logic used by all
/// delimiter-specific table rules (pipe, exclamation, comma, colon).
#[allow(clippy::too_many_lines)]
fn parse_table_block_impl<'input>(
    params: &TableParseParams<'_>,
    state: &mut ParserState<'input>,
    block_metadata: &BlockParsingMetadata<'input>,
) -> Result<Block<'input>, Error> {
    let &TableParseParams {
        start,
        offset,
        table_start,
        content_start,
        content_end: _content_end,
        end,
        open_delim,
        content,
        default_separator,
        closing,
    } = params;

    let mut metadata = block_metadata.metadata.clone();
    metadata.move_positional_attributes_to_attributes();
    let location = state.create_block_location(start, end, offset);
    let table_location = state.create_block_location(table_start, end, offset);
    let open_delimiter_location = state.create_location(
        table_start + offset,
        table_start + offset + open_delim.len().saturating_sub(1),
    );
    let close_delimiter_location = match closing {
        TableClosing::Terminated {
            close_delim,
            close_start,
        } => {
            check_delimiters(
                open_delim,
                close_delim,
                "table",
                state.create_error_source_location(state.create_block_location(start, end, offset)),
            )?;
            Some(state.create_block_location(close_start, end, offset))
        }
        TableClosing::Unterminated => {
            state.add_warning(crate::Warning::new(
                crate::WarningKind::UnterminatedTable {
                    delimiter: open_delim.to_string(),
                },
                Some(state.create_error_source_location(open_delimiter_location.clone())),
            ));
            None
        }
    };

    let separator = if let Some(AttributeValue::String(sep)) =
        block_metadata.metadata.attributes.get("separator")
    {
        sep.to_string()
    } else if let Some(AttributeValue::String(format)) =
        block_metadata.metadata.attributes.get("format")
    {
        match &**format {
            "csv" => ",",
            "dsv" => ":",
            "tsv" => "\t",
            unknown_format => {
                state.add_generic_warning_at(
                    format!("unknown table format '{unknown_format}', using default separator"),
                    table_location.clone(),
                );
                default_separator
            }
        }
        .to_string()
    } else {
        default_separator.to_string()
    };

    let (ncols, column_formats) = if let Some(AttributeValue::String(cols)) =
        block_metadata.metadata.attributes.get("cols")
    {
        // Parse cols attribute
        // Full syntax: [multiplier*][halign][valign][width][style]
        // Examples: "3*", "^.>2a", "2*>.^1m", "<,^,>", "15%,30%,55%"
        let mut specs = Vec::new();

        for part in cols.split(',') {
            let s = strip_quotes(part.trim());

            // Check for "N*" notation (e.g., "3*" means 3 columns with same spec)
            let (multiplier, spec_str) = if let Some(pos) = s.find('*') {
                let mult_str = &s[..pos];
                let mult = mult_str.parse::<usize>().unwrap_or(1);
                (mult, &s[pos + 1..])
            } else {
                (1, s)
            };

            let mut halign = crate::HorizontalAlignment::default();
            let mut valign = crate::VerticalAlignment::default();
            let mut width = crate::ColumnWidth::default();
            let mut style = crate::ColumnStyle::default();

            // Parse style (last character if it's a letter: a, d, e, h, l, m, s)
            let spec_str = if let Some(last_char) = spec_str.chars().last() {
                match last_char {
                    'a' => {
                        style = crate::ColumnStyle::AsciiDoc;
                        &spec_str[..spec_str.len() - 1]
                    }
                    'd' => {
                        style = crate::ColumnStyle::Default;
                        &spec_str[..spec_str.len() - 1]
                    }
                    'e' => {
                        style = crate::ColumnStyle::Emphasis;
                        &spec_str[..spec_str.len() - 1]
                    }
                    'h' => {
                        style = crate::ColumnStyle::Header;
                        &spec_str[..spec_str.len() - 1]
                    }
                    'l' => {
                        style = crate::ColumnStyle::Literal;
                        &spec_str[..spec_str.len() - 1]
                    }
                    'm' => {
                        style = crate::ColumnStyle::Monospace;
                        &spec_str[..spec_str.len() - 1]
                    }
                    's' => {
                        style = crate::ColumnStyle::Strong;
                        &spec_str[..spec_str.len() - 1]
                    }
                    _ => spec_str,
                }
            } else {
                spec_str
            };

            // Parse vertical alignment markers: .<, .^, .>
            if spec_str.contains(".<") {
                valign = crate::VerticalAlignment::Top;
            } else if spec_str.contains(".^") {
                valign = crate::VerticalAlignment::Middle;
            } else if spec_str.contains(".>") {
                valign = crate::VerticalAlignment::Bottom;
            }

            // Parse horizontal alignment markers: <, ^, > (not preceded by .)
            for (i, c) in spec_str.char_indices() {
                let prev_char = if i > 0 {
                    spec_str.chars().nth(i - 1)
                } else {
                    None
                };
                if prev_char == Some('.') {
                    continue; // This is a vertical alignment marker
                }
                match c {
                    '<' => halign = crate::HorizontalAlignment::Left,
                    '^' => halign = crate::HorizontalAlignment::Center,
                    '>' => halign = crate::HorizontalAlignment::Right,
                    _ => {}
                }
            }

            // Parse width: integer (proportional), percentage, or ~ (auto)
            // The ~ (tilde) for auto-width was added in Asciidoctor 1.5.7
            // See: https://github.com/asciidoctor/asciidoctor/issues/1844
            // Remove alignment markers to find the width
            let width_str: String = spec_str
                .chars()
                .filter(|c| !matches!(c, '<' | '^' | '>' | '.'))
                .collect();
            if !width_str.is_empty() {
                if width_str == "~" {
                    width = crate::ColumnWidth::Auto;
                } else if width_str.ends_with('%') {
                    if let Ok(pct) = width_str.trim_end_matches('%').parse::<u32>() {
                        width = crate::ColumnWidth::Percentage(pct);
                    }
                } else if let Ok(prop) = width_str.parse::<u32>() {
                    width = crate::ColumnWidth::Proportional(prop);
                }
            }

            // Add the spec for each column in the multiplier (including defaults)
            let spec = crate::ColumnFormat {
                halign,
                valign,
                width,
                style,
            };
            for _ in 0..multiplier {
                specs.push(spec.clone());
            }
        }

        (Some(specs.len()), specs)
    } else {
        (None, Vec::new())
    };

    // Set this to true if the user mandates it!
    let mut has_header = block_metadata.metadata.options.contains(&"header");
    let raw_rows = Table::parse_rows_with_positions(
        content,
        &separator,
        &mut has_header,
        content_start + offset,
        ncols,
    );

    // If the user forces a noheader, we should not have a header, so after we've
    // tried to figure out if there are any headers, we should set it to false one
    // last time.
    if block_metadata.metadata.options.contains(&"noheader") {
        has_header = false;
    }
    let has_footer = block_metadata.metadata.options.contains(&"footer");

    let mut header = None;
    let mut footer = None;
    // `rows` ends up with one entry per raw row (minus header/footer split).
    let mut rows = Vec::with_capacity(raw_rows.len());

    // Track rowspan state: maps column positions to remaining rowspan count.
    // When a cell has rowspan > 1, we track how many more rows it occupies.
    // Each entry: (column_position, remaining_rows, colspan_width)
    let mut active_rowspans: Vec<(usize, usize, usize)> = Vec::new();

    for (i, row) in raw_rows.iter().enumerate() {
        // Each raw cell produces at least one `columns` entry; duplication
        // produces more but is rare. `row.len()` is a tight lower bound and
        // sizes the common case exactly.
        let mut columns = Vec::with_capacity(row.len());
        let mut col_idx = 0; // Track current column index for column format lookup
        for cell in row {
            // Apply column format style if cell doesn't have explicit style
            let effective_cell = if cell.style.is_none()
                && let Some(col_format) = column_formats.get(col_idx)
                && col_format.style != crate::ColumnStyle::Default
            {
                let mut cell_with_style = cell.clone();
                cell_with_style.style = Some(col_format.style);
                cell_with_style
            } else {
                cell.clone()
            };

            // Cell content is owned by the ParsedCell; intern into the parser
            // arena so downstream block parsing can borrow at `'input`.
            let cell_content: &'input str = state.intern_str(&effective_cell.content);
            let parsed = parse_table_cell(
                cell_content,
                state,
                effective_cell.content_start,
                block_metadata.parent_section_level,
                &effective_cell,
            )?;
            if effective_cell.is_duplication && effective_cell.duplication_count > 1 {
                // Duplicate the cell N times
                for _ in 0..effective_cell.duplication_count {
                    columns.push(parsed.clone());
                }
                col_idx += effective_cell.duplication_count * effective_cell.colspan;
            } else {
                columns.push(parsed);
                col_idx += effective_cell.colspan;
            }
        }

        // Row location from first cell (falls back to the table location
        // if the row is empty, which shouldn't happen in practice).
        let row_location = if let Some(first) = row.first() {
            state.create_location(first.start, first.end)
        } else {
            table_location.clone()
        };

        // Calculate occupied columns from active rowspans
        let occupied_from_rowspans: usize = active_rowspans
            .iter()
            .map(|(_pos, _remaining, width)| *width)
            .sum();

        // Logical column count = columns occupied by rowspans + colspans of new cells
        let logical_col_count: usize =
            occupied_from_rowspans + columns.iter().map(|c| c.colspan).sum::<usize>();

        if let Some(ncols) = ncols
            && logical_col_count != ncols
        {
            // Check if any cell's colspan exceeds the table width
            let has_overflow = columns.iter().any(|c| c.colspan > ncols);
            if has_overflow {
                state.add_generic_warning_at(
                    format!(
                        "dropping cell because it exceeds specified number of columns: actual={logical_col_count}, expected={ncols}"
                    ),
                    row_location,
                );
            } else {
                state.add_generic_warning_at(
                    format!(
                        "table row has incorrect column count: actual={logical_col_count}, expected={ncols}, occupied_from_rowspans={occupied_from_rowspans}"
                    ),
                    row_location,
                );
            }
            continue;
        }

        // Update active rowspans for this row:
        // 1. Decrement remaining count for existing rowspans
        // 2. Remove rowspans that are now exhausted
        active_rowspans.retain_mut(|(_pos, remaining, _width)| {
            *remaining -= 1;
            *remaining > 0
        });

        // 3. Add new rowspans from current row's cells
        let mut col_position = 0;
        for (_, active_pos, _, colspan) in active_rowspans.iter().map(|(p, r, c)| (*p, *p, *r, *c))
        {
            if col_position == active_pos {
                col_position += colspan;
            }
        }
        for cell in &columns {
            // Skip over positions occupied by rowspans
            while active_rowspans
                .iter()
                .any(|(pos, _, width)| col_position >= *pos && col_position < pos + width)
            {
                if let Some((_, _, width)) = active_rowspans
                    .iter()
                    .find(|(pos, _, w)| col_position >= *pos && col_position < pos + w)
                {
                    col_position += width;
                }
            }
            if cell.rowspan > 1 {
                active_rowspans.push((col_position, cell.rowspan - 1, cell.colspan));
            }
            col_position += cell.colspan;
        }

        // if we have a header, we need to add the columns we have to the header
        if has_header {
            header = Some(TableRow { columns });
            has_header = false;
            continue;
        }

        // if we have a footer, we need to add the columns we have to the footer
        if has_footer && i == raw_rows.len() - 1 {
            footer = Some(TableRow { columns });
            continue;
        }

        // if we get here, these columns are a row
        rows.push(TableRow { columns });
    }

    let table = Table {
        header,
        footer,
        rows,
        columns: column_formats,
        location: table_location.clone(),
    };

    Ok(Block::DelimitedBlock(DelimitedBlock {
        metadata: metadata.clone(),
        delimiter: state.intern_str(open_delim),
        inner: DelimitedBlockType::DelimitedTable(table),
        title: block_metadata.title.clone(),
        location,
        open_delimiter_location: Some(open_delimiter_location),
        close_delimiter_location,
    }))
}

/// Scans `bytes[pos..]` for a description-list marker (`::`, `:::`, `::::`, or
/// `;;`) preceded by at least one term character and followed by EOL, space, or
/// (optionally) end-of-input. Returns `true` on the first complete marker, or
/// `false` if the bound is reached first.
///
/// `scan_across_eol = false` bounds the scan at the next `\n` (line-local).
/// `scan_across_eol = true` bounds it at the next blank line (`\n\n`).
/// `allow_eoi = true` treats end-of-input after the marker as valid context.
///
/// Replaces the per-byte PEG lookahead used by `check_start_of_description_list`,
/// `check_line_is_description_list`, and the inline negation in
/// `description_list_item`'s continuation pattern. The PEG version called
/// `description_list_marker()` (4 string-alts) at every byte, dominating CPU
/// time on macro-heavy paragraphs that don't actually contain dlist markers.
#[inline]
fn find_dlist_marker(bytes: &[u8], pos: usize, scan_across_eol: bool, allow_eoi: bool) -> bool {
    let mut i = pos;
    while let Some(&b) = bytes.get(i) {
        if b == b'\n' && (!scan_across_eol || bytes.get(i + 1) == Some(&b'\n')) {
            return false;
        }
        if i > pos && (b == b':' || b == b';') {
            let marker_len = if b == b':' {
                let mut k = 1;
                while k < 4 && bytes.get(i + k) == Some(&b':') {
                    k += 1;
                }
                if k >= 2 { k } else { 0 }
            } else if bytes.get(i + 1) == Some(&b';') {
                2
            } else {
                0
            };
            if marker_len > 0 {
                let after = bytes.get(i + marker_len).copied();
                if matches!(after, Some(b'\n' | b' ')) || (allow_eoi && after.is_none()) {
                    return true;
                }
            }
        }
        i += 1;
    }
    false
}

peg::parser! {
    pub(crate) grammar document_parser(state: &mut ParserState<'input>) for str {
        use std::str::FromStr;
        use crate::model::{substitute, Substitution};
        use crate::grammar::inlines::inline_parser;

        // We ignore empty lines before we set the start position of the document because
        // the asciidoc document should not consider empty lines at the beginning or end
        // of the file.
        //
        // We also ignore comments before the header - maybe we should change this but as
        // it stands in our current model, it makes no sense to have comments in the
        // blocks as it is a completely separate part of the document.
        pub(crate) rule document() -> Result<Document<'input>, Error>
        = eol()* start:position() comments_before_header:comment_line_block(0)* header_result:header() blocks:blocks(0, None) end:position!() (eol()* / ![_]) {
            let header = header_result?;
            let blocks: Vec<Block<'_>> = comments_before_header.into_iter().collect::<Result<Vec<_>, Error>>()?.into_iter().chain(blocks?).collect();

            // Ensure end offset is on a valid UTF-8 boundary
            let mut document_end_offset = end;
            if document_end_offset > state.input.len() {
                document_end_offset = state.input.len();
            }
            // If not on a boundary, round forward to the next boundary
            while document_end_offset < state.input.len() && !state.input.is_char_boundary(document_end_offset) {
                document_end_offset += 1;
            }
            // Then decrement by one byte to get the last byte of content
            let document_end_offset = if document_end_offset == 0 {
                0
            } else {
                crate::grammar::utf8_utils::safe_decrement_offset(state.input, document_end_offset)
            };

            // Ensure the invariant: absolute_start <= absolute_end
            let (absolute_start, absolute_end) = if start.offset > document_end_offset {
                // This can happen with whitespace-only input where eol()* consumes all content
                // In this case, treat as an empty document at the start position
                (start.offset, start.offset)
            } else {
                (start.offset, document_end_offset)
            };

            // Special case for truly empty input: TCK expects column 0
            // Only for zero-byte input, not whitespace-only
            let (start_position, end_position) = if state.input.is_empty() || (absolute_start == 0 && absolute_end == 0) {
                // Whitespace-only documents should use column 1
                (
                    crate::Position { line: 1, column: 0 },
                    crate::Position { line: 1, column: 0 }
                )
            } else {
                (
                    start.position,
                    state.line_map.offset_to_position(absolute_end, state.input)
                )
            };

            // Warn if the first section skips level 1 (e.g. document jumps
            // straight from `= Doc Title` to `=== Heading`). Matches asciidoctor's
            // "section title out of sequence" check; only fires when a doc title
            // is present — titleless documents accept any first-section level.
            if header.as_ref().is_some_and(|h| !h.title.is_empty())
                && let Some(first_section) = blocks.iter().find_map(|b| {
                    if let Block::Section(s) = b { Some(s) } else { None }
                })
                && first_section.level > 1
            {
                let level = first_section.level;
                let markers = "=".repeat(usize::from(level) + 1);
                let location = state.create_error_source_location(first_section.location.clone());
                state.add_warning(crate::Warning::new(
                    crate::WarningKind::SectionLevelOutOfSequence {
                        got: level,
                        markers,
                    },
                    Some(location),
                ));
            }

            Ok(Document {
                header,
                location: Location {
                    absolute_start,
                    absolute_end,
                    start: start_position,
                    end: end_position,
                },
                attributes: DocumentAttributes::clone(&state.document_attributes),
                blocks,
                footnotes: state.footnote_tracker.borrow().footnotes.clone(),
                toc_entries: state.toc_tracker.entries.clone(),
            })
        }

        pub(crate) rule header() -> Result<Option<Header<'input>>, Error>
            = start:position!()
            ((document_attribute() / comment()) (eol()+ / ![_]))*
            // Parse header metadata (anchors and attributes) before the document title
            metadata:header_metadata()
            title_authors:(title_authors:title_authors() { title_authors })?
            (eol()+ (document_attribute() / comment()))*
            end:position!()
            (eol()*<,2> / ![_])
        {
            if let Some((title, subtitle, authors)) = title_authors {
                let mut location = state.create_location(start, end);
                // Decrement end by one character (for byte offset, use safe UTF-8 decrement)
                location.absolute_end = crate::grammar::utf8_utils::safe_decrement_offset(state.input, location.absolute_end);
                location.end.column = location.end.column.saturating_sub(1);
                let mut header = Header {
                    metadata,
                    title,
                    subtitle,
                    authors,
                    location,
                };

                // Derive author attributes bidirectionally
                derive_author_attrs(
                    state.arena,
                    &mut header,
                    Rc::make_mut(&mut state.document_attributes),
                );

                // Derive manpage attributes from header if doctype=manpage
                // This must happen during parsing so {mantitle} etc. work in body
                if is_manpage_doctype(&state.document_attributes) {
                    derive_manpage_header_attrs(
                        Some(&header),
                        Rc::make_mut(&mut state.document_attributes),
                        state.options.strict,
                        state.current_file.as_deref(),
                    )?;
                }

                Ok(Some(header))
            } else {
                tracing::debug!("No title or authors found in the document header.");
                Ok(None)
            }
        }

        /// Parse block metadata lines (anchors and attributes) that can appear before a document title.
        /// Only consumes metadata if followed by a document title to avoid stealing attributes
        /// meant for the first block when there's no document title.
        rule header_metadata() -> BlockMetadata<'input>
            = lines:(
                anchor:anchor() { HeaderMetadataLine::Anchor(anchor) }
                / attr:attributes_line() { HeaderMetadataLine::Attributes((attr.0, Box::new(attr.1))) }
            )+ &document_title()
            {
                let mut metadata = BlockMetadata::default();

                for line in lines {
                    match line {
                        HeaderMetadataLine::Anchor(anchor) => metadata.anchors.push(anchor),
                        HeaderMetadataLine::Attributes((_, attr_metadata)) => {
                            let attr_metadata = *attr_metadata;
                            // Merge attribute metadata - last one wins for id/style
                            if attr_metadata.id.is_some() {
                                metadata.id = attr_metadata.id;
                            }
                            if attr_metadata.style.is_some() {
                                metadata.style = attr_metadata.style;
                            }
                            metadata.roles.extend(attr_metadata.roles);
                            metadata.options.extend(attr_metadata.options);
                            metadata.attributes = attr_metadata.attributes;
                            metadata.positional_attributes = attr_metadata.positional_attributes;
                        }
                    }
                }
                metadata
            }
            / { BlockMetadata::default() }

        pub(crate) rule title_authors() -> (Title<'input>, Option<Subtitle<'input>>, Vec<Author<'input>>)
        = title_and_subtitle:document_title() eol() authors:authors_and_revision() &(eol()+ / ![_])
        {
            let (title, subtitle) = title_and_subtitle;
            tracing::debug!(?title, ?subtitle, ?authors, "Found title and authors in the document header.");
            (title, subtitle, authors)
        }
        / title_and_subtitle:document_title() &eol() {
            let (title, subtitle) = title_and_subtitle;
            tracing::debug!(?title, ?subtitle, "Found title in the document header without authors.");
            (title, subtitle, vec![])
        }

        pub(crate) rule document_title() -> (Title<'input>, Option<Subtitle<'input>>)
        = document_title_atx()
        / document_title_setext()

        /// ATX-style document title: `= Title` or `# Title`
        rule document_title_atx() -> (Title<'input>, Option<Subtitle<'input>>)
        = document_title_token() whitespace() start:position!() title:$([^'\n']*) end:position!()
        {?
            tracing::debug!(?title, "Processing ATX document title");
            let block_metadata = BlockParsingMetadata::default();

            let (title_inlines, subtitle) = if let Some(colon_pos) = title.rfind(':') {
                let subtitle_raw = &title[colon_pos + 1..];
                let subtitle_text = subtitle_raw.trim();
                if subtitle_text.is_empty() {
                    // Empty subtitle after colon, treat whole text as title
                    let inlines = process_inlines(state, &block_metadata, start, end, 0, title)
                        .map_err(|_| "could not process document title")?;
                    (inlines, None)
                } else {
                    // Title: trim trailing whitespace before colon
                    let title_raw = &title[..colon_pos];
                    let title_text = title_raw.trim_end();
                    let title_end = start + title_text.len();
                    let inlines = process_inlines(state, &block_metadata, start, title_end, 0, title_text)
                        .map_err(|_| "could not process document title")?;

                    // Subtitle: trim leading whitespace after colon
                    let sub_leading = subtitle_raw.len() - subtitle_raw.trim_start().len();
                    let sub_start_offset = start + colon_pos + 1 + sub_leading;
                    let subtitle_start = PositionWithOffset {
                        offset: sub_start_offset,
                        position: state.line_map.offset_to_position(sub_start_offset, state.input),
                    };
                    let sub_end = sub_start_offset + subtitle_text.len();
                    let subtitle_inlines = process_inlines(state, &block_metadata, subtitle_start.offset, sub_end, 0, subtitle_text)
                        .map_err(|_| "could not process document subtitle")?;

                    (inlines, Some(Subtitle::new(subtitle_inlines)))
                }
            } else {
                let inlines = process_inlines(state, &block_metadata, start, end, 0, title)
                    .map_err(|_| "could not process document title")?;
                (inlines, None)
            };

            Ok((Title::new(title_inlines), subtitle))
        }

        /// Setext-style document title: Title underlined with `=` characters
        ///
        /// ```text
        /// Document Title
        /// ==============
        /// ```
        ///
        /// The underline must be within ±2 characters of the title width.
        /// Only enabled when the setext feature is compiled in AND the runtime
        /// option is enabled.
        rule document_title_setext() -> (Title<'input>, Option<Subtitle<'input>>)
        = start:position!() title:$([^'\n']+) end:position!() eol()
          underline:$("="+) &(eol() / ![_])
        {?
            // Check if setext mode is enabled
            if !setext::is_enabled(state) {
                return Err("setext mode not enabled");
            }

            let title_text = title.trim();
            let title_width = title_text.chars().count();
            let underline_width = underline.chars().count();

            // Check underline width tolerance (±2 characters)
            if !setext::width_ok(title_width, underline_width) {
                return Err("underline width out of tolerance");
            }

            // Check underline is level 0 (document title uses =)
            if !underline.starts_with('=') {
                return Err("document title must use = underline");
            }

            tracing::debug!(?title_text, "Processing setext document title");
            let block_metadata = BlockParsingMetadata::default();

            let (title_inlines, subtitle) = if let Some(colon_pos) = title.rfind(':') {
                let subtitle_raw = &title[colon_pos + 1..];
                let subtitle_text = subtitle_raw.trim();
                if subtitle_text.is_empty() {
                    let inlines = process_inlines(state, &block_metadata, start, end, 0, title)
                        .map_err(|_| "could not process setext document title")?;
                    (inlines, None)
                } else {
                    // Title: trim trailing whitespace before colon
                    let title_raw = &title[..colon_pos];
                    let title_text = title_raw.trim_end();
                    let title_end = start + title_text.len();
                    let inlines = process_inlines(state, &block_metadata, start, title_end, 0, title_text)
                        .map_err(|_| "could not process setext document title")?;

                    // Subtitle: trim leading whitespace after colon
                    let sub_leading = subtitle_raw.len() - subtitle_raw.trim_start().len();
                    let sub_start_offset = start + colon_pos + 1 + sub_leading;
                    let subtitle_start = PositionWithOffset {
                        offset: sub_start_offset,
                        position: state.line_map.offset_to_position(sub_start_offset, state.input),
                    };
                    let sub_end = sub_start_offset + subtitle_text.len();
                    let subtitle_inlines = process_inlines(state, &block_metadata, subtitle_start.offset, sub_end, 0, subtitle_text)
                        .map_err(|_| "could not process setext document subtitle")?;

                    (inlines, Some(Subtitle::new(subtitle_inlines)))
                }
            } else {
                let inlines = process_inlines(state, &block_metadata, start, end, 0, title)
                    .map_err(|_| "could not process setext document title")?;
                (inlines, None)
            };

            Ok((Title::new(title_inlines), subtitle))
        }

        rule document_title_token() = "=" / "#"

        rule authors_and_revision() -> Vec<Author<'input>>
            // Capture the author line, substitute any attribute references, then parse
            = author_line:$([^'\n']+) (eol() revision_pre_substitution())? {?
                let substituted_cow = substitute(author_line.trim(), HEADER, &state.document_attributes);
                // Intern any owned substitution result so the downstream
                // `authors()` parse can yield `Author<'input>` that outlives
                // this action block.
                let substituted: &'input str = match substituted_cow {
                    Cow::Borrowed(s) => s,
                    Cow::Owned(s) => state.intern_str(&s),
                };
                tracing::debug!(?author_line, ?substituted, "Processing author line with substitution");

                // Parse the substituted content as authors
                let mut temp_state = ParserState::for_inline_parsing(substituted, state);

                match document_parser::authors(substituted, &mut temp_state) {
                    Ok(authors) => {
                        tracing::debug!(?authors, "Parsed authors from line");
                        Ok(authors)
                    }
                    Err(_) => Err("line did not parse as authors")
                }
            }

        pub(crate) rule authors() -> Vec<Author<'input>>
            = authors:(author() ++ (";" whitespace()*)) {
                authors
            }

        /// Parse an author in various formats:
        /// - "First Middle Last <email>"
        /// - "First Last <email>"
        /// - "First <email>"
        /// - "First Last"
        pub(crate) rule author() -> Author<'input>
            = name:author_name() email:author_email()? {
                let mut author = name;
                if let Some(email_addr) = email {
                    author.email = Some(email_addr);
                }
                author
            }

        /// Parse author name in format: "First [Middle] Last" or just "First"
        rule author_name() -> Author<'input>
        = first:name_part() whitespace()+ middle:name_part() whitespace()+ last:$(name_part() ++ whitespace()) {
            Author::new(state.arena, first, Some(middle), Some(last))
        }
        / first:name_part() whitespace()+ last:name_part() {
            Author::new(state.arena, first, None, Some(last))
        }
        / first:name_part() {
            Author::new(state.arena, first, None, None)
        }

        /// Parse email address in format: " <email@domain>"
        rule author_email() -> &'input str
            = whitespace()* "<" email:$([^'>']*) ">" { email }

        rule name_part() -> &'input str
            = name:$([c if c.is_alphanumeric() || c == '.' || c == '-' || c == '\'']+ ("_" [c if c.is_alphanumeric() || c == '.' || c == '-' || c == '\'']+)*) {
                name
            }

        pub(crate) rule revision() -> ()
            = start:position!() number:$("v"? digits() ++ ".") date:revision_date()? remark:revision_remark()? end:position!() {
                let revision_info = RevisionInfo {
                    number: Cow::Owned(number.to_string()),
                    date: date.map(|d| Cow::Owned(d.to_string())),
                    remark: remark.map(|r| Cow::Owned(r.to_string())),
                };
                if revision_info.number.is_empty() {
                    // No revision number found, nothing to do
                    return;
                }
                let revision_location = state.create_location(start, end);
                let ignored: IgnoredRevisionFields = {
                    let document_attributes = Rc::make_mut(&mut state.document_attributes);
                    process_revision_info(revision_info, document_attributes)
                };
                if ignored.number {
                    state.add_generic_warning_at(
                        "Revision number found in revision line but ignoring due to being set through attribute entries.".to_string(),
                        revision_location.clone(),
                    );
                }
                if ignored.date {
                    state.add_generic_warning_at(
                        "Revision date found in revision line but ignoring due to being set through attribute entries.".to_string(),
                        revision_location.clone(),
                    );
                }
                if ignored.remark {
                    state.add_generic_warning_at(
                        "Revision remark found in revision line but ignoring due to being set through attribute entries.".to_string(),
                        revision_location,
                    );
                }
            }

        /// Parse revision line with attribute reference support
        rule revision_pre_substitution() -> ()
            // Capture the revision line, substitute any attribute references, then parse
            = rev_line:$([^'\n']+) {?
                let substituted_cow = substitute(rev_line.trim(), HEADER, &state.document_attributes);
                let substituted: &'input str = match substituted_cow {
                    Cow::Borrowed(s) => s,
                    Cow::Owned(s) => state.intern_str(&s),
                };
                tracing::debug!(?rev_line, ?substituted, "Processing revision line with substitution");

                // Parse the substituted content as revision
                let mut temp_state = ParserState::for_inline_parsing(substituted, state);

                match document_parser::revision(substituted, &mut temp_state) {
                    Ok(()) => {
                        // Copy revision attributes from temp_state back to main state
                        for key in ["revnumber", "revdate", "revremark"] {
                            if let Some(value) = temp_state.document_attributes.get(key) {
                                Rc::make_mut(&mut state.document_attributes).insert(key.into(), value.clone());
                            }
                        }
                        tracing::debug!("Parsed revision from line");
                        Ok(())
                    }
                    Err(_) => Err("line did not parse as revision")
                }
            }

        rule revision_date() -> &'input str
            = ", " date:$([^ (':'|'\n')]+) {
                date
            }

        rule revision_remark() -> &'input str
            = ": " remark:$([^'\n']+) {
                remark
            }

        rule document_attribute() -> ()
        = att:document_attribute_match() (&eol() / ![_])
        {
            let AttributeEntry{key, value, set} = att;
            tracing::debug!(%set, %key, %value, "Found document attribute in the document header");
            // Apply definition-time substitution: if value contains {attr} references,
            // expand them using currently defined attributes (matching asciidoctor behavior)
            let value = match value {
                AttributeValue::String(s) => {
                    let substituted = substitute(&s, HEADER, &state.document_attributes);
                    AttributeValue::String(Cow::Borrowed(state.intern_str(&substituted)))
                }
                AttributeValue::Bool(_) | AttributeValue::None => value,
            };
            Rc::make_mut(&mut state.document_attributes).set(key.into(), value);
        }

        pub(crate) rule blocks(offset: usize, parent_section_level: Option<SectionLevel>) -> Result<Vec<Block<'input>>, Error>
        = blocks:block(offset, parent_section_level)*
        {
            blocks.into_iter().collect::<Result<Vec<_>, Error>>()
        }

        /// Blocks for table cells without `AsciiDoc` style - excludes block types that require full parsing.
        /// Table cells use a simplified block parser that excludes sections, document attributes,
        /// and block types like lists, delimited blocks, toc, page breaks, and markdown blockquotes.
        pub(crate) rule blocks_for_table_cell(offset: usize, parent_section_level: Option<SectionLevel>) -> Result<Vec<Block<'input>>, Error>
        = eol()*
        blocks:(
            comment_line_block(offset) /
            block_generic_for_table_cell(offset, parent_section_level)
        )*
        {
            blocks.into_iter().collect::<Result<Vec<_>, Error>>()
        }

        pub(crate) rule block(offset: usize, parent_section_level: Option<SectionLevel>) -> Result<Block<'input>, Error>
        = eol()*
        // First check: if we're at a same-or-higher-level section, fail the entire block
        // This prevents section content from consuming sibling/parent sections as paragraphs
        !same_or_higher_level_section(offset, parent_section_level)
        block:(
            comment_line_block(offset) /
            document_attribute_block(offset) /
            &"[discrete" dh:discrete_header(offset) { dh } /
            section:section(offset, parent_section_level) { section } /
            // Try setext-style sections (only enabled with setext feature + runtime flag)
            section_setext:section_setext(offset, parent_section_level) { section_setext } /
            block_generic(offset, parent_section_level)
        )
        {
            block
        }

        /// Single-line comment that becomes a block in the AST.
        /// Line comments begin with `//` (but not `///` or `////` which are block comment delimiters).
        rule comment_line_block(offset: usize) -> Result<Block<'input>, Error>
        = start:position!() "//" !("/") content:$([^'\n']*) end:position!() (eol() / ![_])
        {
            Ok(Block::Comment(Comment {
                content,
                location: state.create_location(start + offset, end + offset),
            }))
        }

        // Check if the upcoming content is a section at same or higher level (which
        // should not be parsed as content)
        //
        // This rule skips optional metadata (anchors, attributes, etc.) before checking
        // the section level, so that `[[anchor]]\n== Section` is correctly identified as
        // a sibling section.
        //
        // Checks both ATX-style (= or #) and setext-style (underlined) sections.
        rule same_or_higher_level_section(offset: usize, parent_section_level: Option<SectionLevel>) -> ()
        = (anchor() / attributes_line() / document_attribute_line() / title_line(offset))*
          (
            // ATX-style section check - require space after marker to avoid matching
            // description list items like `#term::` as sections
            level:section_level(offset, parent_section_level) &" "
            {?
                if let Some(parent_level) = parent_section_level {
                    let upcoming_level = level.1 + 1; // Convert to 1-based
                    if upcoming_level <= parent_level {
                        Ok(()) // This IS a same or higher level section
                    } else {
                        Err("not a same or higher level section")
                    }
                } else {
                    Err("no parent section level to compare")
                }
            }
            /
            // Setext-style section check (title followed by underline)
            &setext_section_lookahead(parent_section_level)
          )

        /// Lookahead rule to detect setext sections at same or higher level.
        /// Used by same_or_higher_level_section to properly terminate sections.
        /// Excludes description list items (e.g., `term:: content`) which would otherwise
        /// match as setext titles.
        rule setext_section_lookahead(parent_section_level: Option<SectionLevel>) -> ()
        = title:$([^'\n']+) eol() underline:$(['-' | '~' | '^' | '+']+) &(eol() / ![_])
        {?
            // Exclude description list items
            if title_looks_like_description_list(title) {
                return Err("title looks like a description list item");
            }
            // Only check if setext mode is enabled
            if !setext::is_enabled(state) {
                return Err("setext mode not enabled");
            }

            // Validate underline width
            let title_width = title.trim().chars().count();
            let underline_width = underline.chars().count();
            if !setext::width_ok(title_width, underline_width) {
                return Err("underline width out of tolerance");
            }

            // Get level from underline character
            let underline_char = underline.chars().next().ok_or("empty underline")?;
            let level = setext::char_to_level(underline_char).ok_or("invalid setext char")?;

            // Level 0 (=) is document title, not section — unless doctype is book (parts)
            if level == 0 && !is_book_doctype(&state.document_attributes) {
                return Err("not a section, seems like you're trying to define a document title");
            }

            // Check if this is a same-or-higher level section
            if let Some(parent_level) = parent_section_level {
                if level <= parent_level {
                    Ok(()) // This IS a same or higher level setext section
                } else {
                    Err("not a same or higher level section")
                }
            } else {
                Err("no parent section level to compare")
            }
        }

        rule discrete_header(offset: usize) -> Result<Block<'input>, Error>
        = start:position!()
        block_metadata:(bm:block_metadata(offset, None) {?
            bm.map_err(|e| {
                tracing::error!(?e, "error parsing block metadata in discrete_header");
                "block metadata parse error"
            })
        })
        section_level:section_level(offset, None) whitespace()
        title_start:position!() title:section_title(offset, &block_metadata) title_end:position!() end:position!() &(eol()*<1,2> / ![_])
        {
            let title = title?;
            tracing::debug!(?block_metadata, ?title, ?title_start, ?title_end, "parsing discrete header block");

            let level = section_level.1;
            let location = state.create_block_location(start, end, offset);

            Ok(Block::DiscreteHeader(DiscreteHeader {
                metadata: block_metadata.metadata,
                title,
                level,
                location,
            }))
        }

        pub(crate) rule document_attribute_block(offset: usize) -> Result<Block<'input>, Error>
        = start:position!() att:document_attribute_match() end:position!()
        {
            let AttributeEntry{ key, value, .. } = att;
            // Apply definition-time substitution (matching asciidoctor behavior)
            let value = match value {
                AttributeValue::String(s) => {
                    let substituted = substitute(&s, HEADER, &state.document_attributes);
                    AttributeValue::String(Cow::Borrowed(state.intern_str(&substituted)))
                }
                AttributeValue::Bool(_) | AttributeValue::None => value,
            };
            Rc::make_mut(&mut state.document_attributes).set(key.into(), value.clone());
            Ok(Block::DocumentAttribute(DocumentAttribute {
                name: key.into(),
                value,
                location: state.create_location(start+offset, end+offset)
            }))
        }

        pub(crate) rule section(offset: usize, parent_section_level: Option<SectionLevel>) -> Result<Block<'input>, Error>
        = start:position!()
        block_metadata:(bm:block_metadata(offset, parent_section_level) {?
            bm.map_err(|e| {
                tracing::error!(?e, "error parsing block metadata in section");
                "block metadata parse error"
            })
        })
        section_level_start:position!()
        section_level:section_level(offset, parent_section_level)
        section_level_end:position!()
        whitespace()
        title_start:position!()
        section_header:(title:section_title(offset, &block_metadata) title_end:position!() &(eol()*<1,2> / ![_]) {
            let title = title?;
            let section_id: &'input str = Section::generate_id(state.arena, &block_metadata.metadata, &title).as_arena_str(state.arena);

            // Extract xreflabel from the last anchor (same anchor used for section ID)
            // This matches asciidoctor behavior: [[id,xreflabel]] provides custom cross-reference text
            let xreflabel = block_metadata.metadata.anchors.last().and_then(|a| a.xreflabel);

            // Special section styles (bibliography, glossary, etc.) should not be numbered
            let numbered = !block_metadata.metadata.style
                .is_some_and(|s| UNNUMBERED_SECTION_STYLES.contains(&s));

            // Register section for TOC immediately after title is parsed, before content
            state.toc_tracker.register_section(title.clone(), section_level.1, section_id, xreflabel, numbered, block_metadata.metadata.style);

            Ok::<(Title<'input>, &'input str), Error>((title, section_id))
        })
        content:section_content(offset, Some(section_level.1+1))? end:position!()
        {
            let (title, section_id) = section_header?;
            tracing::debug!(?offset, ?block_metadata, ?title, "parsing section block");

            // Validate section level against parent section level if any is provided
            if let Some(parent_level) = parent_section_level && (
                section_level.1 < parent_level  || section_level.1+1 > parent_level+1 || section_level.1 > 5) {
                    return Err(Error::NestedSectionLevelMismatch(
                        Box::new(state.create_error_source_location(state.create_block_location(section_level_start, section_level_end, offset))),
                        section_level.1+1,
                        parent_level + 1,
                    ));
            }

            let level = section_level.1;
            let location = state.create_block_location(start, end, offset);

            // Derive manname/manpurpose from NAME section in manpage documents
            //
            // This must happen before subsequent sections are parsed so {manname} works
            // in SYNOPSIS, DESCRIPTION, etc.
            if level == 1 && is_manpage_doctype(&state.document_attributes) {
                let title_text = extract_plain_text(&title);
                if title_text.eq_ignore_ascii_case("NAME")
                    && let Some(Ok(ref blocks)) = content
                    && let Some(Block::Paragraph(para)) = blocks.first()
                {
                    let para_text_owned = extract_plain_text(&para.content);
                    let para_text: &'input str = state.intern_str(&para_text_owned);
                    derive_name_section_attrs(para_text, Rc::make_mut(&mut state.document_attributes));
                }
            }

            Ok(Block::Section(Section {
                metadata: block_metadata.metadata,
                title,
                level,
                content: content.unwrap_or(Ok(Vec::new()))?,
                location
            }))
        }

        /// Setext-style section header: Title underlined with `-`, `~`, `^`, or `+`
        ///
        /// ```text
        /// Section Title
        /// -------------
        /// ```
        ///
        /// The underline character determines the section level:
        /// - `-` = Level 1
        /// - `~` = Level 2
        /// - `^` = Level 3
        /// - `+` = Level 4
        ///
        /// The underline must be within ±2 characters of the title width.
        /// Only enabled when the setext feature is compiled in AND the runtime
        /// option is enabled.
        /// Parse a setext section level from the underline character.
        /// Returns the level (1-4) corresponding to -, ~, ^, +
        rule setext_section_level(title_width: usize, parent_section_level: Option<SectionLevel>) -> u8
        = underline:$(['-' | '~' | '^' | '+']+) &(eol() / ![_])
        {?
            // Check if setext mode is enabled
            if !setext::is_enabled(state) {
                return Err("setext mode not enabled");
            }

            let underline_width = underline.chars().count();

            // Check underline width tolerance (±2 characters)
            if !setext::width_ok(title_width, underline_width) {
                return Err("underline width out of tolerance");
            }

            // Get the underline character and determine section level
            let underline_char = underline.chars().next().ok_or("empty underline")?;
            let level = setext::char_to_level(underline_char).ok_or("invalid setext underline character")?;

            // Document title (level 0) uses =, not allowed here — unless doctype is book (parts)
            if level == 0 && !is_book_doctype(&state.document_attributes) {
                return Err("use = underline for document title, not section");
            }

            // Validate section level against parent section level if any is provided
            if let Some(parent_level) = parent_section_level
                && (level < parent_level || level > parent_level + 1 || level > 5)
            {
                return Err("section level mismatch with parent");
            }

            Ok(level)
        }

        /// Parse a setext-style section (title followed by underline).
        /// Excludes description list items (e.g., `term:: content`) which would otherwise
        /// match as setext titles.
        pub(crate) rule section_setext(offset: usize, parent_section_level: Option<SectionLevel>) -> Result<Block<'input>, Error>
        = start:position!()
        !check_line_is_description_list(offset)
        block_metadata:(bm:block_metadata(offset, parent_section_level) {?
            bm.map_err(|e| {
                tracing::error!(?e, "error parsing block metadata in section_setext");
                "block metadata parse error"
            })
        })
        title_start:position!() title:$([^'\n']+) title_end:position!() eol()
        setext_level:setext_section_level(title.trim().chars().count(), parent_section_level)
        section_header:({
            // Parse the title using inline processing
            match process_inlines(state, &block_metadata, title_start, title_end, offset, title) {
                Ok(processed_title) => {
                    let processed_title = Title::new(processed_title);
                    let section_id_str = Section::generate_id(state.arena, &block_metadata.metadata, &processed_title).to_string();
                    let section_id: &'input str = state.intern_str(&section_id_str);

                    // Extract xreflabel from the last anchor
                    let xreflabel = block_metadata.metadata.anchors.last().and_then(|a| a.xreflabel);

                    // Special section styles (bibliography, glossary, etc.) should not be numbered
                    let numbered = !block_metadata.metadata.style
                        .is_some_and(|s| UNNUMBERED_SECTION_STYLES.contains(&s));

                    // Register section for TOC
                    state.toc_tracker.register_section(processed_title.clone(), setext_level, section_id, xreflabel, numbered, block_metadata.metadata.style);

                    Ok::<(Title<'input>, &'input str), Error>((processed_title, section_id))
                }
                Err(e) => Err(e),
            }
        })
        content:section_content(offset, Some(setext_level + 1))? end:position!()
        {
            let (title, _section_id) = section_header?;
            let location = state.create_block_location(start, end, offset);

            // Derive manname/manpurpose from NAME section in manpage documents
            if setext_level == 1 && is_manpage_doctype(&state.document_attributes) {
                let title_text = extract_plain_text(&title);
                if title_text.eq_ignore_ascii_case("NAME")
                    && let Some(Ok(ref blocks)) = content
                    && let Some(Block::Paragraph(para)) = blocks.first()
                {
                    let para_text_owned = extract_plain_text(&para.content);
                    let para_text: &'input str = state.intern_str(&para_text_owned);
                    derive_name_section_attrs(para_text, Rc::make_mut(&mut state.document_attributes));
                }
            }

            Ok(Block::Section(Section {
                metadata: block_metadata.metadata,
                title,
                level: setext_level,
                content: content.unwrap_or(Ok(Vec::new()))?,
                location,
            }))
        }

        rule block_metadata(offset: usize, parent_section_level: Option<SectionLevel>) -> Result<BlockParsingMetadata<'input>, Error>
        = meta_start:position!() lines:(
            anchor:anchor() { Ok::<BlockMetadataLine<'input>, Error>(BlockMetadataLine::Anchor(anchor)) }
            / attr:attributes_line() { Ok::<BlockMetadataLine<'input>, Error>(BlockMetadataLine::Attributes((attr.0, Box::new(attr.1)))) }
            / doc_attr:document_attribute_line() { Ok::<BlockMetadataLine<'input>, Error>(BlockMetadataLine::DocumentAttribute(Cow::Borrowed(doc_attr.key), doc_attr.value)) }
            / title:title_line(offset) { title.map(BlockMetadataLine::Title) }
        )* meta_end:position!()
        {
            let mut metadata = BlockMetadata::default();
            let mut discrete = false;
            let mut title = Title::default();

            for line in lines {
                // Skip errors from title parsing (e.g., empty titles like "." + newline)
                let Ok(value) = line else {
                    state.add_generic_warning(format!("failed to parse block metadata line, skipping: {line:?}"));
                    continue
                };
                match value {
                    BlockMetadataLine::Anchor(value) => metadata.anchors.push(value),
                    BlockMetadataLine::Attributes((attr_discrete, attr_metadata)) => {
                        let attr_metadata = *attr_metadata;
                        discrete = attr_discrete;
                        if attr_metadata.id.is_some() {
                            metadata.id = attr_metadata.id;
                        }
                        if attr_metadata.style.is_some() {
                            metadata.style = attr_metadata.style;
                        }
                        metadata.roles.extend(attr_metadata.roles);
                        metadata.options.extend(attr_metadata.options);
                        for (k, v) in attr_metadata.attributes.iter() {
                            metadata.attributes.insert(k.clone(), v.clone());
                        }
                        metadata.positional_attributes.extend(attr_metadata.positional_attributes);
                        if attr_metadata.substitutions.is_some() {
                            metadata.substitutions = attr_metadata.substitutions;
                        }
                        if attr_metadata.attribution.is_some() {
                            metadata.attribution = attr_metadata.attribution;
                        }
                        if attr_metadata.citetitle.is_some() {
                            metadata.citetitle = attr_metadata.citetitle;
                        }
                    },
                    BlockMetadataLine::DocumentAttribute(key, value) => {
                        // Set the document attribute immediately so it's available for
                        // subsequent attribute references (e.g., in title lines)
                        // Apply definition-time substitution (matching asciidoctor behavior)
                        let value = match value {
                            AttributeValue::String(s) => {
                                let substituted = substitute(&s, HEADER, &state.document_attributes);
                                AttributeValue::String(Cow::Borrowed(state.intern_str(&substituted)))
                            }
                            AttributeValue::Bool(_) | AttributeValue::None => value,
                        };
                        Rc::make_mut(&mut state.document_attributes).set(key, value);
                    },
                    BlockMetadataLine::Title(inner) => {
                        title = inner;
                    }
                }
            }
            if meta_start != meta_end {
                metadata.location = Some(state.create_block_location(meta_start, meta_end, offset));
            }
            let (macros_enabled, attributes_enabled) = if cfg!(feature = "pre-spec-subs") {
                (
                    metadata.substitutions.as_ref().is_none_or(|spec| !spec.macros_disabled()),
                    metadata.substitutions.as_ref().is_none_or(|spec| !spec.attributes_disabled()),
                )
            } else {
                (true, true)
            };
            Ok(BlockParsingMetadata {
                metadata,
                title,
                parent_section_level,
                macros_enabled,
                attributes_enabled,
            })
        }

        // A title line can be a simple title or a section title
        //
        // A title line is a line that starts with a period (.) followed by a non-whitespace character
        rule title_line(offset: usize) -> Result<Title<'input>, Error>
        = period() start:position!() title:$(![' ' | '\t' | '\n' | '\r' | '.'] [^'\n']*) end:position!() eol()
        {
            tracing::debug!(?title, ?start, ?end, "Found title line in block metadata");
            let block_metadata = BlockParsingMetadata::default();
            let title = process_inlines(state, &block_metadata, start, end, offset, title)?;
            Ok(title.into())
        }

        // A document attribute line in block metadata context
        // This allows document attributes to be set between block attributes and the block content
        // Uses the same parsing logic as document attributes in the header
        rule document_attribute_line() -> AttributeEntry<'input>
        = attr:document_attribute_match() eol()
        {
            tracing::debug!(?attr, "Found document attribute in block metadata");
            attr
        }

        rule section_level(offset: usize, parent_section_level: Option<SectionLevel>) -> (&'input str, SectionLevel)
        = start:position!() level:$(("=" / "#")*<1,6>) end:position!()
        {
            let base_level: SectionLevel = level.len().try_into().unwrap_or(1) - 1;
            let byte_offset = start + offset;
            (level, apply_leveloffset(base_level, byte_offset, &state.leveloffset_ranges, &state.document_attributes))
        }

        rule section_level_at_line_start(offset: usize, parent_section_level: Option<SectionLevel>) -> (&'input str, SectionLevel)
        = start:position!() level:$(("=" / "#")*<1,6>) end:position!()
        {?
            // This rule is invoked as a negative lookahead from paragraph
            // parsing, so it runs speculatively on every continuation line.
            // `position!()` captures only the byte offset — the cheap
            // line-start byte check below rejects most speculations, so the
            // (line, column) pair that `position()` would materialise is
            // virtually always discarded.
            let absolute_pos = start + offset;
            let at_line_start = absolute_pos == 0 || {
                let prev_byte_pos = absolute_pos.saturating_sub(1);
                state.input.as_bytes().get(prev_byte_pos).is_some_and(|&b| b == b'\n')
            };

            if !at_line_start {
                return Err("section level must be at line start");
            }

            let base_level: SectionLevel = level.len().try_into().unwrap_or(1) - 1;
            let byte_offset = start + offset;
            Ok((level, apply_leveloffset(base_level, byte_offset, &state.leveloffset_ranges, &state.document_attributes)))
        }

        rule section_title(offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Title<'input>, Error>
        = title_start:position!() title:$([^'\n']*) end:position!()
        {
            tracing::debug!(?title, ?title_start, ?end, offset, "Found section title");
            let content = process_inlines(state, block_metadata, title_start, end, offset, title)?;
            Ok(Title::new(content))
        }

        rule section_content(offset: usize, parent_section_level: Option<SectionLevel>) -> Result<Vec<Block<'input>>, Error>
        = blocks(offset, parent_section_level) / { Ok(vec![]) }

        pub(crate) rule block_generic(offset: usize, parent_section_level: Option<SectionLevel>) -> Result<Block<'input>, Error>
        = start:position!()
        block_metadata:(bm:block_metadata(offset, parent_section_level) {?
            bm.map_err(|e| {
                tracing::error!(?e, "error parsing block metadata in block_generic");
                "block metadata parse error"
            })
        })
        block:(
            delimited_block:delimited_block(start, offset, &block_metadata) { delimited_block }
            / image:image(start, offset, &block_metadata) { image }
            / audio:audio(start, offset, &block_metadata) { audio }
            / video:video(start, offset, &block_metadata) { video }
            / toc:toc(start, offset, &block_metadata) { toc }
            / thematic_break:thematic_break(start, offset, &block_metadata) { thematic_break }
            / page_break:page_break(start, offset, &block_metadata) { page_break }
            / list:list(start, offset, &block_metadata) { list }
            / quoted_paragraph:quoted_paragraph(start, offset, &block_metadata) { quoted_paragraph }
            / markdown_blockquote:markdown_blockquote(start, offset, &block_metadata) { markdown_blockquote }
            / paragraph:paragraph(start, offset, &block_metadata) { paragraph }
        ) {
            block
        }

        // Block parsing for continuation context - lists inside continuations cannot consume
        // further continuations (those belong to the parent item that started the continuation)
        rule block_in_continuation(offset: usize, parent_section_level: Option<SectionLevel>) -> Result<Block<'input>, Error>
        = start:position!()
        block_metadata:(bm:block_metadata(offset, parent_section_level) {?
            bm.map_err(|e| {
                tracing::error!(?e, "error parsing block metadata in block_in_continuation");
                "block metadata parse error"
            })
        })
        block:(
            delimited_block:delimited_block(start, offset, &block_metadata) { delimited_block }
            / image:image(start, offset, &block_metadata) { image }
            / audio:audio(start, offset, &block_metadata) { audio }
            / video:video(start, offset, &block_metadata) { video }
            / toc:toc(start, offset, &block_metadata) { toc }
            / thematic_break:thematic_break(start, offset, &block_metadata) { thematic_break }
            / page_break:page_break(start, offset, &block_metadata) { page_break }
            // Lists in continuation context cannot consume further continuations
            / list:list_with_continuation(start, offset, &block_metadata, false) { list }
            / quoted_paragraph:quoted_paragraph(start, offset, &block_metadata) { quoted_paragraph }
            / markdown_blockquote:markdown_blockquote(start, offset, &block_metadata) { markdown_blockquote }
            / paragraph:paragraph(start, offset, &block_metadata) { paragraph }
        ) {
            block
        }

        /// Block parsing for table cells without `AsciiDoc` style - excludes block types that require full parsing.
        /// Only `a` (`AsciiDoc`) style cells should have full block parsing.
        /// Excluded: delimited_block, list, toc, page_break, markdown_blockquote
        rule block_generic_for_table_cell(offset: usize, parent_section_level: Option<SectionLevel>) -> Result<Block<'input>, Error>
        = eol()*
        start:position!()
        block_metadata:(bm:block_metadata(offset, parent_section_level) {?
            bm.map_err(|e| {
                tracing::error!(?e, "error parsing block metadata in block_generic_for_table_cell");
                "block metadata parse error"
            })
        })
        block:(
            // NOTE: delimited_block is intentionally excluded - only valid with 'a' cell style
            image:image(start, offset, &block_metadata) { image }
            / audio:audio(start, offset, &block_metadata) { audio }
            / video:video(start, offset, &block_metadata) { video }
            / thematic_break:thematic_break(start, offset, &block_metadata) { thematic_break }
            / quoted_paragraph:quoted_paragraph(start, offset, &block_metadata) { quoted_paragraph }
            // NOTE: toc, page_break, list, markdown_blockquote are excluded - only valid with 'a' cell style
            / paragraph:paragraph(start, offset, &block_metadata) { paragraph }
        ) {
            block
        }

        rule delimited_block(
            start: usize,
            offset: usize,
            block_metadata: &BlockParsingMetadata<'input>,
        ) -> Result<Block<'input>, Error>
        = comment_block(start, offset, block_metadata)
        / example_block(start, offset, block_metadata)
        / listing_block(start, offset, block_metadata)
        / literal_block(start, offset, block_metadata)
        / open_block(start, offset, block_metadata)
        / sidebar_block(start, offset, block_metadata)
        / table_block(start, offset, block_metadata)
        / pass_block(start, offset, block_metadata)
        / quote_block(start, offset, block_metadata)

        // Delimiter recognition rules
        rule comment_delimiter() -> &'input str = delim:$("/"*<4,>) { delim }
        rule example_delimiter() -> &'input str = delim:$("="*<4,>) { delim }
        rule listing_delimiter() -> &'input str = delim:$("-"*<4,>) { delim }
        rule literal_delimiter() -> &'input str = delim:$("."*<4,>) { delim }
        rule open_delimiter() -> &'input str = delim:$("-"*<2,2> / "~"*<4,>) { delim }
        rule sidebar_delimiter() -> &'input str = delim:$("*"*<4,>) { delim }
        rule table_delimiter() -> &'input str = delim:$((['|' | ',' | ':' | '!'] "="*<3,>)) { delim }

        // Delimiter-specific table delimiter rules for nested table support.
        // PEG negative lookahead can't accept runtime parameters, so we need
        // separate rules for each delimiter type to correctly parse nested tables.
        rule pipe_table_delimiter() -> &'input str = delim:$("|" "="*<3,>) { delim }
        rule excl_table_delimiter() -> &'input str = delim:$("!" "="*<3,>) { delim }
        rule comma_table_delimiter() -> &'input str = delim:$("," "="*<3,>) { delim }
        rule colon_table_delimiter() -> &'input str = delim:$(":" "="*<3,>) { delim }

        rule pass_delimiter() -> &'input str = delim:$("+"*<4,>) { delim }
        rule markdown_code_delimiter() -> &'input str = delim:$("`"*<3,>) { delim }
        rule quote_delimiter() -> &'input str = delim:$("_"*<4,>) { delim }

        // Exact delimiter matching rules - these use conditional actions to ensure
        // the matched delimiter is identical to the expected one. This prevents
        // content lines that happen to contain delimiter-like characters (but of
        // different length) from being incorrectly treated as closing delimiters.
        rule exact_comment_delimiter(expected: &str) -> &'input str
            = delim:comment_delimiter() {? if delim == expected { Ok(delim) } else { Err("comment delimiter mismatch") } }
        rule exact_example_delimiter(expected: &str) -> &'input str
            = delim:example_delimiter() {? if delim == expected { Ok(delim) } else { Err("example delimiter mismatch") } }
        rule exact_listing_delimiter(expected: &str) -> &'input str
            = delim:listing_delimiter() {? if delim == expected { Ok(delim) } else { Err("listing delimiter mismatch") } }
        rule exact_literal_delimiter(expected: &str) -> &'input str
            = delim:literal_delimiter() {? if delim == expected { Ok(delim) } else { Err("literal delimiter mismatch") } }
        rule exact_open_delimiter(expected: &str) -> &'input str
            = delim:open_delimiter() {? if delim == expected { Ok(delim) } else { Err("open delimiter mismatch") } }
        rule exact_sidebar_delimiter(expected: &str) -> &'input str
            = delim:sidebar_delimiter() {? if delim == expected { Ok(delim) } else { Err("sidebar delimiter mismatch") } }
        rule exact_pass_delimiter(expected: &str) -> &'input str
            = delim:pass_delimiter() {? if delim == expected { Ok(delim) } else { Err("pass delimiter mismatch") } }
        rule exact_markdown_code_delimiter(expected: &str) -> &'input str
            = delim:markdown_code_delimiter() {? if delim == expected { Ok(delim) } else { Err("markdown code delimiter mismatch") } }
        rule exact_quote_delimiter(expected: &str) -> &'input str
            = delim:quote_delimiter() {? if delim == expected { Ok(delim) } else { Err("quote delimiter mismatch") } }

        rule until_comment_delimiter(expected: &str) -> &'input str
        = content:$((!(eol() exact_comment_delimiter(expected)) [_])*) { content }

        rule until_example_delimiter(expected: &str) -> &'input str
        = content:$((!(eol() exact_example_delimiter(expected)) [_])*) { content }

        rule until_listing_delimiter(expected: &str) -> &'input str
        = content:$((!(eol() exact_listing_delimiter(expected)) [_])*) { content }

        rule until_literal_delimiter(expected: &str) -> &'input str
        = content:$((!(eol() exact_literal_delimiter(expected)) [_])*) { content }

        rule until_open_delimiter(expected: &str) -> &'input str
        = content:$((!(eol() exact_open_delimiter(expected)) [_])*) { content }

        rule until_sidebar_delimiter(expected: &str) -> &'input str
        = content:$((!(eol() exact_sidebar_delimiter(expected)) [_])*) { content }

        rule until_table_delimiter() -> &'input str
        = content:$((!(eol() table_delimiter()) [_])*) { content }

        // Delimiter-specific content rules for nested table support.
        // Each rule only looks ahead for its specific delimiter, allowing
        // nested tables with different delimiters to be parsed correctly.
        rule until_pipe_table_delimiter() -> &'input str
        = content:$((!(eol() pipe_table_delimiter()) [_])*) { content }

        rule until_excl_table_delimiter() -> &'input str
        = content:$((!(eol() excl_table_delimiter()) [_])*) { content }

        rule until_comma_table_delimiter() -> &'input str
        = content:$((!(eol() comma_table_delimiter()) [_])*) { content }

        rule until_colon_table_delimiter() -> &'input str
        = content:$((!(eol() colon_table_delimiter()) [_])*) { content }

        rule until_pass_delimiter(expected: &str) -> &'input str
        = content:$((!(eol() exact_pass_delimiter(expected)) [_])*) { content }

        rule until_quote_delimiter(expected: &str) -> &'input str
        = content:$((!(eol() exact_quote_delimiter(expected)) [_])*) { content }

        rule until_markdown_code_delimiter(expected: &str) -> &'input str
        = content:$((!(eol() exact_markdown_code_delimiter(expected)) [_])*) { content }

        rule markdown_language() -> &'input str
        = lang:$((['a'..='z'] / ['A'..='Z'] / ['0'..='9'] / "_" / "+" / "-")+) { lang }

        rule example_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
        = open_start:position!() open_delim:example_delimiter() eol()
        content_start:position!() content:until_example_delimiter(open_delim) content_end:position!()
        eol() close_start:position!() close_delim:example_delimiter() end:position!()
        {
            tracing::debug!(?start, ?offset, ?content_start, ?block_metadata, ?content, "Parsing example block");

            check_delimiters(open_delim, close_delim, "example", state.create_error_source_location(state.create_block_location(start, end, offset)))?;
            let mut metadata = block_metadata.metadata.clone();
            metadata.move_positional_attributes_to_attributes();
            let location = state.create_block_location(start, end, offset);
            let open_delimiter_location = state.create_location(
                open_start + offset,
                open_start + offset + open_delim.len().saturating_sub(1),
            );
            let close_delimiter_location = state.create_block_location(close_start, end, offset);

            let blocks = if content.trim().is_empty() {
                Vec::new()
            } else {
                document_parser::blocks(content, state, content_start+offset, block_metadata.parent_section_level).unwrap_or_else(|e| {
                    adjust_and_log_parse_error(&e, content, content_start+offset, state, "Error parsing example content as blocks in example block");
                    Ok(Vec::new())
                })?
            };

            // We want to detect if this is an admonition block. We do that by checking if
            // we have a style that matches an admonition variant.
            if let Some(style) = block_metadata.metadata.style &&
            let Ok(admonition_variant) = AdmonitionVariant::from_str(style) {
                tracing::debug!(?admonition_variant, "Detected admonition block with variant");
                metadata.style = None; // Clear style to avoid confusion (reuse existing clone)
                return Ok(Block::Admonition(Admonition::new(admonition_variant, blocks, location).with_metadata(metadata).with_title(block_metadata.title.clone())));
            }

            Ok(Block::DelimitedBlock(DelimitedBlock {
                metadata, // Use the existing clone instead of cloning again
                delimiter: open_delim,
                inner: DelimitedBlockType::DelimitedExample(blocks),
                title: block_metadata.title.clone(),
                location,
                open_delimiter_location: Some(open_delimiter_location),
                close_delimiter_location: Some(close_delimiter_location),
            }))
        }

        rule comment_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
            = open_start:position!() open_delim:comment_delimiter() eol()
            content_start:position!() content:until_comment_delimiter(open_delim) content_end:position!()
            eol() close_start:position!() close_delim:comment_delimiter() end:position!()
        {
            check_delimiters(open_delim, close_delim, "comment", state.create_error_source_location(state.create_block_location(start, end, offset)))?;
            let mut metadata = block_metadata.metadata.clone();
            metadata.move_positional_attributes_to_attributes();

            let location = state.create_block_location(start, end, offset);
            let content_location = state.create_block_location(content_start, content_end, offset);
            let open_delimiter_location = state.create_location(
                open_start + offset,
                open_start + offset + open_delim.len().saturating_sub(1),
            );
            let close_delimiter_location = state.create_block_location(close_start, end, offset);

            Ok(Block::DelimitedBlock(DelimitedBlock {
                metadata,
                delimiter: open_delim,
                inner: DelimitedBlockType::DelimitedComment(vec![InlineNode::PlainText(Plain {
                    content,
                    location: content_location,
                    escaped: false,
                })]),
                title: block_metadata.title.clone(),
                location,
                open_delimiter_location: Some(open_delimiter_location),
                close_delimiter_location: Some(close_delimiter_location),
            }))
        }

        rule listing_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
            = traditional_listing_block(start, offset, block_metadata)
            / markdown_listing_block(start, offset, block_metadata)

        rule traditional_listing_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
            = open_start:position!() open_delim:listing_delimiter() eol()
            content_start:position!() content:until_listing_delimiter(open_delim) content_end:position!()
            eol() close_start:position!() close_delim:listing_delimiter() end:position!()
        {
            check_delimiters(open_delim, close_delim, "listing", state.create_error_source_location(state.create_block_location(start, end, offset)))?;
            let mut metadata = block_metadata.metadata.clone();
            metadata.move_positional_attributes_to_attributes();
            let location = state.create_block_location(start, end, offset);
            let content_location = state.create_block_location(content_start, content_end, offset);
            let open_delimiter_location = state.create_location(
                open_start + offset,
                open_start + offset + open_delim.len().saturating_sub(1),
            );
            let close_delimiter_location = state.create_block_location(close_start, end, offset);

            let (inlines, callouts) = resolve_verbatim_callouts(state.arena, content, content_location);
            state.last_block_was_verbatim = true;
            state.last_verbatim_callouts = callouts;

            Ok(Block::DelimitedBlock(DelimitedBlock {
                metadata: metadata.clone(),
                delimiter: open_delim,
                inner: DelimitedBlockType::DelimitedListing(inlines),
                title: block_metadata.title.clone(),
                location,
                open_delimiter_location: Some(open_delimiter_location),
                close_delimiter_location: Some(close_delimiter_location),
            }))
        }

        rule markdown_listing_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
            = open_start:position!() open_delim:markdown_code_delimiter() lang:markdown_language()? eol()
            content_start:position!() content:until_markdown_code_delimiter(open_delim) content_end:position!()
            eol() close_start:position!() close_delim:markdown_code_delimiter() end:position!()
        {
            check_delimiters(open_delim, close_delim, "listing", state.create_error_source_location(state.create_block_location(start, end, offset)))?;
            let mut metadata = block_metadata.metadata.clone();

            // If we captured a language, add it as a positional attribute and set style
            // to "source". This matches the behavior of [source,lang] blocks so that
            // detect_language() works.
            if let Some(language) = lang {
                metadata.positional_attributes.insert(0, language);
                metadata.style = Some("source");
            }

            metadata.move_positional_attributes_to_attributes();
            let location = state.create_block_location(start, end, offset);
            let content_location = state.create_block_location(content_start, content_end, offset);
            let open_delimiter_location = state.create_location(
                open_start + offset,
                open_start + offset + open_delim.len().saturating_sub(1),
            );
            let close_delimiter_location = state.create_block_location(close_start, end, offset);

            let (inlines, callouts) = resolve_verbatim_callouts(state.arena, content, content_location);
            state.last_block_was_verbatim = true;
            state.last_verbatim_callouts = callouts;

            Ok(Block::DelimitedBlock(DelimitedBlock {
                metadata: metadata.clone(),
                delimiter: open_delim,
                inner: DelimitedBlockType::DelimitedListing(inlines),
                title: block_metadata.title.clone(),
                location,
                open_delimiter_location: Some(open_delimiter_location),
                close_delimiter_location: Some(close_delimiter_location),
            }))
        }

        pub(crate) rule literal_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
        =
        open_start:position!()
        open_delim:literal_delimiter()
        eol()
        content_start:position!() content:until_literal_delimiter(open_delim) content_end:position!()
        eol()
        close_start:position!()
        close_delim:literal_delimiter()
        end:position!()
        {
            check_delimiters(open_delim, close_delim, "literal", state.create_error_source_location(state.create_block_location(start, end, offset)))?;
            let mut metadata = block_metadata.metadata.clone();
            metadata.move_positional_attributes_to_attributes();
            let location = state.create_block_location(start, end, offset);
            let content_location = state.create_block_location(content_start, content_end, offset);
            let open_delimiter_location = state.create_location(
                open_start + offset,
                open_start + offset + open_delim.len().saturating_sub(1),
            );
            let close_delimiter_location = state.create_block_location(close_start, end, offset);

            let (inlines, callouts) = resolve_verbatim_callouts(state.arena, content, content_location);
            state.last_block_was_verbatim = true;
            state.last_verbatim_callouts = callouts;

            Ok(Block::DelimitedBlock(DelimitedBlock {
                metadata,
                delimiter: open_delim,
                inner: DelimitedBlockType::DelimitedLiteral(inlines),
                title: block_metadata.title.clone(),
                location,
                open_delimiter_location: Some(open_delimiter_location),
                close_delimiter_location: Some(close_delimiter_location),
            }))
        }

        rule open_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
            = open_start:position!() open_delim:open_delimiter() eol()
            content_start:position!() content:until_open_delimiter(open_delim) content_end:position!()
            eol() close_start:position!() close_delim:open_delimiter() end:position!()
        {
            check_delimiters(open_delim, close_delim, "open", state.create_error_source_location(state.create_block_location(start, end, offset)))?;
            let mut metadata = block_metadata.metadata.clone();
            metadata.move_positional_attributes_to_attributes();
            let location = state.create_block_location(start, end, offset);
            let open_delimiter_location = state.create_location(
                open_start + offset,
                open_start + offset + open_delim.len().saturating_sub(1),
            );
            let close_delimiter_location = state.create_block_location(close_start, end, offset);

            let blocks = if content.trim().is_empty() {
                Vec::new()
            } else {
                document_parser::blocks(content, state, content_start+offset, block_metadata.parent_section_level).unwrap_or_else(|e| {
                    adjust_and_log_parse_error(&e, content, content_start+offset, state, "Error parsing content as blocks in open block");
                    Ok(Vec::new())
                })?
            };

            Ok(Block::DelimitedBlock(DelimitedBlock {
                metadata: metadata.clone(),
                delimiter: open_delim,
                inner: DelimitedBlockType::DelimitedOpen(blocks),
                title: block_metadata.title.clone(),
                location,
                open_delimiter_location: Some(open_delimiter_location),
                close_delimiter_location: Some(close_delimiter_location),
            }))
        }

        rule sidebar_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
            = open_start:position!() open_delim:sidebar_delimiter() eol()
            content_start:position!() content:until_sidebar_delimiter(open_delim) content_end:position!()
            eol() close_start:position!() close_delim:sidebar_delimiter() end:position!()
        {
            tracing::debug!(?start, ?offset, ?content_start, ?block_metadata, ?content, "Parsing sidebar block");

            check_delimiters(open_delim, close_delim, "sidebar", state.create_error_source_location(state.create_block_location(start, end, offset)))?;
            let mut metadata = block_metadata.metadata.clone();
            metadata.move_positional_attributes_to_attributes();
            let location = state.create_block_location(start, end, offset);
            let open_delimiter_location = state.create_location(
                open_start + offset,
                open_start + offset + open_delim.len().saturating_sub(1),
            );
            let close_delimiter_location = state.create_block_location(close_start, end, offset);

            let blocks = if content.trim().is_empty() {
                Vec::new()
            } else {
                document_parser::blocks(content, state, content_start+offset, block_metadata.parent_section_level).unwrap_or_else(|e| {
                    adjust_and_log_parse_error(&e, content, content_start+offset, state, "Error parsing sidebar content as blocks");
                    Ok(Vec::new())
                })?
            };

            Ok(Block::DelimitedBlock(DelimitedBlock {
                metadata: metadata.clone(),
                delimiter: open_delim,
                inner: DelimitedBlockType::DelimitedSidebar(blocks),
                title: block_metadata.title.clone(),
                location,
                open_delimiter_location: Some(open_delimiter_location),
                close_delimiter_location: Some(close_delimiter_location),
            }))
        }

        // Table block dispatcher - tries each delimiter-specific variant in order.
        // This enables nested tables: |=== outer can contain !=== inner because
        // each rule only looks for its own closing delimiter.
        //
        // Terminated variants are tried first; unterminated fallbacks only match
        // when an opening delimiter runs to end-of-input without a close.
        rule table_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
            = pipe_table_block(start, offset, block_metadata)
            / excl_table_block(start, offset, block_metadata)
            / comma_table_block(start, offset, block_metadata)
            / colon_table_block(start, offset, block_metadata)
            / unterminated_pipe_table_block(start, offset, block_metadata)
            / unterminated_excl_table_block(start, offset, block_metadata)
            / unterminated_comma_table_block(start, offset, block_metadata)
            / unterminated_colon_table_block(start, offset, block_metadata)

        rule pipe_table_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
            = table_start:position!() open_delim:pipe_table_delimiter() eol()
              content_start:position!() content:until_pipe_table_delimiter() content_end:position!()
              eol() close_start:position!() close_delim:pipe_table_delimiter() end:position!()
        {
            parse_table_block_impl(
                &TableParseParams {
                    start, offset, table_start, content_start, content_end, end,
                    open_delim, content, default_separator: "|",
                    closing: TableClosing::Terminated { close_delim, close_start },
                },
                state,
                block_metadata,
            )
        }

        rule excl_table_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
            = table_start:position!() open_delim:excl_table_delimiter() eol()
              content_start:position!() content:until_excl_table_delimiter() content_end:position!()
              eol() close_start:position!() close_delim:excl_table_delimiter() end:position!()
        {
            parse_table_block_impl(
                &TableParseParams {
                    start, offset, table_start, content_start, content_end, end,
                    open_delim, content, default_separator: "!",
                    closing: TableClosing::Terminated { close_delim, close_start },
                },
                state,
                block_metadata,
            )
        }

        rule comma_table_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
            = table_start:position!() open_delim:comma_table_delimiter() eol()
              content_start:position!() content:until_comma_table_delimiter() content_end:position!()
              eol() close_start:position!() close_delim:comma_table_delimiter() end:position!()
        {
            parse_table_block_impl(
                &TableParseParams {
                    start, offset, table_start, content_start, content_end, end,
                    open_delim, content, default_separator: ",",
                    closing: TableClosing::Terminated { close_delim, close_start },
                },
                state,
                block_metadata,
            )
        }

        rule colon_table_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
            = table_start:position!() open_delim:colon_table_delimiter() eol()
              content_start:position!() content:until_colon_table_delimiter() content_end:position!()
              eol() close_start:position!() close_delim:colon_table_delimiter() end:position!()
        {
            parse_table_block_impl(
                &TableParseParams {
                    start, offset, table_start, content_start, content_end, end,
                    open_delim, content, default_separator: ":",
                    closing: TableClosing::Terminated { close_delim, close_start },
                },
                state,
                block_metadata,
            )
        }

        // Unterminated table fallbacks: match an opening table delimiter
        // that runs to end-of-input without a closing delimiter. These
        // alternatives are tried only after all terminated variants fail,
        // so a document with a valid close never takes this path. When
        // taken, `parse_table_block_impl` emits an `UnterminatedTable`
        // warning and still produces a table, matching asciidoctor's
        // recovery behavior.
        //
        // The `(eol() / ![_])` after the open delimiter accepts both
        // `|===\n...` and `|===<EOF>`: the preprocessor's `normalize`
        // strips a single trailing newline (mirroring `str::lines`), so a
        // file ending with just `|===\n` reaches the grammar as `|===`.
        rule unterminated_pipe_table_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
            = table_start:position!() open_delim:pipe_table_delimiter() (eol() / ![_])
              content_start:position!() content:until_pipe_table_delimiter() content_end:position!()
              end:position!() ![_]
        {
            parse_table_block_impl(
                &TableParseParams {
                    start, offset, table_start, content_start, content_end, end,
                    open_delim, content, default_separator: "|",
                    closing: TableClosing::Unterminated,
                },
                state,
                block_metadata,
            )
        }

        rule unterminated_excl_table_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
            = table_start:position!() open_delim:excl_table_delimiter() (eol() / ![_])
              content_start:position!() content:until_excl_table_delimiter() content_end:position!()
              end:position!() ![_]
        {
            parse_table_block_impl(
                &TableParseParams {
                    start, offset, table_start, content_start, content_end, end,
                    open_delim, content, default_separator: "!",
                    closing: TableClosing::Unterminated,
                },
                state,
                block_metadata,
            )
        }

        rule unterminated_comma_table_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
            = table_start:position!() open_delim:comma_table_delimiter() (eol() / ![_])
              content_start:position!() content:until_comma_table_delimiter() content_end:position!()
              end:position!() ![_]
        {
            parse_table_block_impl(
                &TableParseParams {
                    start, offset, table_start, content_start, content_end, end,
                    open_delim, content, default_separator: ",",
                    closing: TableClosing::Unterminated,
                },
                state,
                block_metadata,
            )
        }

        rule unterminated_colon_table_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
            = table_start:position!() open_delim:colon_table_delimiter() (eol() / ![_])
              content_start:position!() content:until_colon_table_delimiter() content_end:position!()
              end:position!() ![_]
        {
            parse_table_block_impl(
                &TableParseParams {
                    start, offset, table_start, content_start, content_end, end,
                    open_delim, content, default_separator: ":",
                    closing: TableClosing::Unterminated,
                },
                state,
                block_metadata,
            )
        }

        rule pass_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
            = open_start:position!() open_delim:pass_delimiter() eol()
            content_start:position!() content:until_pass_delimiter(open_delim) content_end:position!()
            eol() close_start:position!() close_delim:pass_delimiter() end:position!()
        {
            check_delimiters(open_delim, close_delim, "pass", state.create_error_source_location(state.create_block_location(start, end, offset)))?;
            let mut metadata = block_metadata.metadata.clone();
            metadata.move_positional_attributes_to_attributes();
            let location = state.create_block_location(start, end, offset);
            let content_location = state.create_block_location(content_start, content_end, offset);
            let open_delimiter_location = state.create_location(
                open_start + offset,
                open_start + offset + open_delim.len().saturating_sub(1),
            );
            let close_delimiter_location = state.create_block_location(close_start, end, offset);

            // Check if this is a stem block
            let inner = if let Some(style) = metadata.style {
                if style == "stem" {
                    // Get notation from :stem: document attribute
                    let notation = match state.document_attributes.get("stem") {
                        Some(AttributeValue::String(s)) => {
                            StemNotation::from_str(s).unwrap_or(StemNotation::Latexmath)
                        }
                        Some(AttributeValue::Bool(true) | AttributeValue::None) => {
                            StemNotation::Latexmath
                        }
                        _ => StemNotation::Latexmath,
                    };
                    metadata.style = None; // Clear style to avoid confusion
                    DelimitedBlockType::DelimitedStem(StemContent {
                        content,
                        notation,
                    })
                } else {
                    DelimitedBlockType::DelimitedPass(vec![InlineNode::RawText(Raw {
                        content,
                        location: content_location,
                        subs: vec![],
                    })])
                }
            } else {
                DelimitedBlockType::DelimitedPass(vec![InlineNode::RawText(Raw {
                    content,
                    location: content_location,
                    subs: vec![],
                })])
            };

            Ok(Block::DelimitedBlock(DelimitedBlock {
                metadata: metadata.clone(),
                delimiter: open_delim,
                inner,
                title: block_metadata.title.clone(),
                location,
                open_delimiter_location: Some(open_delimiter_location),
                close_delimiter_location: Some(close_delimiter_location),
            }))
        }

        rule quote_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
            = open_start:position!() open_delim:quote_delimiter() eol()
            content_start:position!() content:until_quote_delimiter(open_delim) content_end:position!()
            eol() close_start:position!() close_delim:quote_delimiter() end:position!()
        {
            // Parse attribution/citetitle through the inline pipeline so that URLs,
            // macros, and other inline markup are properly resolved (#373).
            // Only re-parse if the content contains characters that suggest
            // inline markup is present (URLs, macros, formatting, etc.).
            fn needs_inline_processing(content: &str) -> bool {
                content.contains("://") || content.contains('[') || content.contains('{')
                    || content.contains('*') || content.contains('_') || content.contains('`')
                    || content.contains("<<") || content.contains("link:") || content.contains("mailto:")
            }

            check_delimiters(open_delim, close_delim, "quote", state.create_error_source_location(state.create_block_location(start, end, offset)))?;
            let mut metadata = block_metadata.metadata.clone();
            metadata.move_positional_attributes_to_attributes();
            let location = state.create_block_location(start, end, offset);
            let content_location = state.create_block_location(content_start, content_end, offset);
            let open_delimiter_location = state.create_location(
                open_start + offset,
                open_start + offset + open_delim.len().saturating_sub(1),
            );
            let close_delimiter_location = state.create_block_location(close_start, end, offset);

            // Collect params first (releases the borrow on metadata.attribution
            // before we reassign below).
            let attribution_params = if let Some(ref attr) = metadata.attribution
            && let Some(InlineNode::PlainText(plain)) = attr.first()
            && needs_inline_processing(plain.content)
            {
                let attr_pos = PositionWithOffset {
                    offset: plain.location.absolute_start.saturating_sub(offset),
                    position: plain.location.start.clone(),
                };
                let attr_end = plain.location.absolute_end.saturating_sub(offset);
                let content: &'input str = plain.content;
                Some((content, attr_pos, attr_end))
            } else { None };
            if let Some((content, attr_pos, attr_end)) = attribution_params
                && let Ok(inlines) = process_inlines(state, block_metadata, attr_pos.offset, attr_end, offset, content)
                && !inlines.is_empty()
            {
                metadata.attribution = Some(Attribution::new(inlines));
            }

            let citetitle_params = if let Some(ref cite) = metadata.citetitle
            && let Some(InlineNode::PlainText(plain)) = cite.first()
            && needs_inline_processing(plain.content)
            {
                let cite_pos = PositionWithOffset {
                    offset: plain.location.absolute_start.saturating_sub(offset),
                    position: plain.location.start.clone(),
                };
                let cite_end = plain.location.absolute_end.saturating_sub(offset);
                let content: &'input str = plain.content;
                Some((content, cite_pos, cite_end))
            } else { None };
            if let Some((content, cite_pos, cite_end)) = citetitle_params
                && let Ok(inlines) = process_inlines(state, block_metadata, cite_pos.offset, cite_end, offset, content)
                && !inlines.is_empty()
            {
                metadata.citetitle = Some(CiteTitle::new(inlines));
            }

            let inner = if let Some(style) = metadata.style {
                if style == "verse" {
                    DelimitedBlockType::DelimitedVerse(vec![InlineNode::PlainText(Plain {
                        content,
                        location: content_location.clone(),
                        escaped: false,
                    })])
                } else {
                    let blocks = document_parser::blocks(content, state, content_start+offset, block_metadata.parent_section_level).unwrap_or_else(|e| {
                        adjust_and_log_parse_error(&e, content, content_start+offset, state, "Error parsing example content as blocks in quote block");
                        Ok(Vec::new())
                    })?;
                    DelimitedBlockType::DelimitedQuote(blocks)
                }
            } else {
                let blocks = if content.trim().is_empty() {
                    Vec::new()
                } else {
                    document_parser::blocks(content, state, content_start+offset, block_metadata.parent_section_level).unwrap_or_else(|e| {
                        adjust_and_log_parse_error(&e, content, content_start+offset, state, "Error parsing content as blocks in quote block");
                        Ok(Vec::new())
                    })?
                };
                DelimitedBlockType::DelimitedQuote(blocks)
            };

            Ok(Block::DelimitedBlock(DelimitedBlock {
                metadata: metadata.clone(),
                delimiter: open_delim,
                inner,
                title: block_metadata.title.clone(),
                location,
                open_delimiter_location: Some(open_delimiter_location),
                close_delimiter_location: Some(close_delimiter_location),
            }))
        }

        rule toc(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
        = "toc::" attributes:attributes() end:position!()
          trailing:$([^'\n']*)
        {
            let (_discrete, metadata_from_attributes, _title_position) = attributes;
            let mut metadata = block_metadata.metadata.clone();
            metadata.merge(&metadata_from_attributes);
            metadata.move_positional_attributes_to_attributes();
            state.warn_trailing_macro_content("toc", trailing, end, offset);
            tracing::debug!("Found Table of Contents block");
            Ok(Block::TableOfContents(TableOfContents {
                metadata,
                location: state.create_location(start+offset, end+offset),
            }))
        }

        rule image(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
        = "image::" source:source() attributes:macro_attributes() end:position!()
          trailing:$([^'\n']*)
        {
            state.warn_trailing_macro_content("image", trailing, end, offset);
            let (_discrete, metadata_from_attributes, _title_position) = attributes;
            let title = block_metadata.title.clone();
            let mut metadata = block_metadata.metadata.clone();
            metadata.merge(&metadata_from_attributes);
            if let Some(style) = metadata.style {
                metadata.style = None; // Clear style to avoid confusion
                metadata.attributes.insert("alt".into(), AttributeValue::String(Cow::Borrowed(style)));
            }
            if metadata.positional_attributes.len() >= 2 {
                metadata.attributes.insert("height".into(), AttributeValue::String(Cow::Borrowed(metadata.positional_attributes.remove(1))));
            }
            if !metadata.positional_attributes.is_empty() {
                metadata.attributes.insert("width".into(), AttributeValue::String(Cow::Borrowed(metadata.positional_attributes.remove(0))));
            }
            metadata.move_positional_attributes_to_attributes();
            Ok(Block::Image(Image {
                title,
                source,
                metadata,
                location: state.create_block_location(start, end, offset),

            }))
        }

        rule audio(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
        = "audio::" source:source() attributes:macro_attributes() end:position!()
          trailing:$([^'\n']*)
        {
            state.warn_trailing_macro_content("audio", trailing, end, offset);
            let (_discrete, metadata_from_attributes, _title_position) = attributes;
            let title = block_metadata.title.clone();
            let mut metadata = block_metadata.metadata.clone();
            metadata.merge(&metadata_from_attributes);
            metadata.move_positional_attributes_to_attributes();
            Ok(Block::Audio(Audio {
                title,
                source,
                metadata,
                location: state.create_block_location(start, end, offset),
            }))
        }

        // The video block is similar to the audio and image blocks, but it supports
        // multiple sources. This is for example to allow passing multiple youtube video
        // ids to form a playlist.
        rule video(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
        = "video::" sources:(source() ** comma()) attributes:macro_attributes() end:position!()
          trailing:$([^'\n']*)
        {
            state.warn_trailing_macro_content("video", trailing, end, offset);
            let (_discrete, metadata_from_attributes, _title_position) = attributes;
            let title = block_metadata.title.clone();
            let mut metadata = block_metadata.metadata.clone();
            metadata.merge(&metadata_from_attributes);
            if let Some(style) = metadata.style {
                metadata.style = None;
                if style == "youtube" || style == "vimeo" {
                    tracing::debug!(?metadata, "transforming video metadata style into attribute");
                    metadata.attributes.insert(Cow::Borrowed(style), AttributeValue::Bool(true));
                } else {
                    // assume poster
                    tracing::debug!(?metadata, "transforming video metadata style into attribute, assuming poster");
                    metadata.attributes.insert("poster".into(), AttributeValue::String(Cow::Borrowed(style)));
                }
            }
            if metadata.positional_attributes.len() >= 2 {
                metadata.attributes.insert("height".into(), AttributeValue::String(Cow::Borrowed(metadata.positional_attributes.remove(1))));
            }
            if !metadata.positional_attributes.is_empty() {
                metadata.attributes.insert("width".into(), AttributeValue::String(Cow::Borrowed(metadata.positional_attributes.remove(0))));
            }
            metadata.move_positional_attributes_to_attributes();
            Ok(Block::Video(Video {
                title,
                sources,
                metadata,
                location: state.create_block_location(start, end, offset),
            }))
        }

        rule thematic_break(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
            = ("'''"
               // Below are the markdown-style thematic breaks
               / "---"
               / "- - -"
               / "***"
               / "* * *"
            ) end:position!()
        {
            tracing::debug!("Found thematic break block");
            Ok(Block::ThematicBreak(ThematicBreak {
                anchors: block_metadata.metadata.anchors.clone(), // TODO(nlopes): should this simply be metadata?
                title: block_metadata.title.clone(),
                location: state.create_block_location(start, end, offset),
            }))
        }

        rule page_break(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
            = "<<<" end:position!() &eol()*<2,2>
        {
            tracing::debug!("Found page break block");
            let mut metadata = block_metadata.metadata.clone();
            metadata.move_positional_attributes_to_attributes();

            Ok(Block::PageBreak(PageBreak {
                title: block_metadata.title.clone(),
                metadata,
                location: state.create_location(start+offset, end+offset),
            }))
        }

        rule list(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
        = list_with_continuation(start, offset, block_metadata, true)

        // Parameterized list rule - allow_continuation controls whether list items can consume
        // explicit continuations. Set to false when parsing lists inside continuation blocks
        // to prevent nested lists from consuming parent-level continuations.
        rule list_with_continuation(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>, allow_continuation: bool) -> Result<Block<'input>, Error>
        = callout_list(start, offset, block_metadata)
        / unordered_list(start, offset, block_metadata, None, allow_continuation, false)
        / ordered_list(start, offset, block_metadata, None, allow_continuation, false)
        / description_list(start, offset, block_metadata)

        rule unordered_list_marker() -> &'input str = $("*"+ / "-")

        rule ordered_list_marker() -> &'input str = $(digits()? "."+)

        rule description_list_marker() -> &'input str = $("::::" / ":::" / "::" / ";;")

        rule callout_list_marker() -> &'input str = $("<" (digits() / ".") ">")

        rule section_level_marker() -> &'input str = $(("=" / "#")+)

        // Helper rule to check if we're at the start of a new list item (lookahead)
        rule at_list_item_start() = whitespace()* (unordered_list_marker() / ordered_list_marker()) whitespace()

        // Helper rule to check if we're at the start of a section heading (lookahead)
        // This is used to terminate list continuations when a section follows
        rule at_section_start() = (anchor() / attributes_line())* ("=" / "#")+ " "

        // Helper rule to check if we're at an ordered list marker ahead (after newlines)
        rule at_ordered_marker_ahead() = eol()+ whitespace()* ordered_list_marker()

        // Helper rule to check if we're at an unordered list marker ahead (after newlines)
        rule at_unordered_marker_ahead() = eol()+ whitespace()* unordered_list_marker()

        // Helper rule to check if we're at a root-level (non-indented) ordered marker (current position)
        rule at_root_ordered_marker() = !whitespace() ordered_list_marker()

        // Helper rule to check if we're at a root-level (non-indented) unordered marker (current position)
        rule at_root_unordered_marker() = !whitespace() unordered_list_marker()

        // Helper rule to check if we're at an ancestor-level ordered marker
        // Used in cross-type nesting to prevent consuming sibling ordered markers
        // that belong to a parent ordered list context
        rule at_ancestor_ordered_marker(ancestor: Option<&'input str>)
        = whitespace()* marker:ordered_list_marker() whitespace() {?
            match ancestor {
                Some(m) if marker.len() <= m.len() => Ok(()),
                _ => Err("not ancestor")
            }
        }

        // Helper rule to check if we're at an ancestor-level unordered marker
        // Used in cross-type nesting to prevent consuming sibling unordered markers
        // that belong to a parent unordered list context
        rule at_ancestor_unordered_marker(ancestor: Option<&'input str>)
        = whitespace()* marker:unordered_list_marker() whitespace() {?
            match ancestor {
                Some(m) if marker.len() <= m.len() => Ok(()),
                _ => Err("not ancestor")
            }
        }

        // Helper rule to check if we're at a shallower unordered marker
        // Used to terminate nested lists when a blank line precedes a shallower item
        // Same-level markers continue the list as siblings; only shallower markers end it
        rule at_shallower_unordered_marker(base_marker: &str)
        = whitespace()* marker:unordered_list_marker() whitespace() {?
            if marker.len() < base_marker.len() { Ok(()) } else { Err("same-or-deeper") }
        }

        // Helper rule to check if we're at a shallower ordered marker
        // Used to terminate nested lists when a blank line precedes a shallower item
        // Same-level markers continue the list as siblings; only shallower markers end it
        rule at_shallower_ordered_marker(base_marker: &str)
        = whitespace()* marker:ordered_list_marker() whitespace() {?
            if marker.len() < base_marker.len() { Ok(()) } else { Err("same-or-deeper") }
        }

        // Helper rule to check if we're at a deeper unordered marker (for nested same-type lists)
        // Used by unordered_list_item_nested_content to detect nested unordered lists
        rule at_deeper_unordered_marker(base_marker: &str)
        = whitespace()* marker:unordered_list_marker() whitespace() {?
            if marker.len() > base_marker.len() { Ok(()) } else { Err("same-or-shallower") }
        }

        // Helper rule to check if we're at a deeper ordered marker (for nested same-type lists)
        // Used by ordered_list_item_nested_content to detect nested ordered lists
        rule at_deeper_ordered_marker(base_marker: &str)
        = whitespace()* marker:ordered_list_marker() whitespace() {?
            if marker.len() > base_marker.len() { Ok(()) } else { Err("same-or-shallower") }
        }

        // Helper rule to check if we're at a list separator (forces list termination)
        // Matches either a line comment (//) or empty block attributes ([]) on their own line
        // Note: Separator must be preceded by at least one blank line (2+ newlines)
        // Without a blank line before it, a comment is just skipped, not a separator
        rule at_list_separator()
        = eol()*<2,> at_list_separator_content()

        // Helper rule to check for separator content at current position (no leading newlines)
        // Used by continuation_lines to stop at separators
        rule at_list_separator_content()
        = "//" [^'\n']* (&eol() / ![_])  // Line comment separator
        / whitespace()* "[" whitespace()* "]" whitespace()* (&eol() / ![_])  // Empty block attributes

        // Helper rule to check if we're at a blank line followed by block attributes or anchor
        // Used by description lists to terminate when new block metadata appears after a blank line
        // This signals a new block context where the attributes/anchor should apply to a new list
        // Matches: 2+ newlines, then either:
        //   - `[` at column 1 followed by non-empty content and `]` (block attributes)
        //   - `[[` at column 1 followed by content and `]]` (anchor/id)
        // Note: NO whitespace before `[` - indented brackets are not block metadata
        rule at_dlist_block_boundary()
        = eol()*<2,> &(
            ("[" ![']' | '['] [^']' | '\n']+ "]" whitespace()* eol())
            / ("[[" [^']']+ "]]" whitespace()* eol())
        )

        rule unordered_list(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>, parent_ordered_marker: Option<&'input str>, allow_continuation: bool, is_nested: bool) -> Result<Block<'input>, Error>
        // Parse whitespace + marker first to capture base_marker for rest items
        // marker_start captures position before marker for correct first item location
        = whitespace()* marker_start:position!() base_marker:$(unordered_list_marker()) &whitespace()
        first:unordered_list_item_after_marker(offset, block_metadata, allow_continuation, base_marker, marker_start, parent_ordered_marker)
        rest:(unordered_list_rest_item(offset, block_metadata, parent_ordered_marker, allow_continuation, base_marker))*
        end:position!()
        {
            tracing::debug!("Found unordered list block");
            let mut content = vec![first?];
            for item in rest {
                content.push(item?);
            }
            let end = content.last().map_or(end, |(_, item_end)| *item_end);
            let items: Vec<ListItem<'input>> = content.into_iter().map(|(item, _)| item).collect();
            let marker = items.first().map_or("", |item| item.marker);

            Ok(Block::UnorderedList(UnorderedList {
                title: if is_nested { Title::default() } else { block_metadata.title.clone() },
                metadata: if is_nested { BlockMetadata::default() } else { block_metadata.metadata.clone() },
                items,
                marker,
                location: state.create_location(start+offset, end+offset),
            }))
        }

        // Parse first item content after marker has been consumed by unordered_list
        // marker_start is the position where the marker began, for correct location tracking
        rule unordered_list_item_after_marker(offset: usize, block_metadata: &BlockParsingMetadata<'input>, allow_continuation: bool, marker: &'input str, marker_start: usize, parent_ordered_marker: Option<&'input str>) -> Result<(ListItem<'input>, usize), Error>
        = item:unordered_list_item_with_continuation_after_marker(offset, block_metadata, marker, marker_start, parent_ordered_marker) {? if allow_continuation { Ok(item) } else { Err("skip") } }
        / item:unordered_list_item_no_continuation_after_marker(offset, block_metadata, marker, marker_start, parent_ordered_marker) { item }

        // Zero-cost guards for the front-of-alternative branch selector in
        // `*_list_rest_item`. Keeps the expensive item parse out of the branch
        // whose trailing semantic action would have just discarded it.
        rule parent_is_some(parent: Option<&'input str>) -> ()
        = {? if parent.is_some() { Ok(()) } else { Err("parent_is_none") } }

        rule parent_is_none(parent: Option<&'input str>) -> ()
        = {? if parent.is_none() { Ok(()) } else { Err("parent_is_some") } }

        rule unordered_list_rest_item(offset: usize, block_metadata: &BlockParsingMetadata<'input>, parent_ordered_marker: Option<&'input str>, allow_continuation: bool, base_marker: &str) -> Result<(ListItem<'input>, usize), Error>
        // `parent_ordered_marker` is fixed for the whole `unordered_list` call, so
        // rather than parse the (expensive) item first and reject via a trailing
        // `{? }` action on three of four alternatives, guard each alternative at
        // the front with a zero-cost check and only parse when the branch applies.
        // The `!at_ordered_marker_ahead()` lookahead is kept only in the
        // `parent_ordered_marker.is_some()` branch where it actually pays off.
        // See fixtures: nested_unordered_in_ordered.adoc, nested_ordered_in_unordered.adoc
        //
        // Branch: parent is ordered
        = parent_is_some(parent_ordered_marker) !at_list_separator() !eol() comment_line()* !at_ordered_marker_ahead() item:unordered_list_item(offset, block_metadata, allow_continuation, parent_ordered_marker)
          { item }
        / parent_is_some(parent_ordered_marker) !at_list_separator() eol()+ comment_line()* !at_shallower_unordered_marker(base_marker) !at_ordered_marker_ahead() item:unordered_list_item(offset, block_metadata, allow_continuation, parent_ordered_marker)
          { item }
        // Branch: no ordered parent
        / parent_is_none(parent_ordered_marker) !at_list_separator() !eol() comment_line()* item:unordered_list_item(offset, block_metadata, allow_continuation, parent_ordered_marker)
          { item }
        / parent_is_none(parent_ordered_marker) !at_list_separator() eol()+ comment_line()* !at_shallower_unordered_marker(base_marker) item:unordered_list_item(offset, block_metadata, allow_continuation, parent_ordered_marker)
          { item }

        rule ordered_list(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>, parent_unordered_marker: Option<&'input str>, allow_continuation: bool, is_nested: bool) -> Result<Block<'input>, Error>
        // Parse whitespace + marker first to capture base_marker for rest items
        // marker_start captures position before marker for correct first item location
        = whitespace()* marker_start:position!() base_marker:$(ordered_list_marker()) &whitespace()
        first:ordered_list_item_after_marker(offset, block_metadata, allow_continuation, base_marker, marker_start, parent_unordered_marker)
        rest:(ordered_list_rest_item(offset, block_metadata, parent_unordered_marker, allow_continuation, base_marker))*
        end:position!()
        {
            tracing::debug!("Found ordered list block");
            let mut content = vec![first?];
            for item in rest {
                content.push(item?);
            }
            let end = content.last().map_or(end, |(_, item_end)| *item_end);
            let items: Vec<ListItem<'input>> = content.into_iter().map(|(item, _)| item).collect();
            let marker = items.first().map_or("", |item| item.marker);

            Ok(Block::OrderedList(OrderedList {
                title: if is_nested { Title::default() } else { block_metadata.title.clone() },
                metadata: if is_nested { BlockMetadata::default() } else { block_metadata.metadata.clone() },
                items,
                marker,
                location: state.create_location(start+offset, end+offset),
            }))
        }

        // Parse first item content after marker has been consumed by ordered_list
        // marker_start is the position where the marker began, for correct location tracking
        rule ordered_list_item_after_marker(offset: usize, block_metadata: &BlockParsingMetadata<'input>, allow_continuation: bool, marker: &'input str, marker_start: usize, parent_unordered_marker: Option<&'input str>) -> Result<(ListItem<'input>, usize), Error>
        = item:ordered_list_item_with_continuation_after_marker(offset, block_metadata, marker, marker_start, parent_unordered_marker) {? if allow_continuation { Ok(item) } else { Err("skip") } }
        / item:ordered_list_item_no_continuation_after_marker(offset, block_metadata, marker, marker_start, parent_unordered_marker) { item }

        rule ordered_list_rest_item(offset: usize, block_metadata: &BlockParsingMetadata<'input>, parent_unordered_marker: Option<&'input str>, allow_continuation: bool, base_marker: &str) -> Result<(ListItem<'input>, usize), Error>
        // Mirror of `unordered_list_rest_item`'s front-guard structure. See that
        // rule's comment for the rationale.
        //
        // Branch: parent is unordered
        = parent_is_some(parent_unordered_marker) !at_list_separator() !eol() comment_line()* !at_unordered_marker_ahead() item:ordered_list_item(offset, block_metadata, allow_continuation, parent_unordered_marker)
          { item }
        / parent_is_some(parent_unordered_marker) !at_list_separator() eol()+ comment_line()* !at_shallower_ordered_marker(base_marker) !at_unordered_marker_ahead() item:ordered_list_item(offset, block_metadata, allow_continuation, parent_unordered_marker)
          { item }
        // Branch: no unordered parent
        / parent_is_none(parent_unordered_marker) !at_list_separator() !eol() comment_line()* item:ordered_list_item(offset, block_metadata, allow_continuation, parent_unordered_marker)
          { item }
        / parent_is_none(parent_unordered_marker) !at_list_separator() eol()+ comment_line()* !at_shallower_ordered_marker(base_marker) item:ordered_list_item(offset, block_metadata, allow_continuation, parent_unordered_marker)
          { item }

        // Note: The `*_with_continuation` and `*_no_continuation` variants exist because
        // PEG parsers are greedy - nested items must NOT consume explicit continuations
        // that belong to their parent. Attempting to handle this in semantic actions
        // (by always parsing continuations then discarding them) would consume input
        // needed by the parent rule. This structural duplication is intentional.
        rule unordered_list_item(offset: usize, block_metadata: &BlockParsingMetadata<'input>, allow_continuation: bool, parent_ordered_marker: Option<&'input str>) -> Result<(ListItem<'input>, usize), Error>
        = item:unordered_list_item_with_continuation(offset, block_metadata, parent_ordered_marker) {? if allow_continuation { Ok(item) } else { Err("skip") } }
        / item:unordered_list_item_no_continuation(offset, block_metadata, parent_ordered_marker) { item }

        rule unordered_list_item_with_continuation(offset: usize, block_metadata: &BlockParsingMetadata<'input>, parent_ordered_marker: Option<&'input str>) -> Result<(ListItem<'input>, usize), Error>
        = start:position!()
        whitespace()*
        marker:unordered_list_marker()
        whitespace()
        checked:checklist_item()?
        first_line_start:position!()
        // Parse first line (principal text)
        first_line:$((!(eol()) [_])*)
        // Parse continuation lines that are part of the same paragraph
        // Stop at: blank line, list item start, explicit continuation marker, section heading, or list separator
        continuation_lines:(eol() !(&eol() / &at_list_item_start() / &"+" / &at_section_start() / &at_list_separator_content()) cont_line:$((!(eol()) [_])*) { cont_line })*
        first_line_end:position!()
        // Try to parse nested list (ordered, or unordered with deeper markers)
        // Don't consume newlines if we're at a list separator (comment or [])
        // Nested items cannot consume parent-level continuations (allow_continuation: false)
        // NOTE: nested_content is NOT optional here - if no nested content matches, the entire
        // alternative fails and backtracks, leaving eol() unconsumed for explicit_continuation
        nested:(!at_list_separator() eol()+ nested_content:unordered_list_item_nested_content(offset, block_metadata, marker, parent_ordered_marker) { nested_content })?
        // Try to parse explicit continuations (+ marker)
        // Don't consume newlines if we're at a list separator (comment or [])
        // Parent items accept both:
        // - Immediate continuations (0 empty lines) for content directly after principal text
        // - Ancestor continuations (1+ empty lines) for content that bubbles up from nested items
        // Use * to match a mixed sequence of immediate and ancestor continuations
        explicit_continuations:(!at_list_separator() cont:(
            list_explicit_continuation_immediate(offset, block_metadata)
            / list_explicit_continuation_ancestor(offset, block_metadata)
        ) { cont })*
        end:position!()
        {
            tracing::debug!(%first_line, ?continuation_lines, %marker, ?checked, "found unordered list item");
            let level = ListLevel::try_from(ListItem::parse_depth_from_marker(marker).unwrap_or(1))?;
            let principal_text: &'input str = assemble_principal_text(state, first_line, &continuation_lines);
            let item_end = calculate_item_end(principal_text.is_empty(), start, first_line_end);

            // Process principal text as inline nodes
            let principal = if principal_text.trim().is_empty() {
                vec![]
            } else {
                process_inlines(state, block_metadata, first_line_start, first_line_end, offset, principal_text)?
            };

            let mut blocks = Vec::new();
            // nested_content is no longer optional in the grammar, so one less Some level
            if let Some(Some(Ok(nested_list))) = nested {
                blocks.push(nested_list);
            }
            // Collect all continuation blocks (each is a Result<Block<'input>, Error>)
            blocks.extend(explicit_continuations.into_iter().flatten());

            // Use end position after all blocks if we have any, otherwise use item_end
            let actual_end = if blocks.is_empty() { item_end } else { end.saturating_sub(1) };

            Ok((ListItem {
                principal,
                blocks,
                level,
                marker,
                checked,
                location: state.create_location(start+offset, actual_end+offset),
            }, actual_end))
        }

        // Version with immediate continuations only (for nested items)
        // Nested items consume continuations with 0 empty lines (immediate attachment).
        // Continuations with 1+ empty lines bubble up to ancestor items.
        rule unordered_list_item_no_continuation(offset: usize, block_metadata: &BlockParsingMetadata<'input>, parent_ordered_marker: Option<&'input str>) -> Result<(ListItem<'input>, usize), Error>
        = start:position!()
        whitespace()*
        marker:unordered_list_marker()
        whitespace()
        checked:checklist_item()?
        first_line_start:position!()
        first_line:$((!(eol()) [_])*)
        continuation_lines:(eol() !(&eol() / &at_list_item_start() / &"+" / &at_section_start() / &at_list_separator_content()) cont_line:$((!(eol()) [_])*) { cont_line })*
        first_line_end:position!()
        // Nested items can still have nested lists, but those also cannot consume parent continuations
        // NOTE: nested_content is NOT optional here - if no nested content matches, the entire
        // alternative fails and backtracks, leaving eol() unconsumed for immediate_continuation
        nested:(!at_list_separator() eol()+ nested_content:unordered_list_item_nested_content(offset, block_metadata, marker, parent_ordered_marker) { nested_content })?
        // Parse immediate continuations (0 empty lines) - these attach to this item
        // Ancestor continuations (1+ empty lines) bubble up to parent items
        immediate_continuations:(!at_list_separator() cont:list_explicit_continuation_immediate(offset, block_metadata) { cont })*
        end:position!()
        {
            tracing::debug!(%first_line, ?continuation_lines, %marker, ?checked, "found unordered list item (immediate continuation only)");
            let level = ListLevel::try_from(ListItem::parse_depth_from_marker(marker).unwrap_or(1))?;
            let principal_text: &'input str = assemble_principal_text(state, first_line, &continuation_lines);
            let item_end = calculate_item_end(principal_text.is_empty(), start, first_line_end);

            let principal = if principal_text.trim().is_empty() {
                vec![]
            } else {
                process_inlines(state, block_metadata, first_line_start, first_line_end, offset, principal_text)?
            };

            let mut blocks = Vec::new();
            // nested_content is no longer optional in the grammar, so one less Some level
            if let Some(Some(Ok(nested_list))) = nested {
                blocks.push(nested_list);
            }
            // Collect all immediate continuation blocks
            blocks.extend(immediate_continuations.into_iter().flatten());

            // Use end position after all blocks if we have any, otherwise use item_end
            let actual_end = if blocks.is_empty() { item_end } else { end.saturating_sub(1) };

            Ok((ListItem {
                principal,
                blocks,
                level,
                marker,
                checked,
                location: state.create_location(start+offset, actual_end+offset),
            }, actual_end))
        }

        // After-marker variants: used when marker has already been consumed by parent rule
        // These are identical to the regular variants except they take marker as a parameter
        // instead of parsing it, and start after the marker position
        rule unordered_list_item_with_continuation_after_marker(offset: usize, block_metadata: &BlockParsingMetadata<'input>, marker: &'input str, marker_start: usize, parent_ordered_marker: Option<&'input str>) -> Result<(ListItem<'input>, usize), Error>
        = start:position!()
        whitespace()
        checked:checklist_item()?
        first_line_start:position!()
        first_line:$((!(eol()) [_])*)
        continuation_lines:(eol() !(&eol() / &at_list_item_start() / &"+" / &at_section_start() / &at_list_separator_content()) cont_line:$((!(eol()) [_])*) { cont_line })*
        first_line_end:position!()
        nested:(!at_list_separator() eol()+ nested_content:unordered_list_item_nested_content(offset, block_metadata, marker, parent_ordered_marker) { nested_content })?
        explicit_continuations:(!at_list_separator() cont:(
            list_explicit_continuation_immediate(offset, block_metadata)
            / list_explicit_continuation_ancestor(offset, block_metadata)
        ) { cont })*
        end:position!()
        {
            tracing::debug!(%first_line, ?continuation_lines, %marker, ?checked, "found unordered list item (after marker)");
            let level = ListLevel::try_from(ListItem::parse_depth_from_marker(marker).unwrap_or(1))?;
            let principal_text: &'input str = assemble_principal_text(state, first_line, &continuation_lines);
            let item_end = calculate_item_end(principal_text.is_empty(), start, first_line_end);

            let principal = if principal_text.trim().is_empty() {
                vec![]
            } else {
                process_inlines(state, block_metadata, first_line_start, first_line_end, offset, principal_text)?
            };

            let mut blocks = Vec::new();
            if let Some(Some(Ok(nested_list))) = nested {
                blocks.push(nested_list);
            }
            blocks.extend(explicit_continuations.into_iter().flatten());

            let actual_end = if blocks.is_empty() { item_end } else { end.saturating_sub(1) };

            Ok((ListItem {
                principal,
                blocks,
                level,
                marker,
                checked,
                location: state.create_location(marker_start+offset, actual_end+offset),
            }, actual_end))
        }

        rule unordered_list_item_no_continuation_after_marker(offset: usize, block_metadata: &BlockParsingMetadata<'input>, marker: &'input str, marker_start: usize, parent_ordered_marker: Option<&'input str>) -> Result<(ListItem<'input>, usize), Error>
        = start:position!()
        whitespace()
        checked:checklist_item()?
        first_line_start:position!()
        first_line:$((!(eol()) [_])*)
        continuation_lines:(eol() !(&eol() / &at_list_item_start() / &"+" / &at_section_start() / &at_list_separator_content()) cont_line:$((!(eol()) [_])*) { cont_line })*
        first_line_end:position!()
        nested:(!at_list_separator() eol()+ nested_content:unordered_list_item_nested_content(offset, block_metadata, marker, parent_ordered_marker) { nested_content })?
        immediate_continuations:(!at_list_separator() cont:list_explicit_continuation_immediate(offset, block_metadata) { cont })*
        end:position!()
        {
            tracing::debug!(%first_line, ?continuation_lines, %marker, ?checked, "found unordered list item (after marker, immediate only)");
            let level = ListLevel::try_from(ListItem::parse_depth_from_marker(marker).unwrap_or(1))?;
            let principal_text: &'input str = assemble_principal_text(state, first_line, &continuation_lines);
            let item_end = calculate_item_end(principal_text.is_empty(), start, first_line_end);

            let principal = if principal_text.trim().is_empty() {
                vec![]
            } else {
                process_inlines(state, block_metadata, first_line_start, first_line_end, offset, principal_text)?
            };

            let mut blocks = Vec::new();
            if let Some(Some(Ok(nested_list))) = nested {
                blocks.push(nested_list);
            }
            blocks.extend(immediate_continuations.into_iter().flatten());

            let actual_end = if blocks.is_empty() { item_end } else { end.saturating_sub(1) };

            Ok((ListItem {
                principal,
                blocks,
                level,
                marker,
                checked,
                location: state.create_location(marker_start+offset, actual_end+offset),
            }, actual_end))
        }

        /// Parse nested content within an unordered list item (e.g., nested ordered or unordered list)
        /// Note: allow_continuation is false to prevent nested items from consuming parent-level continuations
        /// current_marker: the marker of the parent unordered list item (e.g., "*" or "**")
        /// parent_ordered_marker: the marker of an ancestor ordered list (if any), to prevent
        /// consuming sibling ordered markers that belong to a parent ordered list context
        rule unordered_list_item_nested_content(offset: usize, block_metadata: &BlockParsingMetadata<'input>, current_marker: &'input str, parent_ordered_marker: Option<&'input str>) -> Option<Result<Block<'input>, Error>>
        // !at_root_ordered_marker() prevents root-level ordered items (no leading
        // whitespace) from being incorrectly parsed as nested. Without this, `. item` at
        // column 1 would be nested inside the parent unordered item instead of being a
        // sibling list.
        // !at_ancestor_ordered_marker() prevents sibling ordered markers from a parent
        // ordered list context from being consumed by this nested unordered item.
        = !at_root_ordered_marker() !at_ancestor_ordered_marker(parent_ordered_marker) nested_start:position!() list:ordered_list(nested_start, offset, block_metadata, Some(current_marker), false, true) {
            Some(list)
        }
        // Nested unordered list with deeper markers (e.g., ** inside *)
        // Uses unordered_list_nested which only parses items deeper than current_marker
        / &at_deeper_unordered_marker(current_marker)
          nested_start:position!()
          list:unordered_list_nested(nested_start, offset, block_metadata, current_marker, parent_ordered_marker)
        {
            Some(list)
        }

        /// Parse a nested unordered list where all items have markers deeper than parent_marker.
        /// This is used to parse same-type nesting (e.g., ** inside *) as hierarchical content
        /// rather than flat siblings, enabling proper ancestor continuation handling.
        /// Uses allow_continuation=false to prevent nested items from consuming parent continuations.
        rule unordered_list_nested(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>, parent_marker: &str, parent_ordered_marker: Option<&'input str>) -> Result<Block<'input>, Error>
        // Parse first item - must have a deeper marker than parent_marker
        = &at_deeper_unordered_marker(parent_marker)
          whitespace()* marker_start:position!() base_marker:$(unordered_list_marker()) &whitespace()
          first:unordered_list_item_after_marker(offset, block_metadata, false, base_marker, marker_start, parent_ordered_marker)
          // Parse rest items - only those at same level as base_marker (not deeper, not shallower than parent)
          rest:(unordered_list_nested_rest_item(offset, block_metadata, parent_marker, base_marker, parent_ordered_marker))*
          end:position!()
        {
            tracing::debug!(?parent_marker, ?base_marker, "Found nested unordered list block");
            let mut content = vec![first?];
            for item in rest {
                content.push(item?);
            }
            let end = content.last().map_or(end, |(_, item_end)| *item_end);
            let items: Vec<ListItem<'input>> = content.into_iter().map(|(item, _)| item).collect();
            let marker = items.first().map_or("", |item| item.marker);

            Ok(Block::UnorderedList(UnorderedList {
                title: Title::default(),
                metadata: BlockMetadata::default(),
                items,
                marker,
                location: state.create_location(start+offset, end+offset),
            }))
        }

        /// Parse rest items in a nested unordered list.
        /// Items must be deeper than parent_marker and at same-or-deeper level as base_marker.
        /// Stops when we encounter a marker at or shallower than parent_marker.
        rule unordered_list_nested_rest_item(offset: usize, block_metadata: &BlockParsingMetadata<'input>, parent_marker: &str, base_marker: &str, parent_ordered_marker: Option<&'input str>) -> Result<(ListItem<'input>, usize), Error>
        // Case 1: No blank lines - accept same-level or deeper items
        = !at_list_separator() !eol() comment_line()*
          // Must not be at shallower-or-equal to parent (that would end the nested list)
          !at_shallower_or_equal_unordered_marker(parent_marker)
          item:unordered_list_item(offset, block_metadata, false, parent_ordered_marker)
        { item }
        // Case 2: Blank lines present - only accept same-level items (deeper would be its own nesting)
        / !at_list_separator() eol()+ comment_line()*
          // Must not be at shallower-or-equal to parent
          !at_shallower_or_equal_unordered_marker(parent_marker)
          // Must not be deeper than base (that would be nested inside this item)
          !at_deeper_unordered_marker(base_marker)
          item:unordered_list_item(offset, block_metadata, false, parent_ordered_marker)
        { item }

        // Helper rule to check if we're at a marker that's shallower than or equal to parent_marker
        // Used to terminate nested lists when encountering parent-level or ancestor-level items
        rule at_shallower_or_equal_unordered_marker(parent_marker: &str)
        = whitespace()* marker:unordered_list_marker() whitespace() {?
            if marker.len() <= parent_marker.len() { Ok(()) } else { Err("deeper") }
        }

        // See comment on unordered_list_item for why *_with/without_continuation variants exist.
        rule ordered_list_item(offset: usize, block_metadata: &BlockParsingMetadata<'input>, allow_continuation: bool, parent_unordered_marker: Option<&'input str>) -> Result<(ListItem<'input>, usize), Error>
        = item:ordered_list_item_with_continuation(offset, block_metadata, parent_unordered_marker) {? if allow_continuation { Ok(item) } else { Err("skip") } }
        / item:ordered_list_item_no_continuation(offset, block_metadata, parent_unordered_marker) { item }

        rule ordered_list_item_with_continuation(offset: usize, block_metadata: &BlockParsingMetadata<'input>, parent_unordered_marker: Option<&'input str>) -> Result<(ListItem<'input>, usize), Error>
        = start:position!()
        whitespace()*
        marker:ordered_list_marker()
        whitespace()
        checked:checklist_item()?
        first_line_start:position!()
        // Parse first line (principal text)
        first_line:$((!(eol()) [_])*)
        // Parse continuation lines that are part of the same paragraph
        // Stop at: blank line, list item start, explicit continuation marker, section heading, or list separator
        continuation_lines:(eol() !(&eol() / &at_list_item_start() / &"+" / &at_section_start() / &at_list_separator_content()) cont_line:$((!(eol()) [_])*) { cont_line })*
        first_line_end:position!()
        // Try to parse nested list (unordered, or ordered with deeper markers)
        // Don't consume newlines if we're at a list separator (comment or [])
        // Nested items cannot consume parent-level continuations (allow_continuation: false)
        // NOTE: nested_content is NOT optional here - if no nested content matches, the entire
        // alternative fails and backtracks, leaving eol() unconsumed for explicit_continuation
        nested:(!at_list_separator() eol()+ nested_content:ordered_list_item_nested_content(offset, block_metadata, marker, parent_unordered_marker) { nested_content })?
        // Try to parse explicit continuations (+ marker)
        // Don't consume newlines if we're at a list separator (comment or [])
        // Parent items accept both:
        // - Immediate continuations (0 empty lines) for content directly after principal text
        // - Ancestor continuations (1+ empty lines) for content that bubbles up from nested items
        // Use * to match a mixed sequence of immediate and ancestor continuations
        explicit_continuations:(!at_list_separator() cont:(
            list_explicit_continuation_immediate(offset, block_metadata)
            / list_explicit_continuation_ancestor(offset, block_metadata)
        ) { cont })*
        end:position!()
        {
            tracing::debug!(%first_line, ?continuation_lines, %marker, ?checked, "found ordered list item");
            let level = ListLevel::try_from(ListItem::parse_depth_from_marker(marker).unwrap_or(1))?;
            let principal_text: &'input str = assemble_principal_text(state, first_line, &continuation_lines);
            let item_end = calculate_item_end(principal_text.is_empty(), start, first_line_end);

            // Process principal text as inline nodes
            let principal = if principal_text.trim().is_empty() {
                vec![]
            } else {
                process_inlines(state, block_metadata, first_line_start, first_line_end, offset, principal_text)?
            };

            let mut blocks = Vec::new();
            // nested_content is no longer optional in the grammar, so one less Some level
            if let Some(Some(Ok(nested_list))) = nested {
                blocks.push(nested_list);
            }
            // Collect all continuation blocks (each is a Result<Block<'input>, Error>)
            blocks.extend(explicit_continuations.into_iter().flatten());

            // Use end position after all blocks if we have any, otherwise use item_end
            let actual_end = if blocks.is_empty() { item_end } else { end.saturating_sub(1) };

            Ok((ListItem {
                principal,
                blocks,
                level,
                marker,
                checked,
                location: state.create_location(start+offset, actual_end+offset),
            }, actual_end))
        }

        // Version with immediate continuations only (for nested items)
        // Nested items consume continuations with 0 empty lines (immediate attachment).
        // Continuations with 1+ empty lines bubble up to ancestor items.
        rule ordered_list_item_no_continuation(offset: usize, block_metadata: &BlockParsingMetadata<'input>, parent_unordered_marker: Option<&'input str>) -> Result<(ListItem<'input>, usize), Error>
        = start:position!()
        whitespace()*
        marker:ordered_list_marker()
        whitespace()
        checked:checklist_item()?
        first_line_start:position!()
        first_line:$((!(eol()) [_])*)
        continuation_lines:(eol() !(&eol() / &at_list_item_start() / &"+" / &at_section_start() / &at_list_separator_content()) cont_line:$((!(eol()) [_])*) { cont_line })*
        first_line_end:position!()
        // Nested items can still have nested lists, but those also cannot consume parent continuations
        // NOTE: nested_content is NOT optional here - if no nested content matches, the entire
        // alternative fails and backtracks, leaving eol() unconsumed for immediate_continuation
        nested:(!at_list_separator() eol()+ nested_content:ordered_list_item_nested_content(offset, block_metadata, marker, parent_unordered_marker) { nested_content })?
        // Parse immediate continuations (0 empty lines) - these attach to this item
        // Ancestor continuations (1+ empty lines) bubble up to parent items
        immediate_continuations:(!at_list_separator() cont:list_explicit_continuation_immediate(offset, block_metadata) { cont })*
        end:position!()
        {
            tracing::debug!(%first_line, ?continuation_lines, %marker, ?checked, "found ordered list item (immediate continuation only)");
            let level = ListLevel::try_from(ListItem::parse_depth_from_marker(marker).unwrap_or(1))?;
            let principal_text: &'input str = assemble_principal_text(state, first_line, &continuation_lines);
            let item_end = calculate_item_end(principal_text.is_empty(), start, first_line_end);

            let principal = if principal_text.trim().is_empty() {
                vec![]
            } else {
                process_inlines(state, block_metadata, first_line_start, first_line_end, offset, principal_text)?
            };

            let mut blocks = Vec::new();
            // nested_content is no longer optional in the grammar, so one less Some level
            if let Some(Some(Ok(nested_list))) = nested {
                blocks.push(nested_list);
            }
            // Collect all immediate continuation blocks
            blocks.extend(immediate_continuations.into_iter().flatten());

            // Use end position after all blocks if we have any, otherwise use item_end
            let actual_end = if blocks.is_empty() { item_end } else { end.saturating_sub(1) };

            Ok((ListItem {
                principal,
                blocks,
                level,
                marker,
                checked,
                location: state.create_location(start+offset, actual_end+offset),
            }, actual_end))
        }

        // After-marker variants for ordered lists: used when marker has already been consumed by parent rule
        rule ordered_list_item_with_continuation_after_marker(offset: usize, block_metadata: &BlockParsingMetadata<'input>, marker: &'input str, marker_start: usize, parent_unordered_marker: Option<&'input str>) -> Result<(ListItem<'input>, usize), Error>
        = start:position!()
        whitespace()
        checked:checklist_item()?
        first_line_start:position!()
        first_line:$((!(eol()) [_])*)
        continuation_lines:(eol() !(&eol() / &at_list_item_start() / &"+" / &at_section_start() / &at_list_separator_content()) cont_line:$((!(eol()) [_])*) { cont_line })*
        first_line_end:position!()
        nested:(!at_list_separator() eol()+ nested_content:ordered_list_item_nested_content(offset, block_metadata, marker, parent_unordered_marker) { nested_content })?
        explicit_continuations:(!at_list_separator() cont:(
            list_explicit_continuation_immediate(offset, block_metadata)
            / list_explicit_continuation_ancestor(offset, block_metadata)
        ) { cont })*
        end:position!()
        {
            tracing::debug!(%first_line, ?continuation_lines, %marker, ?checked, "found ordered list item (after marker)");
            let level = ListLevel::try_from(ListItem::parse_depth_from_marker(marker).unwrap_or(1))?;
            let principal_text: &'input str = assemble_principal_text(state, first_line, &continuation_lines);
            let item_end = calculate_item_end(principal_text.is_empty(), start, first_line_end);

            let principal = if principal_text.trim().is_empty() {
                vec![]
            } else {
                process_inlines(state, block_metadata, first_line_start, first_line_end, offset, principal_text)?
            };

            let mut blocks = Vec::new();
            if let Some(Some(Ok(nested_list))) = nested {
                blocks.push(nested_list);
            }
            blocks.extend(explicit_continuations.into_iter().flatten());

            let actual_end = if blocks.is_empty() { item_end } else { end.saturating_sub(1) };

            Ok((ListItem {
                principal,
                blocks,
                level,
                marker,
                checked,
                location: state.create_location(marker_start+offset, actual_end+offset),
            }, actual_end))
        }

        rule ordered_list_item_no_continuation_after_marker(offset: usize, block_metadata: &BlockParsingMetadata<'input>, marker: &'input str, marker_start: usize, parent_unordered_marker: Option<&'input str>) -> Result<(ListItem<'input>, usize), Error>
        = start:position!()
        whitespace()
        checked:checklist_item()?
        first_line_start:position!()
        first_line:$((!(eol()) [_])*)
        continuation_lines:(eol() !(&eol() / &at_list_item_start() / &"+" / &at_section_start() / &at_list_separator_content()) cont_line:$((!(eol()) [_])*) { cont_line })*
        first_line_end:position!()
        nested:(!at_list_separator() eol()+ nested_content:ordered_list_item_nested_content(offset, block_metadata, marker, parent_unordered_marker) { nested_content })?
        immediate_continuations:(!at_list_separator() cont:list_explicit_continuation_immediate(offset, block_metadata) { cont })*
        end:position!()
        {
            tracing::debug!(%first_line, ?continuation_lines, %marker, ?checked, "found ordered list item (after marker, immediate only)");
            let level = ListLevel::try_from(ListItem::parse_depth_from_marker(marker).unwrap_or(1))?;
            let principal_text: &'input str = assemble_principal_text(state, first_line, &continuation_lines);
            let item_end = calculate_item_end(principal_text.is_empty(), start, first_line_end);

            let principal = if principal_text.trim().is_empty() {
                vec![]
            } else {
                process_inlines(state, block_metadata, first_line_start, first_line_end, offset, principal_text)?
            };

            let mut blocks = Vec::new();
            if let Some(Some(Ok(nested_list))) = nested {
                blocks.push(nested_list);
            }
            blocks.extend(immediate_continuations.into_iter().flatten());

            let actual_end = if blocks.is_empty() { item_end } else { end.saturating_sub(1) };

            Ok((ListItem {
                principal,
                blocks,
                level,
                marker,
                checked,
                location: state.create_location(marker_start+offset, actual_end+offset),
            }, actual_end))
        }

        /// Parse nested content within an ordered list item (e.g., nested unordered or ordered list)
        /// Note: allow_continuation is false to prevent nested items from consuming parent-level continuations
        /// current_marker: the marker of the parent ordered list item (e.g., "." or "..")
        /// parent_unordered_marker: the marker of an ancestor unordered list (if any), to prevent
        /// consuming sibling unordered markers that belong to a parent unordered list context
        rule ordered_list_item_nested_content(offset: usize, block_metadata: &BlockParsingMetadata<'input>, current_marker: &'input str, parent_unordered_marker: Option<&'input str>) -> Option<Result<Block<'input>, Error>>
        // !at_root_unordered_marker() prevents root-level unordered items (no leading
        // whitespace) from being incorrectly parsed as nested. Without this, `* item` at
        // column 1 would be nested inside the parent ordered item instead of being a
        // sibling list.
        // !at_ancestor_unordered_marker() prevents sibling unordered markers from a parent
        // unordered list context from being consumed by this nested ordered item.
        = !at_root_unordered_marker() !at_ancestor_unordered_marker(parent_unordered_marker) nested_start:position!() list:unordered_list(nested_start, offset, block_metadata, Some(current_marker), false, true) {
            Some(list)
        }
        // Nested ordered list with deeper markers (e.g., .. inside .)
        // Uses ordered_list_nested which only parses items deeper than current_marker
        / &at_deeper_ordered_marker(current_marker)
          nested_start:position!()
          list:ordered_list_nested(nested_start, offset, block_metadata, current_marker, parent_unordered_marker)
        {
            Some(list)
        }

        /// Parse a nested ordered list where all items have markers deeper than parent_marker.
        /// This is used to parse same-type nesting (e.g., .. inside .) as hierarchical content
        /// rather than flat siblings, enabling proper ancestor continuation handling.
        /// Uses allow_continuation=false to prevent nested items from consuming parent continuations.
        rule ordered_list_nested(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>, parent_marker: &str, parent_unordered_marker: Option<&'input str>) -> Result<Block<'input>, Error>
        // Parse first item - must have a deeper marker than parent_marker
        = &at_deeper_ordered_marker(parent_marker)
          whitespace()* marker_start:position!() base_marker:$(ordered_list_marker()) &whitespace()
          first:ordered_list_item_after_marker(offset, block_metadata, false, base_marker, marker_start, parent_unordered_marker)
          // Parse rest items - only those at same level as base_marker (not deeper, not shallower than parent)
          rest:(ordered_list_nested_rest_item(offset, block_metadata, parent_marker, base_marker, parent_unordered_marker))*
          end:position!()
        {
            tracing::debug!(?parent_marker, ?base_marker, "Found nested ordered list block");
            let mut content = vec![first?];
            for item in rest {
                content.push(item?);
            }
            let end = content.last().map_or(end, |(_, item_end)| *item_end);
            let items: Vec<ListItem<'input>> = content.into_iter().map(|(item, _)| item).collect();
            let marker = items.first().map_or("", |item| item.marker);

            Ok(Block::OrderedList(OrderedList {
                title: Title::default(),
                metadata: BlockMetadata::default(),
                items,
                marker,
                location: state.create_location(start+offset, end+offset),
            }))
        }

        /// Parse rest items in a nested ordered list.
        /// Items must be deeper than parent_marker and at same-or-deeper level as base_marker.
        /// Stops when we encounter a marker at or shallower than parent_marker.
        rule ordered_list_nested_rest_item(offset: usize, block_metadata: &BlockParsingMetadata<'input>, parent_marker: &str, base_marker: &str, parent_unordered_marker: Option<&'input str>) -> Result<(ListItem<'input>, usize), Error>
        // Case 1: No blank lines - accept same-level or deeper items
        = !at_list_separator() !eol() comment_line()*
          // Must not be at shallower-or-equal to parent (that would end the nested list)
          !at_shallower_or_equal_ordered_marker(parent_marker)
          item:ordered_list_item(offset, block_metadata, false, parent_unordered_marker)
        { item }
        // Case 2: Blank lines present - only accept same-level items (deeper would be its own nesting)
        / !at_list_separator() eol()+ comment_line()*
          // Must not be at shallower-or-equal to parent
          !at_shallower_or_equal_ordered_marker(parent_marker)
          // Must not be deeper than base (that would be nested inside this item)
          !at_deeper_ordered_marker(base_marker)
          item:ordered_list_item(offset, block_metadata, false, parent_unordered_marker)
        { item }

        // Helper rule to check if we're at a marker that's shallower than or equal to parent_marker
        // Used to terminate nested lists when encountering parent-level or ancestor-level items
        rule at_shallower_or_equal_ordered_marker(parent_marker: &str)
        = whitespace()* marker:ordered_list_marker() whitespace() {?
            if marker.len() <= parent_marker.len() { Ok(()) } else { Err("deeper") }
        }

        /// Predicate rule that succeeds when we're NOT after a verbatim block
        /// Used with negative lookahead to ensure callout lists only match after verbatim blocks
        rule not_after_verbatim_block() -> ()
        = {?
            if state.last_block_was_verbatim {
                Err("is_after_verbatim")
            } else {
                Ok(())
            }
        }

        rule callout_list(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
        // !not_after_verbatim_block(): callout lists only make sense after source/listing
        // blocks The double negative succeeds only when last_block_was_verbatim is true
        = !not_after_verbatim_block()
        // OPTIMIZATION: This positive lookahead fails fast when not at a callout marker
        // (<1>, <.>, etc.) Without it, callout_list_item would be called and fail - same
        // result, just slower
        &(whitespace()* callout_list_marker() whitespace())
        first:callout_list_item(offset, block_metadata)
        rest:(callout_list_rest_item(offset, block_metadata))*
        end:position!()
        {
            tracing::debug!("Found callout list block");
            let mut content = vec![first?];
            for item in rest {
                content.push(item?);
            }
            let end = content.last().map_or(end, |(_, _, item_end)| *item_end);

            // Resolve auto-numbered callouts and collect items
            let mut auto_number = 1usize;
            let mut items: Vec<CalloutListItem> = Vec::with_capacity(content.len());

            for (mut item, marker, _end) in content {
                // Resolve auto-numbered callouts
                if marker == "<.>" {
                    item.callout = CalloutRef::auto(auto_number, item.callout.location.clone());
                    auto_number += 1;
                }
                items.push(item);
            }

            // Validate callout list items
            for (expected_number, item) in (1..).zip(items.iter()) {
                let actual_number = item.callout.number;

                // Check sequential order
                if actual_number != expected_number {
                    state.add_generic_warning_at(
                        format!(
                            "callout list item index: expected {expected_number}, got {actual_number}"
                        ),
                        item.location.clone(),
                    );
                }

                // Check if the EXPECTED callout exists in the verbatim block
                // (This warns when sequence is broken and the expected number is missing)
                let callout_exists = state
                    .last_verbatim_callouts
                    .iter()
                    .any(|c| c.number == expected_number);
                if !callout_exists {
                    state.add_generic_warning_at(
                        format!("no callout found for <{expected_number}>"),
                        item.location.clone(),
                    );
                }
            }

            // Reset the flag after successfully parsing the callout list
            state.last_block_was_verbatim = false;
            state.last_verbatim_callouts.clear();

            Ok(Block::CalloutList(CalloutList {
                title: block_metadata.title.clone(),
                metadata: block_metadata.metadata.clone(),
                items,
                location: state.create_location(start+offset, end+offset),
            }))
        }

        rule callout_list_rest_item(offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<(CalloutListItem<'input>, String, usize), Error>
        = eol()+ item:callout_list_item(offset, block_metadata)
        {?
            Ok(item)
        }

        rule callout_list_item(offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<(CalloutListItem<'input>, String, usize), Error>
        = start:position!()
        whitespace()*
        marker:callout_list_marker()
        whitespace()
        first_line_start:position!()
        // Parse first line (principal text)
        first_line:$((!(eol()) [_])*)
        // Parse continuation lines that are part of the same paragraph
        // Stop at: list markers, blank lines, section headers, or block attributes
        continuation_lines:(
            eol()
            !(whitespace()* (callout_list_marker() / unordered_list_marker() / ordered_list_marker() / section_level_marker() whitespace() / "[" / eol()))
            line:$((!(eol()) [_])*)
            { line }
        )*
        first_line_end:position!()
        {
            // Combine first line and continuation lines
            let principal_text_owned = if continuation_lines.is_empty() {
                first_line.to_string()
            } else {
                let mut text = first_line.to_string();
                for cont_line in continuation_lines {
                    text.push('\n');
                    text.push_str(cont_line);
                }
                text
            };
            let principal_text: &'input str = state.intern_str(&principal_text_owned);

            // The end position for the list item should be at the last character of content
            let item_end = if principal_text.is_empty() {
                start
            } else {
                first_line_end.saturating_sub(1)
            };

            // Process principal text as inline nodes
            let principal = if principal_text.trim().is_empty() {
                vec![]
            } else {
                process_inlines(state, block_metadata, first_line_start, first_line_end, offset, principal_text)?
            };

            // For callout lists, we don't support nested content or attached blocks
            let blocks = vec![];

            let location = state.create_location(start+offset, item_end+offset);

            // Create a placeholder callout - will be resolved in callout_list
            // We pass the marker string to the parent rule for resolution
            let callout = if marker == "<.>" {
                CalloutRef::auto(0, location.clone()) // Number will be resolved later
            } else {
                let number = extract_callout_number(marker).unwrap_or(0);
                CalloutRef::explicit(number, location.clone())
            };

            Ok((CalloutListItem {
                callout,
                principal,
                blocks,
                location,
            }, marker.to_string(), item_end))
        }

        rule checklist_item() -> ListItemCheckedStatus
            = checked:(("[x]" / "[X]" / "[*]") { ListItemCheckedStatus::Checked } / "[ ]" { ListItemCheckedStatus::Unchecked }) whitespace()
        {
            checked
        }

        rule check_start_of_description_list(offset: usize)
        = pos:position!() {?
            if find_dlist_marker(state.input.as_bytes(), pos + offset, true, true) {
                Ok(())
            } else {
                Err("no dlist marker before next blank line")
            }
        }

        /// Like check_start_of_description_list but restricted to the current line.
        /// Used by setext section rules to avoid false positives when a description
        /// list marker (::, ;;) appears later in the document but not on the current line.
        rule check_line_is_description_list(offset: usize)
        = pos:position!() {?
            if find_dlist_marker(state.input.as_bytes(), pos + offset, false, true) {
                Ok(())
            } else {
                Err("no dlist marker on current line")
            }
        }

        /// Variant of `check_line_is_description_list` that does not accept
        /// end-of-input as marker context. Mirrors the inline pattern previously
        /// embedded in `description_list_item`'s continuation guard.
        rule check_line_is_description_list_strict(offset: usize)
        = pos:position!() {?
            if find_dlist_marker(state.input.as_bytes(), pos + offset, false, false) {
                Ok(())
            } else {
                Err("no dlist marker on current line")
            }
        }

        rule description_list(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
        = check_start_of_description_list(offset)
        first_item:description_list_item(offset, block_metadata)
        additional_items:description_list_additional_items(offset, block_metadata)*
        end:position!()
        {
            tracing::debug!("Found description list block with auto-attachment support");
            let mut items = vec![first_item?];

            for additional in additional_items {
                items.push(additional?);
            }

            let actual_end = items.last().map_or(end, |item| {
                let loc_end = item.location.absolute_end;
                loc_end - offset
            });

            Ok(Block::DescriptionList(DescriptionList {
                title: block_metadata.title.clone(),
                metadata: block_metadata.metadata.clone(),
                items,
                location: state.create_location(start+offset, actual_end+offset),
            }))
        }

        // Parse additional description list items (after potential auto-attached content)
        //
        // !at_dlist_block_boundary() prevents continuing the list when a blank line is
        // followed by block attributes. This allows attributes to apply to a new list.
        rule description_list_additional_items(offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<DescriptionListItem<'input>, Error>
        = !at_dlist_block_boundary()
        eol()*
        check_start_of_description_list(offset)
        item:description_list_item(offset, block_metadata)
        {
            tracing::debug!("Found additional description list item");
            item
        }

        rule description_list_item(offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<DescriptionListItem<'input>, Error>
        = start:position!()
        term:$((!(description_list_marker() (eol() / " ") / eol()*<2,2>) [_])+)
        delim_start:position!() delimiter:description_list_marker() delim_end:position!()
        whitespace()?
        principal_start:position!()
        principal_content:$(
            (!eol() [_])*
            // Implicit text continuation: consume subsequent non-blank lines that
            // aren't new dlist entries, list items, continuation markers, or block
            // delimiters. This mirrors paragraph multi-line handling but with
            // dlist-specific stop conditions.
            (eol()
             !eol()                                    // not a blank line
             !check_line_is_description_list_strict(offset)  // not a new dlist entry (line-local check)
             !(whitespace()* (unordered_list_marker() / ordered_list_marker()) whitespace())  // not a list item
             !("+" (whitespace() / eol() / ![_]))      // not a continuation marker
             !example_delimiter()                      // not a block delimiter
             !listing_delimiter()
             !literal_delimiter()
             !sidebar_delimiter()
             !quote_delimiter()
             !pass_delimiter()
             !comment_delimiter()
             !table_delimiter()
             !(open_delimiter() (whitespace()* eol()))
             !markdown_code_delimiter()
             !attributes_line()                           // not a block attributes line
             !((anchor() / attributes_line())* section_level_at_line_start(offset, None) (whitespace() / eol() / ![_]))  // not a section heading
             (!eol() [_])+                             // continuation line content
            )*
        )
        // Now handle auto-attachment and explicit continuation
        attached_content:description_list_attached_content(offset, block_metadata)*
        end:position!()
        {
            tracing::debug!(%term, %delimiter, "parsing description list item with auto-attachment");

            state.inline_ctx.offset = start + offset;
            state.inline_ctx.macros_enabled = block_metadata.macros_enabled;
            state.inline_ctx.attributes_enabled = block_metadata.attributes_enabled;
            let term = inline_parser::inlines(term.trim(), state)
                .unwrap_or_else(|e| {
                    adjust_and_log_parse_error(&e, term.trim(), start+offset, state, "Error parsing term as inline content");
                    vec![]
                });

            let principal_end = principal_start + principal_content.len();
            let principal_text = if principal_content.trim().is_empty() {
                Vec::new()
            } else {
                // Parse as inline content with attribute substitution
                process_inlines(state, block_metadata, principal_start, principal_end, offset, principal_content.trim())?
            };

            // Collect all attached blocks (auto-attached and explicitly continued)
            let mut description = Vec::with_capacity(attached_content.len());
            for content in attached_content {
                match content {
                    Ok(blocks) => description.extend(blocks),
                    Err(e) => {
                        tracing::error!(?e, "Error processing attached content");
                    }
                }
            }

            // Calculate actual end from last attached block, or fall back to end of principal/term
            // Note: end:position!() captures position after consuming blank lines looking for more
            // continuations, which ends up at the start of the next item. We need the actual content end.
            let actual_end = description.last().map_or_else(
                || {
                    // No attached content: use end of principal text line
                    if principal_content.is_empty() {
                        // Just term + delimiter
                        principal_start
                    } else {
                        principal_start + principal_content.len()
                    }
                },
                |b| {
                    let loc = b.location();
                    loc.absolute_end - offset
                },
            );

            let delimiter_location = state.create_block_location(delim_start, delim_end, offset);
            Ok(DescriptionListItem {
                anchors: vec![],
                term,
                delimiter,
                delimiter_location: Some(delimiter_location),
                principal_text,
                description,
                location: state.create_location(start+offset, actual_end+offset),
            })
        }

        rule description_list_attached_content(offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Vec<Block<'input>>, Error>
        = eol() content:(
            // Explicit continuation - this uses +, allows any content including delimited
            // blocks
            description_list_explicit_continuation(offset, block_metadata)
            // Auto-attach lists (even with blank lines before them)
            / description_list_auto_attached_list(offset, block_metadata)
        )
        {
            content
        }

        rule description_list_auto_attached_list(offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Vec<Block<'input>>, Error>
        = eol()* // Consume any blank lines before the list
        &(whitespace()* (unordered_list_marker() / ordered_list_marker()) whitespace())
        list_start:position!()
        list:(unordered_list(list_start, offset, block_metadata, None, true, true) / ordered_list(list_start, offset, block_metadata, None, true, true))
        {
            tracing::debug!("Auto-attaching list to description list item");
            Ok(vec![list?])
        }

        // Parse one or more explicit continuations for description lists
        // Same pattern as list_explicit_continuation: + marker followed by a single block
        // Uses block_in_continuation to prevent lists inside continuations from consuming
        // further continuations that belong to the parent item
        rule description_list_explicit_continuation(offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Vec<Block<'input>>, Error>
        = continuations:(
            eol()* "+" eol()
            block:block_in_continuation(offset, block_metadata.parent_section_level)
            { block }
          )+
        {
            tracing::debug!(count = continuations.len(), "Description list explicit continuation blocks");
            Ok(continuations.into_iter().filter_map(Result::ok).collect())
        }

        // Parse a single immediate continuation (0 empty lines before +)
        // These attach to the current (most recent) list item per AsciiDoc spec.
        // Uses block_in_continuation to prevent lists inside continuations from consuming
        // further continuations that belong to the parent item.
        // Pattern: exactly one newline before + (content\n+\nblock)
        rule list_explicit_continuation_immediate(offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
        = eol() !eol() "+" eol()
          block:block_in_continuation(offset, block_metadata.parent_section_level)
        {
            tracing::debug!("List immediate continuation block (0 empty lines)");
            block
        }

        // Parse a single ancestor continuation (1+ empty lines before +)
        // Per AsciiDoc spec: each empty line before + moves attachment up one nesting level.
        // 1 empty line = parent, 2 empty lines = grandparent, etc.
        // Uses block_in_continuation to prevent lists inside continuations from consuming
        // further continuations that belong to the parent item.
        // Pattern: two or more newlines before + (content\n\n+\nblock)
        rule list_explicit_continuation_ancestor(offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
        = eol() eol()+ "+" eol()
          block:block_in_continuation(offset, block_metadata.parent_section_level)
        {
            tracing::debug!("List ancestor continuation block (1+ empty lines)");
            block
        }

        // Parse a quoted paragraph: "content" followed by `-- attribution[, citation]`
        //
        // This matches the AsciiDoc shorthand syntax for blockquotes:
        // ```
        // "I hold it that a little rebellion now and then is a good thing."
        // -- Thomas Jefferson, Papers of Thomas Jefferson
        // ```
        rule quoted_paragraph(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
        = content_start:position!()
          "\"" quoted_content:$((!"\"" [_])+) "\""
          eol()
          "-- " attr_start:position!() attribution_line:$([^'\n']+)
          end:position!()
        {
            tracing::debug!(?quoted_content, ?attribution_line, "found quoted paragraph");

            // Parse attribution line: "Author Name, Source Title" or just "Author Name"
            // Intern the slices into the parser arena so downstream inline parsing
            // can produce nodes with the `'input` lifetime.
            let (attr_str, cite_str): (&'input str, Option<&'input str>) = match attribution_line.split_once(',') {
                Some((attr, cite)) => (state.intern_str(attr.trim()), Some(state.intern_str(cite.trim()))),
                None => (state.intern_str(attribution_line.trim()), None),
            };

            // Parse attribution through inline pipeline
            let attr_end_offset = attr_start + attr_str.len();
            let attr_inlines = process_inlines(
                state,
                block_metadata,
                attr_start,
                attr_end_offset,
                offset,
                attr_str,
            )?;

            // Parse citation through inline pipeline if present
            let cite_inlines = if let Some(cite) = cite_str {
                let cite_offset_in_line = attribution_line.find(',').unwrap_or(0) + 1;
                let cite_raw_start = attr_start + cite_offset_in_line + (attribution_line[cite_offset_in_line..].len() - attribution_line[cite_offset_in_line..].trim_start().len());
                let cite_pos = PositionWithOffset {
                    offset: cite_raw_start,
                    position: state.line_map.offset_to_position(cite_raw_start, state.input),
                };
                Some(process_inlines(
                    state,
                    block_metadata,
                    cite_pos.offset,
                    cite_raw_start + cite.len(),
                    offset,
                    cite,
                )?)
            } else {
                None
            };

            // Parse the quoted content as blocks
            let blocks = document_parser::blocks(quoted_content, state, content_start + offset, block_metadata.parent_section_level).unwrap_or_else(|e| {
                adjust_and_log_parse_error(&e, quoted_content, content_start + offset, state, "Error parsing content as blocks in quoted paragraph");
                Ok(Vec::new())
            })?;

            // Build metadata with quote style and attribution
            let mut metadata = block_metadata.metadata.clone();
            metadata.style = Some("quote");
            metadata.attribution = Some(Attribution::new(attr_inlines));
            if let Some(inlines) = cite_inlines {
                metadata.citetitle = Some(CiteTitle::new(inlines));
            }

            Ok(Block::DelimitedBlock(DelimitedBlock {
                metadata,
                delimiter: "\"",
                inner: DelimitedBlockType::DelimitedQuote(blocks),
                title: block_metadata.title.clone(),
                location: state.create_block_location(start, end, offset),
                open_delimiter_location: None,
                close_delimiter_location: None,
            }))
        }

        /// Parse a markdown-style blockquote: lines starting with `> `
        ///
        /// This matches the Markdown-compatible syntax for blockquotes:
        /// ```
        /// > I hold it that a little rebellion now and then is a good thing,
        /// > and as necessary in the political world as storms in the physical.
        /// > -- Thomas Jefferson, Papers of Thomas Jefferson: Volume 11
        /// ```
        ///
        /// The content after `> ` on each line is joined and parsed as blocks.
        /// Attribution is extracted from a line matching `> -- Author[, Citation]`.
        rule markdown_blockquote(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
        = lines:markdown_blockquote_content_line()+ attribution:markdown_blockquote_attribution()? end:position!()
        {
            tracing::debug!(?lines, ?attribution, "found markdown blockquote");

            let content: &'input str = state.intern_join(lines.iter(), "\n");
            let content_start = start;

            // Build metadata with quote style and attribution
            let mut metadata = block_metadata.metadata.clone();
            metadata.style = Some("quote");
            if let Some((author, author_start, citation)) = attribution {
                let author: &'input str = state.intern_str(&author);
                // Parse author through inline pipeline
                let author_pos = PositionWithOffset {
                    offset: author_start,
                    position: state.line_map.offset_to_position(author_start, state.input),
                };
                let attr_end_offset = author_start + author.len();
                let attr_inlines = process_inlines(
                    state,
                    block_metadata,
                    author_pos.offset,
                    attr_end_offset,
                    offset,
                    author,
                )?;
                metadata.attribution = Some(Attribution::new(attr_inlines));

                if let Some((cite, cite_start)) = citation {
                    let cite: &'input str = state.intern_str(&cite);
                    // Parse citation through inline pipeline
                    let cite_pos = PositionWithOffset {
                        offset: cite_start,
                        position: state.line_map.offset_to_position(cite_start, state.input),
                    };
                    let cite_inlines = process_inlines(
                        state,
                        block_metadata,
                        cite_pos.offset,
                        cite_start + cite.len(),
                        offset,
                        cite,
                    )?;
                    metadata.citetitle = Some(CiteTitle::new(cite_inlines));
                }
            }

            let location = state.create_block_location(start, end, offset);

            // Parse the content as blocks
            let blocks = if content.trim().is_empty() {
                Vec::new()
            } else {
                document_parser::blocks(content, state, content_start + offset, block_metadata.parent_section_level).unwrap_or_else(|e| {
                    adjust_and_log_parse_error(&e, content, content_start + offset, state, "Error parsing content as blocks in markdown blockquote");
                    Ok(Vec::new())
                })?
            };

            Ok(Block::DelimitedBlock(DelimitedBlock {
                metadata,
                delimiter: ">",
                inner: DelimitedBlockType::DelimitedQuote(blocks),
                title: block_metadata.title.clone(),
                location,
                open_delimiter_location: None,
                close_delimiter_location: None,
            }))
        }

        /// Match a content line of a markdown-style blockquote
        /// A line is content if:
        /// 1. It's followed by another `>` line (so `> -- ...` mid-blockquote is content)
        /// 2. OR it doesn't start with `-- ` (so it can't be attribution)
        rule markdown_blockquote_content_line() -> &'input str
        = "> " content:$([^'\n']*) eol() &">" { content }
        / "> " !("-- ") content:$([^'\n']*) (eol() / ![_]) { content }
        / ">" eol() &">" { "" }
        / ">" eol() { "" }
        / ">" ![_] { "" }

        /// Match an attribution line: `> -- Author[, Citation]`
        /// Only matches at the END of a blockquote (not followed by more `>` lines)
        /// Returns (author, author_start, Option<(citation, cite_start)>)
        rule markdown_blockquote_attribution() -> (String, usize, Option<(String, usize)>)
        = "> -- " author_start:position!() author:$([^(',' | '\n')]+) ", " cite_start:position!() citation:$([^'\n']+) ((eol() !">") / ![_]) {
            (author.trim().to_string(), author_start, Some((citation.trim().to_string(), cite_start)))
        }
        / "> -- " author_start:position!() author:$([^'\n']+) ((eol() !">") / ![_]) {
            (author.trim().to_string(), author_start, None)
        }

        rule paragraph(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
        = admonition:admonition()?
        content_start:position!()
        content:$((!(
            eol()*<2,>
            / eol()* ![_]
            / eol() &attributes_line()
            / eol() example_delimiter()
            / eol() listing_delimiter()
            / eol() literal_delimiter()
            / eol() sidebar_delimiter()
            / eol() quote_delimiter()
            / eol() pass_delimiter()
            / eol() table_delimiter()
            / eol() markdown_code_delimiter()
            / eol() comment_delimiter()
            / eol() open_delimiter() &(whitespace()* eol())
            / eol() list(start, offset, block_metadata)
            / eol() &("+" (whitespace() / eol() / ![_]))  // Stop at list continuation marker
            / eol()* &((anchor() / attributes_line())* section_level_at_line_start(offset, None) (whitespace() / eol() / ![_]))
        ) [_])+)
        end:position!()
        {
            // Reset the verbatim flag since paragraph is not a verbatim block
            state.last_block_was_verbatim = false;

            // Check if this is a literal paragraph BEFORE preprocessing
            //
            // Literal paragraphs start with a space and should not have inline
            // preprocessing applied
            if content.starts_with(' ') {
                return Ok(get_literal_paragraph(state, content, start, end, offset, block_metadata));
            }

            let content = process_inlines(state, block_metadata, content_start, end, offset, content)?;

            // Title should either be an attribute named title, or the title parsed from the block metadata
            let title: Title = if let Some(AttributeValue::String(title)) = block_metadata.metadata.attributes.get("title") {
                vec![InlineNode::PlainText(Plain {
                    content: state.intern_cow(title.clone()),
                    location: state.create_location(start+offset, (start+offset).saturating_add(title.len()).saturating_sub(1)),
                    escaped: false,
                })].into()
            } else {
                block_metadata.title.clone()
            };

            if let Some((variant, admonition_start, admonition_end)) = admonition {
                let Ok(parsed_variant) = AdmonitionVariant::from_str(&variant) else {
                    tracing::error!(%variant, "invalid admonition variant");
                    return Err(Error::InvalidAdmonitionVariant(
                        Box::new(state.create_error_source_location(state.create_location(admonition_start + offset, admonition_end + offset - 1))),
                        variant
                    ));
                };
                tracing::debug!(%variant, "found admonition block with variant");
                Ok(Block::Admonition(Admonition{
                    metadata: block_metadata.metadata.clone(),
                    title,
                    blocks: vec![Block::Paragraph(Paragraph {
                        content,
                        metadata: block_metadata.metadata.clone(),
                        title: Title::default(),
                        location: state.create_block_location(content_start, end, offset),
                    })],
                    location: state.create_block_location(start, end, offset),
                    variant: parsed_variant,

                }))
            } else {
                let mut metadata = block_metadata.metadata.clone();
                metadata.move_positional_attributes_to_attributes();

                tracing::debug!(?content, "found paragraph block");
                Ok(Block::Paragraph(Paragraph {
                    content,
                    metadata,
                    title,
                    location: state.create_block_location(start, end, offset),
                }))
            }
        }

        rule admonition() -> (String, usize, usize)
            = start:position!() variant:$("NOTE" / "WARNING" / "TIP" / "IMPORTANT" / "CAUTION") ": " end:position!()
        {
            (variant.to_string(), start, end)
        }

        // Lookahead rule that warns about anchor ID-like patterns containing whitespace.
        //
        // This uses negative lookahead and emits a warning if it detects whitespace. It
        // does not consume the input.
        rule warn_anchor_id_with_whitespace() -> ()
        = start:position!()
        &(
            id:$([^'\'' | ',' | ']' | '.' | '#']+)
            end:position!()
            {?
                if id.chars().any(char::is_whitespace) {
                    let location = state.create_location(start, end);
                    state.add_generic_warning_at(
                        format!("anchor id '{id}' contains whitespace which is not allowed, treating as literal text"),
                        location,
                    );
                }
                // Always fail so the lookahead doesn't match - we just want the side
                // effect
                Err::<(), &'static str>("")
            }
        )

        rule anchor() -> Anchor<'input>
        = start:position!()
        result:(
            // Double-bracket [[id]] syntax - allows dots in ID since no role shorthand
            // possible.
            //
            // Whitespace is excluded per AsciiDoc documentation at
            // https://docs.asciidoctor.org/asciidoc/latest/attributes/id/#valid-id-characters
            double_open_square_bracket() warn_anchor_id_with_whitespace()? id:$([^'\'' | ',' | ']' | ' ' | '\t' | '\n' | '\r']+) comma() reftext:$([^']']+) double_close_square_bracket() {
                (id, Some(reftext))
            } /
            start:position!() double_open_square_bracket() warn_anchor_id_with_whitespace()? id:$([^'\'' | ',' | ']' | ' ' | '\t' | '\n' | '\r']+) double_close_square_bracket() {
                (id, None)
            } /
            // Single-bracket [#id] shorthand - exclude '.', '%' as they start role/option
            // shorthands
            //
            // Whitespace is excluded per AsciiDoc documentation at
            // https://docs.asciidoctor.org/asciidoc/latest/attributes/id/#valid-id-characters
            start:position!() open_square_bracket() "#" warn_anchor_id_with_whitespace()? id:$([^'\'' | ',' | ']' | '.' | '%' | ' ' | '\t' | '\n' | '\r']+) comma() reftext:$([^']']+) close_square_bracket() {
                (id, Some(reftext))
            } /
            start:position!() open_square_bracket() "#" warn_anchor_id_with_whitespace()? id:$([^'\'' | ',' | ']' | '.' | '%' | ' ' | '\t' | '\n' | '\r']+) close_square_bracket() {
                (id, None)
            }
        )
        end:position!()
        eol()
        {
            let (id, reftext) = result;
            let substituted_id = state.intern_cow(substitute(id, HEADER, &state.document_attributes));
            let substituted_reftext = reftext.map(|rt| state.intern_cow(substitute(rt, HEADER, &state.document_attributes)));
            Anchor {
                id: substituted_id,
                xreflabel: substituted_reftext,
                location: state.create_location(start, end)
            }
        }

        rule inline_anchor(offset: usize) -> InlineNode<'input>
        = start:position!()
        double_open_square_bracket()
        // Whitespace is excluded - IDs must not contain spaces
        warn_anchor_id_with_whitespace()?
        id:$([^'\'' | ',' | ']' | '.' | ' ' | '\t' | '\n' | '\r']+)
        reftext:(
            comma() reftext:$([^']']+) {
                Some(reftext)
            } /
            {
                None
            }
        )
        double_close_square_bracket()
        end:position!()
        {
            let substituted_id = state.intern_cow(substitute(id, HEADER, &state.document_attributes));
            let substituted_reftext = reftext.map(|rt| state.intern_cow(substitute(rt, HEADER, &state.document_attributes)));
            InlineNode::InlineAnchor(Anchor {
                id: substituted_id,
                xreflabel: substituted_reftext,
                location: state.create_block_location(start, end, offset)
            })
        }

        rule inline_anchor_match() -> ()
        = double_open_square_bracket() [^'\'' | ',' | ']' | '.' | ' ' | '\t' | '\n' | '\r']+ (comma() [^']']+)? double_close_square_bracket()

        /// Bibliography anchor: `[[[id]]]` or `[[[id,reftext]]]`
        /// Must be parsed before inline_anchor to avoid capturing `[id` as the ID
        rule bibliography_anchor(offset: usize) -> InlineNode<'input>
        = start:position!()
        "[[["
        warn_anchor_id_with_whitespace()?
        id:$([^'\'' | ',' | ']' | '[' | '.' | ' ' | '\t' | '\n' | '\r']+)
        reftext:(comma() reftext:$([^']']+) { Some(reftext) } / { None })
        "]]]"
        end:position!()
        {
            let substituted_id = state.intern_cow(substitute(id, HEADER, &state.document_attributes));
            let substituted_reftext = reftext.map(|rt| state.intern_cow(substitute(rt, HEADER, &state.document_attributes)));
            InlineNode::InlineAnchor(Anchor {
                id: substituted_id,
                xreflabel: substituted_reftext,
                location: state.create_block_location(start, end, offset)
            })
        }

        rule attributes_line() -> (bool, BlockMetadata<'input>)
            // Don't match empty [] followed by blank line - that's a list separator, not
            // block attributes. Without this, `[]\n\n` would be parsed as an empty
            // attributes line, breaking list separation
            = !empty_list_separator() attributes:attributes() eol() {
                let (discrete, metadata, _title_position) = attributes;
                (discrete, metadata)
            }

        // Empty brackets followed by a blank line is a list separator
        rule empty_list_separator()
            = whitespace()* "[" whitespace()* "]" whitespace()* eol() eol()

        pub(crate) rule attributes() -> (bool, BlockMetadata<'input>, Option<(usize, usize)>)
            = start:position!() open_square_bracket() content:(
                // The case in which we keep the style empty
                attributes:(comma() att:attribute() { att })+ {
                    tracing::debug!(?attributes, "Found empty style with attributes");
                    (true, None, attributes)
                } /
                // The case in which there is a block style and other attributes
                style:block_style() attributes:(comma() att:attribute() { att })+ {
                    tracing::debug!(?style, ?attributes, "Found block style with attributes");
                    (false, Some(style), attributes)
                } /
                // The case in which there is a block style and no other attributes
                style:block_style() {
                    tracing::debug!(?style, "Found block style");
                    (false, Some(style), vec![])
                } /
                // The case in which there are only attributes
                attributes:(att:attribute() comma()? { att })* {
                    tracing::debug!(?attributes, "Found attributes");
                    (false, None, attributes)
                })
            close_square_bracket() end:position!() {
                let mut discrete = false;
                let (_empty, maybe_style, attributes) = content;
                let mut metadata = BlockMetadata::default();

                // Process block style (shorthands like .role, #id, %option)
                if let Some((maybe_style_name, id, roles, options)) = maybe_style {
                    if let Some(style_name) = maybe_style_name {
                        if style_name == "discrete" {
                            discrete = true;
                        } else if metadata.style.is_none() {
                            metadata.style = Some(state.intern_cow(style_name));
                        } else {
                            metadata.attributes.insert(style_name, AttributeValue::None);
                        }
                    }
                    metadata.id = id;
                    metadata.roles.extend(roles.into_iter().map(|r| state.intern_cow(r)));
                    metadata.options.extend(options.into_iter().map(|o| state.intern_cow(o)));
                }

                // Process attribute list using shared helper
                let title_position = process_attribute_list(
                    attributes.iter().cloned(),
                    &mut metadata,
                    state,
                    start,
                    end,
                    AttributeProcessingMode::BLOCK,
                );

                // Handle subs= attribute (block-specific, feature-gated)
                if cfg!(feature = "pre-spec-subs") {
                    for (k, v, pos) in attributes.iter().flatten() {
                        if *k == RESERVED_NAMED_ATTRIBUTE_SUBS && let AttributeValue::String(v) = v {
                            let location = pos.map_or_else(
                                || state.create_location(start, end),
                                |(s, e)| state.create_location(s, e),
                            );
                            state.add_generic_warning_at(
                                "The subs= attribute is experimental and may change when the AsciiDoc specification is finalized. See: https://gitlab.eclipse.org/eclipse/asciidoc-lang/asciidoc-lang/-/issues/16".to_string(),
                                location,
                            );
                            metadata.substitutions = Some(parse_subs_attribute(v));
                        }
                    }
                }

                // Extract attribution/citetitle for quote/verse styles using positions
                // from the original attributes vec (before positional_attributes is consumed)
                if metadata.style == Some("quote") || metadata.style == Some("verse") {
                    let positional_positions: Vec<Option<(usize, usize)>> = attributes.iter()
                        .flatten()
                        .filter(|(_, v, _)| *v == AttributeValue::None)
                        .map(|(_, _, pos)| *pos)
                        .collect();

                    if metadata.positional_attributes.len() >= 2 {
                        let cite = metadata.positional_attributes.remove(1).trim().to_string();
                        if !cite.is_empty() {
                            let loc = positional_positions.get(1).copied().flatten()
                                .map_or_else(Location::default, |(s, e)| state.create_location(s, e));
                            metadata.citetitle = Some(CiteTitle::new(vec![InlineNode::PlainText(Plain {
                                content: state.intern_str(&cite),
                                location: loc,
                                escaped: false,
                            })]));
                        }
                    }
                    if !metadata.positional_attributes.is_empty() {
                        let attr = metadata.positional_attributes.remove(0).trim().to_string();
                        if !attr.is_empty() {
                            let loc = positional_positions.first().copied().flatten()
                                .map_or_else(Location::default, |(s, e)| state.create_location(s, e));
                            metadata.attribution = Some(Attribution::new(vec![InlineNode::PlainText(Plain {
                                content: state.intern_str(&attr),
                                location: loc,
                                escaped: false,
                            })]));
                        }
                    }
                }

                (discrete, metadata, title_position)
            }

        /// Macro attribute parsing - simpler than block attributes.
        ///
        /// Does NOT support shorthand syntax (.role, #id, %option).
        /// Shorthands are only valid in block-level attributes, not inside macro brackets.
        ///
        /// Asciidoctor behavior:
        /// - `image::photo.jpg[.role]` -> alt=".role" (literal text, NOT a role)
        /// - `image::photo.jpg[Diablo 4 picture of Lilith.]` -> alt="Diablo 4 picture of Lilith."
        pub(crate) rule macro_attributes() -> (bool, BlockMetadata<'input>, Option<(usize, usize)>)
            = start:position!() open_square_bracket()
              attrs:(att:macro_attribute() comma()? { att })*
              close_square_bracket() end:position!()
        {
            let mut metadata = BlockMetadata::default();
            let title_position = process_attribute_list(
                attrs,
                &mut metadata,
                state,
                start,
                end,
                AttributeProcessingMode::MACRO,
            );
            // macro_attributes never sets discrete flag (that's block-level only)
            (false, metadata, title_position)
        }

        /// Positional value in macro attributes - allows . # % as literal characters
        /// This is the key difference from block attributes.
        rule macro_positional_value() -> Option<String>
            = quoted:inner_attribute_value() {
                let trimmed = strip_quotes(&quoted);
                if trimmed.is_empty() { None } else { Some(trimmed.to_string()) }
            }
            / s:$([^('"' | ',' | ']' | '=')]+) {
                let trimmed = s.trim();
                if trimmed.is_empty() { None } else { Some(trimmed.to_string()) }
            }

        /// Named attribute or additional positional in macro context
        rule macro_attribute() -> Option<(Cow<'input, str>, AttributeValue<'input>, Option<(usize, usize)>)>
            = whitespace()* att:named_attribute() { att }
            / val:macro_positional_value() {
                val.map(|v| (Cow::Owned(v), AttributeValue::None, None))
            }

        rule open_square_bracket() = "["
        rule close_square_bracket() = "]"
        rule double_open_square_bracket() = "[["
        rule double_close_square_bracket() = "]]"
        rule comma() = ","
        rule period() = "."
        rule empty_style() = ""
        rule role() -> &'input str = $([^(',' | ']' | '#' | '.' | '%')]+)

        /// Parse a single attribute shorthand: .role, #id, or %option
        /// Used by block_style() for block-level attributes
        rule shorthand() -> Shorthand<'input>
        = "#" id:block_style_id() { Shorthand::Id(Cow::Borrowed(id)) }
        / "." role:role() { Shorthand::Role(Cow::Borrowed(role)) }
        / "%" option:option() { Shorthand::Option(Cow::Borrowed(option)) }

        // The option rule is used to parse options in the form of "option=value" or
        // "%option" (we don't capture the % here).
        //
        // The option can be a single word or a quoted string. If it is a quoted string,
        // it can contain commas, which we then look for and extract the options in the
        // `attributes()` rule.
        rule option() -> &'input str =
        $(("\"" [^('"' | ']' | '#' | '.' | '%')]+ "\"") / ([^('"' | ',' | ']' | '#' | '.' | '%')]+))

        rule attribute_name() -> &'input str = $((['A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_'])+)

        pub(crate) rule attribute() -> Option<(Cow<'input, str>, AttributeValue<'input>, Option<(usize, usize)>)>
            = whitespace()* att:named_attribute() { att }
              / whitespace()* start:position!() att:positional_attribute_value() end:position!() {
                  let substituted = substitute(&att, &[Substitution::Attributes], &state.document_attributes).into_owned();
                  Some((Cow::Owned(substituted), AttributeValue::None, Some((start, end))))
              }

        // Add a simple ID rule
        rule id() -> String
            = id:$((['A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_'])+) { id.to_string() }

        // TODO(nlopes): this should instead return an enum
        rule named_attribute() -> Option<(Cow<'input, str>, AttributeValue<'input>, Option<(usize, usize)>)>
            = "id" "=" start:position!() id:id() end:position!()
                { Some((Cow::Borrowed(RESERVED_NAMED_ATTRIBUTE_ID), AttributeValue::String(Cow::Owned(id)), Some((start, end)))) }
              / ("role" / "roles") "=" value:named_attribute_value()
                { Some((Cow::Borrowed(RESERVED_NAMED_ATTRIBUTE_ROLE), AttributeValue::String(Cow::Owned(value)), None)) }
              / ("options" / "opts") "=" value:named_attribute_value()
                { Some((Cow::Borrowed(RESERVED_NAMED_ATTRIBUTE_OPTIONS), AttributeValue::String(Cow::Owned(value)), None)) }
              / name:attribute_name() "=" start:position!() value:named_attribute_value() end:position!()
                {
                    let substituted_value = substitute(&value, &[Substitution::Attributes], &state.document_attributes).into_owned();
                    Some((Cow::Borrowed(name), AttributeValue::String(Cow::Owned(substituted_value)), Some((start, end))))
                }

        // The block style is a positional attribute that is used to set the style of a block element.
        //
        // It has an optional "style", followed by the attribute shorthands.
        //
        // # - ID
        // . - role
        // % - option
        //
        // Each shorthand entry is placed directly adjacent to previous one, starting
        // immediately after the optional block style. The order of the entries does not
        // matter, except for the style, which must come first.
        pub(crate) rule block_style() -> (Option<Cow<'input, str>>, Option<Anchor<'input>>, Vec<Cow<'input, str>>, Vec<Cow<'input, str>>)
            = start:position!() content:(
                style:positional_attribute_value() shorthands:(
                    "#" id_start:position!() id:block_style_id() id_end:position!() {
                        (Shorthand::Id(Cow::Owned(id.to_string())), Some((id_start, id_end)))
                    }
                    / s:shorthand() { (s, None) }
                )+ {
                    (Some(Cow::Owned(style)), shorthands)
                } /
                style:positional_attribute_value() !"=" {
                    tracing::debug!(%style, "Found block style without shorthands");
                    (Some(Cow::Owned(style)), Vec::new())
                } /
                shorthands:(
                    "#" id_start:position!() id:block_style_id() id_end:position!() {
                        (Shorthand::Id(Cow::Owned(id.to_string())), Some((id_start, id_end)))
                    }
                    / s:shorthand() { (s, None) }
                )+ {
                    (None, shorthands)
               }
            )
            end:position!() {
                let (style, shorthands) = content;
                let mut maybe_anchor = None;
                let mut roles = Vec::new();
                let mut options = Vec::new();
                for (shorthand, pos) in shorthands {
                    match shorthand {
                        Shorthand::Id(id) => {
                            let (id_start, id_end) = pos.unwrap_or((start, end));
                            maybe_anchor = Some(Anchor {
                                id: state.intern_cow(id),
                                xreflabel: None,
                                location: state.create_location(id_start, id_end)
                            });
                        },
                        Shorthand::Role(role) => roles.push(role),
                        Shorthand::Option(option) => options.push(option),
                    }
                }
                (style, maybe_anchor, roles, options)
            }

        rule id_start_char() = ['A'..='Z' | 'a'..='z' | '_']

        rule block_style_id() -> &'input str = $(id_start_char() block_style_id_subsequent_char()*)

        rule block_style_id_subsequent_char() =
            ['A'..='Z' | 'a'..='z' | '0'..='9' | '_' | '-']

        rule named_attribute_value() -> String
        = &("\"" / "'") inner:inner_attribute_value()
        {
            // Strip surrounding quotes from quoted values
            let trimmed = strip_quotes(&inner);
            tracing::debug!(%inner, %trimmed, "Found named attribute value (inner)");
            trimmed.to_string()
        }
        / s:$([^(',' | '"' | '\'' | ']')]+)
        {
            tracing::debug!(%s, "Found named attribute value");
            s.to_string()
        }

        rule positional_attribute_value() -> String
        = quoted:inner_attribute_value() {
            let trimmed = strip_quotes(&quoted);
            tracing::debug!(%quoted, %trimmed, "Found quoted positional attribute value");
            trimmed.to_string()
        }
        / s:$([^('"' | ',' | ']' | '#' | '.' | '%')] [^(',' | ']' | '#' | '.' | '%' | '=')]*)
        {
            let trimmed = s.trim();
            tracing::debug!(%s, %trimmed, "Found unquoted positional attribute value");
            trimmed.to_string()
        }

        rule inner_attribute_value() -> String
        = s:$("\"" [^'"']* "\"") { s.to_string() }
        / s:$("'" [^'\'']* "'") { s.to_string() }

        /// URL rule matches both web URLs (proto://) and mailto: URLs
        pub rule url() -> String =
        proto:$("https" / "http" / "ftp" / "irc") "://" path:url_path() { format!("{proto}://{path}") }
        / "mailto:" email:email_address() { format!("mailto:{email}") }

        /// Email address pattern (RFC 822 simplified)
        ///
        /// Local part: alphanumeric plus . _ % + -
        /// Domain: alphanumeric plus . - (must contain TLD, must end with alphanumeric)
        ///
        /// - Domain must contain at least one dot (e.g., `foo@bar` is not valid,
        ///   `foo@bar.com` is)
        ///
        /// - Domain must end with alphanumeric (prevents capturing trailing punctuation
        ///   like `user@example.com.` - the dot stays outside the email for sentence
        ///   endings)
        rule email_address() -> String
        = local:$(
            // Quoted local part: "Jane Doe"@example.com
            // Quotes allow spaces and special chars in the local part (RFC 5321).
            "\"" [^'"']+ "\""
            // Unquoted local part (no spaces allowed)
            / ['a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '_' | '%' | '+' | '-']+
        )
        "@"
        // Format: alphanumeric+ (separator alphanumeric+)*
        // This ensures domain ends with alphanumeric (not . or -) and has proper structure.
        // e.g., `example.com.` -> matches `example.com`, trailing dot stays outside
        domain:$(
            ['a'..='z' | 'A'..='Z' | '0'..='9']+
            (['.' | '-'] ['a'..='z' | 'A'..='Z' | '0'..='9']+)*
        )
        {?
            // Require TLD - domain must contain at least one dot. This prevents `foo@bar`
            // from becoming a mailto link.
            if !domain.contains('.') {
                return Err("email domain must have TLD (contain a dot)");
            }

            Ok(format!("{local}@{domain}"))
        }

        /// URL path component - supports query params, fragments, encoding, etc.
        /// Excludes '[' and ']' to respect AsciiDoc macro/attribute boundaries
        rule url_path() -> String = path:$(['A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '.' | '_' | '~' | ':' | '/' | '?' | '#' | '@' | '!' | '$' | '&' | '\'' | '(' | ')' | '*' | '+' | ',' | ';' | '=' | '%' | '\\' ]+)
        {?
            let inline_state = InlinePreprocessorParserState::new_all_enabled(
                path,
                state.line_map.clone(),
                state.input,
                state.arena,
            );
            let processed = inline_preprocessing::run(path, &state.document_attributes, &inline_state)
            .map_err(|e| {
                tracing::error!(?e, "could not preprocess url path");
                "could not preprocess url path"
            })?;
            // Strip backslash escapes before URL parsing to prevent the url crate
            // from normalizing backslashes to forward slashes
            let result = strip_url_backslash_escapes(&processed.text).into_owned();
            let warnings = inline_state.drain_warnings();
            drop(inline_state);
            for warning in warnings {
                state.add_warning(warning);
            }
            Ok(result)
        }

        /// URL for bare autolinks — avoids capturing trailing sentence punctuation
        /// (., ;, !, etc.) by only consuming punctuation when more URL chars follow.
        rule bare_url() -> String =
        proto:$("https" / "http" / "ftp" / "irc") "://" path:bare_url_path()
        { format!("{proto}://{path}") }

        /// URL path for bare autolinks. Like url_path() but:
        /// - Trailing punctuation (. , ; ! ? : ' *) only consumed when followed by more URL chars.
        /// - `)` only consumed as part of a balanced `(...)` group, preventing capture of
        ///   sentence-level parens like `(see http://example.com)`.
        rule bare_url_path() -> String = path:$(
            bare_url_safe_char()
            ( bare_url_safe_char()
            / bare_url_paren_group()
            / "("
            / bare_url_trailing_char() &bare_url_char()
            )*
        )
        {?
            let inline_state = InlinePreprocessorParserState::new_all_enabled(
                path,
                state.line_map.clone(),
                state.input,
                state.arena,
            );
            let processed = inline_preprocessing::run(path, &state.document_attributes, &inline_state)
                .map_err(|e| {
                    tracing::error!(?e, "could not preprocess bare url path");
                    "could not preprocess bare url path"
                })?;
            let result = strip_url_backslash_escapes(&processed.text).into_owned();
            let warnings = inline_state.drain_warnings();
            drop(inline_state);
            for warning in warnings {
                state.add_warning(warning);
            }
            Ok(result)
        }

        /// Balanced parenthesized group in a URL path.
        /// Handles nested parens: `http://example.com/wiki/Foo_(bar_(baz))`
        /// Only `)` consumed via this rule — unbalanced `)` is never captured.
        rule bare_url_paren_group()
        = "(" (bare_url_safe_char() / bare_url_trailing_char() / bare_url_paren_group() / "(")* ")"

        /// URL chars that are safe to end a bare URL — won't be confused with sentence punctuation.
        /// Excludes `(` and `)` which are handled separately via `bare_url_paren_group`.
        rule bare_url_safe_char() = ['A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '~'
            | '/' | '#' | '@' | '$' | '&'
            | '+' | '=' | '%' | '\\']

        /// URL chars that are valid mid-URL but should not end a bare URL.
        /// Excludes `)` which is only consumed via balanced `bare_url_paren_group`.
        rule bare_url_trailing_char() = ['.' | ',' | ';' | '!' | '?' | ':' | '\'' | '*']

        /// Any valid URL path char (for lookahead in trailing char rule).
        /// Includes `(` because it can start a paren group.
        /// Excludes `)` so that trailing chars before `)` aren't greedily consumed
        /// (e.g., `http://example.com.)` keeps both `.` and `)` outside).
        rule bare_url_char() = bare_url_safe_char() / bare_url_trailing_char() / "("

        /// Fragment identifier for URLs and cross-references (e.g., `#section-id`)
        /// Only used by `xref:` and `link:` macros — other macros (`image::`, `video::`, etc.) do not support fragments
        rule path_fragment() -> String
            = "#" fragment:$(['a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-']+)
        {
            format!("#{fragment}")
        }

        /// Filesystem path - conservative character set for cross-platform compatibility
        /// Includes '{' and '}' for `AsciiDoc` attribute substitution
        pub rule path() -> String = path:$(['A'..='Z' | 'a'..='z' | '0'..='9' | '{' | '}' | '_' | '-' | '.' | '/' | '\\' ]+)
        {?
            let inline_state = InlinePreprocessorParserState::new_all_enabled(
                path,
                state.line_map.clone(),
                state.input,
                state.arena,
            );
            let processed = inline_preprocessing::run(path, &state.document_attributes, &inline_state)
            .map_err(|e| {
                tracing::error!(?e, "could not preprocess path");
                "could not preprocess path"
            })?;
            let result = processed.text.into_owned();
            let warnings = inline_state.drain_warnings();
            drop(inline_state);
            for warning in warnings {
                state.add_warning(warning);
            }
            Ok(result)
        }


        pub rule source() -> Source<'input>
            = source:
        (
            u:url() {?
                let interned = state.intern_str(&u);
                Source::from_str_borrowed(interned).map_err(|_| "failed to parse URL")
            }
            / p:path() {?
                let interned = state.intern_str(&p);
                Source::from_str_borrowed(interned).map_err(|_| "failed to parse path")
            }
        )
        { source }

        rule digits() = ['0'..='9']+

        rule whitespace() = quiet!{ " " / "\t" }
        rule eol() = quiet!{ "\n" }

        rule comment_line() = quiet!{ comment() (eol() / ![_]) }
        rule comment() = quiet!{ "//" [^'\n']+ (&eol() / ![_]) }

        // Value parsing for document attributes
        // Handles both single-line values and values with continuation markers (" \" or " + \")
        // The preprocessor preserves these markers for the parser to handle
        rule document_attribute_value() -> String
        = " " lines:document_attribute_value_lines()
        {
            lines.join("\n")
        }

        // Parse value lines, continuing while lines end with backslash
        rule document_attribute_value_lines() -> Vec<&'input str>
        = backslash_continuation_lines() / single_line:$([^'\n']+) { vec![single_line] }

        // Lines ending with backslash continuation - keeps consuming lines until one doesn't end with backslash
        rule backslash_continuation_lines() -> Vec<&'input str>
        = lines:(line:$((!(" \\" eol()) [^'\n'])+ " \\") eol() { line })+
          last:$([^'\n']+)?
        {
            let mut result = lines;
            if let Some(l) = last {
                result.push(l);
            }
            result
        }

        // Document attribute parsing
        // Works identically in both header and block metadata contexts
        rule document_attribute_match() -> AttributeEntry<'input>
        = ":"
        key_entry:(
            "!" key:$([^':']+) { (false, key) }
            / key:$([^('!' | ':')]+) "!" { (false, key) }
            / key:$([^':']+) { (true, key) }
        )
        ":" &" "?
        value:document_attribute_value()?
        {
            let (set, key) = key_entry;
            let attr_value = if !set {
                AttributeValue::Bool(false)
            } else if let Some(v) = value {
                let trimmed = v.trim();
                match trimmed {
                    "true" => AttributeValue::Bool(true),
                    "false" => AttributeValue::Bool(false),
                    _ => AttributeValue::String(Cow::Owned(v)),
                }
            } else {
                AttributeValue::Bool(true)
            };
            AttributeEntry { set, key, value: attr_value }
        }
        / expected!("document attribute key starting with ':'")

        rule position() -> PositionWithOffset = offset:position!() {
            PositionWithOffset {
                offset,
                position: state.line_map.offset_to_position(offset, state.input)
            }
        }

    }
}

/// Resolves callouts in verbatim text, converting them to structured `CalloutRef` nodes.
///
/// This function scans verbatim content for callout markers (`<1>`, `<.>`, etc.) and
/// splits the content into alternating `VerbatimText` and `CalloutRef` inline nodes.
/// Auto-numbered callouts (`<.>`) are resolved to explicit numbers.
///
/// # Arguments
/// * `text` - The raw verbatim text that may contain callout markers
/// * `base_location` - The location of the verbatim content block (used for all nodes)
///
/// # Returns
/// A tuple of:
/// - `Vec<InlineNode>` - Alternating `VerbatimText` and `CalloutRef` nodes
/// - `Vec<CalloutRef>` - Just the callout references (for validation with callout lists)
fn resolve_verbatim_callouts<'a>(
    arena: &'a bumpalo::Bump,
    text: &str,
    base_location: Location,
) -> (Vec<InlineNode<'a>>, Vec<CalloutRef>) {
    let mut inlines = Vec::new();
    let mut callouts = Vec::new();
    let mut auto_number = 1usize;
    // Build text directly in the arena: each flush hands ownership of the
    // current `BumpString` to the AST via `into_bump_str()`, then we start
    // fresh in the same arena. Avoids the heap-`String`-then-arena-copy
    // round-trip per `VerbatimText` node.
    let mut current_text = bumpalo::collections::String::new_in(arena);

    for (line_idx, line) in text.lines().enumerate() {
        // Add newline separator between lines (except first)
        if line_idx > 0 {
            current_text.push('\n');
        }

        let trimmed_end = line.trim_end();

        // Check for auto-numbered callout <.>
        if let Some(pos) = trimmed_end.rfind("<.>") {
            // Add text before the callout
            current_text.push_str(&line[..pos]);

            // Flush current text as VerbatimText
            if !current_text.is_empty() {
                let flushed = std::mem::replace(
                    &mut current_text,
                    bumpalo::collections::String::new_in(arena),
                );
                inlines.push(InlineNode::VerbatimText(Verbatim {
                    content: flushed.into_bump_str(),
                    location: base_location.clone(),
                }));
            }

            // Create CalloutRef for auto-numbered callout
            let callout_ref = CalloutRef::auto(auto_number, base_location.clone());
            inlines.push(InlineNode::CalloutRef(callout_ref.clone()));
            callouts.push(callout_ref);
            auto_number += 1;

            // Add any trailing content after the callout marker
            let after_marker = &line[pos + 3..];
            if !after_marker.is_empty() {
                current_text.push_str(after_marker);
            }
        } else if let Some((number, marker_start)) =
            extract_callout_number_with_position(trimmed_end)
        {
            // Found an explicit callout like <5>
            // Add text before the callout
            current_text.push_str(&line[..marker_start]);

            // Flush current text as VerbatimText
            if !current_text.is_empty() {
                let flushed = std::mem::replace(
                    &mut current_text,
                    bumpalo::collections::String::new_in(arena),
                );
                inlines.push(InlineNode::VerbatimText(Verbatim {
                    content: flushed.into_bump_str(),
                    location: base_location.clone(),
                }));
            }

            // Create CalloutRef for explicit callout
            let callout_ref = CalloutRef::explicit(number, base_location.clone());
            inlines.push(InlineNode::CalloutRef(callout_ref.clone()));
            callouts.push(callout_ref);

            // Add any trailing content after the callout marker
            // Find the end of the marker (the '>')
            if let Some(marker_end_relative) = trimmed_end[marker_start..].find('>') {
                let marker_end = marker_start + marker_end_relative + 1;
                let after_marker = &line[marker_end..];
                if !after_marker.is_empty() {
                    current_text.push_str(after_marker);
                }
            }
        } else {
            // No callout on this line, just add the content
            current_text.push_str(line);
        }
    }

    // Flush any remaining text
    if !current_text.is_empty() {
        inlines.push(InlineNode::VerbatimText(Verbatim {
            content: current_text.into_bump_str(),
            location: base_location,
        }));
    }

    (inlines, callouts)
}

/// Extract callout number and its start position from a line ending with `<N>`
fn extract_callout_number_with_position(line: &str) -> Option<(usize, usize)> {
    if line.ends_with('>')
        && let Some(start) = line.rfind('<')
    {
        let number_str = &line[start + 1..line.len() - 1];
        number_str.parse().ok().map(|n| (n, start))
    } else {
        None
    }
}

/// Extract callout number from a line ending with <N>
fn extract_callout_number(line: &str) -> Option<usize> {
    if line.ends_with('>')
        && let Some(start) = line.rfind('<')
    {
        let number_str = &line[start + 1..line.len() - 1];
        number_str.parse().ok()
    } else {
        None
    }
}

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::unreachable
)]
mod tests {
    use super::*;

    #[test]
    #[tracing_test::traced_test]
    fn test_document() -> Result<(), Error> {
        let input = "// this comment line is ignored
= Document Title
Lorn_Kismet R. Lee <kismet@asciidoctor.org>; Norberto M. Lopes <nlopesml@gmail.com>
v2.9, 01-09-2024: Fall incarnation
:description: The document's description.
:sectanchors:
:url-repo: https://my-git-repo.com";
        let mut state = ParserState::new_for_test(input);
        let result = document_parser::document(input, &mut state)??;
        let header = result.header.expect("document has a header");
        assert_eq!(header.title.len(), 1);
        assert_eq!(
            header.title[0],
            InlineNode::PlainText(Plain {
                content: "Document Title",
                location: Location {
                    absolute_start: 34,
                    absolute_end: 47,
                    start: crate::Position { line: 2, column: 3 },
                    end: crate::Position {
                        line: 2,
                        column: 16,
                    },
                },
                escaped: false,
            })
        );
        assert_eq!(header.authors.len(), 2);
        assert_eq!(header.authors[0].first_name, "Lorn Kismet");
        assert_eq!(header.authors[0].middle_name, Some("R."));
        assert_eq!(header.authors[0].last_name, "Lee");
        assert_eq!(header.authors[0].initials, "LRL");
        assert_eq!(header.authors[0].email, Some("kismet@asciidoctor.org"));
        assert_eq!(header.authors[1].first_name, "Norberto");
        assert_eq!(header.authors[1].middle_name, Some("M."));
        assert_eq!(header.authors[1].last_name, "Lopes");
        assert_eq!(header.authors[1].initials, "NML");
        assert_eq!(header.authors[1].email, Some("nlopesml@gmail.com"));
        assert_eq!(
            state.document_attributes.get("revnumber"),
            Some(&AttributeValue::String("v2.9".into()))
        );
        assert_eq!(
            state.document_attributes.get("revdate"),
            Some(&AttributeValue::String("01-09-2024".into()))
        );
        assert_eq!(
            state.document_attributes.get("revremark"),
            Some(&AttributeValue::String("Fall incarnation".into()))
        );
        assert_eq!(
            state.document_attributes.get("description"),
            Some(&AttributeValue::String(
                "The document's description.".into()
            ))
        );
        assert_eq!(
            state.document_attributes.get("sectanchors"),
            Some(&AttributeValue::Bool(true))
        );
        assert_eq!(
            state.document_attributes.get("url-repo"),
            Some(&AttributeValue::String("https://my-git-repo.com".into()))
        );
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_authors() -> Result<(), Error> {
        let input =
            "Lorn_Kismet R. Lee <kismet@asciidoctor.org>; Norberto M. Lopes <nlopesml@gmail.com>";
        let mut state = ParserState::new_for_test(input);
        let result = document_parser::authors(input, &mut state)?;

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].first_name, "Lorn Kismet");
        assert_eq!(result[0].middle_name, Some("R."));
        assert_eq!(result[0].last_name, "Lee");
        assert_eq!(result[0].initials, "LRL");
        assert_eq!(result[0].email, Some("kismet@asciidoctor.org"));
        assert_eq!(result[1].first_name, "Norberto");
        assert_eq!(result[1].middle_name, Some("M."));
        assert_eq!(result[1].last_name, "Lopes");
        assert_eq!(result[1].initials, "NML");
        assert_eq!(result[1].email, Some("nlopesml@gmail.com"));
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_author() -> Result<(), Error> {
        let input = "Norberto M. Lopes supa dough <nlopesml@gmail.com>";
        let mut state = ParserState::new_for_test(input);
        let result = document_parser::author(input, &mut state)?;
        assert_eq!(result.first_name, "Norberto");
        assert_eq!(result.middle_name, Some("M."));
        assert_eq!(result.last_name, "Lopes supa dough");
        assert_eq!(result.initials, "NML");
        assert_eq!(result.email, Some("nlopesml@gmail.com"));
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_compound_first_name() -> Result<(), Error> {
        let input = "Ann_Marie Jenson";
        let mut state = ParserState::new_for_test(input);
        let result = document_parser::author(input, &mut state)?;
        assert_eq!(result.first_name, "Ann Marie");
        assert_eq!(result.middle_name, None);
        assert_eq!(result.last_name, "Jenson");
        assert_eq!(result.initials, "AJ");
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_compound_last_name() -> Result<(), Error> {
        let input = "Tomás López_del_Toro";
        let mut state = ParserState::new_for_test(input);
        let result = document_parser::author(input, &mut state)?;
        assert_eq!(result.first_name, "Tomás");
        assert_eq!(result.middle_name, None);
        assert_eq!(result.last_name, "López del Toro");
        assert_eq!(result.initials, "TL");
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_compound_middle_name() -> Result<(), Error> {
        let input = "First Middle_Name Last";
        let mut state = ParserState::new_for_test(input);
        let result = document_parser::author(input, &mut state)?;
        assert_eq!(result.first_name, "First");
        assert_eq!(result.middle_name, Some("Middle Name"));
        assert_eq!(result.last_name, "Last");
        assert_eq!(result.initials, "FML");
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_multiple_compound_authors() -> Result<(), Error> {
        let input = "Ann_Marie Jenson; Tomás López_del_Toro";
        let mut state = ParserState::new_for_test(input);
        let result = document_parser::authors(input, &mut state)?;
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].first_name, "Ann Marie");
        assert_eq!(result[0].last_name, "Jenson");
        assert_eq!(result[0].initials, "AJ");
        assert_eq!(result[1].first_name, "Tomás");
        assert_eq!(result[1].last_name, "López del Toro");
        assert_eq!(result[1].initials, "TL");
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_unicode_author_name() -> Result<(), Error> {
        let input = "Tomás Müller";
        let mut state = ParserState::new_for_test(input);
        let result = document_parser::author(input, &mut state)?;
        assert_eq!(result.first_name, "Tomás");
        assert_eq!(result.last_name, "Müller");
        assert_eq!(result.initials, "TM");
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_revision_full() -> Result<(), Error> {
        let input = "v2.9, 01-09-2024: Fall incarnation";
        let mut state = ParserState::new_for_test(input);
        document_parser::revision(input, &mut state)?;
        assert_eq!(
            state.document_attributes.get("revnumber"),
            Some(&AttributeValue::String("v2.9".into()))
        );
        assert_eq!(
            state.document_attributes.get("revdate"),
            Some(&AttributeValue::String("01-09-2024".into()))
        );
        assert_eq!(
            state.document_attributes.get("revremark"),
            Some(&AttributeValue::String("Fall incarnation".into()))
        );
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_revision_with_date_no_remark() -> Result<(), Error> {
        let input = "v2.9, 01-09-2024";
        let mut state = ParserState::new_for_test(input);
        document_parser::revision(input, &mut state)?;
        assert_eq!(
            state.document_attributes.get("revnumber"),
            Some(&AttributeValue::String("v2.9".into()))
        );
        assert_eq!(
            state.document_attributes.get("revdate"),
            Some(&AttributeValue::String("01-09-2024".into()))
        );
        assert_eq!(state.document_attributes.get("revremark"), None);
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_revision_no_date_with_remark() -> Result<(), Error> {
        let input = "v2.9: Fall incarnation";
        let mut state = ParserState::new_for_test(input);
        document_parser::revision(input, &mut state)?;
        assert_eq!(
            state.document_attributes.get("revnumber"),
            Some(&AttributeValue::String("v2.9".into()))
        );
        assert_eq!(state.document_attributes.get("revdate"), None);
        assert_eq!(
            state.document_attributes.get("revremark"),
            Some(&AttributeValue::String("Fall incarnation".into()))
        );
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_revision_no_date_no_remark() -> Result<(), Error> {
        let input = "v2.9";
        let mut state = ParserState::new_for_test(input);
        document_parser::revision(input, &mut state)?;
        assert_eq!(
            state.document_attributes.get("revnumber"),
            Some(&AttributeValue::String("v2.9".into()))
        );
        assert_eq!(state.document_attributes.get("revdate"), None);
        assert_eq!(state.document_attributes.get("revremark"), None);
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_document_title() -> Result<(), Error> {
        let input = "= Document Title";
        let mut state = ParserState::new_for_test(input);
        let result = document_parser::document_title(input, &mut state)?;
        assert_eq!(result.0.len(), 1);
        assert_eq!(
            result.0[0],
            InlineNode::PlainText(Plain {
                content: "Document Title",
                location: Location {
                    absolute_start: 2,
                    absolute_end: 15,
                    start: crate::Position { line: 1, column: 3 },
                    end: crate::Position {
                        line: 1,
                        column: 16,
                    },
                },
                escaped: false,
            })
        );
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_document_title_and_subtitle() -> Result<(), Error> {
        let input = "= Document Title: And a subtitle";
        let mut state = ParserState::new_for_test(input);
        let result = document_parser::document_title(input, &mut state)?;
        assert_eq!(
            result,
            (
                Title::new(vec![InlineNode::PlainText(Plain {
                    content: "Document Title",
                    location: Location {
                        absolute_start: 2,
                        absolute_end: 15,
                        start: crate::Position { line: 1, column: 3 },
                        end: crate::Position {
                            line: 1,
                            column: 16,
                        },
                    },
                    escaped: false,
                })]),
                Some(Subtitle::new(vec![InlineNode::PlainText(Plain {
                    content: "And a subtitle",
                    location: Location {
                        absolute_start: 18,
                        absolute_end: 31,
                        start: crate::Position {
                            line: 1,
                            column: 19,
                        },
                        end: crate::Position {
                            line: 1,
                            column: 32,
                        },
                    },
                    escaped: false,
                })]))
            )
        );
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_header_with_title_and_authors() -> Result<(), Error> {
        let input = "= Document Title
Lorn_Kismet R. Lee <kismet@asciidoctor.org>; Norberto M. Lopes <nlopesml@gmail.com>";
        let mut state = ParserState::new_for_test(input);
        let result =
            document_parser::header(input, &mut state)??.expect("header should be present");
        assert_eq!(result.title.len(), 1);
        assert_eq!(
            result.title[0],
            InlineNode::PlainText(Plain {
                content: "Document Title",
                location: Location {
                    absolute_start: 2,
                    absolute_end: 15,
                    start: crate::Position { line: 1, column: 3 },
                    end: crate::Position {
                        line: 1,
                        column: 16,
                    },
                },
                escaped: false,
            })
        );
        assert_eq!(result.authors.len(), 2);
        assert_eq!(result.authors[0].first_name, "Lorn Kismet");
        assert_eq!(result.authors[0].middle_name, Some("R."));
        assert_eq!(result.authors[0].last_name, "Lee");
        assert_eq!(result.authors[0].initials, "LRL");
        assert_eq!(result.authors[0].email, Some("kismet@asciidoctor.org"));
        assert_eq!(result.authors[1].first_name, "Norberto");
        assert_eq!(result.authors[1].middle_name, Some("M."));
        assert_eq!(result.authors[1].last_name, "Lopes");
        assert_eq!(result.authors[1].initials, "NML");
        assert_eq!(result.authors[1].email, Some("nlopesml@gmail.com"));
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_document_empty_attribute_list() -> Result<(), Error> {
        let input = "[]";
        let mut state = ParserState::new_for_test(input);
        let (discrete, metadata, _title_position) = document_parser::attributes(input, &mut state)?;
        assert!(!discrete); // Not discrete
        assert_eq!(metadata.id, None);
        assert_eq!(metadata.style, None);
        assert!(metadata.roles.is_empty());
        assert!(metadata.options.is_empty());
        assert!(metadata.attributes.is_empty());
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_document_empty_attribute_list_with_discrete() -> Result<(), Error> {
        let input = "[discrete]";
        let mut state = ParserState::new_for_test(input);
        let (discrete, metadata, _title_position) = document_parser::attributes(input, &mut state)?;
        assert!(discrete); // Should be discrete
        assert_eq!(metadata.id, None);
        assert_eq!(metadata.style, None);
        assert!(metadata.roles.is_empty());
        assert!(metadata.options.is_empty());
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_document_attribute_with_id() -> Result<(), Error> {
        let input = "[id=my-id,role=admin,options=read,options=write]";
        let mut state = ParserState::new_for_test(input);
        let (discrete, metadata, _title_position) = document_parser::attributes(input, &mut state)?;
        assert!(!discrete); // Not discrete
        assert_eq!(
            metadata.id,
            Some(Anchor {
                id: "my-id",
                xreflabel: None,
                location: Location {
                    absolute_start: 4,
                    absolute_end: 9,
                    start: crate::Position { line: 1, column: 5 },
                    end: crate::Position {
                        line: 1,
                        column: 10,
                    }
                }
            })
        );
        assert_eq!(metadata.style, None);
        assert!(metadata.roles.contains(&"admin"));
        assert!(metadata.options.contains(&"read"));
        assert!(metadata.options.contains(&"write"));
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_document_attribute_with_id_mixed() -> Result<(), Error> {
        let input = "[astyle#myid.admin,options=read,options=write]";
        let mut state = ParserState::new_for_test(input);
        let (discrete, metadata, _title_position) = document_parser::attributes(input, &mut state)?;
        assert!(!discrete); // Not discrete
        assert_eq!(
            metadata.id,
            Some(Anchor {
                id: "myid",
                xreflabel: None,
                location: Location {
                    absolute_start: 8,
                    absolute_end: 12,
                    start: crate::Position { line: 1, column: 9 },
                    end: crate::Position {
                        line: 1,
                        column: 13,
                    }
                }
            })
        );
        assert_eq!(metadata.style, Some("astyle"));
        assert!(metadata.roles.contains(&"admin"));
        assert!(metadata.options.contains(&"read"));
        assert!(metadata.options.contains(&"write"));
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_document_attribute_with_id_mixed_with_quotes() -> Result<(), Error> {
        let input = "[astyle#myid.admin,options=\"read,write\"]";
        let mut state = ParserState::new_for_test(input);
        let (discrete, metadata, _title_position) = document_parser::attributes(input, &mut state)?;
        assert!(!discrete); // Not discrete
        assert_eq!(
            metadata.id,
            Some(Anchor {
                id: "myid",
                xreflabel: None,
                location: Location {
                    absolute_start: 8,
                    absolute_end: 12,
                    start: crate::Position { line: 1, column: 9 },
                    end: crate::Position {
                        line: 1,
                        column: 13,
                    }
                }
            })
        );
        assert_eq!(metadata.style, Some("astyle"));
        assert!(metadata.roles.contains(&"admin"));
        assert!(metadata.options.contains(&"read"));
        assert!(metadata.options.contains(&"write"));
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_shorthand_id_role_combined() -> Result<(), Error> {
        // Test [#id.role] syntax - ID with role, no style
        let input = "[#bracket-id.some-role]";
        let mut state = ParserState::new_for_test(input);
        let (discrete, metadata, _title_position) = document_parser::attributes(input, &mut state)?;
        assert!(!discrete);
        assert_eq!(
            metadata.id,
            Some(Anchor {
                id: "bracket-id",
                xreflabel: None,
                location: Location {
                    absolute_start: 2,
                    absolute_end: 12,
                    start: crate::Position { line: 1, column: 3 },
                    end: crate::Position {
                        line: 1,
                        column: 13,
                    }
                }
            })
        );
        assert_eq!(metadata.style, None);
        assert!(metadata.roles.contains(&"some-role"));
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_shorthand_id_role_option_combined() -> Result<(), Error> {
        // Test [#id.role%option] syntax - ID with role and option
        let input = "[#my-id.my-role%my-option]";
        let mut state = ParserState::new_for_test(input);
        let (discrete, metadata, _title_position) = document_parser::attributes(input, &mut state)?;
        assert!(!discrete);
        assert_eq!(
            metadata.id,
            Some(Anchor {
                id: "my-id",
                xreflabel: None,
                location: Location {
                    absolute_start: 2,
                    absolute_end: 7,
                    start: crate::Position { line: 1, column: 3 },
                    end: crate::Position { line: 1, column: 8 }
                }
            })
        );
        assert_eq!(metadata.style, None);
        assert!(metadata.roles.contains(&"my-role"));
        assert!(metadata.options.contains(&"my-option"));
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_shorthand_multiple_roles() -> Result<(), Error> {
        // Test [#id.role1.role2] syntax - ID with multiple roles
        let input = "[#my-id.role-one.role-two]";
        let mut state = ParserState::new_for_test(input);
        let (discrete, metadata, _title_position) = document_parser::attributes(input, &mut state)?;
        assert!(!discrete);
        assert_eq!(metadata.id.as_ref().map(|a| a.id), Some("my-id"));
        assert!(metadata.roles.contains(&"role-one"));
        assert!(metadata.roles.contains(&"role-two"));
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_shorthand_style_id_role() -> Result<(), Error> {
        // Test [style#id.role] syntax - already tested in test_document_attribute_with_id_mixed
        // but let's verify it still works
        let input = "[quote#my-id.my-role]";
        let mut state = ParserState::new_for_test(input);
        let (discrete, metadata, _title_position) = document_parser::attributes(input, &mut state)?;
        assert!(!discrete);
        assert_eq!(metadata.id.as_ref().map(|a| a.id), Some("my-id"));
        assert_eq!(metadata.style, Some("quote"));
        assert!(metadata.roles.contains(&"my-role"));
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_shorthand_just_roles() -> Result<(), Error> {
        // Test [.role1.role2] syntax - just roles, no ID
        let input = "[.role-one.role-two]";
        let mut state = ParserState::new_for_test(input);
        let (discrete, metadata, _title_position) = document_parser::attributes(input, &mut state)?;
        assert!(!discrete);
        assert_eq!(metadata.id, None);
        assert!(metadata.roles.contains(&"role-one"));
        assert!(metadata.roles.contains(&"role-two"));
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_toc_simple() -> Result<(), Error> {
        let input =
            "= Document Title\n\n== Section 1\n\nSome content.\n\n== Section 2\n\nMore content.";
        let mut state = ParserState::new_for_test(input);
        let result = document_parser::document(input, &mut state)??;

        // Check that TOC entries were generated
        assert_eq!(result.toc_entries.len(), 2);
        assert_eq!(result.toc_entries[0].level, 1);
        assert_eq!(result.toc_entries[0].id, "_section_1");
        assert_eq!(result.toc_entries[1].level, 1);
        assert_eq!(result.toc_entries[1].id, "_section_2");
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_toc_tree() -> Result<(), Error> {
        let input = "= Document Title\n\n== Section A\n\nContent A.\n\n=== Section A.1\n\nContent A.1\n\n== Section B\n\nContent B.";
        let mut state = ParserState::new_for_test(input);
        let result = document_parser::document(input, &mut state)??;

        // Check that TOC entries were generated and ordered correctly
        assert_eq!(result.toc_entries.len(), 3);
        assert_eq!(result.toc_entries[0].id, "_section_a");
        assert_eq!(result.toc_entries[1].id, "_section_a_1");
        assert_eq!(result.toc_entries[2].id, "_section_b");
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_toc_empty_document() -> Result<(), Error> {
        let input = "= Document Title\n\nJust some content without sections.";
        let mut state = ParserState::new_for_test(input);
        let result = document_parser::document(input, &mut state)??;
        assert_eq!(result.toc_entries.len(), 0);
        Ok(())
    }

    #[cfg(feature = "setext")]
    #[test]
    #[tracing_test::traced_test]
    fn test_setext_document_title() -> Result<(), Error> {
        let input = "Document Title
==============

Some content.
";
        let mut state = ParserState::new_for_test(input);
        std::rc::Rc::make_mut(&mut state.options).setext = true;
        let result = document_parser::document(input, &mut state)??;
        let header = result.header.expect("document has a header");
        assert_eq!(header.title.len(), 1);
        assert!(
            matches!(&header.title[0], InlineNode::PlainText(Plain { content, .. }) if *content == "Document Title")
        );
        Ok(())
    }

    #[cfg(feature = "setext")]
    #[test]
    #[tracing_test::traced_test]
    fn test_setext_section() -> Result<(), Error> {
        let input = "= Document Title

Section One
-----------

Content.
";
        let mut state = ParserState::new_for_test(input);
        std::rc::Rc::make_mut(&mut state.options).setext = true;
        let result = document_parser::document(input, &mut state)??;

        // Find the section
        let section = result.blocks.iter().find_map(|b| {
            if let Block::Section(s) = b {
                Some(s)
            } else {
                None
            }
        });
        let section = section.expect("should have a section");
        assert_eq!(section.level, 1);
        assert!(
            matches!(&section.title[0], InlineNode::PlainText(Plain { content, .. }) if *content == "Section One")
        );
        Ok(())
    }

    #[cfg(feature = "setext")]
    #[test]
    #[tracing_test::traced_test]
    fn test_setext_disabled_by_default() {
        let input = "Document Title
==============

Some content.
";
        let mut state = ParserState::new_for_test(input);
        // setext is disabled by default
        assert!(!state.options.setext);
        // Should not parse as setext title when disabled
        let result = document_parser::document(input, &mut state);
        // The document will be parsed but without recognizing the setext title
        // The title line will be parsed as a paragraph or similar
        if let Ok(Ok(doc)) = result {
            // No header should be found when setext is disabled
            assert!(doc.header.is_none());
        }
    }

    #[cfg(feature = "setext")]
    #[test]
    #[tracing_test::traced_test]
    fn test_setext_single_section_per_level() -> Result<(), Error> {
        // Test a single setext section with document title
        // Note: Multiple same-level setext sections currently nest incorrectly
        // (tracked as known limitation). This test verifies basic functionality.
        let input = "Document Title
==============

Section One
-----------

Content here.
";
        let mut state = ParserState::new_for_test(input);
        std::rc::Rc::make_mut(&mut state.options).setext = true;
        let result = document_parser::document(input, &mut state)??;

        // Check document title (level 0)
        let header = result.header.expect("document has a header");
        assert!(
            matches!(&header.title[0], InlineNode::PlainText(Plain { content, .. }) if *content == "Document Title")
        );

        // Find the section
        let section = result
            .blocks
            .iter()
            .find_map(|b| {
                if let Block::Section(s) = b {
                    Some(s)
                } else {
                    None
                }
            })
            .expect("should have a section");

        assert_eq!(section.level, 1);
        assert!(
            matches!(&section.title[0], InlineNode::PlainText(Plain { content, .. }) if *content == "Section One")
        );

        Ok(())
    }

    #[cfg(feature = "setext")]
    #[test]
    #[tracing_test::traced_test]
    fn test_setext_sibling_sections() -> Result<(), Error> {
        // Test that multiple same-level setext sections are parsed as siblings, not nested
        let input = "Document Title
==============

Section A
---------

Content A.

Section B
---------

Content B.

Section C
---------

Content C.
";
        let mut state = ParserState::new_for_test(input);
        std::rc::Rc::make_mut(&mut state.options).setext = true;
        let result = document_parser::document(input, &mut state)??;

        // Check document title
        let header = result.header.expect("document has a header");
        assert!(
            matches!(&header.title[0], InlineNode::PlainText(Plain { content, .. }) if *content == "Document Title")
        );

        // All three sections should be at the top level (siblings, not nested)
        let sections: Vec<&Section> = result
            .blocks
            .iter()
            .filter_map(|b| {
                if let Block::Section(s) = b {
                    Some(s)
                } else {
                    None
                }
            })
            .collect();

        assert_eq!(
            sections.len(),
            3,
            "should have 3 top-level sibling sections"
        );

        // Verify all are level 1
        for (i, section) in sections.iter().enumerate() {
            assert_eq!(section.level, 1, "section {i} should be level 1");
        }

        // Verify titles
        assert!(
            matches!(&sections[0].title[0], InlineNode::PlainText(Plain { content, .. }) if *content == "Section A")
        );
        assert!(
            matches!(&sections[1].title[0], InlineNode::PlainText(Plain { content, .. }) if *content == "Section B")
        );
        assert!(
            matches!(&sections[2].title[0], InlineNode::PlainText(Plain { content, .. }) if *content == "Section C")
        );

        Ok(())
    }

    #[cfg(feature = "setext")]
    #[test]
    #[tracing_test::traced_test]
    fn test_setext_all_underline_characters() -> Result<(), Error> {
        // Test each setext underline character individually
        // = → level 0 (document title)
        // - → level 1
        // ~ → level 2
        // ^ → level 3
        // + → level 4

        // Test level 1 with -
        let input = "= Doc\n\nLevel One\n---------\n\nContent.\n";
        let mut state = ParserState::new_for_test(input);
        std::rc::Rc::make_mut(&mut state.options).setext = true;
        let result = document_parser::document(input, &mut state)??;
        let section = result
            .blocks
            .iter()
            .find_map(|b| {
                if let Block::Section(s) = b {
                    Some(s)
                } else {
                    None
                }
            })
            .expect("level 1 section");
        assert_eq!(section.level, 1);

        // Test level 2 with ~
        let input = "= Doc\n\nLevel Two\n~~~~~~~~~\n\nContent.\n";
        let mut state = ParserState::new_for_test(input);
        std::rc::Rc::make_mut(&mut state.options).setext = true;
        let result = document_parser::document(input, &mut state)??;
        let section = result
            .blocks
            .iter()
            .find_map(|b| {
                if let Block::Section(s) = b {
                    Some(s)
                } else {
                    None
                }
            })
            .expect("level 2 section");
        assert_eq!(section.level, 2);

        // Test level 3 with ^
        let input = "= Doc\n\nLevel Three\n^^^^^^^^^^^\n\nContent.\n";
        let mut state = ParserState::new_for_test(input);
        std::rc::Rc::make_mut(&mut state.options).setext = true;
        let result = document_parser::document(input, &mut state)??;
        let section = result
            .blocks
            .iter()
            .find_map(|b| {
                if let Block::Section(s) = b {
                    Some(s)
                } else {
                    None
                }
            })
            .expect("level 3 section");
        assert_eq!(section.level, 3);

        // Test level 4 with +
        let input = "= Doc\n\nLevel Four\n++++++++++\n\nContent.\n";
        let mut state = ParserState::new_for_test(input);
        std::rc::Rc::make_mut(&mut state.options).setext = true;
        let result = document_parser::document(input, &mut state)??;
        let section = result
            .blocks
            .iter()
            .find_map(|b| {
                if let Block::Section(s) = b {
                    Some(s)
                } else {
                    None
                }
            })
            .expect("level 4 section");
        assert_eq!(section.level, 4);

        Ok(())
    }

    #[cfg(feature = "setext")]
    #[test]
    #[tracing_test::traced_test]
    fn test_setext_manpage_style_document() -> Result<(), Error> {
        let input = "gitdatamodel(7)\n===============\n\nNAME\n----\ngitdatamodel - Git's core data model\n\nSYNOPSIS\n--------\ngitdatamodel\n";
        let mut state = ParserState::new_for_test(input);
        std::rc::Rc::make_mut(&mut state.options).setext = true;
        let result = document_parser::document(input, &mut state)??;

        // Verify document title parsed
        let header = result.header.expect("document has a header");
        assert!(
            matches!(&header.title[0], InlineNode::PlainText(Plain { content, .. }) if content.contains("gitdatamodel"))
        );

        // Verify NAME and SYNOPSIS are level-1 sections
        let sections: Vec<&Section> = result
            .blocks
            .iter()
            .filter_map(|b| {
                if let Block::Section(s) = b {
                    Some(s)
                } else {
                    None
                }
            })
            .collect();

        assert_eq!(
            sections.len(),
            2,
            "should have 2 top-level sections (NAME and SYNOPSIS)"
        );
        assert_eq!(sections[0].level, 1);
        assert_eq!(sections[1].level, 1);
        assert!(
            matches!(&sections[0].title[0], InlineNode::PlainText(Plain { content, .. }) if *content == "NAME")
        );
        assert!(
            matches!(&sections[1].title[0], InlineNode::PlainText(Plain { content, .. }) if *content == "SYNOPSIS")
        );

        Ok(())
    }

    #[cfg(feature = "setext")]
    #[test]
    #[tracing_test::traced_test]
    fn test_setext_with_description_lists() -> Result<(), Error> {
        // Regression: description list markers (::) anywhere in the document
        // used to cause setext sections to fail because the lookahead
        // `check_start_of_description_list` scanned the entire remaining input
        let input = "\
gitdatamodel(7)
===============

NAME
----
gitdatamodel - description

SYNOPSIS
--------
gitdatamodel

OBJECTS
-------

commit::
    A commit.

REFERENCES
----------

References.
";
        let options = crate::Options::builder().with_setext().build();
        let parsed = crate::parse(input, &options)?;
        let result = parsed.document();

        let header = result.header.as_ref().expect("document has a header");
        assert!(
            matches!(&header.title[0], InlineNode::PlainText(Plain { content, .. }) if content.contains("gitdatamodel"))
        );

        let sections: Vec<&Section> = result
            .blocks
            .iter()
            .filter_map(|b| {
                if let Block::Section(s) = b {
                    Some(s)
                } else {
                    None
                }
            })
            .collect();

        assert_eq!(
            sections.len(),
            4,
            "should have 4 sections (NAME, SYNOPSIS, OBJECTS, REFERENCES)"
        );
        for section in &sections {
            assert_eq!(section.level, 1);
        }

        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_index_term_flow() -> Result<(), Error> {
        use crate::InlineMacro;

        let input = "= Test\n\nThis is about ((Arthur)) the king.\n";
        let mut state = ParserState::new_for_test(input);
        let result = document_parser::document(input, &mut state)??;

        // Find the paragraph
        let paragraph = result
            .blocks
            .iter()
            .find_map(|b| {
                if let Block::Paragraph(p) = b {
                    Some(p)
                } else {
                    None
                }
            })
            .expect("paragraph exists");

        // Check that the index term was parsed
        let has_index_term = paragraph.content.iter().any(|inline| {
            matches!(inline, InlineNode::Macro(InlineMacro::IndexTerm(it)) if it.is_visible() && it.term() == "Arthur")
        });

        assert!(
            has_index_term,
            "Expected to find visible index term 'Arthur', but found: {:?}",
            paragraph.content
        );
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_index_term_concealed() -> Result<(), Error> {
        use crate::InlineMacro;

        let input = "= Test\n\n(((Sword, Broadsword)))This is a concealed index term.\n";
        let mut state = ParserState::new_for_test(input);
        let result = document_parser::document(input, &mut state)??;

        // Find the paragraph
        let paragraph = result
            .blocks
            .iter()
            .find_map(|b| {
                if let Block::Paragraph(p) = b {
                    Some(p)
                } else {
                    None
                }
            })
            .expect("paragraph exists");

        // Check that the concealed index term was parsed
        let has_concealed_term = paragraph.content.iter().any(|inline| {
            matches!(inline, InlineNode::Macro(InlineMacro::IndexTerm(it)) if !it.is_visible() && it.term() == "Sword")
        });

        assert!(
            has_concealed_term,
            "Expected to find concealed index term 'Sword', but found: {:?}",
            paragraph.content
        );
        Ok(())
    }

    /// Test that macro attributes (like `image::`) correctly allow . # % as literal characters.
    ///
    /// This verifies the fix for the issue where `image::photo.jpg[Diablo 4 picture of Lilith.]`
    /// would fail because the trailing `.` was interpreted as a role shorthand prefix.
    ///
    /// In asciidoctor, shorthand syntax (.role, #id, %option) is only valid in block-level
    /// attributes, NOT inside macro brackets. Macro brackets should treat these characters
    /// as literal content.
    #[test]
    #[tracing_test::traced_test]
    fn test_macro_attributes_allow_literal_special_chars() -> Result<(), Error> {
        // Helper to extract the first Image block from a document
        fn get_image<'a>(doc: &'a Document<'a>) -> &'a Image<'a> {
            doc.blocks
                .iter()
                .find_map(|b| {
                    if let Block::Image(img) = b {
                        Some(img)
                    } else {
                        None
                    }
                })
                .expect("document should have an image block")
        }

        // Test trailing period in alt text
        let input = "image::photo.jpg[Diablo 4 picture of Lilith.]";
        let mut state = ParserState::new_for_test(input);
        let result = document_parser::document(input, &mut state)??;
        let img = get_image(&result);
        assert_eq!(
            img.metadata.attributes.get("alt"),
            Some(&AttributeValue::String(
                "Diablo 4 picture of Lilith.".into()
            )),
            "Trailing period should be preserved in alt text"
        );

        // Test .role as literal text (not a shorthand)
        let input = "image::photo.jpg[.role]";
        let mut state = ParserState::new_for_test(input);
        let result = document_parser::document(input, &mut state)??;
        let img = get_image(&result);
        assert_eq!(
            img.metadata.attributes.get("alt"),
            Some(&AttributeValue::String(".role".into())),
            ".role should be literal alt text, not a CSS class"
        );
        assert!(
            img.metadata.roles.is_empty(),
            "roles should be empty - .role is literal text"
        );

        // Test #id as literal text (not a shorthand)
        let input = "image::photo.jpg[Issue #42]";
        let mut state = ParserState::new_for_test(input);
        let result = document_parser::document(input, &mut state)??;
        let img = get_image(&result);
        assert_eq!(
            img.metadata.attributes.get("alt"),
            Some(&AttributeValue::String("Issue #42".into())),
            "#42 should be preserved as literal text"
        );
        assert!(
            img.metadata.id.is_none(),
            "id should be empty - #42 is literal text"
        );

        // Test named role= attribute still works
        let input = "image::photo.jpg[role=thumbnail]";
        let mut state = ParserState::new_for_test(input);
        let result = document_parser::document(input, &mut state)??;
        let img = get_image(&result);
        assert_eq!(
            img.metadata.roles,
            vec![std::borrow::Cow::Borrowed("thumbnail")],
            "Named role= attribute should work"
        );

        Ok(())
    }

    /// Block macros with trailing content after the attribute list should parse
    /// successfully and emit a warning rather than failing.
    /// Regression test for #337.
    #[test]
    #[tracing_test::traced_test]
    fn test_block_macro_trailing_content_emits_warning() -> Result<(), Error> {
        // image macro with trailing content followed by an attributed paragraph
        let input = "image::foo.svg[role=inline][100,100]\n\n[.lead]\nHello\n";
        let mut state = ParserState::new_for_test(input);
        let result = document_parser::document(input, &mut state)??;

        // Should have parsed an image block and a paragraph
        assert!(
            result.blocks.iter().any(|b| matches!(b, Block::Image(_))),
            "document should contain an image block"
        );
        assert!(
            result
                .blocks
                .iter()
                .any(|b| matches!(b, Block::Paragraph(_))),
            "document should contain a paragraph block"
        );

        // Should have emitted a warning about the trailing content. With
        // no current_file set the location has `file: None`, but the
        // positioning still points at the trailing `[100,100]` span.
        let warnings = state.warnings.borrow();
        let warning = warnings
            .iter()
            .find(|w| {
                let kind_msg = w.kind.to_string();
                kind_msg.contains("unexpected content after image macro")
                    && kind_msg.contains("[100,100]")
            })
            .expect("expected trailing-content warning");
        let loc = warning
            .source_location()
            .expect("trailing-content warning should carry a location");
        assert!(loc.file.is_none(), "expected no file for test input");
        Ok(())
    }

    /// When `source_ranges` are set, `warn_trailing_macro_content` should resolve
    /// the correct file name and line number from the included file.
    #[test]
    fn test_trailing_content_warning_resolves_source_range() {
        use crate::model::SourceRange;
        use std::path::PathBuf;

        // Simulate: lines 0..30 are from the main file, lines 30..80 are from
        // "sponsor.adoc" (included), and the trailing content is at byte 45.
        let input = "a]b\n".repeat(20); // 80 bytes total (4 bytes per line)
        let mut state = ParserState::new_for_test(&input);
        state.current_file = Some(PathBuf::from("/docs/main.adoc"));
        state.source_ranges = vec![SourceRange {
            start_offset: 28, // byte 28 starts the included region
            end_offset: 60,
            file: PathBuf::from("/docs/sponsor.adoc"),
            start_line: 1,
        }];

        // Trigger warning at byte offset 40 (inside the included range)
        // 40 - 28 = 12 bytes into the included content = 3 newlines = line 4
        state.warn_trailing_macro_content("image", "[100,100]", 40, 0);

        let warnings = state.warnings.borrow();
        assert_eq!(warnings.len(), 1);
        let loc = warnings[0]
            .source_location()
            .expect("warning should have a location");
        assert_eq!(
            loc.file.as_deref(),
            Some(std::path::Path::new("/docs/sponsor.adoc")),
            "should reference the included file, got: {:?}",
            loc.file,
        );
        let position_line = match &loc.positioning {
            crate::Positioning::Location(l) => l.start.line,
            crate::Positioning::Position(p) => p.line,
        };
        assert_eq!(
            position_line, 4,
            "should reference line 4 in included file, got line {position_line}",
        );
    }

    /// When offset is outside any `source_range`, `warn_trailing_macro_content`
    /// should fall back to the entry-point file.
    #[test]
    fn test_trailing_content_warning_falls_back_to_entry_file() {
        use crate::model::SourceRange;
        use std::path::PathBuf;

        let input = "image::x.png[alt]extra\nsecond line\n";
        let mut state = ParserState::new_for_test(input);
        state.current_file = Some(PathBuf::from("/docs/main.adoc"));
        state.source_ranges = vec![SourceRange {
            start_offset: 100, // well beyond input - shouldn't match
            end_offset: 200,
            file: PathBuf::from("/docs/other.adoc"),
            start_line: 1,
        }];

        state.warn_trailing_macro_content("image", "extra", 17, 0);

        let warnings = state.warnings.borrow();
        assert_eq!(warnings.len(), 1);
        let loc = warnings[0]
            .source_location()
            .expect("warning should have a location");
        assert_eq!(
            loc.file.as_deref(),
            Some(std::path::Path::new("/docs/main.adoc")),
            "should reference the entry-point file, got: {:?}",
            loc.file,
        );
    }

    /// When the document has a title and the first section skips level 1,
    /// the parser should warn (asciidoctor's "section title out of sequence").
    #[test]
    fn test_first_section_not_level_1_emits_warning() -> Result<(), Error> {
        let input = "= Doc Title\n\n=== Starts at level 2\n\nContent\n";
        let mut state = ParserState::new_for_test(input);
        let _ = document_parser::document(input, &mut state)??;
        let warnings = state.warnings.borrow();
        let warning = warnings
            .iter()
            .find(|w| {
                matches!(
                    &w.kind,
                    crate::WarningKind::SectionLevelOutOfSequence { got: 2, .. },
                )
            })
            .expect("expected out-of-sequence warning");
        // The warning should carry the location of the offending section
        // (byte 13 = line 3 in the test input).
        let loc = warning
            .source_location()
            .expect("warning should carry a location");
        match &loc.positioning {
            crate::Positioning::Location(l) => assert_eq!(l.start.line, 3),
            crate::Positioning::Position(p) => assert_eq!(p.line, 3),
        }
        Ok(())
    }

    /// Without a document title, the first-section-level check is silent
    /// (matches asciidoctor's behavior).
    #[test]
    fn test_first_section_without_doc_title_does_not_warn() -> Result<(), Error> {
        let input = "=== No title above me\n\nContent\n";
        let mut state = ParserState::new_for_test(input);
        let _ = document_parser::document(input, &mut state)??;
        let warnings = state.warnings.borrow();
        assert!(
            !warnings.iter().any(|w| matches!(
                &w.kind,
                crate::WarningKind::SectionLevelOutOfSequence { .. }
            )),
            "should not warn without doc title, got: {warnings:?}",
        );
        Ok(())
    }

    /// Valid structure (doc title + level 1 first section) must not warn.
    #[test]
    fn test_first_section_level_1_no_warning() -> Result<(), Error> {
        let input = "= Doc Title\n\n== Good\n\n=== Nested\n";
        let mut state = ParserState::new_for_test(input);
        let _ = document_parser::document(input, &mut state)??;
        let warnings = state.warnings.borrow();
        assert!(
            !warnings.iter().any(|w| matches!(
                &w.kind,
                crate::WarningKind::SectionLevelOutOfSequence { .. }
            )),
            "should not warn for valid structure, got: {warnings:?}",
        );
        Ok(())
    }

    /// An opened table that never closes before EOF emits an
    /// `UnterminatedTable { separator, equals }` warning and still
    /// produces a table (matching asciidoctor's recovery).
    #[test]
    fn test_unterminated_pipe_table_emits_warning() -> Result<(), Error> {
        let input = "|===\n| A | B\n| C | D\n";
        let mut state = ParserState::new_for_test(input);
        let doc = document_parser::document(input, &mut state)??;

        let warnings = state.warnings.borrow();
        let warning = warnings
            .iter()
            .find(|w| {
                matches!(
                    &w.kind,
                    crate::WarningKind::UnterminatedTable { delimiter } if delimiter == "|===",
                )
            })
            .expect("expected unterminated table warning");
        let loc = warning
            .source_location()
            .expect("warning should carry a location");
        // Warning should point to the opening `|===` on line 1.
        match &loc.positioning {
            crate::Positioning::Location(l) => assert_eq!(l.start.line, 1),
            crate::Positioning::Position(p) => assert_eq!(p.line, 1),
        }

        // The document should still contain a table block.
        let has_table = doc.blocks.iter().any(|b| {
            matches!(
                b,
                Block::DelimitedBlock(DelimitedBlock {
                    inner: DelimitedBlockType::DelimitedTable(_),
                    ..
                })
            )
        });
        assert!(has_table, "expected a table block in the document");
        Ok(())
    }

    /// The `!===` (exclamation) table delimiter is also covered by the
    /// unterminated fallback, and the warning carries the actual opening
    /// delimiter so consumers can distinguish between delimiter variants.
    #[test]
    fn test_unterminated_excl_table_emits_warning() -> Result<(), Error> {
        let input = "!===\n! A ! B\n";
        let mut state = ParserState::new_for_test(input);
        let _ = document_parser::document(input, &mut state)??;
        let warnings = state.warnings.borrow();
        assert!(
            warnings.iter().any(|w| matches!(
                &w.kind,
                crate::WarningKind::UnterminatedTable { delimiter } if delimiter == "!===",
            )),
            "expected unterminated table warning with `!===` delimiter, got: {warnings:?}",
        );
        Ok(())
    }

    /// Diagnostics emitted from inside an `a`-style cell must point at the
    /// offending token within the cell, not at the cell's `a|` style prefix.
    /// Repro for the case where a nested `!===` is left unterminated:
    /// the warning's reported line should match the line of `!===`, not the
    /// line of `a|`.
    #[test]
    fn test_warning_in_ascii_cell_points_at_inner_token() -> Result<(), Error> {
        // Lines:
        //   1: `[cols="1a"]`
        //   2: `|===`
        //   3: `a|`           <- cell style prefix
        //   4: `!===`         <- offending unterminated inner table
        //   5: `! Inner A ! Inner B`
        //   6: `|===`
        let input = "[cols=\"1a\"]\n|===\na|\n!===\n! Inner A ! Inner B\n|===\n";
        let mut state = ParserState::new_for_test(input);
        let _ = document_parser::document(input, &mut state)??;
        let warnings = state.warnings.borrow();
        let warning = warnings
            .iter()
            .find(|w| {
                matches!(
                    &w.kind,
                    crate::WarningKind::UnterminatedTable { delimiter } if delimiter == "!===",
                )
            })
            .expect("expected unterminated inner-table warning");
        let loc = warning
            .source_location()
            .expect("warning should carry a location");
        let line = match &loc.positioning {
            crate::Positioning::Location(l) => l.start.line,
            crate::Positioning::Position(p) => p.line,
        };
        assert_eq!(
            line, 4,
            "warning should point at line 4 (the `!===`), not the `a|` line; got {line}",
        );
        Ok(())
    }

    /// A properly closed table must not emit an unterminated warning.
    #[test]
    fn test_terminated_table_does_not_warn() -> Result<(), Error> {
        let input = "|===\n| A | B\n|===\n";
        let mut state = ParserState::new_for_test(input);
        let _ = document_parser::document(input, &mut state)??;
        let warnings = state.warnings.borrow();
        assert!(
            !warnings
                .iter()
                .any(|w| matches!(&w.kind, crate::WarningKind::UnterminatedTable { .. })),
            "should not warn for a properly closed table, got: {warnings:?}",
        );
        Ok(())
    }

    /// Degenerate case: the document is just an opening delimiter with no
    /// content and no close. Asciidoctor still warns ("unterminated table
    /// block"). The unterminated fallback rule should match and produce an
    /// empty table rather than falling through to paragraph parsing.
    #[test]
    fn test_unterminated_pipe_table_with_no_content_emits_warning() -> Result<(), Error> {
        let input = "|===\n";
        let mut state = ParserState::new_for_test(input);
        let _ = document_parser::document(input, &mut state)??;
        let warnings = state.warnings.borrow();
        assert!(
            warnings.iter().any(|w| matches!(
                &w.kind,
                crate::WarningKind::UnterminatedTable { delimiter } if delimiter == "|===",
            )),
            "expected unterminated table warning for empty open, got: {warnings:?}",
        );
        Ok(())
    }

    /// Same as above but exercised through the public `parse` entry point
    /// (which runs the preprocessor first). Catches the case where the
    /// preprocessor normalises the input in a way that breaks the
    /// unterminated fallback.
    #[test]
    fn test_unterminated_pipe_table_empty_through_parse_entry() {
        let opts = crate::Options::default();
        let res = crate::parse("|===\n", &opts).expect("parse should succeed");
        let has_warning = res.warnings().iter().any(|w| {
            matches!(
                &w.kind,
                crate::WarningKind::UnterminatedTable { delimiter } if delimiter == "|===",
            )
        });
        assert!(
            has_warning,
            "expected unterminated table warning through parse(), got: {:?}",
            res.warnings(),
        );
    }
}
