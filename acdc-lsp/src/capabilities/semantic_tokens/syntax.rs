use acdc_parser::{
    ColumnStyle, Location,
    ast::{
        AttributeValueRef, AttributesRef, BlockKind, BlockRef, ContentRef, DocumentRef, HeaderRef,
        IdOrigin, LocationRef, PositionRef, StructuralRef, TableRef, TableRowRef,
    },
};
use tower_lsp_server::ls_types::{Position, Range};

use super::{
    BOLD, CODE, ITALIC, LineIndex, RawToken, add_attribute_token, add_range_with_priority,
    add_token_for_location, add_token_for_range, byte_range_to_utf16_range,
    collect_tokens_from_inlines, is_primary_location,
};
use crate::{convert::position_to_offset, state::ConditionalBlock};

pub(super) fn collect(
    document: DocumentRef<'_>,
    source: &LineIndex<'_>,
    tokens: &mut Vec<RawToken>,
) {
    for node in StructuralRef::Document(document).descendants() {
        match node {
            StructuralRef::Header(header) => collect_header(header, source, tokens),
            StructuralRef::Block(block) => collect_block(block, source, tokens),
            StructuralRef::ListItem(item) => {
                if let Some(range) = primary_range(item.location(), source) {
                    prefix(range.start, item.marker(), 8, source, tokens);
                    if let Some(line) = source.line(range.start.line)
                        && let Some(start) =
                            position_to_offset(line, Position::new(0, range.start.character))
                        && let Some(rest) = line.get(start + item.marker().len()..)
                    {
                        let rest = rest.trim_start();
                        if rest.starts_with("[x]")
                            || rest.starts_with("[ ]")
                            || rest.starts_with("[*]")
                        {
                            let start = line.len() - rest.len();
                            line_bytes(range.start.line, start..start + 3, 8, 0, source, tokens);
                        }
                    }
                }
                collect_tokens_from_inlines(item.principal(), source, tokens);
            }
            StructuralRef::DescriptionListTerm(term) => {
                content(term.content(), 4, 0, source, tokens);
                if let Some(location) = term.delimiter_location() {
                    add_token_for_location(location, 8, 0, source, tokens);
                }
            }
            StructuralRef::DescriptionListDefinition(definition) => {
                if let Some(principal) = definition.principal() {
                    collect_tokens_from_inlines(principal, source, tokens);
                }
            }
            StructuralRef::CalloutListItem(item) => {
                add_token_for_location(item.callout().location(), 8, 0, source, tokens);
                collect_tokens_from_inlines(item.principal(), source, tokens);
            }
            StructuralRef::Document(_)
            | StructuralRef::DescriptionListItem(_)
            | StructuralRef::TableRow(_)
            | StructuralRef::TableCell(_)
            | _ => {}
        }
    }
}

/// Highlight the attribute entries and comments in the document header.
///
/// The parser consumes them without creating blocks, so this reads the lines
/// of its header span, with or without a title. Other lines in the span, such
/// as the title, author and revision lines, block metadata, and preprocessor
/// directives, get their tokens elsewhere.
pub(super) fn header(
    document: DocumentRef<'_>,
    conditionals: &[ConditionalBlock],
    includes: &[(String, Location)],
    source: &LineIndex<'_>,
    tokens: &mut Vec<RawToken>,
) {
    let Some(span) = document.header_location() else {
        return;
    };
    let Some((first, last)) = header_lines(span, includes) else {
        return;
    };
    let inactive = |line: u32| {
        let line = line as usize;
        conditionals.iter().any(|conditional| {
            !conditional.is_active
                && conditional
                    .end_line
                    .is_some_and(|end| conditional.start_line < line && line < end)
        })
    };
    let ends_in_primary = outermost_include(span.end()).is_none();
    let mut comment_delimiter: Option<&str> = None;
    let mut continuation = false;
    for line in first.. {
        let Some(text) = source.line(line) else {
            break;
        };
        let text = text.trim_end_matches(['\r', '\n']);
        // The span can stop short of the header's last source lines: a value
        // folded from continuation lines maps to its entry's first line, and
        // the preprocessor drops line comments that directly follow the header.
        if line > last {
            if !ends_in_primary || inactive(line) || text.trim().is_empty() {
                break;
            }
            if continuation {
                line_bytes(line, 0..text.len(), 3, 0, source, tokens);
                continuation = continues_value(text);
            } else if is_line_comment(text) {
                line_bytes(line, 0..text.len(), 5, 0, source, tokens);
            } else {
                break;
            }
            continue;
        }
        // Comment blocks bypass preprocessor directives, so they are tracked
        // before conditionals.
        if let Some(delimiter) = comment_delimiter {
            if text.trim_end() == delimiter {
                comment_delimiter = None;
                line_bytes(line, 0..delimiter.len(), 8, 0, source, tokens);
            } else {
                line_bytes(line, 0..text.len(), 5, 0, source, tokens);
            }
            continue;
        }
        if inactive(line) {
            continue;
        }
        if text.trim().is_empty() {
            continuation = false;
            continue;
        }
        if continuation {
            line_bytes(line, 0..text.len(), 3, 0, source, tokens);
            continuation = continues_value(text);
            continue;
        }
        if let Some(delimiter) = comment_block_delimiter(text) {
            comment_delimiter = Some(delimiter);
            line_bytes(line, 0..delimiter.len(), 8, 0, source, tokens);
        } else if is_line_comment(text) {
            line_bytes(line, 0..text.len(), 5, 0, source, tokens);
        } else if let Some((name, value)) = attribute_entry(text) {
            attribute_line(name, line, source, tokens);
            continuation = continues_value(value);
        }
    }
}

/// The header's first and last lines in the primary file, 0-based.
///
/// Either end of the span can sit in an included file. The include directive
/// that reached it then bounds the primary lines: the first matching directive
/// after the start for the end, or the last one before the end for the start.
/// Directive targets are raw text, so a target that needs attribute
/// substitution falls back to the nearest directive. That can only shorten the
/// range.
fn header_lines(span: LocationRef<'_>, includes: &[(String, Location)]) -> Option<(u32, u32)> {
    let zero_based = |position: PositionRef<'_>| position.line().saturating_sub(1);
    let directives: Vec<_> = includes
        .iter()
        .filter(|(_, location)| is_primary_location(location))
        .map(|(target, location)| {
            (
                target.as_str(),
                zero_based(LocationRef::from(location).start()),
            )
        })
        .collect();
    let start = outermost_include(span.start());
    let end = outermost_include(span.end());

    let last = match end {
        None => zero_based(span.end()),
        Some(target) => {
            let from = if start.is_none() {
                zero_based(span.start())
            } else {
                0
            };
            let after_start = || directives.iter().filter(|(_, line)| *line >= from);
            after_start()
                .find(|(written, _)| *written == target)
                .or_else(|| after_start().next())?
                .1
        }
    };
    let first = match start {
        None => zero_based(span.start()),
        Some(target) => {
            let before_end = || directives.iter().filter(|(_, line)| *line <= last);
            before_end()
                .rfind(|(written, _)| *written == target)
                .or_else(|| before_end().next_back())?
                .1
                + 1
        }
    };
    (first <= last).then_some((first, last))
}

fn outermost_include(position: PositionRef<'_>) -> Option<&str> {
    position.include_chain().and_then(|mut chain| chain.next())
}

/// A `////` comment block delimiter. Like the parser, trailing whitespace is
/// accepted.
fn comment_block_delimiter(text: &str) -> Option<&str> {
    let delimiter = text.trim_end();
    (delimiter.len() >= 4 && delimiter.bytes().all(|byte| byte == b'/')).then_some(delimiter)
}

/// Like the parser, a line comment is `//` not followed by another `/`.
fn is_line_comment(text: &str) -> bool {
    text.starts_with("//") && !text.starts_with("///")
}

/// The name and value of an attribute entry line. Like the parser, an entry
/// needs a name, then whitespace or the end of the line after `:name:`.
fn attribute_entry(text: &str) -> Option<(&str, &str)> {
    let (name, value) = text.strip_prefix(':')?.split_once(':')?;
    let name = name.trim_matches('!');
    (!name.is_empty() && (value.is_empty() || value.starts_with([' ', '\t'])))
        .then(|| (name, value.trim_start_matches([' ', '\t'])))
}

// Like the parser, ` \` continues a value only after at least one character,
// so a lone `\` is a literal value.
fn continues_value(text: &str) -> bool {
    text.trim_end()
        .strip_suffix(" \\")
        .is_some_and(|rest| !rest.is_empty())
}

fn collect_header(header: HeaderRef<'_>, source: &LineIndex<'_>, tokens: &mut Vec<RawToken>) {
    if let Some(title) = header.title() {
        heading(title, 0, source, tokens);
    }
    if let Some(subtitle) = header.subtitle() {
        content(subtitle, 0, 0, source, tokens);
    }
}

fn block_metadata(block: BlockRef<'_>, source: &LineIndex<'_>, tokens: &mut Vec<RawToken>) {
    if let Some(location) = block.attributes().and_then(AttributesRef::location)
        && let Some(range) = primary_range(location, source)
    {
        add_range_with_priority(range, 2, 0, 0, source, tokens);
    }
    for anchor in block.anchors() {
        add_token_for_location(anchor.location(), 7, 1, source, tokens);
    }
    if !matches!(
        block.kind(),
        BlockKind::Section(_) | BlockKind::DiscreteHeading(_)
    ) && let Some(title) = block.title()
    {
        content(title, 0, 0, source, tokens);
    }
    delimiters(block.kind(), source, tokens);
}

fn collect_block(block: BlockRef<'_>, source: &LineIndex<'_>, tokens: &mut Vec<RawToken>) {
    block_metadata(block, source, tokens);
    match block.kind() {
        BlockKind::Section(section) => {
            if let Some(title) = section.title() {
                let modifiers = if section
                    .id()
                    .is_some_and(|id| id.origin() == IdOrigin::Explicit)
                {
                    2
                } else {
                    0
                };
                heading(title, modifiers, source, tokens);
            }
        }
        BlockKind::DiscreteHeading(node) => {
            if let Some(title) = node.title() {
                heading(title, 0, source, tokens);
            }
        }
        BlockKind::Paragraph(node) => collect_tokens_from_inlines(node.content(), source, tokens),
        BlockKind::Listing(node) => verbatim(block.kind(), node.content(), source, tokens),
        BlockKind::Literal(node) => verbatim(block.kind(), node.content(), source, tokens),
        BlockKind::Passthrough(node) => verbatim(block.kind(), node.content(), source, tokens),
        BlockKind::Stem(node) => {
            if let Some(range) = delimited_body(block.kind(), source)
                .or_else(|| primary_range(node.location(), source))
            {
                add_range_with_priority(range, 3, CODE, 0, source, tokens);
            }
        }
        BlockKind::Verse(node) => collect_tokens_from_inlines(node.content(), source, tokens),
        BlockKind::DelimitedComment(node) => {
            if let Some(range) = delimited_body(block.kind(), source)
                .or_else(|| primary_range(node.location(), source))
            {
                add_range_with_priority(range, 5, 0, 0, source, tokens);
            }
        }
        BlockKind::Comment(node) => add_token_for_location(node.location(), 5, 0, source, tokens),
        BlockKind::DocumentAttribute(node) => {
            if let Some(range) = primary_range(node.location(), source) {
                attribute_line(node.name(), range.start.line, source, tokens);
                if range.end.line > range.start.line {
                    add_token_for_range(
                        Range::new(Position::new(range.start.line + 1, 0), range.end),
                        3,
                        0,
                        source,
                        tokens,
                    );
                }
            }
        }
        BlockKind::Admonition(node) => {
            let label = format!("{}:", node.variant()).to_ascii_uppercase();
            if let Some(range) = primary_range(node.location(), source)
                && let Some(position) = find_prefix(range, &[&label], source)
            {
                prefix(position, &label, 6, source, tokens);
            }
        }
        BlockKind::Table(table) => table_tokens(table, source, tokens),
        BlockKind::Image(_)
        | BlockKind::Audio(_)
        | BlockKind::Video(_)
        | BlockKind::TableOfContents(_) => macro_tokens(block, source, tokens),
        BlockKind::ThematicBreak(_) | BlockKind::PageBreak(_) => {
            if let Some(range) = primary_range(block.location(), source) {
                let start = if range.start.line == range.end.line {
                    range.start.character
                } else {
                    0
                };
                add_token_for_range(
                    Range::new(Position::new(range.end.line, start), range.end),
                    8,
                    0,
                    source,
                    tokens,
                );
            }
        }
        BlockKind::Example(_)
        | BlockKind::Open(_)
        | BlockKind::Sidebar(_)
        | BlockKind::Quote(_)
        | BlockKind::UnorderedList(_)
        | BlockKind::OrderedList(_)
        | BlockKind::CalloutList(_)
        | BlockKind::DescriptionList(_)
        | _ => {}
    }
}

fn macro_tokens(block: BlockRef<'_>, source: &LineIndex<'_>, tokens: &mut Vec<RawToken>) {
    if let Some(mut range) = primary_range(block.location(), source)
        && let Some(start) = find_prefix(range, &["image::", "audio::", "video::", "toc::"], source)
    {
        range.start = start;
        add_token_for_range(range, 1, 0, source, tokens);
    }
    for field in StructuralRef::Block(block).content_fields() {
        collect_tokens_from_inlines(field.content(), source, tokens);
    }
}

fn heading(
    title: ContentRef<'_>,
    modifiers: u32,
    source: &LineIndex<'_>,
    tokens: &mut Vec<RawToken>,
) {
    content(title, 0, modifiers, source, tokens);
    let Some(range) = content_range(title, source) else {
        return;
    };
    if let Some(line) = source.line(range.start.line) {
        let marker = line.trim_start().chars().take_while(|c| *c == '=').count();
        if marker > 0 {
            let start = line.len() - line.trim_start().len();
            line_bytes(
                range.start.line,
                start..start + marker,
                8,
                0,
                source,
                tokens,
            );
        }
    }
    let underline = range.end.line + 1;
    if let Some(line) = source.line(underline) {
        let text = line.trim();
        if text.len() >= 2
            && text.chars().next().is_some_and(|first| {
                matches!(first, '=' | '-' | '~' | '^' | '+') && text.chars().all(|c| c == first)
            })
        {
            let start = line.len() - line.trim_start().len();
            line_bytes(underline, start..start + text.len(), 8, 0, source, tokens);
        }
    }
}

fn delimiters(kind: BlockKind<'_>, source: &LineIndex<'_>, tokens: &mut Vec<RawToken>) {
    for location in delimiter_locations(kind).into_iter().flatten() {
        add_token_for_location(location, 8, 0, source, tokens);
    }
}

fn delimiter_locations(kind: BlockKind<'_>) -> [Option<LocationRef<'_>>; 2] {
    macro_rules! boundaries {
        ($node:expr) => {
            [
                $node.open_delimiter_location(),
                $node.close_delimiter_location(),
            ]
        };
    }
    match kind {
        BlockKind::Example(node) => boundaries!(node),
        BlockKind::Open(node) => boundaries!(node),
        BlockKind::Sidebar(node) => boundaries!(node),
        BlockKind::Quote(node) => boundaries!(node),
        BlockKind::Listing(node) => boundaries!(node),
        BlockKind::Literal(node) => boundaries!(node),
        BlockKind::Passthrough(node) => boundaries!(node),
        BlockKind::Verse(node) => boundaries!(node),
        BlockKind::DelimitedComment(node) => boundaries!(node),
        BlockKind::Table(node) => boundaries!(node),
        BlockKind::Stem(node) => boundaries!(node),
        BlockKind::Section(_)
        | BlockKind::Paragraph(_)
        | BlockKind::Admonition(_)
        | BlockKind::UnorderedList(_)
        | BlockKind::OrderedList(_)
        | BlockKind::DescriptionList(_)
        | BlockKind::CalloutList(_)
        | BlockKind::DocumentAttribute(_)
        | BlockKind::Comment(_)
        | BlockKind::DiscreteHeading(_)
        | BlockKind::ThematicBreak(_)
        | BlockKind::PageBreak(_)
        | BlockKind::Image(_)
        | BlockKind::Audio(_)
        | BlockKind::Video(_)
        | BlockKind::TableOfContents(_)
        | _ => [None, None],
    }
}

fn verbatim(
    kind: BlockKind<'_>,
    body: ContentRef<'_>,
    source: &LineIndex<'_>,
    tokens: &mut Vec<RawToken>,
) {
    if let Some(range) = delimited_body(kind, source) {
        add_range_with_priority(range, 3, CODE, 0, source, tokens);
        collect_tokens_from_inlines(body, source, tokens);
    } else {
        content(body, 3, CODE, source, tokens);
    }
}

fn delimited_body(kind: BlockKind<'_>, source: &LineIndex<'_>) -> Option<Range> {
    let [open, close] = delimiter_locations(kind)
        .map(|location| location.and_then(|location| primary_range(location, source)));
    let (open, close) = (open?, close?);
    // Delimiters retain the original source, including preprocessor escapes.
    let start = Position::new(open.end.line + 1, 0);
    let end = Position::new(close.start.line, 0);
    (start <= end).then_some(Range::new(start, end))
}

fn find_prefix(range: Range, prefixes: &[&str], source: &LineIndex<'_>) -> Option<Position> {
    for line in range.start.line..=range.end.line {
        let text = source.line(line)?;
        let from = if line == range.start.line {
            position_to_offset(text, Position::new(0, range.start.character))?
        } else {
            0
        };
        let rest = text.get(from..)?.trim_start();
        if prefixes.iter().any(|prefix| rest.starts_with(prefix)) {
            let byte = text.len() - rest.len();
            let range = byte_range_to_utf16_range(text, byte..byte)?;
            return Some(Position::new(line, range.start.character));
        }
    }
    None
}

fn table_tokens(table: TableRef<'_>, source: &LineIndex<'_>, tokens: &mut Vec<RawToken>) {
    if let Some(header) = table.header() {
        row_styles(header, BOLD, source, tokens);
    }
    for row in table.rows().chain(table.footer()) {
        row_styles(row, 0, source, tokens);
    }
    let Some(open) = table
        .open_delimiter_location()
        .and_then(|loc| primary_range(loc, source))
    else {
        return;
    };
    let Some(close) = table
        .close_delimiter_location()
        .and_then(|loc| primary_range(loc, source))
    else {
        return;
    };
    let format = table
        .attributes()
        .get("format")
        .and_then(AttributeValueRef::as_str);
    let separator = table
        .attributes()
        .get("separator")
        .and_then(AttributeValueRef::as_str)
        .and_then(|value| value.chars().next())
        .or_else(|| match format {
            Some("csv") => Some(','),
            Some("dsv") => Some(':'),
            _ => table.delimiter().chars().next(),
        });
    let Some(separator) = separator else { return };
    let csv = format == Some("csv") || table.delimiter().starts_with(',');
    let mut quoted = false;
    for line_number in open.end.line + 1..close.start.line {
        let Some(line) = source.line(line_number) else {
            continue;
        };
        let mut characters = line.char_indices().peekable();
        let mut escaped = false;
        while let Some((byte, character)) = characters.next() {
            if csv && character == '"' {
                if quoted && characters.peek().is_some_and(|(_, next)| *next == '"') {
                    characters.next();
                } else {
                    quoted = !quoted;
                }
            } else if !csv && character == '\\' && !escaped {
                escaped = true;
                continue;
            } else if character == separator && !quoted && !escaped {
                line_bytes(
                    line_number,
                    byte..byte + character.len_utf8(),
                    8,
                    0,
                    source,
                    tokens,
                );
            }
            escaped = false;
        }
    }
}

fn row_styles(
    row: TableRowRef<'_>,
    inherited: u32,
    source: &LineIndex<'_>,
    tokens: &mut Vec<RawToken>,
) {
    for cell in row.cells() {
        let modifiers = inherited
            | match cell.style().unwrap_or_default() {
                ColumnStyle::Header | ColumnStyle::Strong => BOLD,
                ColumnStyle::Emphasis => ITALIC,
                ColumnStyle::Literal | ColumnStyle::Monospace => CODE,
                ColumnStyle::AsciiDoc | ColumnStyle::Default | _ => 0,
            };
        if modifiers == 0 {
            continue;
        }
        for block in cell.blocks() {
            if let Some(range) = primary_range(block.location(), source) {
                let kind = if modifiers & CODE != 0 { 3 } else { 4 };
                add_range_with_priority(range, kind, modifiers, 0, source, tokens);
            }
        }
    }
}

fn content_range(content: ContentRef<'_>, source: &LineIndex<'_>) -> Option<Range> {
    let mut inlines = content.inlines();
    let first = inlines.next()?;
    let last = inlines.next_back().unwrap_or(first);
    if !is_primary_location(first.location()) || !is_primary_location(last.location()) {
        return None;
    }
    source.range_between(first.location().start(), last.location().end())
}

fn content(
    content: ContentRef<'_>,
    kind: u32,
    modifiers: u32,
    source: &LineIndex<'_>,
    tokens: &mut Vec<RawToken>,
) {
    // Individual spans remain usable when a container crosses an include boundary.
    for inline in content.inlines() {
        if let Some(range) = primary_range(inline.location(), source) {
            add_range_with_priority(range, kind, modifiers, 0, source, tokens);
        }
    }
    collect_tokens_from_inlines(content, source, tokens);
}

fn primary_range(location: LocationRef<'_>, source: &LineIndex<'_>) -> Option<Range> {
    is_primary_location(location)
        .then(|| source.range(location))
        .flatten()
}

fn prefix(
    position: Position,
    spelling: &str,
    kind: u32,
    source: &LineIndex<'_>,
    tokens: &mut Vec<RawToken>,
) {
    let Some(line) = source.line(position.line) else {
        return;
    };
    let Some(start) = position_to_offset(line, Position::new(0, position.character)) else {
        return;
    };
    if !spelling.is_empty()
        && line
            .get(start..)
            .is_some_and(|rest| rest.starts_with(spelling))
    {
        line_bytes(
            position.line,
            start..start + spelling.len(),
            kind,
            0,
            source,
            tokens,
        );
    }
}

fn line_bytes(
    line: u32,
    bytes: std::ops::Range<usize>,
    kind: u32,
    modifiers: u32,
    source: &LineIndex<'_>,
    tokens: &mut Vec<RawToken>,
) {
    if let Some(text) = source.line(line)
        && let Some(mut range) = byte_range_to_utf16_range(text, bytes)
    {
        range.start.line = line;
        range.end.line = line;
        add_token_for_range(range, kind, modifiers, source, tokens);
    }
}

fn attribute_line(name: &str, line: u32, source: &LineIndex<'_>, tokens: &mut Vec<RawToken>) {
    let before = tokens.len();
    add_attribute_token(name, line, source, tokens);
    if tokens.len() == before {
        return;
    }
    let Some(text) = source.line(line) else {
        return;
    };
    let Some((_, value)) = text
        .trim_start()
        .strip_prefix(':')
        .and_then(|rest| rest.split_once(':'))
    else {
        return;
    };
    let value = value.trim_end_matches(['\r', '\n']).trim_start();
    if !value.is_empty() {
        let end = text.trim_end_matches(['\r', '\n']).len();
        line_bytes(line, end - value.len()..end, 3, 0, source, tokens);
    }
}

pub(super) fn attribute_references(
    location: LocationRef<'_>,
    source: &LineIndex<'_>,
    tokens: &mut Vec<RawToken>,
) {
    let Some(range) = primary_range(location, source) else {
        return;
    };
    for line in range.start.line..=range.end.line {
        let Some(text) = source.line(line) else {
            continue;
        };
        let start = if line == range.start.line {
            position_to_offset(text, Position::new(0, range.start.character)).unwrap_or(text.len())
        } else {
            0
        };
        let end = if line == range.end.line {
            position_to_offset(text, Position::new(0, range.end.character)).unwrap_or(start)
        } else {
            text.trim_end_matches(['\r', '\n']).len()
        };
        let Some(text) = text.get(start..end) else {
            continue;
        };
        for (offset, _) in text.match_indices('{') {
            if text
                .get(..offset)
                .is_some_and(|prefix| prefix.ends_with('\\'))
            {
                continue;
            }
            if let Some((name, _)) = text.get(offset + 1..).and_then(|rest| rest.split_once('}'))
                && !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-'))
            {
                line_bytes(
                    line,
                    start + offset..start + offset + name.len() + 2,
                    2,
                    0,
                    source,
                    tokens,
                );
            }
        }
    }
}
