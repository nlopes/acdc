use acdc_parser::{
    ColumnStyle,
    ast::{
        AttributeValueRef, AttributesRef, BlockKind, BlockRef, ContentRef, DocumentRef, HeaderRef,
        IdOrigin, LocationRef, StructuralRef, TableRef, TableRowRef,
    },
};
use tower_lsp_server::ls_types::{Position, Range};

use super::{
    BOLD, CODE, ITALIC, LineIndex, RawToken, add_attribute_token, add_range_with_priority,
    add_token_for_location, add_token_for_range, byte_range_to_utf16_range,
    collect_tokens_from_inlines, is_primary_location,
};
use crate::convert::position_to_offset;

pub(super) fn collect(
    document: DocumentRef<'_>,
    source: &LineIndex<'_>,
    tokens: &mut Vec<RawToken>,
) {
    if document.header().is_none() {
        collect_titleless_header(source, tokens);
    }
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

// The parser consumes leading header attributes even without a title, but then
// exposes neither a HeaderRef nor DocumentAttribute blocks for that region.
// Keep scanning across empty lines, as asciidoctor does before the document
// title; an entry gets the same tokens whether it lands in the header or the
// body. Stop at the first line that is neither an attribute entry nor a comment
// so body text cannot become attributes.
fn collect_titleless_header(source: &LineIndex<'_>, tokens: &mut Vec<RawToken>) {
    let mut comment_delimiter = None;
    let mut continuation = false;
    for line in 0.. {
        let Some(text) = source.line(line) else {
            break;
        };
        let text = text.trim_end_matches(['\r', '\n']);
        if let Some(delimiter) = comment_delimiter {
            if text == delimiter {
                comment_delimiter = None;
            }
            continue;
        }
        if text.trim().is_empty() {
            continuation = false;
            continue;
        }
        if continuation {
            line_bytes(line, 0..text.len(), 3, 0, source, tokens);
            continuation = text.trim_end().ends_with(" \\");
            continue;
        }
        if text.len() >= 4 && text.bytes().all(|byte| byte == b'/') {
            comment_delimiter = Some(text);
            continue;
        }
        if text.starts_with("//") {
            continue;
        }
        let Some((name, _)) = text.strip_prefix(':').and_then(|rest| rest.split_once(':')) else {
            break;
        };
        attribute_line(name.trim_matches('!'), line, source, tokens);
        continuation = text.trim_end().ends_with(" \\");
    }
}

fn collect_header(header: HeaderRef<'_>, source: &LineIndex<'_>, tokens: &mut Vec<RawToken>) {
    if let Some(title) = header.title() {
        heading(title, 0, source, tokens);
    }
    if let Some(subtitle) = header.subtitle() {
        content(subtitle, 0, 0, source, tokens);
    }
    if let Some(range) = primary_range(header.location(), source) {
        for line in range.start.line..=range.end.line {
            if let Some(text) = source.line(line)
                && let Some(rest) = text.trim_start().strip_prefix(':')
                && let Some((name, _)) = rest.split_once(':')
            {
                attribute_line(name.trim_matches('!'), line, source, tokens);
            }
        }
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
