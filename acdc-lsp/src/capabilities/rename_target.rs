//! Find editable target text inside anchors, cross-references, and includes.

use acdc_parser::Location;
use tower_lsp_server::ls_types::Range;

use crate::{
    convert::{byte_range_to_utf16_range, location_to_utf16_range, position_to_offset, to_lsp_u32},
    state::XrefTarget,
};

pub(super) fn include_range(text: &str, location: &Location, target: &str) -> Option<Range> {
    let range = location_to_utf16_range(text, location)?;
    let start = position_to_offset(text, range.start)?;
    let end = position_to_offset(text, range.end)?;
    (text.get(start..end)? == target
        && text.get(..start)?.ends_with("include::")
        && text.get(end..)?.starts_with('['))
    .then_some(range)
}

pub(super) fn anchor_range(text: &str, location: &Location, id: &str) -> Option<Range> {
    if !id.is_empty() && location.start.file == location.end.file {
        let range = location_to_utf16_range(text, location)?;
        let start = position_to_offset(text, range.start)?;
        let end = position_to_offset(text, range.end)?;
        if text.get(start..end)? == id {
            return Some(range);
        }
    }
    let (target, range) = target_range(text, location, &["[[[", "[["], &[',', ']'])
        .or_else(|| target_range(text, location, &["[#"], &[',', ']', '.', '%']))
        .or_else(|| target_range(text, location, &["anchor:"], &['[']))?;
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
    let mut range = byte_range_to_utf16_range(line, start..end)?;
    range.start.line = line_number;
    range.end.line = line_number;
    Some((target.to_string(), range))
}
