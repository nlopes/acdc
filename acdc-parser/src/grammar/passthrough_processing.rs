use bumpalo::Bump;

use crate::{
    AttributeValue, InlineMacro, InlineNode, LineBreak, Location, ParseInlineResult, Pass,
    PassthroughKind, Plain, ProcessedContent, Raw, Substitution,
    model::substitution::{SubsFlags, resolve_passthrough_substitutions},
    parsed::OwnedInput,
};

use super::{ParserState, inlines::inline_parser, location_mapping::clamp_inline_node_locations};

/// Apply an inline passthrough's substitutions in source order.
fn process_passthrough<'a>(
    content: &'a str,
    passthrough: &Pass<'a>,
    state: &ParserState<'a>,
) -> Vec<InlineNode<'a>> {
    let raw = Raw {
        content,
        location: passthrough_content_location(passthrough, content, state),
        subs: Vec::new(),
    };
    let substitutions = resolve_passthrough_substitutions(&passthrough.substitutions);
    process_raw_substitutions(raw, &substitutions, state)
}

fn passthrough_content_location(
    passthrough: &Pass<'_>,
    content: &str,
    state: &ParserState<'_>,
) -> Location {
    let delimiter_len = match passthrough.kind {
        PassthroughKind::Macro | PassthroughKind::Single => 1,
        PassthroughKind::Double => 2,
        PassthroughKind::Triple => 3,
        PassthroughKind::AttributeRef => return passthrough.location.clone(),
    };
    let total_len = passthrough.location.absolute_end - passthrough.location.absolute_start;
    let prefix_len = total_len.saturating_sub(content.len() + delimiter_len);
    let absolute_start = passthrough.location.absolute_start + prefix_len;
    let absolute_end = absolute_start + content.len();
    Location {
        absolute_start,
        absolute_end,
        start: state
            .line_map
            .offset_to_position(absolute_start, state.input),
        end: state.line_map.offset_to_position(absolute_end, state.input),
    }
}

fn process_raw_substitutions<'a>(
    mut raw: Raw<'a>,
    substitutions: &[Substitution],
    state: &ParserState<'a>,
) -> Vec<InlineNode<'a>> {
    let Some((substitution, remaining)) = substitutions.split_first() else {
        return vec![InlineNode::RawText(raw)];
    };

    match substitution {
        Substitution::SpecialChars => {
            raw.subs.push(substitution.clone());
            process_raw_substitutions(raw, remaining, state)
        }
        Substitution::Replacements => {
            if !raw.subs.contains(&Substitution::Replacements) {
                raw.subs.push(substitution.clone());
            }
            process_raw_substitutions(raw, remaining, state)
        }
        Substitution::Attributes => {
            process_inline_nodes(expand_raw_attributes(&raw, state), remaining, state)
        }
        Substitution::Quotes | Substitution::Macros | Substitution::PostReplacements => {
            process_inline_nodes(
                parse_raw_substitution(raw, substitution, state),
                remaining,
                state,
            )
        }
        // Inline passthroughs do not support callouts. Group variants were
        // expanded before processing started.
        Substitution::Callouts | Substitution::Normal | Substitution::Verbatim => {
            process_raw_substitutions(raw, remaining, state)
        }
    }
}

fn process_inline_nodes<'a>(
    nodes: Vec<InlineNode<'a>>,
    substitutions: &[Substitution],
    state: &ParserState<'a>,
) -> Vec<InlineNode<'a>> {
    if substitutions.is_empty() {
        return nodes;
    }

    if let Some((Substitution::PostReplacements, remaining)) = substitutions.split_first() {
        let nodes = process_post_replacements(nodes, state);
        return process_inline_nodes(nodes, remaining, state);
    }

    let mut result = Vec::with_capacity(nodes.len());
    for mut node in nodes {
        if let InlineNode::RawText(raw) = node {
            result.extend(process_raw_substitutions(raw, substitutions, state));
            continue;
        }
        process_inline_children(&mut node, substitutions, state);
        result.push(node);
    }
    result
}

fn process_post_replacements<'a>(
    nodes: Vec<InlineNode<'a>>,
    state: &ParserState<'a>,
) -> Vec<InlineNode<'a>> {
    let mut staged = Vec::with_capacity(nodes.len());
    for mut node in nodes {
        if let InlineNode::RawText(raw) = node {
            staged.extend(parse_raw_substitution(
                raw,
                &Substitution::PostReplacements,
                state,
            ));
        } else {
            process_inline_children(&mut node, &[Substitution::PostReplacements], state);
            staged.push(node);
        }
    }
    replace_cross_boundary_hardbreaks(staged, state)
}

fn replace_cross_boundary_hardbreaks<'a>(
    nodes: Vec<InlineNode<'a>>,
    state: &ParserState<'a>,
) -> Vec<InlineNode<'a>> {
    let mut result = Vec::with_capacity(nodes.len());
    let mut pending = None;

    for node in nodes {
        let InlineNode::RawText(right) = node else {
            if let Some(left) = pending.take() {
                result.push(InlineNode::RawText(left));
            }
            result.push(node);
            continue;
        };

        let Some(left) = pending.take() else {
            pending = Some(right);
            continue;
        };

        if is_cross_boundary_hardbreak(&result, &left, &right) {
            let location = cross_boundary_hardbreak_location(&left, &right, state);
            remove_cross_boundary_hardbreak_marker(&mut result, left, state);
            result.push(InlineNode::LineBreak(LineBreak { location }));
            pending = raw_without_first_byte(right, state);
        } else {
            result.push(InlineNode::RawText(left));
            pending = Some(right);
        }
    }

    if let Some(raw) = pending {
        result.push(InlineNode::RawText(raw));
    }
    result
}

fn is_cross_boundary_hardbreak(result: &[InlineNode<'_>], left: &Raw<'_>, right: &Raw<'_>) -> bool {
    if !left.content.ends_with('+') || !right.content.starts_with('\n') {
        return false;
    }
    left.content
        .strip_suffix('+')
        .and_then(|prefix| prefix.chars().next_back())
        .or_else(|| match result.last() {
            Some(InlineNode::RawText(raw)) => raw.content.chars().next_back(),
            _ => None,
        })
        == Some(' ')
}

fn remove_cross_boundary_hardbreak_marker<'a>(
    result: &mut Vec<InlineNode<'a>>,
    left: Raw<'a>,
    state: &ParserState<'_>,
) {
    if left.content.len() > 1 {
        if let Some(prefix) = raw_without_suffix(left, 2, state) {
            result.push(InlineNode::RawText(prefix));
        }
        return;
    }

    let Some(InlineNode::RawText(previous)) = result.pop() else {
        return;
    };
    if let Some(prefix) = raw_without_suffix(previous, 1, state) {
        result.push(InlineNode::RawText(prefix));
    }
}

fn raw_without_suffix<'a>(
    mut raw: Raw<'a>,
    suffix_len: usize,
    state: &ParserState<'_>,
) -> Option<Raw<'a>> {
    let end = raw.content.len().checked_sub(suffix_len)?;
    if end == 0 {
        return None;
    }
    raw.location = raw_segment_location(&raw, 0, end, state);
    raw.content = &raw.content[..end];
    Some(raw)
}

fn raw_without_first_byte<'a>(mut raw: Raw<'a>, state: &ParserState<'_>) -> Option<Raw<'a>> {
    if raw.content.len() <= 1 {
        return None;
    }
    raw.location = raw_segment_location(&raw, 1, raw.content.len(), state);
    raw.content = &raw.content[1..];
    Some(raw)
}

fn cross_boundary_hardbreak_location(
    left: &Raw<'_>,
    right: &Raw<'_>,
    state: &ParserState<'_>,
) -> Location {
    let left_source_len = left.location.absolute_end - left.location.absolute_start;
    let absolute_start = if left_source_len == left.content.len() {
        left.location.absolute_end.saturating_sub(1)
    } else {
        left.location.absolute_start
    };
    let absolute_end = (right.location.absolute_start + 1).min(right.location.absolute_end);
    Location {
        absolute_start,
        absolute_end,
        start: state
            .line_map
            .offset_to_position(absolute_start, state.input),
        end: state.line_map.offset_to_position(absolute_end, state.input),
    }
}

fn process_inline_children<'a>(
    node: &mut InlineNode<'a>,
    substitutions: &[Substitution],
    state: &ParserState<'a>,
) {
    macro_rules! process_content {
        ($value:expr) => {
            $value.content =
                process_inline_nodes(std::mem::take(&mut $value.content), substitutions, state)
        };
    }

    match node {
        InlineNode::BoldText(value) => process_content!(value),
        InlineNode::ItalicText(value) => process_content!(value),
        InlineNode::MonospaceText(value) => process_content!(value),
        InlineNode::HighlightText(value) => process_content!(value),
        InlineNode::SubscriptText(value) => process_content!(value),
        InlineNode::SuperscriptText(value) => process_content!(value),
        InlineNode::CurvedQuotationText(value) => process_content!(value),
        InlineNode::CurvedApostropheText(value) => process_content!(value),
        InlineNode::Macro(macro_node) => match macro_node {
            InlineMacro::Footnote(value) => process_content!(value),
            InlineMacro::Url(value) => {
                value.text =
                    process_inline_nodes(std::mem::take(&mut value.text), substitutions, state);
            }
            InlineMacro::Link(value) => {
                value.text =
                    process_inline_nodes(std::mem::take(&mut value.text), substitutions, state);
            }
            InlineMacro::Mailto(value) => {
                value.text =
                    process_inline_nodes(std::mem::take(&mut value.text), substitutions, state);
            }
            InlineMacro::CrossReference(value) => {
                value.text =
                    process_inline_nodes(std::mem::take(&mut value.text), substitutions, state);
            }
            InlineMacro::Icon(_)
            | InlineMacro::Image(_)
            | InlineMacro::Keyboard(_)
            | InlineMacro::Button(_)
            | InlineMacro::Menu(_)
            | InlineMacro::Autolink(_)
            | InlineMacro::Pass(_)
            | InlineMacro::Stem(_)
            | InlineMacro::IndexTerm(_) => {}
        },
        InlineNode::PlainText(_)
        | InlineNode::RawText(_)
        | InlineNode::VerbatimText(_)
        | InlineNode::StandaloneCurvedApostrophe(_)
        | InlineNode::LineBreak(_)
        | InlineNode::InlineAnchor(_)
        | InlineNode::CalloutRef(_) => {}
    }
}

fn expand_raw_attributes<'a>(raw: &Raw<'a>, state: &ParserState<'a>) -> Vec<InlineNode<'a>> {
    let mut result = Vec::new();
    let mut cursor = 0;
    while let Some(relative_start) = raw.content[cursor..].find('{') {
        let start = cursor + relative_start;
        let Some(relative_end) = raw.content[start + 1..].find('}') else {
            break;
        };
        let end = start + 1 + relative_end + 1;
        let name = &raw.content[start + 1..end - 1];
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            cursor = end;
            continue;
        }

        let Some(value) = state.document_attributes.get(name) else {
            cursor = end;
            continue;
        };
        push_raw_segment(&mut result, raw, cursor, start, raw.subs.clone(), state);
        let reference_location = raw_segment_location(raw, start, end, state);
        match value {
            AttributeValue::String(value) => result.push(InlineNode::RawText(Raw {
                content: state.intern_str(value),
                location: reference_location,
                subs: vec![Substitution::SpecialChars],
            })),
            AttributeValue::Bool(true) => {}
            AttributeValue::Bool(false) | AttributeValue::None => {
                push_raw_segment(&mut result, raw, start, end, raw.subs.clone(), state);
            }
        }
        cursor = end;
    }
    push_raw_segment(
        &mut result,
        raw,
        cursor,
        raw.content.len(),
        raw.subs.clone(),
        state,
    );
    result
}

fn push_raw_segment<'a>(
    result: &mut Vec<InlineNode<'a>>,
    raw: &Raw<'a>,
    start: usize,
    end: usize,
    subs: Vec<Substitution>,
    state: &ParserState<'_>,
) {
    if start < end {
        result.push(InlineNode::RawText(Raw {
            content: &raw.content[start..end],
            location: raw_segment_location(raw, start, end, state),
            subs,
        }));
    }
}

fn raw_segment_location(
    raw: &Raw<'_>,
    start: usize,
    end: usize,
    state: &ParserState<'_>,
) -> Location {
    let source_len = raw.location.absolute_end - raw.location.absolute_start;
    let mapped_start = start.min(source_len);
    let mapped_end = end.min(source_len);
    let absolute_start = raw.location.absolute_start + mapped_start;
    let absolute_end = raw.location.absolute_start + mapped_end;
    Location {
        absolute_start,
        absolute_end,
        start: state
            .line_map
            .offset_to_position(absolute_start, state.input),
        end: state.line_map.offset_to_position(absolute_end, state.input),
    }
}

fn parse_raw_substitution<'a>(
    raw: Raw<'a>,
    substitution: &Substitution,
    state: &ParserState<'a>,
) -> Vec<InlineNode<'a>> {
    if raw.content.is_empty() {
        return Vec::new();
    }

    let mut child = ParserState::for_inline_parsing(raw.content, state);
    child.inline_ctx.subs_flags = match substitution {
        Substitution::Quotes => SubsFlags::QUOTES,
        Substitution::Macros => SubsFlags::MACROS,
        Substitution::PostReplacements => SubsFlags::POST_REPLACEMENTS,
        Substitution::SpecialChars
        | Substitution::Attributes
        | Substitution::Replacements
        | Substitution::Normal
        | Substitution::Verbatim
        | Substitution::Callouts => SubsFlags::empty(),
    };
    child.inline_ctx.hardbreaks = false;
    child.inline_ctx.allow_autolinks = matches!(substitution, Substitution::Macros);
    child.inline_ctx.block_level = matches!(substitution, Substitution::PostReplacements);

    let parsed = if matches!(substitution, Substitution::Quotes) {
        inline_parser::quotes_only_inlines(raw.content, &mut child)
    } else {
        inline_parser::inlines(raw.content, &mut child)
    };
    let Ok(mut parsed) = parsed else {
        return vec![InlineNode::RawText(raw)];
    };
    for node in &mut parsed {
        map_stage_locations(node, &raw, state);
        convert_plain_to_raw(node, &raw.subs);
    }
    parsed
}

fn map_stage_locations(node: &mut InlineNode<'_>, raw: &Raw<'_>, state: &ParserState<'_>) {
    let source_len = raw.location.absolute_end - raw.location.absolute_start;
    super::location_walk::walk_inline_locations_mut(node, &mut |location| {
        let relative_start = location.absolute_start.min(source_len);
        let relative_end = location.absolute_end.min(source_len);
        location.absolute_start = raw.location.absolute_start + relative_start;
        location.absolute_end = raw.location.absolute_start + relative_end;
        location.start = state
            .line_map
            .offset_to_position(location.absolute_start, state.input);
        location.end = state
            .line_map
            .offset_to_position(location.absolute_end, state.input);
    });
}

fn convert_plain_to_raw(node: &mut InlineNode<'_>, subs: &[Substitution]) {
    if let InlineNode::PlainText(plain) = node {
        *node = InlineNode::RawText(Raw {
            content: plain.content,
            location: plain.location.clone(),
            subs: subs.to_vec(),
        });
        return;
    }
    match node {
        InlineNode::BoldText(value) => convert_plain_slice(&mut value.content, subs),
        InlineNode::ItalicText(value) => convert_plain_slice(&mut value.content, subs),
        InlineNode::MonospaceText(value) => convert_plain_slice(&mut value.content, subs),
        InlineNode::HighlightText(value) => convert_plain_slice(&mut value.content, subs),
        InlineNode::SubscriptText(value) => convert_plain_slice(&mut value.content, subs),
        InlineNode::SuperscriptText(value) => convert_plain_slice(&mut value.content, subs),
        InlineNode::CurvedQuotationText(value) => convert_plain_slice(&mut value.content, subs),
        InlineNode::CurvedApostropheText(value) => convert_plain_slice(&mut value.content, subs),
        InlineNode::Macro(macro_node) => match macro_node {
            InlineMacro::Footnote(value) => convert_plain_slice(&mut value.content, subs),
            InlineMacro::Url(value) => convert_plain_slice(&mut value.text, subs),
            InlineMacro::Link(value) => convert_plain_slice(&mut value.text, subs),
            InlineMacro::Mailto(value) => convert_plain_slice(&mut value.text, subs),
            InlineMacro::CrossReference(value) => convert_plain_slice(&mut value.text, subs),
            InlineMacro::Icon(_)
            | InlineMacro::Image(_)
            | InlineMacro::Keyboard(_)
            | InlineMacro::Button(_)
            | InlineMacro::Menu(_)
            | InlineMacro::Autolink(_)
            | InlineMacro::Pass(_)
            | InlineMacro::Stem(_)
            | InlineMacro::IndexTerm(_) => {}
        },
        InlineNode::PlainText(_)
        | InlineNode::RawText(_)
        | InlineNode::VerbatimText(_)
        | InlineNode::StandaloneCurvedApostrophe(_)
        | InlineNode::LineBreak(_)
        | InlineNode::InlineAnchor(_)
        | InlineNode::CalloutRef(_) => {}
    }
}

fn convert_plain_slice(nodes: &mut [InlineNode<'_>], subs: &[Substitution]) {
    for node in nodes {
        convert_plain_to_raw(node, subs);
    }
}

/// Parse text for inline formatting markup (bold, italic, monospace, etc.).
///
/// Public entry point — returns a `ParseInlineResult` that owns the arena
/// the resulting `InlineNode`s borrow from. Callers reach the nodes via
/// `.inlines()`. Each call allocates a fresh arena; memory is reclaimed
/// when the returned value is dropped (no leaks). The returned
/// `ParseInlineResult::warnings()` slice is always empty for this entry
/// point — the quotes-only grammar never raises warnings.
///
/// # Supported Patterns
///
/// - `*bold*` and `**bold**` (constrained/unconstrained)
/// - `_italic_` and `__italic__`
/// - `` `monospace` `` and ``` ``monospace`` ```
/// - `^superscript^` and `~subscript~`
/// - `#highlight#` and `##highlight##`
/// - `` "`curved quotes`" `` and `` '`curved apostrophe`' ``
///
/// # Example
///
/// ```
/// use acdc_parser::parse_text_for_quotes;
///
/// let parsed = parse_text_for_quotes("This has *bold* text.");
/// assert_eq!(parsed.inlines().len(), 3); // "This has ", Bold("bold"), " text."
/// ```
pub fn parse_text_for_quotes(content: &str) -> ParseInlineResult {
    let owner = OwnedInput::new(content.into());
    ParseInlineResult::from_infallible(owner, |owner| {
        parse_text_for_quotes_in(&owner.arena, &owner.source)
    })
}

/// Arena-parameterised variant for internal callers that already have an
/// arena threaded through `ParserState`. Avoids the per-call `Bump`
/// allocation that the public entry point does.
pub(crate) fn parse_text_for_quotes_in<'a>(
    arena: &'a Bump,
    content: &'a str,
) -> Vec<InlineNode<'a>> {
    if content.is_empty() {
        return Vec::new();
    }

    // Fast path: if content has no formatting markers, return as plain text
    // without creating a ParserState or invoking the PEG parser.
    // Covers ~87% of calls in typical documents.
    if !content
        .bytes()
        .any(|b| matches!(b, b'*' | b'_' | b'`' | b'#' | b'^' | b'~' | b'"' | b'\''))
    {
        return vec![InlineNode::PlainText(Plain {
            content,
            location: Location::default(),
            escaped: false,
        })];
    }

    let mut state = ParserState::new_quotes_only(content, arena);
    match inline_parser::quotes_only_inlines(content, &mut state) {
        Ok(nodes) => nodes,
        Err(err) => {
            tracing::warn!(
                ?err,
                ?content,
                "quotes-only PEG parse failed, falling back to plain text"
            );
            vec![InlineNode::PlainText(Plain {
                content,
                location: Location::default(),
                escaped: false,
            })]
        }
    }
}

/// Build an `InlineNode::PlainText` at `text`, located at
/// `base_location.start + offset` and extending over `text.len()` columns on
/// the same line.
fn plain_text_at<'a>(text: &'a str, base_location: &Location, offset: usize) -> InlineNode<'a> {
    let abs_start = base_location.absolute_start + offset;
    let col_start = base_location.start.column + u32::try_from(offset).unwrap_or(u32::MAX);
    let line = base_location.start.line;
    InlineNode::PlainText(Plain {
        content: text,
        location: Location {
            absolute_start: abs_start,
            absolute_end: abs_start + text.len(),
            start: crate::Position::new(line, col_start),
            end: crate::Position::new(
                line,
                col_start + u32::try_from(text.len()).unwrap_or(u32::MAX),
            ),
        },
        escaped: false,
    })
}

/// Process passthrough placeholders in content, returning expanded `InlineNode`s.
///
/// This function handles the multi-pass parsing needed for passthroughs with quote substitutions.
/// It splits the content around placeholders and processes each passthrough according to its
/// substitution settings.
pub(crate) fn process_passthrough_placeholders<'a>(
    content: &'a str,
    processed: &'a ProcessedContent<'a>,
    state: &ParserState<'a>,
    base_location: &Location,
) -> Vec<InlineNode<'a>> {
    // Each passthrough produces at most (placeholder-count × small factor) +
    // one trailing-plain. Upper-bound at 2 × placeholders + 1 so a paragraph
    // full of passthroughs doesn't trigger log-N reallocs.
    let mut result = Vec::with_capacity(processed.passthroughs.len() * 2 + 1);
    let mut remaining = content;
    let mut processed_offset = 0; // Position in the processed content (with placeholders)

    // Process each passthrough placeholder in order
    for (index, passthrough) in processed.passthroughs.iter().enumerate() {
        let placeholder = format!("���{index}���");

        if let Some(placeholder_pos) = remaining.find(&placeholder) {
            let before_content = if placeholder_pos > 0 {
                Some(&remaining[..placeholder_pos])
            } else {
                None
            };

            // Add content before the placeholder if any, using original string positions
            if let Some(before) = before_content
                && !before.is_empty()
            {
                result.push(plain_text_at(before, base_location, processed_offset));
                processed_offset += before.len();
            }

            // Process the passthrough content using original string positions from passthrough.location
            if let Some(passthrough_content) = &passthrough.text {
                let processed_nodes = process_passthrough(passthrough_content, passthrough, state);
                for node in processed_nodes {
                    result.push(node);
                }
            }

            // Move past the placeholder in the processed content
            let skip_len = placeholder_pos + placeholder.len();
            remaining = &remaining[skip_len..];
            // Update processed_offset to account for the original passthrough macro length
            processed_offset +=
                passthrough.location.absolute_end - passthrough.location.absolute_start;
        }
    }

    // Add any remaining content as plain text
    if !remaining.is_empty() {
        // Check if the last node is PlainText and merge if so
        if let Some(InlineNode::PlainText(last_plain)) = result.last_mut() {
            // Merge remaining content with the last plain text node
            last_plain.content =
                state.intern_fmt(format_args!("{}{remaining}", last_plain.content));
            // Extend the location to include the remaining content
            last_plain.location.absolute_end = base_location.absolute_end;
            last_plain.location.end = base_location.end.clone();
        } else {
            // Add as separate node if last node is not plain text. Extend
            // the end to cover `base_location.end` (this is the final
            // trailing segment).
            let mut node = plain_text_at(remaining, base_location, processed_offset);
            if let InlineNode::PlainText(ref mut p) = node {
                p.location.absolute_end = base_location.absolute_end;
                p.location.end = base_location.end.clone();
            }
            result.push(node);
        }
    }

    // If no placeholders were found, return the original content as plain text
    if result.is_empty() {
        result.push(InlineNode::PlainText(Plain {
            content,
            location: base_location.clone(),
            escaped: false,
        }));
    }

    // Clamp all locations to valid bounds within the input string
    for node in &mut result {
        clamp_inline_node_locations(node, state.input);
    }

    // Merge adjacent plain text nodes
    merge_adjacent_plain_text_nodes(state, result)
}

/// Merge adjacent plain text nodes into single nodes to simplify the output.
/// Arena-interns the concatenated content so the merged node keeps lifetime `'a`.
pub(crate) fn merge_adjacent_plain_text_nodes<'a>(
    state: &ParserState<'a>,
    nodes: Vec<InlineNode<'a>>,
) -> Vec<InlineNode<'a>> {
    // Worst case: no merges possible, so the output matches the input length.
    let mut result: Vec<InlineNode<'a>> = Vec::with_capacity(nodes.len());

    for node in nodes {
        match (result.last_mut(), node) {
            (Some(InlineNode::PlainText(last_plain)), InlineNode::PlainText(current_plain)) => {
                // Merge current plain text with the last one
                last_plain.content = state.intern_fmt(format_args!(
                    "{}{}",
                    last_plain.content, current_plain.content
                ));
                // Extend the location to cover both nodes
                last_plain.location.absolute_end = current_plain.location.absolute_end;
                last_plain.location.end = current_plain.location.end;
            }
            (_, node) => {
                // Not adjacent plain text nodes, add as separate node
                result.push(node);
            }
        }
    }

    result
}

pub(crate) fn replace_passthrough_placeholders(
    content: &str,
    processed: &ProcessedContent,
) -> String {
    let mut result: String = content.into();

    // Replace each passthrough placeholder with its content
    for (index, passthrough) in processed.passthroughs.iter().enumerate() {
        let placeholder = format!("���{index}���");
        if let Some(text) = &passthrough.text {
            result = result.replace(&placeholder, text);
        }
    }

    result
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)] // Tests verify length before indexing
mod tests {
    use super::*;

    // === Divergence Prevention Tests ===
    //
    // These tests verify that parse_text_for_quotes produces the same structural
    // output as the main PEG parser for common inline formatting patterns.
    // If these tests fail after grammar changes, update parse_text_for_quotes.

    #[test]
    fn test_constrained_bold_pattern() {
        let parsed = parse_text_for_quotes("This is *bold* text.");
        let nodes = parsed.inlines();
        assert_eq!(nodes.len(), 3);
        assert!(matches!(nodes[0], InlineNode::PlainText(_)));
        assert!(
            matches!(&nodes[1], InlineNode::BoldText(b) if matches!(b.content.first(), Some(InlineNode::PlainText(p)) if p.content == "bold"))
        );
        assert!(matches!(nodes[2], InlineNode::PlainText(_)));
    }

    #[test]
    fn test_unconstrained_bold_pattern() {
        let parsed = parse_text_for_quotes("This**bold**word");
        let nodes = parsed.inlines();
        assert_eq!(nodes.len(), 3);
        assert!(
            matches!(&nodes[1], InlineNode::BoldText(b) if matches!(b.content.first(), Some(InlineNode::PlainText(p)) if p.content == "bold"))
        );
    }

    #[test]
    fn test_constrained_italic_pattern() {
        let parsed = parse_text_for_quotes("This is _italic_ text.");
        let nodes = parsed.inlines();
        assert_eq!(nodes.len(), 3);
        assert!(
            matches!(&nodes[1], InlineNode::ItalicText(i) if matches!(i.content.first(), Some(InlineNode::PlainText(p)) if p.content == "italic"))
        );
    }

    #[test]
    fn test_unconstrained_italic_pattern() {
        let parsed = parse_text_for_quotes("This__italic__word");
        let nodes = parsed.inlines();
        assert_eq!(nodes.len(), 3);
        assert!(
            matches!(&nodes[1], InlineNode::ItalicText(i) if matches!(i.content.first(), Some(InlineNode::PlainText(p)) if p.content == "italic"))
        );
    }

    #[test]
    fn test_constrained_monospace_pattern() {
        let parsed = parse_text_for_quotes("Use `code` here.");
        let nodes = parsed.inlines();
        assert_eq!(nodes.len(), 3);
        assert!(
            matches!(&nodes[1], InlineNode::MonospaceText(m) if matches!(m.content.first(), Some(InlineNode::PlainText(p)) if p.content == "code"))
        );
    }

    #[test]
    fn test_superscript_pattern() {
        let parsed = parse_text_for_quotes("E=mc^2^");
        let nodes = parsed.inlines();
        assert_eq!(nodes.len(), 2);
        assert!(
            matches!(&nodes[1], InlineNode::SuperscriptText(s) if matches!(s.content.first(), Some(InlineNode::PlainText(p)) if p.content == "2"))
        );
    }

    #[test]
    fn test_subscript_pattern() {
        let parsed = parse_text_for_quotes("H~2~O");
        let nodes = parsed.inlines();
        assert_eq!(nodes.len(), 3);
        assert!(
            matches!(&nodes[1], InlineNode::SubscriptText(s) if matches!(s.content.first(), Some(InlineNode::PlainText(p)) if p.content == "2"))
        );
    }

    #[test]
    fn test_highlight_pattern() {
        let parsed = parse_text_for_quotes("This is #highlighted# text.");
        let nodes = parsed.inlines();
        assert_eq!(nodes.len(), 3);
        assert!(
            matches!(&nodes[1], InlineNode::HighlightText(h) if matches!(h.content.first(), Some(InlineNode::PlainText(p)) if p.content == "highlighted"))
        );
    }

    #[test]
    fn test_escaped_superscript_not_parsed() {
        // Backslash-escaped markers should not be parsed as formatting
        let parsed = parse_text_for_quotes(r"E=mc\^2^");
        let nodes = parsed.inlines();
        // Should remain as plain text (escape prevents parsing)
        assert!(
            nodes.iter().all(|n| matches!(n, InlineNode::PlainText(_))),
            "Escaped superscript should not be parsed"
        );
    }

    #[test]
    fn test_escaped_subscript_not_parsed() {
        let parsed = parse_text_for_quotes(r"H\~2~O");
        let nodes = parsed.inlines();
        assert!(
            nodes.iter().all(|n| matches!(n, InlineNode::PlainText(_))),
            "Escaped subscript should not be parsed"
        );
    }

    #[test]
    fn test_multiple_formats_in_sequence() {
        let parsed = parse_text_for_quotes("*bold* and _italic_ and `code`");
        let nodes = parsed.inlines();
        assert!(nodes.iter().any(|n| matches!(n, InlineNode::BoldText(_))));
        assert!(nodes.iter().any(|n| matches!(n, InlineNode::ItalicText(_))));
        assert!(
            nodes
                .iter()
                .any(|n| matches!(n, InlineNode::MonospaceText(_)))
        );
    }

    #[test]
    fn test_plain_text_only() {
        let parsed = parse_text_for_quotes("Just plain text here.");
        let nodes = parsed.inlines();
        assert_eq!(nodes.len(), 1);
        assert!(matches!(nodes[0], InlineNode::PlainText(_)));
    }

    #[test]
    fn test_empty_input() {
        let parsed = parse_text_for_quotes("");
        let nodes = parsed.inlines();
        assert!(nodes.is_empty());
    }
}
