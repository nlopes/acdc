use std::{
    borrow::Cow,
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
};

use peg::parser;

use crate::{
    AttributeValue, DocumentAttributes, Error, Location, Pass, PassthroughKind, Position,
    Positioning, SourceLocation, Substitution, Warning, WarningKind, grammar::LineMap,
    model::substitution::parse_substitution,
};

/// Parser state for the inline preprocessor.
///
/// Uses `Cell` for simple values and `RefCell` for collections to support
/// interior mutability within PEG action blocks.
///
/// Position tracking uses `LineMap` (immutable, O(log n) lookups) instead of
/// incremental `PositionTracker` - we only maintain the byte offset and compute
/// line/column on demand.
#[derive(Debug)]
pub(crate) struct InlinePreprocessorParserState<'a> {
    pub(crate) pass_found_count: Cell<usize>,
    pub(crate) passthroughs: RefCell<Vec<Pass<'a>>>,
    pub(crate) attributes: RefCell<HashMap<usize, Location>>,
    /// Current byte offset in the full document input.
    pub(crate) current_offset: Cell<usize>,
    /// Pre-computed line map for O(log n) offset→position lookups.
    pub(crate) line_map: Rc<LineMap>,
    /// Full document input (for `LineMap` position lookups).
    pub(crate) full_input: &'a str,
    /// Arena for interning synthesised passthrough strings produced during
    /// preprocessing (character-replacement expansions, escape-stripped text).
    pub(crate) arena: &'a bumpalo::Bump,
    pub(crate) source_map: RefCell<SourceMap>,
    /// The substring currently being parsed.
    pub(crate) input: RefCell<&'a str>,
    pub(crate) substring_start_offset: Cell<usize>,
    /// Warnings collected during PEG parsing for post-parse emission.
    /// Uses `RefCell` for interior mutability in PEG action blocks.
    pub(crate) warnings: RefCell<Vec<Warning>>,
    /// Whether macro substitutions are enabled for this block.
    /// When `false`, `pass:[]` macros are not extracted by the preprocessor.
    pub(crate) macros_enabled: bool,
    /// Whether attribute substitutions are enabled for this block.
    /// When `false`, `{attribute}` references are not expanded by the preprocessor.
    pub(crate) attributes_enabled: bool,
}

impl<'a> InlinePreprocessorParserState<'a> {
    /// Create a new inline preprocessor state.
    ///
    /// # Arguments
    /// * `input` - The substring to parse
    /// * `line_map` - Pre-computed line map for the full document
    /// * `full_input` - The full document input (for position lookups)
    /// * `arena` - Arena for interning synthesised strings
    /// * `macros_enabled` - Whether macro substitutions are active
    /// * `attributes_enabled` - Whether attribute substitutions are active
    pub(crate) fn new(
        input: &'a str,
        line_map: Rc<LineMap>,
        full_input: &'a str,
        arena: &'a bumpalo::Bump,
        macros_enabled: bool,
        attributes_enabled: bool,
    ) -> Self {
        Self {
            pass_found_count: Cell::new(0),
            passthroughs: RefCell::new(Vec::new()),
            attributes: RefCell::new(HashMap::new()),
            current_offset: Cell::new(0),
            line_map,
            full_input,
            arena,
            source_map: RefCell::new(SourceMap::default()),
            input: RefCell::new(input),
            substring_start_offset: Cell::new(0),
            warnings: RefCell::new(Vec::new()),
            macros_enabled,
            attributes_enabled,
        }
    }

    /// Create a new state with all substitutions enabled (macros + attributes).
    pub(crate) fn new_all_enabled(
        input: &'a str,
        line_map: Rc<LineMap>,
        full_input: &'a str,
        arena: &'a bumpalo::Bump,
    ) -> Self {
        Self::new(input, line_map, full_input, arena, true, true)
    }

    /// Set the initial position for parsing a substring within the document.
    pub(crate) fn set_initial_position(&mut self, _location: &Location, absolute_offset: usize) {
        self.substring_start_offset.set(absolute_offset);
        self.current_offset.set(absolute_offset);
    }

    /// Get current position using `LineMap` lookup.
    fn get_position(&self) -> Position {
        self.line_map
            .offset_to_position(self.current_offset.get(), self.full_input)
    }

    /// Get current byte offset.
    fn get_offset(&self) -> usize {
        self.current_offset.get()
    }

    /// Advance offset by string length (bytes).
    fn advance(&self, s: &str) {
        self.current_offset.set(self.current_offset.get() + s.len());
    }

    /// Advance offset by a fixed byte count.
    fn advance_by(&self, n: usize) {
        self.current_offset.set(self.current_offset.get() + n);
    }

    /// Collect a warning for post-parse emission. Deduplicates by value.
    pub(crate) fn add_warning(&self, warning: Warning) {
        let mut warnings = self.warnings.borrow_mut();
        if !warnings.contains(&warning) {
            warnings.push(warning);
        }
    }

    /// Build a `SourceLocation` pointing at `[start_offset, end_offset)`
    /// within the full document input. Uses the shared `LineMap` for
    /// line/column resolution.
    pub(crate) fn source_location_for(
        &self,
        start_offset: usize,
        end_offset: usize,
    ) -> SourceLocation {
        let start = self
            .line_map
            .offset_to_position(start_offset, self.full_input);
        let end = self
            .line_map
            .offset_to_position(end_offset, self.full_input);
        SourceLocation {
            file: None,
            positioning: Positioning::Location(Location {
                absolute_start: start_offset,
                absolute_end: end_offset,
                start,
                end,
            }),
        }
    }

    /// Drain collected warnings (for transfer to main `ParserState`).
    pub(crate) fn drain_warnings(&self) -> Vec<Warning> {
        self.warnings.borrow_mut().drain(..).collect()
    }

    /// Extract the subs-spec string, content, and parsed substitutions from
    /// a matched `pass:SUBS[CONTENT]` string.
    fn parse_pass_macro_parts(full: &str) -> (&str, &str, Vec<Substitution>) {
        let subs_end = full[5..].find('[').unwrap_or(0);
        let subs_str = &full[5..5 + subs_end];
        let content = &full[5 + subs_end + 1..full.len() - 1];
        let substitutions = if subs_str.is_empty() {
            Vec::new()
        } else {
            subs_str
                .split(',')
                .filter_map(|s| parse_substitution(s.trim()))
                .collect()
        };
        (subs_str, content, substitutions)
    }

    /// When macros are disabled, a `pass:SUBS[CONTENT]` macro is treated as literal text.
    /// However, if its sub-spec includes attributes (`a` or `n`), we still expand
    /// attribute references in the content — matching asciidoctor behavior.
    fn expand_disabled_pass_macro(
        &self,
        full: &'a str,
        document_attributes: &DocumentAttributes<'a>,
    ) -> String {
        let (subs_str, content, substitutions) = Self::parse_pass_macro_parts(full);

        let has_attr_subs = substitutions
            .iter()
            .any(|s| matches!(s, Substitution::Attributes | Substitution::Normal));

        if !has_attr_subs {
            self.advance(full);
            return full.into();
        }

        let expanded = inline_preprocessing::attribute_reference_substitutions(
            content,
            document_attributes,
            self,
        )
        .unwrap_or_else(|_| content.into());
        let reconstructed = format!("pass:{subs_str}[{expanded}]");

        let absolute_start = self.get_offset();
        self.advance(full);
        if reconstructed.chars().count() != full.chars().count() {
            self.source_map.borrow_mut().add_replacement(
                absolute_start,
                absolute_start + full.len(),
                reconstructed.chars().count(),
                ProcessedKind::Attribute,
            );
        }
        reconstructed
    }

    /// Calculate location for a matched construct.
    ///
    /// Advances the offset by `content.len() + padding` and returns a Location
    /// spanning from the current position to the new position.
    fn calculate_location(&self, start: Position, content: &str, padding: usize) -> Location {
        let absolute_start = self.get_offset();
        self.advance(content);
        self.advance_by(padding);
        Location {
            absolute_start,
            absolute_end: self.get_offset(),
            start,
            end: self.get_position(),
        }
    }
}

#[derive(Debug)]
pub(crate) struct ProcessedContent<'a> {
    pub text: Cow<'a, str>,
    pub passthroughs: Vec<Pass<'a>>,
    pub(crate) source_map: SourceMap,
}

#[derive(Debug, Clone)]
pub(crate) struct Replacement {
    pub absolute_start: usize,
    pub absolute_end: usize,
    pub processed_end: usize, // absolute_start + physical placeholder length
    pub kind: ProcessedKind,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct SourceMap {
    pub replacements: Vec<Replacement>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProcessedKind {
    Attribute,
    Passthrough,
}

/// Convert usize to i32, logging on overflow.
fn to_signed(value: usize, context: &str) -> Result<i32, Error> {
    i32::try_from(value).map_err(|e| {
        tracing::error!(value, context, error = %e, "position overflow");
        e.into()
    })
}

/// Convert i32 back to usize, logging on underflow.
fn to_unsigned(value: i32, context: &str) -> Result<usize, Error> {
    usize::try_from(value).map_err(|e| {
        tracing::error!(value, context, error = %e, "negative position");
        e.into()
    })
}

impl SourceMap {
    /// Record a substitution.
    /// - `absolute_start`: where in the processed text the placeholder was inserted.
    /// - `absolute_end`: where in the original text the placeholder ends
    /// - `processed_end`: where in the processed text the placeholder ends
    /// - `physical_length`: the length of the inserted placeholder (in char count, not bytes)
    pub(crate) fn add_replacement(
        &mut self,
        absolute_start: usize,
        absolute_end: usize,
        physical_length: usize,
        kind: ProcessedKind,
    ) {
        self.replacements.push(Replacement {
            absolute_start,
            absolute_end,
            processed_end: absolute_start + physical_length,
            kind,
        });
        // Ensure replacements are sorted by where they occur in the processed text.
        self.replacements.sort_by_key(|r| r.absolute_start);
    }

    /// Map a position in the processed text back to the original source.
    pub(crate) fn map_position(&self, pos: usize) -> Result<usize, Error> {
        let signed_pos = to_signed(pos, "pos")?;

        // The adjustment is the total number of characters removed/added during preprocessing.
        // For example, if we have a passthrough like `+a+`: the original text is 3 characters,
        // but the processed text is 7 characters (FFF0FFF). So the adjustment is 7 - 3 = 4.
        let mut adjustment: i32 = 0;

        for rep in &self.replacements {
            let rep_start = to_signed(rep.absolute_start, "rep.absolute_start")?;
            let rep_end = to_signed(rep.absolute_end, "rep.absolute_end")?;
            let rep_processed_end = to_signed(rep.processed_end, "rep.processed_end")?;

            // Position is before this replacement - done adjusting
            if signed_pos <= rep_start {
                break;
            }

            // Position is within this replacement
            if signed_pos < rep_processed_end {
                return match rep.kind {
                    ProcessedKind::Attribute => {
                        // All inserted characters map to the left-most original position
                        Ok(rep.absolute_start)
                    }
                    ProcessedKind::Passthrough if signed_pos >= rep_end => {
                        // Position is past the original passthrough end
                        Ok(rep.absolute_end - 1)
                    }
                    ProcessedKind::Passthrough => {
                        // Within passthrough - apply current adjustment
                        to_unsigned(signed_pos - adjustment, "within_passthrough")
                    }
                };
            }

            // Position is past this replacement - accumulate adjustment
            adjustment += rep_processed_end - rep_end;
        }

        // Not within any replacement - apply total adjustment
        to_unsigned(signed_pos - adjustment, "final_position")
    }
}

parser!(
    pub(crate) grammar inline_preprocessing(document_attributes: &DocumentAttributes<'input>, state: &InlinePreprocessorParserState<'input>) for str {

        pub rule run() -> ProcessedContent<'input>
            = content:inlines()+ {
                ProcessedContent {
                    text: Cow::Owned(content.join("")),
                    passthroughs: state.passthroughs.borrow().clone(),
                    source_map: state.source_map.borrow().clone(),
                }
            }

        rule inlines() -> String = quiet!{
            // We add kbd_macro here to avoid conflicts with passthroughs as kbd macros
            // also can have + signs on each side.
            // We add monospace before passthrough to skip content inside backticks
            kbd_macro()
            / monospace()
            / passthrough()
            // counter_reference must come BEFORE attribute_reference because counters
            // have a colon in the name (e.g., {counter:num}) which is not valid in
            // standard attribute names
            / counter_reference()
            / attribute_reference()
            / unprocessed_text()
        } / expected!("inlines parser failed")

        // Match and skip monospace content (content inside backticks)
        // This prevents the preprocessor from processing passthroughs inside monospace
        rule monospace() -> String
            // Unconstrained (double backticks) or constrained (single backticks)
            = text:$("``" (!"``" [_])+ "``" / "`" [^('`' | ' ' | '\t' | '\n')] [^'`']* "`") {
                tracing::debug!(text, "monospace matched");
                state.advance(text);
                text.into()
            }

        rule kbd_macro() -> String
            = text:$("kbd:[" (!"]" [_])* "]") {
                state.advance(text);
                text.into()
            }

        /// Counter reference: `{counter:name}`, `{counter:name:initial}`, `{counter2:name}`
        ///
        /// Counters are not supported. Per asciidoctor maintainer feedback, counters are
        /// "a disaster" that they want to redesign or remove. We detect them, emit a warning,
        /// and return empty string (the counter syntax is silently removed from output).
        rule counter_reference() -> String
            = start_offset:byte_offset() "{"
              counter_type:$("counter2" / "counter") ":"
              name:$(['a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-']+)
              (":" ['a'..='z' | 'A'..='Z' | '0'..='9']+)?
              "}" end_offset:byte_offset()
            {
                let source_location = state.source_location_for(start_offset, end_offset);
                state.add_warning(Warning::new(
                    WarningKind::Other(Cow::Owned(format!(
                        "Counters ({{{counter_type}:{name}}}) are not supported and will be removed from output"
                    ))),
                    Some(source_location),
                ));

                // Return empty string - counter is removed from output
                String::new()
            }

        rule attribute_reference() -> String
            = start:position() "{" attribute_name:attribute_name() "}" {
                if !state.attributes_enabled {
                    let text = format!("{{{attribute_name}}}");
                    state.advance(&text);
                    return text;
                }

                let location = state.calculate_location(start, attribute_name, 2);

                // Special handling for character replacement attributes that need passthrough behavior.
                // In asciidoctor, expanded attribute values don't get re-parsed for inline syntax.
                // In acdc, the inline preprocessor expands attributes into a string that the main
                // PEG grammar fully re-parses. This means ALL character replacement attributes with
                // syntactically significant ASCII values need passthrough protection to prevent the
                // expanded values from being misinterpreted as AsciiDoc syntax (e.g., {plus} → "+"
                // being matched as a line break, {asterisk} → "*" triggering bold formatting).
                // See: https://docs.asciidoctor.org/asciidoc/latest/attributes/character-replacement-ref/
                let is_char_ref = matches!(
                    attribute_name,
                    "lt" | "gt" | "amp"
                        | "plus" | "pp" | "cpp" | "cxx"
                        | "asterisk" | "backtick" | "caret" | "tilde"
                        | "vbar" | "startsb" | "endsb" | "backslash"
                        | "two-colons" | "two-semicolons"
                        | "apos" | "quot"
                );

                match document_attributes.get(attribute_name) {
                    Some(AttributeValue::String(value)) => {
                        if is_char_ref {
                            // Treat character replacement attributes as passthroughs (like +++<+++)
                            // Empty substitutions = RawText = bypasses further inline parsing
                            state.passthroughs.borrow_mut().push(Pass {
                                text: Some(state.arena.alloc_str(value)),
                                substitutions: Vec::new(),
                                location: location.clone(),
                                kind: PassthroughKind::AttributeRef,
                            });
                            let new_content = format!("\u{FFFD}\u{FFFD}\u{FFFD}{}\u{FFFD}\u{FFFD}\u{FFFD}", state.pass_found_count.get());
                            state.source_map.borrow_mut().add_replacement(
                                location.absolute_start,
                                location.absolute_end,
                                new_content.chars().count(),
                                ProcessedKind::Passthrough,
                            );
                            state.pass_found_count.set(state.pass_found_count.get() + 1);
                            new_content
                        } else {
                            // Normal attribute substitution
                            let mut attributes = state.attributes.borrow_mut();
                            state.source_map.borrow_mut().add_replacement(
                                location.absolute_start,
                                location.absolute_end,
                                value.chars().count(),
                                ProcessedKind::Attribute,
                            );
                            attributes.insert(state.source_map.borrow().replacements.len(), location);
                            value.to_string()
                        }
                    },
                    Some(AttributeValue::Bool(true)) => {
                        let mut attributes = state.attributes.borrow_mut();
                        state.source_map.borrow_mut().add_replacement(
                            location.absolute_start,
                            location.absolute_end,
                            0,
                            ProcessedKind::Attribute,
                        );
                        attributes.insert(state.source_map.borrow().replacements.len(), location);
                        String::new()
                    },
                    _ => {
                        // For non-string attributes, keep original text
                        format!("{{{attribute_name}}}")
                    }
                }
            }

        rule attribute_name() -> &'input str
            = start:position() attribute_name:$(attribute_name_pattern()) {
                attribute_name
            }

        rule passthrough() -> String = quiet!{
            triple_plus_passthrough() / double_plus_passthrough() / single_plus_passthrough() / pass_macro()
        } / expected!("passthrough parser failed")

        rule single_plus_passthrough() -> String
        = start:position() start_offset:byte_offset()
        "+"
        // Content: must not start with whitespace, can contain + if not followed by boundary
        content:$(![(' '|'\t'|'\n'|'\r')] (!("+" &([' '|'\t'|'\n'|'\r'|','|';'|'"'|'.'|'?'|'!'|':'|')'|']'|'}'|'/'|'-'|'<'|'>'] / ![_])) [_])*)
        "+"
        {
            if !state.macros_enabled {
                let text = format!("+{content}+");
                state.advance(&text);
                return text;
            }

            // Check if we're at start OR preceded by word boundary character
            // Convert absolute offset to relative offset within the substring
            let substring_start = state.substring_start_offset.get();
            let relative_offset = start_offset - substring_start;

            let input_bytes = state.input.borrow();
            let prev_byte_value = if relative_offset > 0 {
                input_bytes.as_bytes().get(relative_offset - 1).copied()
            } else {
                None
            };

            let valid_boundary = relative_offset == 0 || {
                if let Some(b) = prev_byte_value {
                    matches!(
                        b,
                        b' ' | b'\t' | b'\n' | b'\r' | b'(' | b'{' | b'[' | b')' | b'}' | b']'
                            | b'/' | b'-' | b'|' | b',' | b';' | b'.' | b'?' | b'!' | b'\''
                            | b'"' | b'<' | b'>'
                    )
                } else {
                    false
                }
            };

            // Also check trailing boundary - must be followed by whitespace, punctuation, or EOF
            // Calculate position after closing + based on: relative_offset + '+' + content + '+'
            let trailing_valid = {
                let input_bytes = state.input.borrow();
                // Position after: start '+' (1) + content (len) + end '+' (1) = relative_offset + 1 + content.len() + 1
                let after_plus_relative = relative_offset + 1 + content.len() + 1;
                if after_plus_relative >= input_bytes.len() {
                    // At EOF - valid trailing boundary
                    true
                } else if let Some(next_byte) = input_bytes.as_bytes().get(after_plus_relative) {
                    matches!(
                        *next_byte,
                        b' ' | b'\t' | b'\n' | b'\r' | b',' | b';' | b'"' | b'.' | b'?' | b'!'
                            | b':' | b')' | b']' | b'}' | b'/' | b'-' | b'<' | b'>'
                    )
                } else {
                    false
                }
            };

            // Calculate location to advance offset (even for invalid boundaries)
            let location = state.calculate_location(start, content, 2);

            if !valid_boundary || !trailing_valid {
                // Not a valid constrained passthrough - return literal text without creating passthrough
                return format!("+{content}+");
            }
            state.passthroughs.borrow_mut().push(Pass {
                text: Some(content),
                // We add SpecialChars here for single and double but we don't do
                // anything with them, only the converter does.
                substitutions: vec![Substitution::SpecialChars].into_iter().collect(),
                location: location.clone(),
                kind: PassthroughKind::Single,
            });
            let new_content = format!("\u{FFFD}\u{FFFD}\u{FFFD}{}\u{FFFD}\u{FFFD}\u{FFFD}", state.pass_found_count.get());
            let original_span = location.absolute_end - location.absolute_start;
            state.source_map.borrow_mut().add_replacement(
                location.absolute_start,
                location.absolute_end,
                new_content.chars().count(),
                ProcessedKind::Passthrough,
            );
            state.pass_found_count.set(state.pass_found_count.get() + 1);
            new_content
        }

        rule double_plus_passthrough() -> String
            = start:position() "++" content:$((!"++" [_])+) "++" {
                if !state.macros_enabled {
                    state.advance(&format!("++{content}++"));
                    return format!("++{content}++");
                }
                let location = state.calculate_location(start, content, 4);
                state.passthroughs.borrow_mut().push(Pass {
                    text: Some(content),
                    // We add SpecialChars here for single and double but we don't do
                    // anything with them, only the converter does.
                    substitutions: vec![Substitution::SpecialChars].into_iter().collect(),
                    location: location.clone(),
                    kind: PassthroughKind::Double,
                });
                let new_content = format!("\u{FFFD}\u{FFFD}\u{FFFD}{}\u{FFFD}\u{FFFD}\u{FFFD}", state.pass_found_count.get());
                let original_span = location.absolute_end - location.absolute_start;
                state.source_map.borrow_mut().add_replacement(
                    location.absolute_start,
                    location.absolute_end,
                    new_content.chars().count(),
                    ProcessedKind::Passthrough,
                );
                state.pass_found_count.set(state.pass_found_count.get() + 1);
                new_content
            }

        rule triple_plus_passthrough() -> String
            = start:position() "+++" content:$((!"+++" [_])+) "+++" {
                if !state.macros_enabled {
                    let text = format!("+++{content}+++");
                    state.advance(&text);
                    return text;
                }
                let location = state.calculate_location(start, content, 6);
                state.passthroughs.borrow_mut().push(Pass {
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
                    new_content.chars().count(),
                    ProcessedKind::Passthrough,
                );
                state.pass_found_count.set(state.pass_found_count.get() + 1);
                new_content
            }

        rule pass_macro() -> String
        = start:position() full:$("pass:" substitutions() "[" [^']']* "]") {
            if !state.macros_enabled {
                return state.expand_disabled_pass_macro(full, document_attributes);
            }

            let (subs_str, content, substitutions) =
                InlinePreprocessorParserState::parse_pass_macro_parts(full);

            // For pass macro: "pass:" (5) + substitutions + "[" (1) + "]" (1)
            let padding = 5 + subs_str.len() + 1 + 1; // "pass:" + subs + "[" + "]"
            let location = state.calculate_location(start, content, padding);
            // Normal substitution group includes Attributes, so check for both
            let content = if substitutions.contains(&Substitution::Attributes)
                || substitutions.contains(&Substitution::Normal)
            {
                inline_preprocessing::attribute_reference_substitutions(content, document_attributes, state).unwrap_or_else(|_| content.into())
            } else {
                content.into()
            };
                state.passthroughs.borrow_mut().push(Pass {
                    text: Some(state.arena.alloc_str(&content)),
                    substitutions: substitutions.clone(),
                    location: location.clone(),
                    kind: PassthroughKind::Macro,
                });
                let new_content = format!("\u{FFFD}\u{FFFD}\u{FFFD}{}\u{FFFD}\u{FFFD}\u{FFFD}", state.pass_found_count.get());
                state.source_map.borrow_mut().add_replacement(
                    location.absolute_start,
                    location.absolute_end,
                    new_content.chars().count(),
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

        rule substitution_value() -> &'input str
            = $(['a'..='z' | 'A'..='Z' | '0'..='9']+)

        rule unprocessed_text() -> String
            = text:$((
                [^'{' | '+' | '`' | 'k' | 'p']+
                /
                !(passthrough_pattern() / counter_reference_pattern() / attribute_reference_pattern() / kbd_macro_pattern() / monospace_pattern()) [_]
            )+) {
                state.advance(text);
                text.to_string()
            }

        /// Pattern for counter references: {counter:name} or {counter:name:initial} or {counter2:...}
        rule counter_reference_pattern() = "{" ("counter2" / "counter") ":" ['a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-']+ (":" ['a'..='z' | 'A'..='Z' | '0'..='9']+)? "}"

        rule attribute_reference_pattern() = "{" attribute_name_pattern() "}"

        rule attribute_name_pattern() = ['a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_']+

        rule kbd_macro_pattern() = "kbd:[" (!"]" [_])* "]"

        rule monospace_pattern() =
            "``" (!"``" [_])+ "``" /
            "`" [^('`' | ' ' | '\t' | '\n')] [^'`']* "`"

        // Simple pattern for unprocessed_text negative lookahead
        // Doesn't check boundaries - that's done in the full rules
        // For single +, allows + in content if not followed by boundary (greedy matching)
        rule passthrough_pattern() =
        "+++" (!("+++") [_])+ "+++" /
        "++" (!("++") [_])+ "++" /
        "+" ![' '|'\t'|'\n'|'\r'] (!("+" &([' '|'\t'|'\n'|'\r'|','|';'|'"'|'.'|'?'|'!'|':'|')'|']'|'}'|'/'|'-'|'<'|'>'] / ![_])) [_])* "+" /
        "pass:" substitutions()? "[" [^']']* "]"

        pub rule attribute_reference_substitutions() -> String
            = content:(attribute_reference_content() / unprocessed_text_content())+ {
                content.join("")
            }

        rule attribute_reference_content() -> String
            = "{" attribute_name:attribute_name() "}" {
                match document_attributes.get(attribute_name) {
                    Some(AttributeValue::String(value)) => value.to_string(),
                        // TODO(nlopes): do we need to handle other types?
                        // For non-string attributes, keep original text
                    _ => format!("{{{attribute_name}}}"),
                }
            }

        rule unprocessed_text_content() -> String
            = text:$((
                [^'{' | '+' | '`' | 'p']+
                /
                !(passthrough_pattern() / attribute_reference_pattern()) [_]
            )+) {
                text.to_string()
            }

        rule ANY() = [_]

        rule position() -> Position = { state.get_position() }

        rule byte_offset() -> usize = { state.get_offset() }
    }
);

#[cfg(test)]
#[allow(
    clippy::panic,
    clippy::indexing_slicing,
    clippy::expect_used,
    clippy::unwrap_used
)]
mod tests {
    use super::*;
    use crate::DocumentAttributes;

    fn setup_attributes() -> DocumentAttributes<'static> {
        let mut attributes = DocumentAttributes::default();
        attributes.insert("s".into(), AttributeValue::String("link:/nonono".into()));
        attributes.insert("version".into(), AttributeValue::String("1.0".into()));
        attributes.insert("title".into(), AttributeValue::String("My Title".into()));
        attributes
    }

    fn setup_state(content: &str) -> InlinePreprocessorParserState<'_> {
        // Leak a per-call arena so test states have the required lifetime.
        let arena: &'static bumpalo::Bump = Box::leak(Box::new(bumpalo::Bump::new()));
        InlinePreprocessorParserState {
            pass_found_count: Cell::new(0),
            passthroughs: RefCell::new(Vec::new()),
            attributes: RefCell::new(HashMap::new()),
            current_offset: Cell::new(0),
            line_map: Rc::new(LineMap::new(content)),
            full_input: content,
            arena,
            source_map: RefCell::new(SourceMap::default()),
            input: RefCell::new(content),
            substring_start_offset: Cell::new(0),
            warnings: RefCell::new(Vec::new()),
            macros_enabled: true,
            attributes_enabled: true,
        }
    }

    #[test]
    fn test_preprocess_inline_passthrough_single() -> Result<(), Error> {
        let attributes = setup_attributes();
        let input = "+hello+";
        let state = setup_state(input);
        let result = inline_preprocessing::run(input, &attributes, &state)?;
        assert_eq!(
            result.text,
            "\u{FFFD}\u{FFFD}\u{FFFD}0\u{FFFD}\u{FFFD}\u{FFFD}"
        );
        assert_eq!(state.pass_found_count.get(), 1);
        let passthroughs = state.passthroughs.into_inner();
        assert_eq!(passthroughs.len(), 1);
        let Some(first) = passthroughs.first() else {
            panic!("expected first passthrough");
        };
        assert_eq!(first.text, Some("hello"));
        assert_eq!(first.kind, PassthroughKind::Single);
        Ok(())
    }

    #[test]
    fn test_preprocess_inline_passthrough_double() -> Result<(), Error> {
        let attributes = setup_attributes();
        let input = "++hello++";
        let state = setup_state(input);
        let result = inline_preprocessing::run(input, &attributes, &state)?;
        assert_eq!(
            result.text,
            "\u{FFFD}\u{FFFD}\u{FFFD}0\u{FFFD}\u{FFFD}\u{FFFD}"
        );
        assert_eq!(result.passthroughs.len(), 1);
        let Some(first) = result.passthroughs.first() else {
            panic!("expected first passthrough");
        };
        assert_eq!(first.text, Some("hello"));
        assert_eq!(first.kind, PassthroughKind::Double);
        Ok(())
    }

    #[test]
    fn test_preprocess_inline_passthrough_triple() -> Result<(), Error> {
        let attributes = setup_attributes();
        let input = "+++hello+++";
        let state = setup_state(input);
        let result = inline_preprocessing::run(input, &attributes, &state)?;
        assert_eq!(
            result.text,
            "\u{FFFD}\u{FFFD}\u{FFFD}0\u{FFFD}\u{FFFD}\u{FFFD}"
        );
        assert_eq!(result.passthroughs.len(), 1);
        let Some(first) = result.passthroughs.first() else {
            panic!("expected first passthrough");
        };
        assert_eq!(first.text, Some("hello"));
        assert_eq!(first.kind, PassthroughKind::Triple);
        Ok(())
    }

    #[test]
    fn test_preprocess_inline_passthrough_single_plus() -> Result<(), Error> {
        let attributes = setup_attributes();
        let input = "+hello+ world+";
        let state = setup_state(input);
        let result = inline_preprocessing::run(input, &attributes, &state)?;
        assert_eq!(
            result.text,
            "\u{FFFD}\u{FFFD}\u{FFFD}0\u{FFFD}\u{FFFD}\u{FFFD} world+"
        );
        assert_eq!(result.passthroughs.len(), 1);
        let Some(first) = result.passthroughs.first() else {
            panic!("expected first passthrough");
        };
        assert_eq!(first.text, Some("hello"));
        assert_eq!(first.kind, PassthroughKind::Single);
        Ok(())
    }

    #[test]
    fn test_preprocess_inline_passthrough_multiple() -> Result<(), Error> {
        let attributes = setup_attributes();
        let input = "Something\n\nHere is some +*bold*+ text and ++**more bold**++ text.";
        //                 SomethingNNHere is some +*bold*+ text and ++**more bold**++ text.
        //                 0123456789012345678901234567890123456789012345678901234567890123456
        //                          1         2         3         4         5         6
        //                                         ^^^^^^^^          ^^^^^^^^^^^^^^^^^
        //                 Here is some +*bold*+ text and ++**more bold**++ text.
        //                 123456789012345678901234567890123456789012345678901234
        //                          1         2         3         4         5
        //                              ^^^^^^^^          ^^^^^^^^^^^^^^^^^
        let state = setup_state(input);
        let result = inline_preprocessing::run(input, &attributes, &state)?;

        // Verify processed text has placeholders
        assert_eq!(
            result.text,
            "Something\n\nHere is some \u{FFFD}\u{FFFD}\u{FFFD}0\u{FFFD}\u{FFFD}\u{FFFD} text and \u{FFFD}\u{FFFD}\u{FFFD}1\u{FFFD}\u{FFFD}\u{FFFD} text."
        );

        // Verify passthroughs were captured
        assert_eq!(result.passthroughs.len(), 2);

        // Check first passthrough
        let Some(first) = result.passthroughs.first() else {
            panic!("expected first passthrough");
        };
        assert!(matches!(&first.text, Some(s) if *s == "*bold*"));
        assert_eq!(first.location.absolute_start, 24);
        assert_eq!(first.location.absolute_end, 32);
        assert_eq!(first.location.start.line, 3);
        assert_eq!(first.location.start.column, 14);
        assert_eq!(first.location.end.line, 3);
        assert_eq!(first.location.end.column, 22);

        // Check second passthrough
        let Some(second) = result.passthroughs.get(1) else {
            panic!("expected second passthrough");
        };
        assert!(matches!(&second.text, Some(s) if *s == "**more bold**"));
        assert_eq!(second.location.absolute_start, 42);
        assert_eq!(second.location.absolute_end, 59);
        assert_eq!(second.location.start.line, 3);
        assert_eq!(second.location.start.column, 32);
        assert_eq!(second.location.end.line, 3);
        assert_eq!(second.location.end.column, 49);
        Ok(())
    }

    #[test]
    fn test_preprocess_attribute_in_link() -> Result<(), Error> {
        let attributes = setup_attributes();
        let input = "The {s}[syntax page] provides complete stuff.";
        let state = setup_state(input);

        let result = inline_preprocessing::run(input, &attributes, &state)?;

        assert_eq!(
            result.text,
            "The link:/nonono[syntax page] provides complete stuff."
        );

        // Check that source positions are mapped correctly
        // Original:  "The {s}[syntax page] provides complete stuff."
        //             012345678901234567890123456789012345678901234567890123
        // Processed: "The link:/nonono[syntax page] provides complete stuff."
        assert_eq!(result.source_map.map_position(15)?, 4); // This is still within the attribute so map it to the beginning.
        assert_eq!(result.source_map.map_position(16)?, 7); // This is after the attribute so map it to where it should be.
        assert_eq!(result.source_map.map_position(30)?, 21); // This is the `p` from `provides`.
        Ok(())
    }

    #[test]
    fn test_preprocess_inline_in_attributes() -> Result<(), Error> {
        let attributes = setup_attributes();

        // Test block title with attribute reference
        let input = "Version {version} of {title}";
        let state = setup_state(input);
        //                 0123456789012345678901234567
        //                 Version 1.0 of My Title
        //                 {version} -> 1.0 (-6 chars)
        //                 {title} -> My Title (+1 char)
        let result = inline_preprocessing::run(input, &attributes, &state)?;

        assert_eq!(result.text, "Version 1.0 of My Title");

        // Original:  "Version {version} of {title}"
        //             0123456789012345678901234567
        // Processed: "Version 1.0 of My Title"

        // Position 8 in original (start of {version}) should map to position 8 in
        // processed (start of "1.0")
        assert_eq!(result.source_map.map_position(8)?, 8);
        assert_eq!(result.source_map.map_position(15)?, 21);
        Ok(())
    }

    #[test]
    fn test_preprocess_complex_example() -> Result<(), Error> {
        let attributes = setup_attributes();
        // Complex example with attribute in link and passthrough
        let input = "Check the {s}[syntax page] and +this {s} won't expand+ for details.";
        //                 0123456789012345678901234
        //                           ^
        //                           {s} expands to link:/nonono (+9 chars)
        let state = setup_state(input);
        let result = inline_preprocessing::run(input, &attributes, &state)?;

        assert_eq!(
            result.text,
            "Check the link:/nonono[syntax page] and \u{FFFD}\u{FFFD}\u{FFFD}0\u{FFFD}\u{FFFD}\u{FFFD} for details."
        );

        // Verify passthrough was captured and preserved
        assert_eq!(result.passthroughs.len(), 1);
        let Some(first) = result.passthroughs.first() else {
            panic!("expected first passthrough");
        };
        assert!(matches!(
            &first.text,
            Some(s) if *s == "this {s} won't expand"
        ));

        // Verify source mapping
        let pos = result.source_map.map_position(10)?; // Start of {s}
        assert_eq!(pos, 10); // Should map to start of "link:/nonono"
        Ok(())
    }

    #[test]
    fn test_nested_passthrough_with_nested_attributes() -> Result<(), Error> {
        let mut attributes = setup_attributes();
        // Add nested attributes
        attributes.insert("nested1".into(), AttributeValue::String("{version}".into()));
        attributes.insert("nested2".into(), AttributeValue::String("{nested1}".into()));

        // Test passthrough containing attribute that references another attribute
        let input = "Here is a +special {nested2} value+ to test.";
        let state = setup_state(input);
        let result = inline_preprocessing::run(input, &attributes, &state)?;

        // Verify the passthrough preserved the unexpanded attribute
        assert_eq!(
            result.text,
            "Here is a \u{FFFD}\u{FFFD}\u{FFFD}0\u{FFFD}\u{FFFD}\u{FFFD} to test."
        );

        // Verify passthrough content preserved original text without expansion
        assert_eq!(result.passthroughs.len(), 1);
        let Some(first) = result.passthroughs.first() else {
            panic!("expected first passthrough");
        };
        assert!(matches!(
            &first.text,
            Some(s) if *s == "special {nested2} value"
        ));

        // Verify source positions for debugging
        let start_pos = first.location.absolute_start;
        let end_pos = first.location.absolute_end;
        assert_eq!(start_pos, 10); // Start of passthrough content
        assert_eq!(end_pos, 35); // End of passthrough content
        Ok(())
    }

    #[test]
    fn test_line_breaks() -> Result<(), Error> {
        let attributes = setup_attributes();

        let input = "This is a test +\nwith a line break.";
        let state = setup_state(input);
        //                 012345678901234567890123456789012345678
        //                 0         1         2         3         4
        let result = inline_preprocessing::run(input, &attributes, &state)?;
        assert_eq!(result.text, "This is a test +\nwith a line break.");

        // Verify no passthroughs were captured
        assert!(result.passthroughs.is_empty());
        Ok(())
    }

    #[test]
    fn test_section_with_passthrough() -> Result<(), Error> {
        let attributes = setup_attributes();
        // Greedy matching: +<h1>+World+ matches (content: <h1>+World), +<u>+Gemini+ matches (content: <u>+Gemini)
        let input = "= Document Title\nHello +<h1>+World+</h1>+ of +<u>+Gemini+</u>+";
        //                 0123456789012345678901234567890123456789012345678901234567890
        //                 0         1         2         3         4         5         6
        let state = setup_state(input);
        let result = inline_preprocessing::run(input, &attributes, &state)?;

        // Two passthroughs with greedy matching (not four)
        assert_eq!(
            result.text,
            "= Document Title\nHello \u{FFFD}\u{FFFD}\u{FFFD}0\u{FFFD}\u{FFFD}\u{FFFD}</h1>+ of \u{FFFD}\u{FFFD}\u{FFFD}1\u{FFFD}\u{FFFD}\u{FFFD}</u>+"
        );

        assert_eq!(result.passthroughs.len(), 2);

        let Some(first_pass) = result.passthroughs.first() else {
            panic!("expected first passthrough");
        };
        let Some(second_pass) = result.passthroughs.get(1) else {
            panic!("expected second passthrough");
        };

        // Check passthrough content with greedy matching
        assert!(matches!(&first_pass.text, Some(s) if *s == "<h1>+World"));
        assert!(matches!(&second_pass.text, Some(s) if *s == "<u>+Gemini"));

        // Verify substitutions were captured
        assert!(
            first_pass
                .substitutions
                .contains(&Substitution::SpecialChars)
        );
        assert!(
            second_pass
                .substitutions
                .contains(&Substitution::SpecialChars)
        );

        Ok(())
    }

    #[test]
    fn test_pass_macro_with_mixed_content() -> Result<(), Error> {
        let mut attributes = setup_attributes();
        // Add docname attribute
        attributes.insert("docname".into(), AttributeValue::String("test-doc".into()));

        let input = "The text pass:q,a[<u>underline _{docname}_</u>] is underlined.";
        let state = setup_state(input);
        //                 01234567890123456789012345678901234567890123456789012345678901
        //                 0         1         2         3         4         5         6
        //                          ^start of pass        ^docname
        //                "The text FFF0FFF is underlined."
        let result = inline_preprocessing::run(input, &attributes, &state)?;
        assert_eq!(
            result.text,
            "The text \u{FFFD}\u{FFFD}\u{FFFD}0\u{FFFD}\u{FFFD}\u{FFFD} is underlined."
        );

        // Verify passthrough was captured
        assert_eq!(result.passthroughs.len(), 1);

        let Some(pass) = result.passthroughs.first() else {
            panic!("expected first passthrough");
        };

        // Check passthrough content preserved original text
        assert!(matches!(
            &pass.text,
            Some(s) if *s == "<u>underline _test-doc_</u>"
        ));

        // Verify substitutions were captured
        assert!(pass.substitutions.contains(&Substitution::Quotes)); // 'q'
        assert!(pass.substitutions.contains(&Substitution::Attributes)); // 'a'

        // Check positions
        assert_eq!(pass.location.absolute_start, 9); // Start of pass macro
        assert_eq!(pass.location.absolute_end, 47); // End of pass macro content including brackets

        assert_eq!(result.source_map.map_position(9)?, 9); // Start of pass macro
        assert_eq!(result.source_map.map_position(24)?, 55);
        Ok(())
    }

    #[test]
    fn test_all_passthroughs_with_attribute() -> Result<(), Error> {
        let mut attributes = setup_attributes();
        attributes.insert("meh".into(), AttributeValue::String("1.0".into()));

        let input = "1 +2+, ++3++ {meh} and +++4+++ are all numbers.";
        //                 012345678901234567890123456789012345678901234567890123456789012345678901234567890123456
        //                 0         1         2         3         4         5         6         7         8
        //                 1 FFF0FFF, FFF1FFF 1.0 and FFF2FFF are all numbers.
        let state = setup_state(input);
        let result = inline_preprocessing::run(input, &attributes, &state)?;
        assert_eq!(
            result.text,
            "1 \u{FFFD}\u{FFFD}\u{FFFD}0\u{FFFD}\u{FFFD}\u{FFFD}, \u{FFFD}\u{FFFD}\u{FFFD}1\u{FFFD}\u{FFFD}\u{FFFD} 1.0 and \u{FFFD}\u{FFFD}\u{FFFD}2\u{FFFD}\u{FFFD}\u{FFFD} are all numbers."
        );

        assert_eq!(result.passthroughs.len(), 3);
        let Some(first) = result.passthroughs.first() else {
            panic!("expected first passthrough");
        };
        let Some(second) = result.passthroughs.get(1) else {
            panic!("expected second passthrough");
        };
        let Some(third) = result.passthroughs.get(2) else {
            panic!("expected third passthrough");
        };
        assert!(matches!(first.kind, PassthroughKind::Single));
        assert!(matches!(second.kind, PassthroughKind::Double));
        assert!(matches!(third.kind, PassthroughKind::Triple));
        assert!(matches!(&first.text, Some(s) if *s == "2"));
        assert!(matches!(&second.text, Some(s) if *s == "3"));
        assert!(matches!(&third.text, Some(s) if *s == "4"));

        assert_eq!(result.source_map.map_position(2)?, 2);
        // 5 is the 0 within FFF0FFF, which corresponds to the +2+ macro: I believe it should map to the end of the macro.
        assert_eq!(result.source_map.map_position(5)?, 4);
        // 24 is the FFF in passthrough 2, therefore it should map to position 10
        assert_eq!(result.source_map.map_position(24)?, 20);
        // 48 is the n in "and", therefore it should map to position 19
        assert_eq!(result.source_map.map_position(48)?, 44);
        Ok(())
    }

    #[test]
    fn test_greedy_matching_single_plus_passthrough() -> Result<(), Error> {
        let attributes = setup_attributes();
        // Test case 1: +A+B+ should greedily match from first to third +
        let input = "Test +A+B+ end";
        let state = setup_state(input);
        let result = inline_preprocessing::run(input, &attributes, &state)?;
        assert_eq!(result.passthroughs.len(), 1);
        let Some(first) = result.passthroughs.first() else {
            panic!("expected first passthrough");
        };
        assert!(matches!(&first.text, Some(s) if *s == "A+B"));

        // Test case 2: +A+ +B+ should create two separate passthroughs (space breaks greedy)
        let input2 = "Test +A+ +B+ end";
        let state2 = setup_state(input2);
        let result2 = inline_preprocessing::run(input2, &attributes, &state2)?;
        assert_eq!(result2.passthroughs.len(), 2);
        let Some(first) = result2.passthroughs.first() else {
            panic!("expected first passthrough");
        };
        let Some(second) = result2.passthroughs.get(1) else {
            panic!("expected second passthrough");
        };
        assert!(matches!(&first.text, Some(s) if *s == "A"));
        assert!(matches!(&second.text, Some(s) if *s == "B"));

        // Test case 3: +A+B+C+D+ should greedily match all
        let input3 = "Test +A+B+C+D+ end";
        let state3 = setup_state(input3);
        let result3 = inline_preprocessing::run(input3, &attributes, &state3)?;
        assert_eq!(result3.passthroughs.len(), 1);
        let Some(first) = result3.passthroughs.first() else {
            panic!("expected first passthrough");
        };
        assert!(matches!(&first.text, Some(s) if *s == "A+B+C+D"));

        // Test case 4: +HTML+tags+ with boundary characters
        let input4 = "Test +<em>+text+ end";
        let state4 = setup_state(input4);
        let result4 = inline_preprocessing::run(input4, &attributes, &state4)?;
        assert_eq!(result4.passthroughs.len(), 1);
        let Some(first) = result4.passthroughs.first() else {
            panic!("expected first passthrough");
        };
        assert!(matches!(&first.text, Some(s) if *s == "<em>+text"));

        // Test case 5: Multiple + with punctuation boundaries
        let input5 = "Look +here+there+, ok";
        let state5 = setup_state(input5);
        let result5 = inline_preprocessing::run(input5, &attributes, &state5)?;
        assert_eq!(result5.passthroughs.len(), 1);
        let Some(first) = result5.passthroughs.first() else {
            panic!("expected first passthrough");
        };
        assert!(matches!(&first.text, Some(s) if *s == "here+there"));

        // Test case 6: The original bug case from f.adoc
        let input6 = "Hello +<h1>+World+</h1>+ and +<u>+Gemini+</u>+ end";
        let state6 = setup_state(input6);
        let result6 = inline_preprocessing::run(input6, &attributes, &state6)?;
        assert_eq!(result6.passthroughs.len(), 2);
        let Some(first) = result6.passthroughs.first() else {
            panic!("expected first passthrough");
        };
        let Some(second) = result6.passthroughs.get(1) else {
            panic!("expected second passthrough");
        };
        assert!(matches!(&first.text, Some(s) if *s == "<h1>+World"));
        assert!(matches!(&second.text, Some(s) if *s == "<u>+Gemini"));

        Ok(())
    }

    /// Comprehensive test for all character replacement attributes.
    ///
    /// Tests all 31 attributes defined in the `AsciiDoc` specification:
    /// <https://docs.asciidoctor.org/asciidoc/latest/attributes/character-replacement-ref/>
    ///
    /// Note: `{lt}`, `{gt}`, `{amp}` are treated as passthroughs and produce placeholders
    /// in the preprocessed text. They are resolved to `RawText` nodes during passthrough
    /// processing, which bypasses HTML escaping.
    #[test]
    fn test_all_character_replacement_attributes() -> Result<(), Error> {
        let attributes = DocumentAttributes::default();
        let input = concat!(
            // Whitespace & invisible
            "{empty}{blank}{sp}{nbsp}{zwsp}{wj}",
            // Quotes
            "{apos}{quot}{lsquo}{rsquo}{ldquo}{rdquo}",
            // Symbols
            "{deg}{plus}{brvbar}{vbar}{amp}{lt}{gt}",
            // Syntax escaping
            "{startsb}{endsb}{caret}{asterisk}{tilde}{backslash}{backtick}",
            // Sequences
            "{two-colons}{two-semicolons}{cpp}{cxx}{pp}"
        );
        let state = setup_state(input);
        let result = inline_preprocessing::run(input, &attributes, &state)?;

        // Build expected output by concatenating all expected values.
        // All ASCII character replacement attributes now produce passthrough placeholders
        // to prevent the PEG grammar from misinterpreting their values as AsciiDoc syntax.
        // Passthrough indices are assigned in order of appearance.
        let p = |i: usize| format!("\u{FFFD}\u{FFFD}\u{FFFD}{i}\u{FFFD}\u{FFFD}\u{FFFD}");
        let expected = format!(
            concat!(
                // Whitespace: empty, blank, space, nbsp, zwsp, wj (not passthroughs)
                "", "", " ", "\u{00A0}", "\u{200B}", "\u{2060}",
                // Quotes: apos(p0), quot(p1), lsquo, rsquo, ldquo, rdquo
                "{}", // apos
                "{}", // quot
                "\u{2018}", "\u{2019}", "\u{201C}", "\u{201D}",
                // Symbols: deg, plus(p2), brvbar, vbar(p3), amp(p4), lt(p5), gt(p6)
                "\u{00B0}", "{}", // plus
                "\u{00A6}", "{}", // vbar
                "{}", // amp
                "{}", // lt
                "{}", // gt
                // Escaping: startsb(p7), endsb(p8), caret(p9), asterisk(p10),
                //           tilde(p11), backslash(p12), backtick(p13)
                "{}", // startsb
                "{}", // endsb
                "{}", // caret
                "{}", // asterisk
                "{}", // tilde
                "{}", // backslash
                "{}", // backtick
                // Sequences: two-colons(p14), two-semicolons(p15), cpp(p16), cxx(p17), pp(p18)
                "{}", // two-colons
                "{}", // two-semicolons
                "{}", // cpp
                "{}", // cxx
                "{}", // pp
            ),
            p(0),
            p(1),
            p(2),
            p(3),
            p(4),
            p(5),
            p(6),
            p(7),
            p(8),
            p(9),
            p(10),
            p(11),
            p(12),
            p(13),
            p(14),
            p(15),
            p(16),
            p(17),
            p(18),
        );

        assert_eq!(
            result.text, expected,
            "Character replacement attributes did not produce expected values"
        );

        // Verify passthroughs were created for all ASCII character replacement attributes
        assert_eq!(
            result.passthroughs.len(),
            19,
            "Should have 19 passthroughs for all ASCII char replacement attributes"
        );
        // Spot-check a few key passthroughs
        assert_eq!(result.passthroughs[0].text, Some("&#39;")); // apos
        assert_eq!(result.passthroughs[2].text, Some("+")); // plus
        assert_eq!(result.passthroughs[4].text, Some("&")); // amp
        assert_eq!(result.passthroughs[16].text, Some("C++")); // cpp

        Ok(())
    }

    /// Test that character replacement attributes work in context.
    #[test]
    fn test_character_replacement_in_context() -> Result<(), Error> {
        let attributes = DocumentAttributes::default();

        // Test 1: Attributes in sentence
        let input1 = "The temperature is 100{deg}F";
        let state1 = setup_state(input1);
        let result1 = inline_preprocessing::run(input1, &attributes, &state1)?;
        assert_eq!(result1.text, "The temperature is 100\u{00B0}F");

        // Test 2: Multiple attributes (now produce passthrough placeholders)
        let input2 = "Use {startsb}option{endsb} syntax";
        let state2 = setup_state(input2);
        let result2 = inline_preprocessing::run(input2, &attributes, &state2)?;
        assert_eq!(
            result2.text,
            "Use \u{FFFD}\u{FFFD}\u{FFFD}0\u{FFFD}\u{FFFD}\u{FFFD}option\u{FFFD}\u{FFFD}\u{FFFD}1\u{FFFD}\u{FFFD}\u{FFFD} syntax"
        );
        assert_eq!(result2.passthroughs.len(), 2);
        assert_eq!(result2.passthroughs[0].text, Some("["));
        assert_eq!(result2.passthroughs[1].text, Some("]"));

        // Test 3: Adjacent attributes (Unicode chars, not passthroughs)
        let input3 = "{ldquo}Hello{rdquo}";
        let state3 = setup_state(input3);
        let result3 = inline_preprocessing::run(input3, &attributes, &state3)?;
        assert_eq!(result3.text, "\u{201C}Hello\u{201D}");

        // Test 4: Empty/blank produce no visible output
        let input4 = "before{empty}after";
        let state4 = setup_state(input4);
        let result4 = inline_preprocessing::run(input4, &attributes, &state4)?;
        assert_eq!(result4.text, "beforeafter");

        let input5 = "before{blank}after";
        let state5 = setup_state(input5);
        let result5 = inline_preprocessing::run(input5, &attributes, &state5)?;
        assert_eq!(result5.text, "beforeafter");

        // Test 5: C++ variations (now produce passthrough placeholders)
        let input6 = "{cpp} is same as {cxx}";
        let state6 = setup_state(input6);
        let result6 = inline_preprocessing::run(input6, &attributes, &state6)?;
        assert_eq!(
            result6.text,
            "\u{FFFD}\u{FFFD}\u{FFFD}0\u{FFFD}\u{FFFD}\u{FFFD} is same as \u{FFFD}\u{FFFD}\u{FFFD}1\u{FFFD}\u{FFFD}\u{FFFD}"
        );
        assert_eq!(result6.passthroughs.len(), 2);
        assert_eq!(result6.passthroughs[0].text, Some("C++"));
        assert_eq!(result6.passthroughs[1].text, Some("C++"));

        Ok(())
    }

    #[test]
    fn test_counter_reference_collects_warning() -> Result<(), Error> {
        let attributes = setup_attributes();
        let input = "Count: {counter:mycount}";
        let state = setup_state(input);
        let result = inline_preprocessing::run(input, &attributes, &state)?;
        // Counter is removed from output
        assert_eq!(result.text, "Count: ");
        // Warning is collected, not emitted directly
        let warnings = state.warnings.borrow();
        assert_eq!(warnings.len(), 1);
        let msg = warnings[0].to_string();
        assert!(msg.contains("counter"), "got: {msg}");
        assert!(msg.contains("mycount"), "got: {msg}");
        // Counter warning should carry a location pointing at the `{counter:..}` span.
        assert!(warnings[0].source_location().is_some());
        Ok(())
    }

    #[test]
    fn test_counter_reference_deduplication_is_per_position() -> Result<(), Error> {
        let attributes = setup_attributes();
        // Same counter referenced at two different positions — each position is a
        // distinct diagnostic site, so LSP gets two separate warnings to squiggle.
        // Dedup only folds duplicates at the same position (PEG backtracking).
        let input = "{counter:hits} and {counter:hits}";
        let state = setup_state(input);
        let result = inline_preprocessing::run(input, &attributes, &state)?;
        assert_eq!(result.text, " and ");
        let warnings = state.warnings.borrow();
        assert_eq!(
            warnings.len(),
            2,
            "each counter reference is its own diagnostic site, got: {warnings:?}",
        );
        // Both warnings should carry distinct source locations.
        let loc0 = warnings[0].source_location().expect("loc0");
        let loc1 = warnings[1].source_location().expect("loc1");
        assert_ne!(loc0, loc1, "locations should differ between occurrences");
        Ok(())
    }

    #[test]
    fn test_distinct_counter_references_produce_separate_warnings() -> Result<(), Error> {
        let attributes = setup_attributes();
        let input = "{counter:a} and {counter2:b}";
        let state = setup_state(input);
        let result = inline_preprocessing::run(input, &attributes, &state)?;
        assert_eq!(result.text, " and ");
        let warnings = state.warnings.borrow();
        assert_eq!(
            warnings.len(),
            2,
            "different counter warnings should both be collected"
        );
        Ok(())
    }

    fn setup_state_macros_disabled(content: &str) -> InlinePreprocessorParserState<'_> {
        let arena: &'static bumpalo::Bump = Box::leak(Box::new(bumpalo::Bump::new()));
        InlinePreprocessorParserState {
            pass_found_count: Cell::new(0),
            passthroughs: RefCell::new(Vec::new()),
            attributes: RefCell::new(HashMap::new()),
            current_offset: Cell::new(0),
            line_map: Rc::new(LineMap::new(content)),
            full_input: content,
            arena,
            source_map: RefCell::new(SourceMap::default()),
            input: RefCell::new(content),
            substring_start_offset: Cell::new(0),
            warnings: RefCell::new(Vec::new()),
            macros_enabled: false,
            attributes_enabled: true,
        }
    }

    #[test]
    fn test_pass_macro_a_with_macros_disabled_expands_attributes() -> Result<(), Error> {
        let attributes = setup_attributes();
        let input = "pass:a[{version}]";
        let state = setup_state_macros_disabled(input);
        let result = inline_preprocessing::run(input, &attributes, &state)?;
        assert_eq!(result.text, "pass:a[1.0]");
        assert!(state.passthroughs.borrow().is_empty());
        Ok(())
    }

    #[test]
    fn test_pass_macro_no_subs_with_macros_disabled_preserves_attributes() -> Result<(), Error> {
        let attributes = setup_attributes();
        let input = "pass:[{version}]";
        let state = setup_state_macros_disabled(input);
        let result = inline_preprocessing::run(input, &attributes, &state)?;
        assert_eq!(result.text, "pass:[{version}]");
        assert!(state.passthroughs.borrow().is_empty());
        Ok(())
    }

    #[test]
    fn test_pass_macro_q_with_macros_disabled_preserves_content() -> Result<(), Error> {
        let attributes = setup_attributes();
        let input = "pass:q[text]";
        let state = setup_state_macros_disabled(input);
        let result = inline_preprocessing::run(input, &attributes, &state)?;
        assert_eq!(result.text, "pass:q[text]");
        assert!(state.passthroughs.borrow().is_empty());
        Ok(())
    }

    #[test]
    fn test_pass_macro_a_q_with_macros_disabled_expands_attributes() -> Result<(), Error> {
        let attributes = setup_attributes();
        let input = "pass:a,q[{version}]";
        let state = setup_state_macros_disabled(input);
        let result = inline_preprocessing::run(input, &attributes, &state)?;
        assert_eq!(result.text, "pass:a,q[1.0]");
        assert!(state.passthroughs.borrow().is_empty());
        Ok(())
    }
}
