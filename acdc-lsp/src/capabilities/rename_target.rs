//! Find editable target text inside anchor and cross-reference syntax.

use acdc_parser::Location;
use tower_lsp_server::ls_types::{Position, Range};

use crate::{convert::to_lsp_u32, state::XrefTarget};

pub(super) fn anchor_range(text: &str, location: &Location, id: &str) -> Option<Range> {
    let (target, range) = target_range(text, location, &["[[[", "[["], &[',', ']'])
        .or_else(|| target_range(text, location, &["[#"], &[',', ']', '.', '%']))?;
    (target == id).then_some(range)
}

pub(super) fn xref_target(text: &str, location: &Location) -> Option<(String, Range)> {
    target_range(text, location, &["xref:"], &['['])
        .or_else(|| target_range(text, location, &["<<"], &[',', '>']))
}

pub(super) fn xref_anchor_range(text: &str, location: &Location, id: &str) -> Option<Range> {
    let (target, mut range) = xref_target(text, location)?;
    if XrefTarget::parse(&target).anchor.as_deref() != Some(id) {
        return None;
    }
    if let Some((prefix, _)) = target.rsplit_once('#') {
        range.start.character += to_lsp_u32(prefix.encode_utf16().count() + 1);
    }
    Some(range)
}

fn target_range(
    text: &str,
    location: &Location,
    prefixes: &[&str],
    delimiters: &[char],
) -> Option<(String, Range)> {
    if location.start.file != location.end.file {
        return None;
    }
    let line_number = location.start.line.checked_sub(1)?;
    let line = text.lines().nth(line_number as usize)?;
    // Parser columns count Unicode scalar values; LSP columns count UTF-16 units.
    // Use the original line so includes with selection or indentation also work.
    let column = location.start.column.checked_sub(1)? as usize;
    let (start, _) = line.char_indices().nth(column)?;
    let span = line.get(start..)?;
    let prefix = prefixes.iter().find(|prefix| span.starts_with(**prefix))?;
    let rest = span.strip_prefix(prefix)?;
    let end = rest.find(delimiters)?;
    let target = rest.get(..end)?;
    if target.is_empty() {
        return None;
    }
    let start = start + prefix.len();
    let end = start + target.len();
    if location.start.line == location.end.line
        && line.get(..end)?.chars().count() > location.end.column as usize
    {
        return None;
    }
    Some((
        target.to_string(),
        Range::new(
            Position::new(
                line_number,
                to_lsp_u32(line.get(..start)?.encode_utf16().count()),
            ),
            Position::new(
                line_number,
                to_lsp_u32(line.get(..end)?.encode_utf16().count()),
            ),
        ),
    ))
}
