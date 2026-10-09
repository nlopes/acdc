use bumpalo::Bump;
use std::mem::take;

use crate::{
    InlineMacro, InlineNode, LineBreak, Location, ParseInlineResult, Pass, PassthroughKind, Plain,
    ProcessedContent, Raw, Substitution,
    model::substitution::{
        SubstitutionPlan, character_reference_end, escape_attribute_characters,
        is_restorable_character_reference, resolve_passthrough_substitutions,
    },
    parsed::OwnedInput,
};

use super::{
    ParserState,
    inlines::inline_parser,
    location_mapping::clamp_inline_node_locations,
    state::{InlineContext, InlineRules},
    utf8_utils::{RoundDirection, step_char},
};

const QUOTES_THEN_MACROS: &[Substitution] = &[Substitution::Quotes, Substitution::Macros];

/// Expand an attribute at its grammar position, before later macros register.
/// Locations stay relative to the placeholder until the normal source map runs.
pub(crate) fn process_attribute_placeholder<'a>(
    index: usize,
    start: usize,
    end: usize,
    state: &ParserState<'a>,
) -> Vec<InlineNode<'a>> {
    let Some(mut pass) = state.attribute_passthroughs.get(index).cloned() else {
        return Vec::new();
    };
    pass.location = state.create_location(start, end);
    process_passthrough(
        pass.text.unwrap_or_default(),
        &pass,
        state,
        state.inline_ctx.substitutions,
    )
}

/// Apply an inline passthrough's substitutions in source order.
fn process_passthrough<'a>(
    content: &'a str,
    passthrough: &Pass<'a>,
    state: &ParserState<'a>,
    attribute_substitutions: SubstitutionPlan,
) -> Vec<InlineNode<'a>> {
    let raw = Raw {
        content,
        location: passthrough_content_location(passthrough, content, state),
        subs: Vec::new(),
    };
    if !passthrough.attribute_fragments.is_empty() {
        let mut nodes = Vec::new();
        let remaining = attribute_substitutions.after(&Substitution::Attributes);
        let mut cursor = 0;
        for fragment in &passthrough.attribute_fragments {
            if cursor < fragment.range.start {
                nodes.extend(process_raw_substitutions(
                    Raw {
                        content: &content[cursor..fragment.range.start],
                        ..raw.clone()
                    },
                    &remaining,
                    state,
                ));
            }
            nodes.extend(process_raw_substitutions(
                Raw {
                    content: &content[fragment.range.clone()],
                    ..raw.clone()
                },
                &fragment.substitutions,
                state,
            ));
            cursor = fragment.range.end;
        }
        if cursor < content.len() {
            nodes.extend(process_raw_substitutions(
                Raw {
                    content: &content[cursor..],
                    ..raw.clone()
                },
                &remaining,
                state,
            ));
        }
        // These characters replace one source reference, irrespective of the
        // retained text's length or the number of parsing profiles it contains.
        for node in &mut nodes {
            super::location_walk::walk_inline_locations_mut(node, &mut |location| {
                *location = raw.location.clone();
            });
        }
        return nodes;
    }
    let substitutions = resolve_passthrough_substitutions(&passthrough.substitutions);
    if passthrough.kind != PassthroughKind::Macro || !content.contains(r"\]") {
        return process_raw_substitutions(raw, &substitutions, state);
    }

    // Remove bracket escapes before substitutions. Keep source fragments so
    // subsequent parsing can map nodes across each removed backslash.
    let mut fragments = Vec::new();
    let mut cursor = 0;
    for (start, _) in content.match_indices(r"\]") {
        push_raw_segment(&mut fragments, &raw, cursor, start, Vec::new(), state);
        fragments.push(InlineNode::RawText(Raw {
            content: &content[start + 1..start + 2],
            location: raw_segment_location(&raw, start, start + 2, state),
            subs: Vec::new(),
        }));
        cursor = start + 2;
    }
    push_raw_segment(
        &mut fragments,
        &raw,
        cursor,
        content.len(),
        Vec::new(),
        state,
    );
    process_inline_nodes(fragments, &substitutions, state)
}

fn passthrough_content_location(
    passthrough: &Pass<'_>,
    content: &str,
    state: &ParserState<'_>,
) -> Location {
    // Preprocessor ranges are half-open; published node locations are inclusive.
    let delimiter_len = match passthrough.kind {
        PassthroughKind::Macro | PassthroughKind::Single => 1,
        PassthroughKind::Double => 2,
        PassthroughKind::Triple => 3,
        PassthroughKind::AttributeRef => {
            return state.create_block_location(
                passthrough.location.absolute_start,
                passthrough.location.absolute_end,
                0,
            );
        }
    };
    let total_len = passthrough.location.absolute_end - passthrough.location.absolute_start;
    let prefix_len = total_len.saturating_sub(content.len() + delimiter_len);
    let absolute_start = passthrough.location.absolute_start + prefix_len;
    state.create_block_location(absolute_start, absolute_start + content.len(), 0)
}

fn process_raw_substitutions<'a>(
    mut raw: Raw<'a>,
    substitutions: &[Substitution],
    state: &ParserState<'a>,
) -> Vec<InlineNode<'a>> {
    let Some((substitution, remaining)) = substitutions.split_first() else {
        return vec![InlineNode::RawText(raw)];
    };

    if let [Substitution::Quotes, Substitution::Macros, remaining @ ..] = substitutions {
        let parsed = parse_raw_stage(
            raw,
            SubstitutionPlan::from_substitutions(QUOTES_THEN_MACROS),
            state,
        );
        return process_inline_nodes(parsed, remaining, state);
    }

    match substitution {
        Substitution::SpecialChars => {
            if raw.subs.last() == Some(&Substitution::SpecialChars) {
                // Keep one deferred escape stage. Earlier escapes are literal
                // text, including when an attribute introduced the fragment.
                raw.content = state.intern_str(&escape_attribute_characters(raw.content));
            } else {
                raw.subs.push(substitution.clone());
            }
            process_raw_substitutions(raw, remaining, state)
        }
        Substitution::Replacements => {
            if let Some(fragments) = restore_raw_character_references(&raw, state) {
                return process_inline_nodes(fragments, remaining, state);
            }
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

fn restore_raw_character_references<'a>(
    raw: &Raw<'a>,
    state: &ParserState<'a>,
) -> Option<Vec<InlineNode<'a>>> {
    if !raw.content.contains('&') {
        return None;
    }
    let mut fragments = Vec::new();
    let mut cursor = 0;
    let mut subs = raw.subs.clone();
    if !subs.contains(&Substitution::Replacements) {
        subs.push(Substitution::Replacements);
    }
    for (amp, _) in raw.content.match_indices('&') {
        if amp < cursor {
            continue;
        }
        let Some(length) = character_reference_end(&raw.content[amp..]) else {
            continue;
        };
        let end = amp + length;
        // Keep encoded arrow tokens together for deferred replacements. A prior
        // escape stage instead protects the authored reference from that rule.
        let encoded_arrow = match &raw.content[amp..end] {
            "&gt;" => amp > 0 && matches!(raw.content.as_bytes().get(amp - 1), Some(b'-' | b'=')),
            "&lt;" => {
                matches!(raw.content.as_bytes().get(end), Some(b'-' | b'='))
                    || raw.content[end..].starts_with("\\-&gt;")
                    || raw.content[end..].starts_with("\\=&gt;")
            }
            _ => false,
        };
        if encoded_arrow && !raw.subs.contains(&Substitution::SpecialChars) {
            continue;
        }
        let mut start = if amp > cursor && raw.content.as_bytes().get(amp - 1) == Some(&b'\\') {
            amp - 1
        } else {
            amp
        };
        let mut prepared = raw.content[start..end].to_owned();
        for substitution in &raw.subs {
            if *substitution == Substitution::SpecialChars {
                prepared = escape_attribute_characters(&prepared);
            }
        }
        let escaped = prepared.starts_with('\\');
        let reference = prepared.strip_prefix('\\').unwrap_or(&prepared);
        if let Some((body, _)) = reference
            .strip_prefix("&amp;")
            .and_then(|rest| rest.split_once(';'))
            && is_restorable_character_reference(body)
        {
            prepared = if escaped {
                reference.to_owned()
            } else {
                format!("&{}", &reference[5..])
            };
        }

        if prepared.starts_with('\\') {
            start = amp;
            prepared.remove(0);
        }

        // Retain an escaping stage for literal references, but remove it from
        // restored ones so every converter sees the same active character.
        let mut reference_subs = Vec::new();
        for (encoded, decoded) in [("&amp;", "&"), ("&lt;", "<"), ("&gt;", ">")] {
            if let Some(rest) = prepared.strip_prefix(encoded) {
                prepared = format!("{decoded}{rest}");
                reference_subs.push(Substitution::SpecialChars);
                break;
            }
        }
        push_raw_segment(&mut fragments, raw, cursor, start, subs.clone(), state);
        fragments.push(InlineNode::RawText(Raw {
            content: state.intern_str(&prepared),
            location: raw_segment_location(raw, start, end, state),
            subs: reference_subs,
        }));
        cursor = end;
    }
    if fragments.is_empty() {
        return None;
    }
    push_raw_segment(&mut fragments, raw, cursor, raw.content.len(), subs, state);
    Some(fragments)
}

fn process_inline_nodes<'a>(
    nodes: Vec<InlineNode<'a>>,
    substitutions: &[Substitution],
    state: &ParserState<'a>,
) -> Vec<InlineNode<'a>> {
    let Some((substitution, remaining)) = substitutions.split_first() else {
        return nodes;
    };

    if matches!(substitution, Substitution::PostReplacements) {
        let nodes = process_post_replacements(nodes, state);
        return process_inline_nodes(nodes, remaining, state);
    }

    if matches!(substitution, Substitution::Quotes | Substitution::Macros) {
        let (stages, remaining) =
            if let [Substitution::Quotes, Substitution::Macros, remaining @ ..] = substitutions {
                (QUOTES_THEN_MACROS, remaining)
            } else {
                (std::slice::from_ref(substitution), remaining)
            };
        let plan = SubstitutionPlan::from_substitutions(stages);
        let mut result = Vec::with_capacity(nodes.len());
        let mut fragments = Vec::new();
        for mut node in nodes {
            if let InlineNode::RawText(raw) = node {
                fragments.push(raw);
            } else {
                result.extend(parse_raw_fragments(take(&mut fragments), plan, state));
                process_inline_children(&mut node, stages, state);
                result.push(node);
            }
        }
        result.extend(parse_raw_fragments(fragments, plan, state));
        return process_inline_nodes(result, remaining, state);
    }

    // Finish this stage across all fragments before later macros join them.
    let mut result = Vec::with_capacity(nodes.len());
    for mut node in nodes {
        if let InlineNode::RawText(raw) = node {
            result.extend(process_raw_substitutions(
                raw,
                std::slice::from_ref(substitution),
                state,
            ));
            continue;
        }
        process_inline_children(&mut node, std::slice::from_ref(substitution), state);
        result.push(node);
    }
    process_inline_nodes(result, remaining, state)
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
            let location = cross_boundary_hardbreak_location(&result, &left, state);
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
    preceding: &[InlineNode<'_>],
    left: &Raw<'_>,
    state: &ParserState<'_>,
) -> Location {
    let start = if left.content.len() > 1 {
        raw_segment_location(left, left.content.len() - 2, left.content.len(), state).absolute_start
    } else if let Some(InlineNode::RawText(previous)) = preceding.last() {
        raw_segment_location(
            previous,
            previous.content.len().saturating_sub(1),
            previous.content.len(),
            state,
        )
        .absolute_start
    } else {
        left.location.absolute_start
    };
    state.create_location(start, left.location.absolute_end)
}

fn process_inline_children<'a>(
    node: &mut InlineNode<'a>,
    substitutions: &[Substitution],
    state: &ParserState<'a>,
) {
    macro_rules! process_content {
        ($value:expr) => {
            $value.content = process_inline_nodes(take(&mut $value.content), substitutions, state)
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
            InlineMacro::IndexTerm(value) => {
                if value.catalog.is_none() {
                    value.catalog = Some(Box::new((**value).clone()));
                }
                value.for_each_label_mut(|nodes| {
                    *nodes = process_inline_nodes(take(nodes), substitutions, state);
                });
            }
            InlineMacro::Url(value) => {
                value.text = process_inline_nodes(take(&mut value.text), substitutions, state);
            }
            InlineMacro::Link(value) => {
                value.text = process_inline_nodes(take(&mut value.text), substitutions, state);
            }
            InlineMacro::Mailto(value) => {
                value.text = process_inline_nodes(take(&mut value.text), substitutions, state);
            }
            InlineMacro::CrossReference(value) => {
                value.text = process_inline_nodes(take(&mut value.text), substitutions, state);
            }
            // Footnotes retain the body captured at the macro stage.
            InlineMacro::Footnote(_)
            | InlineMacro::Icon(_)
            | InlineMacro::Image(_)
            | InlineMacro::Keyboard(_)
            | InlineMacro::Button(_)
            | InlineMacro::Menu(_)
            | InlineMacro::Autolink(_)
            | InlineMacro::Pass(_)
            | InlineMacro::Stem(_) => {}
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
    let mut copied_until = 0;
    while let Some(relative_start) = raw.content[cursor..].find('{') {
        let start = cursor + relative_start;
        let Some(relative_end) = raw.content[start + 1..].find('}') else {
            break;
        };
        let end = start + 1 + relative_end + 1;
        let name = &raw.content[start + 1..end - 1];
        if let Some((source_start, literal)) =
            escaped_raw_attribute_reference(raw, start, end, state)
        {
            push_raw_segment(
                &mut result,
                raw,
                copied_until,
                source_start,
                raw.subs.clone(),
                state,
            );
            result.push(literal);
            cursor = end;
            copied_until = end;
            continue;
        }
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            cursor = end;
            continue;
        }

        let Some(resolved) = state.document_attributes.get(name) else {
            cursor = end;
            continue;
        };
        push_raw_segment(
            &mut result,
            raw,
            copied_until,
            start,
            raw.subs.clone(),
            state,
        );
        let reference_location = raw_segment_location(raw, start, end, state);
        let mut value = String::new();
        let _ = resolved.write_text(&mut value);
        cursor = end;
        copied_until = end;
        if !resolved.inline_fragments().is_empty() {
            let passthrough = Pass {
                attribute_fragments: resolved.inline_fragments().into(),
                text: Some(state.intern_str(&value)),
                substitutions: Vec::new(),
                location: reference_location,
                kind: PassthroughKind::AttributeRef,
            };
            result.extend(process_passthrough(
                passthrough.text.unwrap_or_default(),
                &passthrough,
                state,
                SubstitutionPlan::only(&Substitution::Attributes),
            ));
            continue;
        }
        let mut value_cursor = 0;
        // Prepared ranges have already undergone definition-time substitutions.
        // Only ordinary attribute text still needs its header escaping here.
        for range in resolved.passthrough_ranges() {
            if value_cursor < range.start {
                result.push(InlineNode::RawText(Raw {
                    content: state.intern_str(&value[value_cursor..range.start]),
                    location: reference_location.clone(),
                    subs: vec![Substitution::SpecialChars],
                }));
            }
            result.push(InlineNode::RawText(Raw {
                content: state.intern_str(&value[range.clone()]),
                location: reference_location.clone(),
                subs: Vec::new(),
            }));
            value_cursor = range.end;
        }
        if value_cursor < value.len() {
            result.push(InlineNode::RawText(Raw {
                content: state.intern_str(&value[value_cursor..]),
                location: reference_location,
                subs: vec![Substitution::SpecialChars],
            }));
        }
    }
    push_raw_segment(
        &mut result,
        raw,
        copied_until,
        raw.content.len(),
        raw.subs.clone(),
        state,
    );
    result
}

fn escaped_raw_attribute_reference<'a>(
    raw: &Raw<'a>,
    start: usize,
    end: usize,
    state: &ParserState<'a>,
) -> Option<(usize, InlineNode<'a>)> {
    let name = &raw.content[start + 1..end - 1];
    let escaped_start = raw.content[..start].ends_with('\\');
    if !escaped_start && !name.ends_with('\\') {
        return None;
    }
    let name = name.strip_suffix('\\').unwrap_or(name);
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return None;
    }
    let source_start = start - usize::from(escaped_start);
    let literal = format!("{{{name}}}");
    Some((
        source_start,
        InlineNode::RawText(Raw {
            content: state.intern_str(&literal),
            location: raw_segment_location(raw, source_start, end, state),
            subs: raw.subs.clone(),
        }),
    ))
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
    let Some(source_start) = raw_source_start(raw, state) else {
        // A generated fragment maps to the complete source reference that produced it.
        return raw.location.clone();
    };
    let start = if start == 0 {
        raw.location.absolute_start
    } else {
        source_start + start
    };
    state.create_block_location(start, source_start + end, 0)
}

fn raw_source_start(raw: &Raw<'_>, state: &ParserState<'_>) -> Option<usize> {
    let end = step_char(
        state.input,
        raw.location.absolute_end,
        RoundDirection::Forward,
    );
    let original = state.input.get(raw.location.absolute_start..end)?;
    let escapes = original.strip_suffix(raw.content)?;
    escapes
        .bytes()
        .all(|byte| byte == b'\\')
        .then_some(raw.location.absolute_start + escapes.len())
}

fn parse_raw_substitution<'a>(
    raw: Raw<'a>,
    substitution: &Substitution,
    state: &ParserState<'a>,
) -> Vec<InlineNode<'a>> {
    parse_raw_stage(raw, SubstitutionPlan::only(substitution), state)
}

fn parse_raw_stage<'a>(
    raw: Raw<'a>,
    plan: SubstitutionPlan,
    state: &ParserState<'a>,
) -> Vec<InlineNode<'a>> {
    if raw.content.is_empty() {
        return Vec::new();
    }

    let Some(mut parsed) = parse_raw_inlines(raw.content, plan, state) else {
        return vec![InlineNode::RawText(raw)];
    };
    for node in &mut parsed {
        convert_plain_to_raw_with_source(node, &raw.subs, Some(raw.content));
        map_stage_locations(node, &raw, state);
    }
    parsed
}

fn parse_raw_inlines<'a>(
    content: &'a str,
    plan: SubstitutionPlan,
    state: &ParserState<'a>,
) -> Option<Vec<InlineNode<'a>>> {
    let mut rules = state.inline_ctx.rules;
    rules.set(InlineRules::AUTOLINKS, plan.enabled(&Substitution::Macros));
    rules.remove(InlineRules::HARD_BREAKS);
    rules.set(
        InlineRules::EOI_HARD_BREAK,
        plan.enabled(&Substitution::PostReplacements),
    );
    let inline_ctx = InlineContext {
        offset: 0,
        substitutions: plan,
        rules,
    };
    let mut child = ParserState::for_inline_parsing(content, state, inline_ctx);

    let parsed = if plan.enabled(&Substitution::Quotes) && !plan.enabled(&Substitution::Macros) {
        inline_parser::quotes_only_inlines(content, &mut child)
    } else {
        inline_parser::inlines(content, &mut child)
    };
    parsed.ok()
}

fn map_stage_locations(node: &mut InlineNode<'_>, raw: &Raw<'_>, state: &ParserState<'_>) {
    super::location_walk::walk_inline_locations_mut(node, &mut |location| {
        let end = step_char(raw.content, location.absolute_end, RoundDirection::Forward);
        *location = raw_segment_location(raw, location.absolute_start, end, state);
    });
}

// Attribute expansion splits raw text to retain its escaping rules. A later
// structural stage must still recognize syntax across those fragment boundaries.
fn parse_raw_fragments<'a>(
    fragments: Vec<Raw<'a>>,
    plan: SubstitutionPlan,
    state: &ParserState<'a>,
) -> Vec<InlineNode<'a>> {
    if fragments.len() <= 1 {
        return fragments
            .into_iter()
            .flat_map(|raw| parse_raw_stage(raw, plan, state))
            .collect();
    }
    let text: String = fragments.iter().map(|raw| raw.content).collect();
    let text = state.intern_str(&text);
    let Some(mut nodes) = parse_raw_inlines(text, plan, state) else {
        return fragments.into_iter().map(InlineNode::RawText).collect();
    };
    for node in &mut nodes {
        convert_plain_to_raw_with_source(node, &[], Some(text));
    }
    let mut offset = 0;
    let spans: Vec<_> = fragments
        .iter()
        .map(|raw| {
            let start = offset;
            offset += raw.content.len();
            (start..offset, raw)
        })
        .collect();
    let mut nodes = restore_fragment_substitutions(nodes, &spans, text);
    for node in &mut nodes {
        super::location_walk::walk_inline_locations_mut(node, &mut |location| {
            location.absolute_start =
                fragment_offset(location.absolute_start, &spans, false, state);
            location.absolute_end = fragment_offset(location.absolute_end, &spans, true, state);
            location.start = state
                .line_map
                .offset_to_position(location.absolute_start, state.input);
            location.end = state
                .line_map
                .offset_to_position(location.absolute_end, state.input);
        });
    }
    nodes
}

fn fragment_offset(
    offset: usize,
    fragments: &[(std::ops::Range<usize>, &Raw<'_>)],
    end: bool,
    state: &ParserState<'_>,
) -> usize {
    let Some((span, raw)) = fragments
        .iter()
        .find(|(span, _)| offset < span.end)
        .or_else(|| fragments.last())
    else {
        return offset;
    };
    let relative = offset.saturating_sub(span.start).min(span.len());
    if let Some(start) = raw_source_start(raw, state) {
        if relative == 0 && !end {
            raw.location.absolute_start
        } else {
            start + relative
        }
    } else if end {
        raw.location.absolute_end
    } else {
        raw.location.absolute_start
    }
}

fn restore_fragment_substitutions<'a>(
    nodes: Vec<InlineNode<'a>>,
    fragments: &[(std::ops::Range<usize>, &Raw<'a>)],
    text: &str,
) -> Vec<InlineNode<'a>> {
    let mut result = Vec::with_capacity(nodes.len());
    for mut node in nodes {
        if let InlineNode::RawText(raw) = &node
            && raw.subs.is_empty()
        {
            let original_start = raw.location.absolute_start;
            // Escaped macro text no longer contains its leading backslash.
            let start = text
                .get(
                    original_start
                        ..step_char(text, raw.location.absolute_end, RoundDirection::Forward),
                )
                .and_then(|source| source.find(raw.content))
                .map_or(original_start, |offset| original_start + offset);
            let end = start + raw.content.len();
            for (span, fragment) in fragments {
                let left = start.max(span.start);
                let right = end.min(span.end);
                if left < right {
                    result.push(InlineNode::RawText(Raw {
                        content: &raw.content[left - start..right - start],
                        location: Location {
                            absolute_start: if left == start { original_start } else { left },
                            absolute_end: step_char(text, right, RoundDirection::Backward),
                            ..raw.location.clone()
                        },
                        subs: fragment.subs.clone(),
                    }));
                }
            }
            continue;
        }
        for_each_inline_children(&mut node, &mut |children| {
            *children = restore_fragment_substitutions(take(children), fragments, text);
        });
        result.push(node);
    }
    result
}

pub(crate) fn convert_plain_to_raw(node: &mut InlineNode<'_>, subs: &[Substitution]) {
    convert_plain_to_raw_with_source(node, subs, None);
}

fn convert_plain_to_raw_with_source(
    node: &mut InlineNode<'_>,
    subs: &[Substitution],
    source: Option<&str>,
) {
    if let InlineNode::PlainText(plain) = node {
        if let Some(source) = source {
            let start = plain.location.absolute_start;
            let end = start + plain.content.len();
            if source.get(start..end) == Some(plain.content) {
                plain.location.absolute_end = step_char(source, end, RoundDirection::Backward);
            }
        }
        *node = InlineNode::RawText(Raw {
            content: plain.content,
            location: plain.location.clone(),
            subs: subs.to_vec(),
        });
        return;
    }
    for_each_inline_children(node, &mut |children| {
        for child in children {
            convert_plain_to_raw_with_source(child, subs, source);
        }
    });
}

fn for_each_inline_children<'a>(
    node: &mut InlineNode<'a>,
    visit: &mut impl FnMut(&mut Vec<InlineNode<'a>>),
) {
    match node {
        InlineNode::BoldText(value) => visit(&mut value.content),
        InlineNode::ItalicText(value) => visit(&mut value.content),
        InlineNode::MonospaceText(value) => visit(&mut value.content),
        InlineNode::HighlightText(value) => visit(&mut value.content),
        InlineNode::SubscriptText(value) => visit(&mut value.content),
        InlineNode::SuperscriptText(value) => visit(&mut value.content),
        InlineNode::CurvedQuotationText(value) => visit(&mut value.content),
        InlineNode::CurvedApostropheText(value) => visit(&mut value.content),
        InlineNode::Macro(macro_node) => match macro_node {
            InlineMacro::Footnote(value) => visit(&mut value.content),
            InlineMacro::Url(value) => visit(&mut value.text),
            InlineMacro::Link(value) => visit(&mut value.text),
            InlineMacro::Mailto(value) => visit(&mut value.text),
            InlineMacro::CrossReference(value) => visit(&mut value.text),
            InlineMacro::IndexTerm(value) => {
                value.for_each_label_mut(&mut *visit);
                if let Some(catalog) = &mut value.catalog {
                    catalog.for_each_label_mut(&mut *visit);
                }
            }
            InlineMacro::Icon(_)
            | InlineMacro::Image(_)
            | InlineMacro::Keyboard(_)
            | InlineMacro::Button(_)
            | InlineMacro::Menu(_)
            | InlineMacro::Autolink(_)
            | InlineMacro::Pass(_)
            | InlineMacro::Stem(_) => {}
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
    if let Ok(nodes) = inline_parser::quotes_only_inlines(content, &mut state) {
        nodes
    } else {
        tracing::warn!(
            input_len = content.len(),
            "quotes-only PEG parse failed, falling back to plain text"
        );
        vec![InlineNode::PlainText(Plain {
            content,
            location: Location::default(),
            escaped: false,
        })]
    }
}

/// Locate a text fragment using byte offsets and the document's line map.
fn plain_text_at<'a>(
    text: &'a str,
    base_location: &Location,
    offset: usize,
    state: &ParserState<'_>,
) -> InlineNode<'a> {
    let start = base_location.absolute_start + offset;
    InlineNode::PlainText(Plain {
        content: text,
        location: state.create_block_location(start, start + text.len(), 0),
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
    processed: &ProcessedContent<'a>,
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
                result.push(plain_text_at(
                    before,
                    base_location,
                    processed_offset,
                    state,
                ));
                processed_offset += before.len();
            }

            // Process the passthrough content using original string positions from passthrough.location
            if let Some(passthrough_content) = &passthrough.text {
                let processed_nodes = process_passthrough(
                    passthrough_content,
                    passthrough,
                    state,
                    processed.attribute_substitutions,
                );
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
            let mut node = plain_text_at(remaining, base_location, processed_offset, state);
            if let InlineNode::PlainText(ref mut p) = node {
                p.location.absolute_end = base_location.absolute_end;
                p.location.end = base_location.end.clone();
            }
            result.push(node);
        }
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
        assert_eq!(nodes, []);
    }
}
