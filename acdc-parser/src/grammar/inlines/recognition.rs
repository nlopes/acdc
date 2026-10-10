//! Inline syntax recognition and substitution-order checks.

use crate::{
    InlineMacro, InlineNode, Substitution,
    grammar::{
        ParserState,
        inline_boundaries::check_constrained_opening_boundary,
        state::{InlineRules, InlineUrlBoundary},
    },
};

/// RFC 5321 max local-part length. An email address must have `@` within this
/// many bytes of the start of the local part.
const EMAIL_LOCAL_PART_MAX: usize = 64;

pub(super) fn catalog_macro_allowed(state: &ParserState<'_>, start: usize) -> bool {
    !state.inline_ctx.rules.contains(InlineRules::INDEX_CATALOG)
        || [
            "image:",
            "icon:",
            "kbd:",
            "btn:",
            "menu:",
            "stem:",
            "latexmath:",
            "asciimath:",
            "pass:",
        ]
        .iter()
        .any(|prefix| state.input[start..].starts_with(prefix))
}

pub(super) fn catalog_escape_allowed(state: &ParserState<'_>, start: usize) -> bool {
    if !state.inline_ctx.rules.contains(InlineRules::INDEX_CATALOG) {
        return true;
    }
    let tail = state.input[start..].trim_start_matches('\\');
    match tail.as_bytes().first() {
        Some(b'*' | b'_' | b'#' | b'`' | b'^' | b'~') => state
            .inline_ctx
            .substitutions
            .enabled(&Substitution::Quotes),
        Some(b'{') => state
            .inline_ctx
            .substitutions
            .enabled(&Substitution::Attributes),
        _ => true,
    }
}

/// Return whether a byte can start ordinary text without further syntax checks.
/// Letters that can start macros need a separate check. A space can still begin
/// the hard-break pattern ` +`.
pub(super) const fn is_plain_text_safe(b: u8) -> bool {
    matches!(
        b,
        b'A'..=b'Z'
            | b'0'..=b'9'
            | b'c' | b'd' | b'e' | b'g' | b'j'
            | b'n' | b'o' | b'q' | b'r' | b't'
            | b'u' | b'v' | b'w' | b'y' | b'z'
            | b' '
    )
}

/// Check whether `@` appears within [`EMAIL_LOCAL_PART_MAX`] bytes from `pos`.
pub(super) fn has_at_sign_ahead(state: &ParserState, pos: usize) -> bool {
    use crate::grammar::state::AtLookahead;

    if let Some(cache) = state.next_at_sign_cache.get() {
        match cache.first_at {
            Some(at) if at >= pos => return at < pos + EMAIL_LOCAL_PART_MAX,
            None if pos + EMAIL_LOCAL_PART_MAX <= cache.scanned_up_to => return false,
            // Cached `@` is behind us, or the cached range doesn't cover the
            // full lookahead window; fall through and rescan.
            Some(_) | None => {}
        }
    }

    let input = state.input.as_bytes();
    let start = pos.min(input.len());
    let scan_end = (pos + 1024).min(input.len());
    let window = input.get(start..scan_end).unwrap_or(&[]);
    let first_at = window
        .iter()
        .position(|&b| b == b'@')
        .map(|off| start + off);
    state.next_at_sign_cache.set(Some(AtLookahead {
        scanned_up_to: scan_end,
        first_at,
    }));
    first_at.is_some_and(|at| at < pos + EMAIL_LOCAL_PART_MAX)
}

pub(super) fn byte_came_from_attribute(state: &ParserState<'_>, position: usize) -> bool {
    let range_index = state
        .attribute_value_ranges
        .partition_point(|range| range.end <= position);
    state
        .attribute_value_ranges
        .get(range_index)
        .is_some_and(|range| range.contains(&position))
}

pub(super) fn structural_token_allowed(
    state: &ParserState<'_>,
    substitution: &Substitution,
    start: usize,
    len: usize,
) -> bool {
    !state
        .inline_ctx
        .substitutions
        .precedes(substitution, &Substitution::Attributes)
        || (!(start..start + len).any(|position| byte_came_from_attribute(state, position))
            // Removing an attribute can join separate characters into a marker
            // that did not exist when the earlier substitution ran.
            && !state.empty_attribute_offsets.iter()
                .any(|offset| start < *offset && *offset < start + len))
}

pub(super) fn macro_token_allowed(state: &ParserState<'_>, start: usize, len: usize) -> bool {
    structural_token_allowed(state, &Substitution::Macros, start, len)
}

pub(super) fn url_opening_allowed(
    state: &ParserState<'_>,
    position: usize,
    bare: bool,
    escaped: bool,
) -> bool {
    let prefix = &state.input[..position];
    let prefix = if escaped {
        prefix.trim_end_matches('\\')
    } else {
        prefix
    };
    // A later attribute value cannot replace the original reference's `}` with
    // a URI boundary. The same check covers references that expand to nothing.
    if state
        .inline_ctx
        .substitutions
        .precedes(&Substitution::Macros, &Substitution::Attributes)
        && state
            .late_attribute_sources
            .iter()
            .any(|(range, _)| range.end == prefix.len())
    {
        return false;
    }
    let previous = match state.last_url_boundary {
        Some(InlineUrlBoundary::Formatted(end)) if end == prefix.len() => return true,
        Some(InlineUrlBoundary::Protected(end)) if end == prefix.len() => return false,
        Some(InlineUrlBoundary::Text(end, previous)) if end == prefix.len() => previous,
        _ => prefix.chars().next_back(),
    };
    previous.is_none_or(|previous| {
        match previous {
            '"' | '\'' => !bare || escaped,
            '<' | '>' | '(' | ')' | '[' | ']' | ';' => true,
            // URI prefixes accept horizontal Unicode blanks and line starts,
            // but not vertical tabs, form feeds or Unicode line separators.
            _ => {
                previous.is_whitespace()
                    && !matches!(
                        previous,
                        '\u{b}' | '\u{c}' | '\u{85}' | '\u{2028}' | '\u{2029}'
                    )
            }
        }
    })
}

pub(super) fn record_url_boundary(
    state: &mut ParserState<'_>,
    node: &InlineNode<'_>,
    end: usize,
    substitution: &Substitution,
) {
    // Only completed inline nodes establish this boundary. Lookahead does not
    // call this helper, and each nested inline parse starts with no prior node.
    let formatted = matches!(
        node,
        InlineNode::BoldText(_)
            | InlineNode::ItalicText(_)
            | InlineNode::MonospaceText(_)
            | InlineNode::HighlightText(_)
            | InlineNode::SubscriptText(_)
            | InlineNode::SuperscriptText(_)
    ) && state
        .inline_ctx
        .substitutions
        .precedes(substitution, &Substitution::Macros);
    state.last_url_boundary = if formatted {
        Some(InlineUrlBoundary::Formatted(end))
    } else if matches!(node, InlineNode::Macro(InlineMacro::Pass(_))) {
        Some(InlineUrlBoundary::Protected(end))
    } else if *substitution == Substitution::Attributes {
        let text = if let InlineNode::PlainText(text) = node {
            Some(text.content)
        } else if let InlineNode::RawText(text) = node {
            Some(text.content)
        } else {
            None
        };
        text.map(|text| InlineUrlBoundary::Text(end, text.chars().next_back()))
    } else {
        None
    };
}

pub(super) fn index_content_present(state: &ParserState<'_>, start: usize, text: &str) -> bool {
    !text.is_empty()
        || (state
            .inline_ctx
            .substitutions
            .precedes(&Substitution::Macros, &Substitution::Attributes)
            && state
                .late_attribute_sources
                .iter()
                .any(|(range, _)| range.start == start && range.is_empty()))
}

pub(super) fn has_inline_line_break_prefix(state: &ParserState<'_>, span_start: usize) -> bool {
    let absolute_pos = span_start + state.inline_ctx.offset;
    let preceded_by_content_or_line_end = absolute_pos > 0
        && state
            .input
            .as_bytes()
            .get(absolute_pos.saturating_sub(1))
            .is_some_and(|&byte| !byte.is_ascii_whitespace() || matches!(byte, b'\n' | b'\r'));
    preceded_by_content_or_line_end
        || state
            .empty_attribute_offsets
            .binary_search(&span_start)
            .is_ok()
}

pub(super) fn code_followed_by_attribute(state: &ParserState<'_>, position: usize) -> bool {
    state
        .inline_ctx
        .substitutions
        .precedes(&Substitution::Quotes, &Substitution::Attributes)
        && state
            .late_attribute_sources
            .iter()
            .any(|(range, _)| range.start == position)
}

pub(super) fn check_code_opening_boundary(state: &ParserState<'_>, position: usize) -> bool {
    // A reference ended in `}` when quotes ran. Its later value must not turn
    // a literal closing backtick into a new opening delimiter.
    if state
        .inline_ctx
        .substitutions
        .precedes(&Substitution::Quotes, &Substitution::Attributes)
        && state
            .late_attribute_sources
            .iter()
            .any(|(range, _)| range.end == position)
    {
        return false;
    }
    check_constrained_opening_boundary(
        position,
        state.input.as_bytes(),
        state.outer_constrained_delimiter,
        b'`',
    )
}
