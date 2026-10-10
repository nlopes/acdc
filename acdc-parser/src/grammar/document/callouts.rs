//! Callout markers in verbatim text.

use crate::{CalloutRef, InlineNode, Location, Verbatim, grammar::ParserState};
use bumpalo::collections::String as BumpString;

/// Splits trailing callout sequences into text and structured references with exact locations.
///
/// Escaped markers remain literal and do not consume an automatic number. Recognized XML
/// markers keep their guards as adjacent text for converter-specific presentation. An explicit
/// block `line-comment` value selects ordinary markers only, even when the value is empty.
pub(super) fn resolve_verbatim_callouts<'a>(
    state: &ParserState<'a>,
    text: &str,
    base_location: Location,
    callouts_enabled: bool,
    xml_callouts_enabled: bool,
) -> (Vec<InlineNode<'a>>, Vec<CalloutRef>) {
    let arena = state.arena;
    if !callouts_enabled {
        return verbatim_without_callouts(state, text, base_location);
    }
    let mut inlines = Vec::new();
    let mut callouts = Vec::new();
    let mut auto_number = 1usize;
    // Allocate text in the arena to avoid a heap allocation and copy for each node.
    let mut segment = VerbatimSegment::new(arena);
    let mut line_start = 0;
    let lines = text.split_inclusive('\n');

    for raw_line in lines {
        let line = raw_line.strip_suffix('\n').unwrap_or(raw_line);
        let line = line.strip_suffix('\r').unwrap_or(line);

        if let Some(first) = first_trailing_callout_marker(line, xml_callouts_enabled) {
            segment.push(
                &line[..first.source_start],
                line_start,
                line_start + first.source_start,
            );

            let mut previous_end = first.source_start;
            let mut marker = first;
            loop {
                segment.push(
                    &line[previous_end..marker.source_start],
                    line_start + previous_end,
                    line_start + marker.source_start,
                );
                if marker.escaped {
                    segment.push(
                        &line[marker.marker_start..marker.end],
                        line_start + marker.source_start,
                        line_start + marker.end,
                    );
                } else {
                    if marker.xml {
                        segment.push(
                            "<!--",
                            line_start + marker.marker_start,
                            line_start + marker.marker_start + 4,
                        );
                    }
                    if let Some(text) = segment.flush(state, base_location.absolute_start) {
                        inlines.push(text);
                    }
                    let callout_start = if marker.xml {
                        marker.marker_start + 4
                    } else {
                        marker.marker_start
                    };
                    let callout_end = if marker.xml {
                        marker.end - 3
                    } else {
                        marker.end
                    };
                    let location = state.create_block_location(
                        line_start + callout_start,
                        line_start + callout_end,
                        base_location.absolute_start,
                    );
                    let callout_ref = match marker.number {
                        ParsedCalloutNumber::Auto => {
                            let callout = CalloutRef::auto(auto_number, location);
                            auto_number += 1;
                            callout
                        }
                        ParsedCalloutNumber::Explicit(number) => {
                            CalloutRef::explicit(number, location)
                        }
                    };
                    inlines.push(InlineNode::CalloutRef(callout_ref.clone()));
                    callouts.push(callout_ref);
                    if marker.xml {
                        segment.push("-->", line_start + marker.end - 3, line_start + marker.end);
                    }
                }
                previous_end = marker.end;
                let Some(next) = next_callout_marker(line, previous_end) else {
                    break;
                };
                marker = next;
            }
            segment.push(
                &line[previous_end..],
                line_start + previous_end,
                line_start + line.len(),
            );
        } else {
            segment.push(line, line_start, line_start + line.len());
        }

        if raw_line.ends_with('\n') {
            segment.push("\n", line_start + line.len(), line_start + raw_line.len());
        }
        line_start += raw_line.len();
    }

    if let Some(text) = segment.flush(state, base_location.absolute_start) {
        inlines.push(text);
    }

    (inlines, callouts)
}

fn verbatim_without_callouts<'a>(
    state: &ParserState<'a>,
    text: &str,
    base_location: Location,
) -> (Vec<InlineNode<'a>>, Vec<CalloutRef>) {
    let location = if text.is_empty() {
        base_location
    } else {
        state.create_block_location(0, text.len(), base_location.absolute_start)
    };
    let mut content = BumpString::new_in(state.arena);
    content.push_str(text);
    (
        vec![InlineNode::VerbatimText(Verbatim {
            content: content.into_bump_str(),
            location,
        })],
        Vec::new(),
    )
}

struct VerbatimSegment<'a> {
    content: BumpString<'a>,
    source_start: Option<usize>,
    source_end: usize,
}

impl<'a> VerbatimSegment<'a> {
    fn new(arena: &'a bumpalo::Bump) -> Self {
        Self {
            content: BumpString::new_in(arena),
            source_start: None,
            source_end: 0,
        }
    }

    fn push(&mut self, content: &str, source_start: usize, source_end: usize) {
        if content.is_empty() {
            return;
        }
        debug_assert!(source_start < source_end);
        debug_assert!(self.source_start.is_none() || self.source_end == source_start);
        self.source_start.get_or_insert(source_start);
        self.source_end = source_end;
        self.content.push_str(content);
    }

    fn flush(&mut self, state: &ParserState<'a>, base_offset: usize) -> Option<InlineNode<'a>> {
        let source_start = self.source_start.take()?;
        let source_end = std::mem::take(&mut self.source_end);
        let content = std::mem::replace(&mut self.content, BumpString::new_in(state.arena));
        Some(InlineNode::VerbatimText(Verbatim {
            content: content.into_bump_str(),
            location: state.create_block_location(source_start, source_end, base_offset),
        }))
    }
}

#[derive(Clone, Copy)]
enum ParsedCalloutNumber {
    Auto,
    Explicit(usize),
}

#[derive(Clone, Copy)]
struct ParsedCalloutMarker {
    source_start: usize,
    marker_start: usize,
    end: usize,
    number: ParsedCalloutNumber,
    escaped: bool,
    xml: bool,
}

fn first_trailing_callout_marker(
    line: &str,
    xml_callouts_enabled: bool,
) -> Option<ParsedCalloutMarker> {
    // Disabled XML markers end the trailing sequence; earlier ordinary markers stay literal too.
    let parse = |end| {
        parse_callout_marker_ending_at(line, end)
            .filter(|marker| xml_callouts_enabled || !marker.xml)
    };
    let mut marker = parse(line.trim_end().len())?;

    loop {
        let adjacent = parse(marker.source_start);
        let spaced = marker
            .source_start
            .checked_sub(1)
            .filter(|index| line.as_bytes().get(*index) == Some(&b' '))
            .and_then(parse);
        let Some(previous) = adjacent.or(spaced) else {
            break;
        };
        marker = previous;
    }

    Some(marker)
}

fn next_callout_marker(line: &str, previous_end: usize) -> Option<ParsedCalloutMarker> {
    parse_callout_marker_starting_at(line, previous_end).or_else(|| {
        previous_end
            .checked_add(1)
            .filter(|_| line.as_bytes().get(previous_end) == Some(&b' '))
            .and_then(|start| parse_callout_marker_starting_at(line, start))
    })
}

fn parse_callout_marker_starting_at(
    line: &str,
    source_start: usize,
) -> Option<ParsedCalloutMarker> {
    let escaped = line.as_bytes().get(source_start) == Some(&b'\\');
    let marker_start = source_start + usize::from(escaped);
    let marker = line.get(marker_start..)?;
    let (number, end, xml) = if let Some(value) = marker.strip_prefix("<!--") {
        let close = value.find("-->")?;
        (
            parse_callout_number(value.get(..close)?)?,
            marker_start + 4 + close + 3,
            true,
        )
    } else {
        let value = marker.strip_prefix('<')?;
        let close = value.find('>')?;
        (
            parse_callout_number(value.get(..close)?)?,
            marker_start + 1 + close + 1,
            false,
        )
    };

    Some(ParsedCalloutMarker {
        source_start,
        marker_start,
        end,
        number,
        escaped,
        xml,
    })
}

fn parse_callout_marker_ending_at(line: &str, end: usize) -> Option<ParsedCalloutMarker> {
    let prefix = line.get(..end)?;
    let marker_start = prefix.rfind('<')?;
    let marker = prefix.get(marker_start..)?;
    let (number, xml) = if marker.starts_with("<!--") && marker.ends_with("-->") {
        (
            parse_callout_number(marker.get(4..marker.len().checked_sub(3)?)?)?,
            true,
        )
    } else if marker.starts_with('<') && marker.ends_with('>') {
        (
            parse_callout_number(marker.get(1..marker.len().checked_sub(1)?)?)?,
            false,
        )
    } else {
        return None;
    };
    let source_start = marker_start
        .checked_sub(1)
        .filter(|index| line.as_bytes().get(*index) == Some(&b'\\'))
        .unwrap_or(marker_start);

    Some(ParsedCalloutMarker {
        source_start,
        marker_start,
        end,
        number,
        escaped: source_start != marker_start,
        xml,
    })
}

fn parse_callout_number(value: &str) -> Option<ParsedCalloutNumber> {
    if value == "." {
        Some(ParsedCalloutNumber::Auto)
    } else {
        value.parse().ok().map(ParsedCalloutNumber::Explicit)
    }
}
