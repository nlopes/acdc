//! Inline preprocessing state and source mapping.

mod peg;
mod recognition;

use recognition::{constrained_passthrough_end, constrained_passthrough_start};

pub(crate) use peg::inline_preprocessing;

use bumpalo::Bump;
use std::{
    borrow::Cow,
    cell::{Cell, RefCell},
    mem::take,
    ops::Range,
    rc::Rc,
};

use crate::{
    DocumentAttributes, Error, Location, Pass, PassthroughKind, Position, SourceLocation,
    Substitution, Warning, grammar::LineMap, model::substitution::parse_substitution,
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
    /// Current byte offset in the full document input.
    pub(crate) current_offset: Cell<usize>,
    /// Pre-computed line map for O(log n) offset→position lookups.
    pub(crate) line_map: Rc<LineMap>,
    /// Full document input (for `LineMap` position lookups).
    pub(crate) full_input: &'a str,
    /// Arena for interning synthesised passthrough strings produced during
    /// preprocessing (character-replacement expansions, escape-stripped text).
    pub(crate) arena: &'a Bump,
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
    pub(crate) defer_monospace: bool,
    pub(crate) attribute_value_ranges: Vec<Range<usize>>,
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
        arena: &'a Bump,
        macros_enabled: bool,
        attributes_enabled: bool,
    ) -> Self {
        Self {
            pass_found_count: Cell::new(0),
            passthroughs: RefCell::new(Vec::new()),
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
            defer_monospace: true,
            attribute_value_ranges: Vec::new(),
        }
    }

    /// Create a new state with all substitutions enabled (macros + attributes).
    pub(crate) fn new_all_enabled(
        input: &'a str,
        line_map: Rc<LineMap>,
        full_input: &'a str,
        arena: &'a Bump,
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
            location: Location {
                absolute_start: start_offset,
                absolute_end: end_offset,
                start,
                end,
            },
        }
    }

    /// Drain collected warnings (for transfer to main `ParserState`).
    pub(crate) fn drain_warnings(&self) -> Vec<Warning> {
        take(&mut *self.warnings.borrow_mut())
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

    fn expand_escaped_passthrough(
        &self,
        source: &'a str,
        document_attributes: &DocumentAttributes<'a>,
        extract_single_passthroughs: bool,
    ) -> String {
        let escape_start = self.get_offset();
        let source_start = escape_start + 1;
        // Escaping `++` or `+++` skips that match, but the remaining pluses still
        // enter the constrained single-plus passthrough pass.
        let expanded = if extract_single_passthroughs {
            self.extract_single_passthroughs(source, source_start, document_attributes)
        } else {
            let mut output = String::with_capacity(source.len());
            self.append_unprotected_text(&mut output, source, source_start, document_attributes);
            output
        };

        self.source_map.borrow_mut().add_replacement(
            escape_start,
            source_start,
            0,
            ProcessedKind::Escape,
        );
        self.advance_by(source.len() + 1);
        expanded
    }

    fn extract_single_passthroughs(
        &self,
        source: &'a str,
        source_start: usize,
        document_attributes: &DocumentAttributes<'a>,
    ) -> String {
        let mut output = String::with_capacity(source.len());
        let mut copied_until = 0;
        let mut search_from = 0;

        while let Some(relative_start) = source[search_from..].find('+') {
            let start = search_from + relative_start;
            let previous = if start == 0 {
                self.full_input[..source_start.saturating_sub(1)]
                    .chars()
                    .next_back()
            } else {
                source[..start].chars().next_back()
            };
            if !constrained_passthrough_start(previous) {
                search_from = start + 1;
                continue;
            }

            let Some(end) = constrained_passthrough_end(source, start) else {
                search_from = start + 1;
                continue;
            };

            self.append_unprotected_text(
                &mut output,
                &source[copied_until..start],
                source_start + copied_until,
                document_attributes,
            );

            let content = &source[start + 1..end];
            let location = self.location_from_offsets(source_start + start, source_start + end + 1);
            self.passthroughs.borrow_mut().push(Pass {
                attribute_fragments: Box::default(),
                text: Some(content),
                substitutions: vec![Substitution::SpecialChars],
                location: location.clone(),
                kind: PassthroughKind::Single,
            });
            let placeholder = format!(
                "\u{FFFD}\u{FFFD}\u{FFFD}{}\u{FFFD}\u{FFFD}\u{FFFD}",
                self.pass_found_count.get()
            );
            self.source_map.borrow_mut().add_replacement(
                location.absolute_start,
                location.absolute_end,
                placeholder.len(),
                ProcessedKind::Passthrough,
            );
            self.pass_found_count.set(self.pass_found_count.get() + 1);
            output.push_str(&placeholder);

            copied_until = end + 1;
            search_from = copied_until;
        }

        self.append_unprotected_text(
            &mut output,
            &source[copied_until..],
            source_start + copied_until,
            document_attributes,
        );
        output
    }

    fn append_unprotected_text(
        &self,
        output: &mut String,
        source: &'a str,
        source_start: usize,
        document_attributes: &DocumentAttributes<'a>,
    ) {
        if !self.attributes_enabled {
            output.push_str(source);
            return;
        }

        let mut copied_until = 0;
        let mut search_from = 0;
        while let Some(relative_start) = source[search_from..].find('{') {
            let start = search_from + relative_start;
            let Some(relative_end) = source[start + 1..].find('}') else {
                break;
            };
            let end = start + relative_end + 2;
            let name = &source[start + 1..end - 1];
            if name.is_empty()
                || !name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
            {
                search_from = end;
                continue;
            }

            output.push_str(&source[copied_until..start]);
            let location = self.location_from_offsets(source_start + start, source_start + end);
            output.push_str(&self.expand_attribute_reference(name, location, document_attributes));
            copied_until = end;
            search_from = end;
        }
        output.push_str(&source[copied_until..]);
    }

    fn expand_attribute_reference(
        &self,
        attribute_name: &str,
        location: Location,
        document_attributes: &DocumentAttributes<'a>,
    ) -> String {
        if !self.attributes_enabled {
            return format!("{{{attribute_name}}}");
        }

        // Character-replacement attributes must bypass the second inline parse;
        // otherwise values such as `+` and `*` become AsciiDoc syntax.
        // https://docs.asciidoctor.org/asciidoc/latest/attributes/character-replacement-ref/
        let is_character_reference = matches!(
            attribute_name,
            "lt" | "gt"
                | "amp"
                | "plus"
                | "pp"
                | "cpp"
                | "cxx"
                | "asterisk"
                | "backtick"
                | "caret"
                | "tilde"
                | "vbar"
                | "startsb"
                | "endsb"
                | "backslash"
                | "two-colons"
                | "two-semicolons"
                | "apos"
                | "quot"
        );

        let Some(resolved) = document_attributes.get(attribute_name) else {
            return format!("{{{attribute_name}}}");
        };

        if (is_character_reference || !resolved.inline_fragments().is_empty())
            && let Some(value) = resolved.as_str()
        {
            let absolute_start = location.absolute_start;
            let absolute_end = location.absolute_end;
            self.passthroughs.borrow_mut().push(Pass {
                attribute_fragments: resolved.inline_fragments().into(),
                text: Some(self.arena.alloc_str(value.as_ref())),
                substitutions: Vec::new(),
                location,
                kind: PassthroughKind::AttributeRef,
            });
            let placeholder = format!(
                "\u{FFFD}\u{FFFD}\u{FFFD}{}\u{FFFD}\u{FFFD}\u{FFFD}",
                self.pass_found_count.get()
            );
            self.source_map.borrow_mut().add_replacement(
                absolute_start,
                absolute_end,
                placeholder.len(),
                ProcessedKind::Passthrough,
            );
            self.pass_found_count.set(self.pass_found_count.get() + 1);
            return placeholder;
        }

        let mut value = String::new();
        let _ = resolved.write_text(&mut value);
        self.source_map.borrow_mut().add_replacement(
            location.absolute_start,
            location.absolute_end,
            value.len(),
            if resolved.passthrough_ranges().is_empty() {
                ProcessedKind::Attribute
            } else {
                ProcessedKind::RawAttribute(resolved.passthrough_ranges().into())
            },
        );
        value
    }

    fn location_from_offsets(&self, absolute_start: usize, absolute_end: usize) -> Location {
        Location {
            absolute_start,
            absolute_end,
            start: self
                .line_map
                .offset_to_position(absolute_start, self.full_input),
            end: self
                .line_map
                .offset_to_position(absolute_end, self.full_input),
        }
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
    pub(crate) attribute_substitutions: crate::model::substitution::SubstitutionPlan,
}

#[derive(Debug, Clone)]
pub(crate) struct Replacement {
    pub absolute_start: usize,
    pub absolute_end: usize,
    pub byte_len: usize,
    pub kind: ProcessedKind,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct SourceMap {
    pub replacements: Vec<Replacement>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProcessedKind {
    Attribute,
    RawAttribute(Box<[Range<usize>]>),
    Escape,
    Passthrough,
}

impl ProcessedKind {
    pub(crate) const fn is_attribute(&self) -> bool {
        matches!(self, Self::Attribute | Self::RawAttribute(_))
    }
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
    /// - `absolute_start`: where in the original text the replaced span begins
    /// - `absolute_end`: where in the original text the replaced span ends
    /// - `replacement_length`: the byte length of the inserted replacement
    pub(crate) fn add_replacement(
        &mut self,
        absolute_start: usize,
        absolute_end: usize,
        replacement_length: usize,
        kind: ProcessedKind,
    ) {
        self.replacements.push(Replacement {
            absolute_start,
            absolute_end,
            byte_len: replacement_length,
            kind,
        });
    }

    /// Map a position in the processed text back to the original source.
    pub(crate) fn map_position(&self, pos: usize) -> Result<usize, Error> {
        self.map_position_with_bias(pos, false)
    }

    /// Map an inclusive end position in the processed text back to the original source.
    pub(crate) fn map_end_position(&self, pos: usize) -> Result<usize, Error> {
        self.map_position_with_bias(pos, true)
    }

    /// Find processed-text offsets where attributes expanded to an empty string.
    pub(crate) fn empty_attribute_offsets(&self, original_start: usize) -> Vec<usize> {
        // Track the same point in the original and processed coordinate spaces.
        let mut original_cursor = original_start;
        let mut processed_cursor = 0;
        let mut offsets = Vec::new();

        for replacement in &self.replacements {
            // Ignore replacements before the processed substring.
            if replacement.absolute_start < original_cursor {
                continue;
            }

            // Unchanged source bytes have the same length in both texts.
            processed_cursor += replacement.absolute_start - original_cursor;
            if replacement.kind.is_attribute() && replacement.byte_len == 0 {
                // A removed attribute occupies this zero-width processed position.
                offsets.push(processed_cursor);
            }
            // Resume after the replacement in each coordinate space.
            processed_cursor += replacement.byte_len;
            original_cursor = replacement.absolute_end;
        }

        offsets
    }

    /// Return attribute-produced spans so nested parsing preserves substitution order.
    pub(crate) fn attribute_value_ranges(&self, original_start: usize) -> Vec<Range<usize>> {
        let mut original_cursor = original_start;
        let mut processed_cursor = 0;
        let mut ranges = Vec::new();

        for replacement in &self.replacements {
            if replacement.absolute_start < original_cursor {
                continue;
            }

            processed_cursor += replacement.absolute_start - original_cursor;
            if replacement.kind.is_attribute() && replacement.byte_len != 0 {
                ranges.push(processed_cursor..processed_cursor + replacement.byte_len);
            }
            processed_cursor += replacement.byte_len;
            original_cursor = replacement.absolute_end;
        }

        ranges
    }

    pub(crate) fn raw_attribute_ranges(&self, original_start: usize) -> Vec<Range<usize>> {
        let mut original_cursor = original_start;
        let mut processed_cursor = 0;
        let mut ranges = Vec::new();
        for replacement in &self.replacements {
            if replacement.absolute_start < original_cursor {
                continue;
            }
            processed_cursor += replacement.absolute_start - original_cursor;
            if let ProcessedKind::RawAttribute(raw) = &replacement.kind {
                ranges.extend(
                    raw.iter()
                        .map(|range| range.start + processed_cursor..range.end + processed_cursor),
                );
            }
            processed_cursor += replacement.byte_len;
            original_cursor = replacement.absolute_end;
        }
        ranges
    }

    fn map_position_with_bias(&self, pos: usize, end_bias: bool) -> Result<usize, Error> {
        let signed_pos = to_signed(pos, "pos")?;

        // The adjustment is the total number of bytes removed or added during preprocessing.
        let mut adjustment: i32 = 0;

        for rep in &self.replacements {
            let rep_start = to_signed(rep.absolute_start, "rep.absolute_start")?;
            let rep_end = to_signed(rep.absolute_end, "rep.absolute_end")?;
            let replacement_length = to_signed(rep.byte_len, "rep.byte_len")?;
            let rep_processed_start = rep_start + adjustment;
            let rep_processed_end = rep_processed_start + replacement_length;

            // Position is before this replacement - done adjusting
            if signed_pos < rep_processed_start || (signed_pos == rep_processed_start && !end_bias)
            {
                break;
            }

            // Position is within this replacement
            if signed_pos < rep_processed_end {
                if end_bias {
                    return Ok(rep.absolute_end.saturating_sub(1));
                }
                return match rep.kind {
                    ProcessedKind::Attribute | ProcessedKind::RawAttribute(_) => {
                        // All inserted characters map to the left-most original position
                        Ok(rep.absolute_start)
                    }
                    ProcessedKind::Escape => Ok(rep.absolute_start),
                    ProcessedKind::Passthrough => {
                        let mapped_pos = signed_pos - adjustment;
                        if mapped_pos >= rep_end {
                            // The marker can be longer than the original passthrough.
                            Ok(rep.absolute_end - 1)
                        } else {
                            to_unsigned(mapped_pos, "within_passthrough")
                        }
                    }
                };
            }

            // Position is past this replacement - accumulate adjustment
            adjustment += replacement_length - (rep_end - rep_start);
        }

        // Not within any replacement - apply total adjustment
        to_unsigned(signed_pos - adjustment, "final_position")
    }
}

#[cfg(test)]
#[allow(
    clippy::panic,
    clippy::indexing_slicing,
    clippy::expect_used,
    clippy::unwrap_used
)]
mod tests;
