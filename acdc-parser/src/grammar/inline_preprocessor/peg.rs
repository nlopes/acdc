//! Inline preprocessing rules.

use std::{borrow::Cow, mem::take};

use crate::{
    DocumentAttributes, Pass, PassthroughKind, Position, Substitution, Warning, WarningKind,
    grammar::inline_boundaries::check_constrained_opening_boundary,
    model::substitution::parse_substitution,
};

use super::{
    InlinePreprocessorParserState, ProcessedContent, ProcessedKind,
    recognition::{constrained_passthrough_end, constrained_passthrough_start},
};

peg::parser! {
    pub(crate) grammar inline_preprocessing(document_attributes: &DocumentAttributes<'input>, state: &InlinePreprocessorParserState<'input>) for str {

        pub rule run() -> ProcessedContent<'input>
            = content:content() {
                let mut source_map = take(&mut *state.source_map.borrow_mut());
                // Mapping is not read during preprocessing, so sort once after all
                // actions finish.
                source_map
                    .replacements
                    .sort_by_key(|replacement| replacement.absolute_start);
                ProcessedContent {
                    text: Cow::Owned(content),
                    passthroughs: take(&mut *state.passthroughs.borrow_mut()),
                    source_map,
                    attribute_substitutions: crate::model::substitution::SubstitutionPlan::default(),
                }
            }

        pub rule content() -> String
            = parts:inlines()+ { parts.join("") }

        rule inlines() -> String = quiet!{
            inherited_attribute()
            /
            // Keyboard macros can contain plus signs that would otherwise start passthroughs.
            kbd_macro()
            / monospace()
            / escaped_passthrough()
            / passthrough()
            // Recognize counters before attributes: counter names contain a colon.
            / counter_reference()
            / escaped_attribute_reference()
            / attribute_reference()
            / unprocessed_text()
        } / expected!("inlines parser failed")

        // Formatting and labels reparse expanded text. Copy inherited values
        // without expanding their references or extracting their passthroughs again.
        rule inherited_attribute() -> String
        = text:#{|input, pos| {
            let absolute = pos + state.substring_start_offset.get();
            // Inherited ranges can extend past trimmed labels; never match zero bytes at EOF.
            match state.attribute_value_ranges.iter().find(|range| pos < input.len() && range.contains(&absolute)) {
                Some(range) => {
                    let end = (range.end - state.substring_start_offset.get()).min(input.len());
                    peg::RuleResult::Matched(end, &input[pos..end])
                }
                None => peg::RuleResult::Failed,
            }
        }} {
            state.advance(text);
            text.into()
        }

        // Check code boundaries before deferring attributes to the nested parse.
        // Earlier attributes still expand before the final code boundary is checked.
        rule monospace() -> String
            = text:$monospace_pattern() {?
                tracing::debug!(input_len = text.len(), "monospace matched");
                if state.defer_monospace {
                    state.advance(text);
                    return Ok(text.into());
                }

                let width = if text.starts_with("``") { 2 } else { 1 };
                let inner = &text[width..text.len() - width];
                let marker = &text[..width];
                let end = state.get_offset() + text.len();
                state.advance_by(width);
                // Passthroughs use code-local boundaries: `+{name}+` must stay
                // protected while ordinary references expand before quotes.
                let saved_input = state.input.replace(inner);
                let saved_start = state.substring_start_offset.replace(state.get_offset());
                let result = inline_preprocessing::content(inner, document_attributes, state);
                state.input.replace(saved_input);
                state.substring_start_offset.set(saved_start);
                state.current_offset.set(end);
                result.map(|content| format!("{marker}{content}{marker}"))
                    .map_err(|_| "could not preprocess code content")
            }

        rule kbd_macro() -> String
            = text:$("kbd:[" (!"]" [_])* "]") {
                state.advance(text);
                text.into()
            }

        /// Recognize unsupported counter references, remove them, and return a warning.
        rule counter_reference() -> String
            = &counter_reference_pattern() start:position!() start_offset:byte_offset() "{"
              counter_type:$("counter2" / "counter") ":"
              name:$(['a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-']+)
              (":" ['a'..='z' | 'A'..='Z' | '0'..='9']+)?
              "}" end:position!()
            {
                let end_offset = start_offset + end - start;
                let source_location = state.source_location_for(start_offset, end_offset);
                state.add_warning(Warning::new(
                    WarningKind::Other(Cow::Owned(format!(
                        "Counters ({{{counter_type}:{name}}}) are not supported and will be removed from output"
                    ))),
                    Some(source_location),
                ));

                // Removed counters still occupy source bytes in later labels and diagnostics.
                state.source_map.borrow_mut().add_replacement(
                    start_offset, end_offset, 0, ProcessedKind::Attribute,
                );
                state.advance_by(end - start);
                String::new()
            }

        rule escaped_attribute_reference() -> String
            = start:position() source:$(escaped_attribute_reference_pattern()) {
                let location = state.calculate_location(start, source, 0);
                let text = if state.attributes_enabled {
                    let text = source.strip_prefix('\\').unwrap_or(source);
                    if let Some(prefix) = text.strip_suffix("\\}") {
                        state.arena.alloc_str(&format!("{prefix}}}"))
                    } else {
                        text
                    }
                } else {
                    source
                };
                let index = state.pass_found_count.get();
                let placeholder = format!("���{index}���");
                state.passthroughs.borrow_mut().push(Pass {
                    attribute_fragments: Box::default(),
                    text: Some(text),
                    substitutions: vec![Substitution::SpecialChars],
                    location: location.clone(),
                    kind: PassthroughKind::AttributeRef,
                });
                state.source_map.borrow_mut().add_replacement(
                    location.absolute_start, location.absolute_end, placeholder.len(), ProcessedKind::Escape,
                );
                state.pass_found_count.set(index + 1);
                placeholder
            }

        // Escape recognition and the text lookahead must consume the same span.
        // Opening escapes also stay literal when attribute substitutions are disabled.
        rule escaped_attribute_reference_pattern()
            = "\\{" attribute_name_pattern() "}"
            / "\\"? "{" attribute_name_pattern() "\\}"
                {? state.attributes_enabled.then_some(()).ok_or("attribute substitutions disabled") }

        rule attribute_reference() -> String
            = start:position() "{" attribute_name:attribute_name() "}" {
                let location = state.calculate_location(start, attribute_name, 2);
                state.expand_attribute_reference(attribute_name, location, document_attributes)
            }

        rule attribute_name() -> &'input str
            = start:position() attribute_name:$(attribute_name_pattern()) {
                attribute_name
            }

        // Disabled passthroughs are ordinary text: neither extraction nor its
        // lookahead may hide references from the enclosing attribute stage.
        rule check_macros() = {? state.macros_enabled.then_some(()).ok_or("macros disabled") }

        rule escaped_passthrough() -> String
            = check_macros() expanded:("\\" source:$(pass_macro_pattern()) {
                let start = state.get_offset();
                let expanded = state.extract_single_passthroughs(source, start + 1, document_attributes);
                state.current_offset.set(start + source.len() + 1);
                format!("\\{expanded}")
            }
            / "\\" source:$("+++" (!"+++" [_])+ "+++") {
                state.expand_escaped_passthrough(source, document_attributes, true)
            }
            / "\\" source:$("++" (!"++" [_])+ "++") {
                state.expand_escaped_passthrough(source, document_attributes, true)
            }
            / "\\" source:single_plus_pattern(true) {
                state.expand_escaped_passthrough(source, document_attributes, false)
            }) { expanded }

        rule passthrough() -> String = quiet!{
            check_macros() text:(triple_plus_passthrough() / double_plus_passthrough() / single_plus_passthrough() / pass_macro()) { text }
        } / expected!("passthrough parser failed")

        rule single_plus_passthrough() -> String
        = start:position() source:single_plus_pattern(false)
        {
            let content = &source[1..source.len() - 1];
            let location = state.calculate_location(start, content, 2);
            state.passthroughs.borrow_mut().push(Pass {
                attribute_fragments: Box::default(),
                text: Some(content),
                substitutions: vec![Substitution::SpecialChars],
                location: location.clone(),
                kind: PassthroughKind::Single,
            });
            let new_content = format!("\u{FFFD}\u{FFFD}\u{FFFD}{}\u{FFFD}\u{FFFD}\u{FFFD}", state.pass_found_count.get());
            state.source_map.borrow_mut().add_replacement(
                location.absolute_start,
                location.absolute_end,
                new_content.len(),
                ProcessedKind::Passthrough,
            );
            state.pass_found_count.set(state.pass_found_count.get() + 1);
            new_content
        }

        rule double_plus_passthrough() -> String
            = start:position() "++" content:$((!"++" [_])+) "++" {
                let location = state.calculate_location(start, content, 4);
                state.passthroughs.borrow_mut().push(Pass {
                attribute_fragments: Box::default(),
                    text: Some(content),
                    // Converters apply special-character substitution to single and double passthroughs.
                    substitutions: vec![Substitution::SpecialChars].into_iter().collect(),
                    location: location.clone(),
                    kind: PassthroughKind::Double,
                });
                let new_content = format!("\u{FFFD}\u{FFFD}\u{FFFD}{}\u{FFFD}\u{FFFD}\u{FFFD}", state.pass_found_count.get());
                let original_span = location.absolute_end - location.absolute_start;
                state.source_map.borrow_mut().add_replacement(
                    location.absolute_start,
                    location.absolute_end,
                    new_content.len(),
                    ProcessedKind::Passthrough,
                );
                state.pass_found_count.set(state.pass_found_count.get() + 1);
                new_content
            }

        rule triple_plus_passthrough() -> String
            = start:position() "+++" content:$((!"+++" [_])+) "+++" {
                let location = state.calculate_location(start, content, 6);
                state.passthroughs.borrow_mut().push(Pass {
                attribute_fragments: Box::default(),
                    text: Some(content),
                    substitutions: Vec::new(),
                    location: location.clone(),
                    kind: PassthroughKind::Triple,
                });
                let new_content = format!("\u{FFFD}\u{FFFD}\u{FFFD}{}\u{FFFD}\u{FFFD}\u{FFFD}", state.pass_found_count.get());
                let original_span = location.absolute_end - location.absolute_start;
                state.source_map.borrow_mut().add_replacement(
                    location.absolute_start,
                    location.absolute_end,
                    new_content.len(),
                    ProcessedKind::Passthrough,
                );
                state.pass_found_count.set(state.pass_found_count.get() + 1);
                new_content
            }

        rule pass_macro() -> String
        = start:position() full:$(pass_macro_pattern()) {
            let (subs_str, content, substitutions) =
                InlinePreprocessorParserState::parse_pass_macro_parts(full);

            // For pass macro: "pass:" (5) + substitutions + "[" (1) + "]" (1)
            let padding = 5 + subs_str.len() + 1 + 1; // "pass:" + subs + "[" + "]"
            let location = state.calculate_location(start, content, padding);
                state.passthroughs.borrow_mut().push(Pass {
                attribute_fragments: Box::default(),
                    text: Some(content),
                    substitutions: substitutions.clone(),
                    location: location.clone(),
                    kind: PassthroughKind::Macro,
                });
                let new_content = format!("\u{FFFD}\u{FFFD}\u{FFFD}{}\u{FFFD}\u{FFFD}\u{FFFD}", state.pass_found_count.get());
                state.source_map.borrow_mut().add_replacement(
                    location.absolute_start,
                    location.absolute_end,
                    new_content.len(),
                    ProcessedKind::Passthrough,
                );
                state.pass_found_count.set(state.pass_found_count.get() + 1);
                new_content
            }

        rule substitutions() -> Vec<Substitution>
            = subs:$(substitution_value() ** ",") {
                if subs.is_empty() {
                    Vec::new()
                } else {
                    subs.split(',')
                        .filter_map(|s| parse_substitution(s.trim()))
                        .collect()
                }
            }

        // An escaped closing bracket belongs to the content, even after another
        // backslash. Share this boundary with extraction and its lookaheads.
        rule pass_macro_pattern()
            = "pass:" substitutions() "[" ("\\]" / [^']'])* "]"

        rule substitution_value() -> &'input str
            = $(['a'..='z' | 'A'..='Z' | '0'..='9']+)

        rule unprocessed_text() -> String
            = text:$((
                [^'{' | '+' | '`' | 'k' | 'p' | '\\']+
                /
                !(escaped_passthrough_pattern() / escaped_attribute_reference_pattern() / passthrough_pattern(false) / counter_reference_pattern() / attribute_reference_pattern() / kbd_macro_pattern() / monospace_pattern()) [_]
            )+) {
                state.advance(text);
                text.to_string()
            }

        /// Pattern for counter references: {counter:name} or {counter:name:initial} or {counter2:...}
        rule counter_reference_pattern()
            = "{" ("counter2" / "counter") ":" ['a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-']+ (":" ['a'..='z' | 'A'..='Z' | '0'..='9']+)? "}"
              {? state.attributes_enabled.then_some(()).ok_or("attribute substitutions disabled") }

        rule attribute_reference_pattern() = "{" attribute_name_pattern() "}"

        rule attribute_name_pattern() = ['a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_']+

        rule kbd_macro_pattern() = "kbd:[" (!"]" [_])* "]"

        rule monospace_pattern()
            = ({? (!state.defer_monospace).then_some(()).ok_or("attributes precede quotes") })
              ("``" (!"``" [_])+ "``" / "`" [^('`' | ' ' | '\t' | '\n')] [^'`']* "`")
            / (start:position!()
              {? (!state.input.borrow()[..start].trim_end_matches('`').ends_with('\\'))
                  .then_some(()).ok_or("escaped code delimiter") })
              ("``" (!"``" [_])+ "``"
              / (start:position!() {? check_constrained_opening_boundary(start, state.input.borrow().as_bytes(), None, b'`')
                  .then_some(()).ok_or("code opening boundary") })
                "`" ![' ' | '\t'..='\r'] [_]
                (!monospace_close() [_])* monospace_close())

        // Skip invalid closing candidates so passthroughs stay inside the code
        // scope selected by the inline parser. Lookahead uses this same rule.
        rule monospace_close()
            = close:position!() "`" after:position!() {?
                let input = state.input.borrow();
                let trailing_space = input.as_bytes().get(close.saturating_sub(1))
                    .is_some_and(|byte| matches!(byte, b' ' | b'\t'..=b'\r'));
                let valid_follow = input[after..].chars().next()
                    .is_none_or(|ch| !ch.is_alphanumeric() && !matches!(ch, '_' | '`' | '\'' | '"'));
                (!trailing_space && valid_follow).then_some(()).ok_or("code closing boundary")
            }

        // Extraction and lookahead must reject the same candidates without advancing
        // source offsets, so invalid spans leave references and later pluses visible.
        rule single_plus_pattern(escaped: bool) -> &'input str
        = source:#{|input, position| {
            let previous = input[..position].chars().next_back();
            // An explicit escape bypasses the opening context, even after a word;
            // content edges and the closing boundary still determine its extent.
            if input.as_bytes().get(position) != Some(&b'+')
                || (!escaped && !constrained_passthrough_start(previous))
            {
                return peg::RuleResult::Failed;
            }
            match constrained_passthrough_end(input, position) {
                Some(end) => peg::RuleResult::Matched(end + 1, &input[position..=end]),
                None => peg::RuleResult::Failed,
            }
        }} { source }

        rule passthrough_pattern(escaped: bool) = check_macros() (
        "+++" (!("+++") [_])+ "+++" /
        "++" (!("++") [_])+ "++" /
        single_plus_pattern(escaped) /
        pass_macro_pattern())

        rule escaped_passthrough_pattern() = "\\" passthrough_pattern(true)

        rule ANY() = [_]

        rule position() -> Position = { state.get_position() }

        rule byte_offset() -> usize = { state.get_offset() }
    }
}
