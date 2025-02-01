use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
};

use peg::parser;

use crate::{
    grammar::PositionTracker, AttributeValue, DocumentAttributes, InlineNode, Location, Pass,
    PassthroughKind, Position, Substitution,
};

// The parser state for the inline preprocessor.
//
// It should use internal mutability to allow the parser to modify the state.
#[derive(Debug)]
pub(crate) struct ParserState {
    pub(crate) pass_found_count: Cell<usize>,
    pub(crate) passthroughs: RefCell<Vec<Pass>>,
    pub(crate) attributes: RefCell<HashMap<usize, Location>>,
    pub(crate) tracker: RefCell<PositionTracker>,
    pub(crate) source_map: RefCell<SourceMap>,
}

impl ParserState {
    pub(crate) fn new() -> Self {
        Self {
            pass_found_count: Cell::new(0),
            passthroughs: RefCell::new(Vec::new()),
            attributes: RefCell::new(HashMap::new()),
            tracker: RefCell::new(PositionTracker::new()),
            source_map: RefCell::new(SourceMap::default()),
        }
    }

    pub(crate) fn set_initial_position(&mut self, location: &Location, absolute_offset: usize) {
        self.tracker
            .borrow_mut()
            .set_initial_position(location, absolute_offset);
    }
}

#[derive(Debug)]
pub(crate) struct ProcessedContent {
    pub text: String,
    pub passthroughs: Vec<Pass>,
    pub attributes: HashMap<usize, Location>,
    pub(crate) source_map: SourceMap,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct SourceMap {
    /// List of offsets to apply to source positions
    ///
    /// Each entry is a tuple of:
    /// (absolute start position, offset adjustment, kind)
    ///
    /// - absolute start position: the position in the original text where the offset should be applied
    /// - offset adjustment: the amount we adjusted as part of the preprocessing
    /// - kind: the kind of preprocessing applied
    pub(crate) offsets: Vec<(usize, i32, ProcessedKind)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProcessedKind {
    Attribute,
    Passthrough,
}

impl SourceMap {
    pub(crate) fn add_offset(&mut self, position: usize, offset: i32, kind: ProcessedKind) {
        self.offsets.push((position, offset, kind));
        self.offsets.sort_by_key(|k| k.0);

        let mut merged = Vec::new();
        let mut current_pos = 0;
        let mut current_offset = 0;
        let mut current_kind = None;

        for (pos, offs, kind) in self.offsets.drain(..) {
            if pos == current_pos {
                current_offset += offs;
                current_kind = Some(kind);
            } else {
                if current_offset != 0 {
                    merged.push((current_pos, current_offset, current_kind.unwrap()));
                }
                current_pos = pos;
                current_offset = offs;
                current_kind = Some(kind);
            }
        }

        if current_offset != 0 {
            merged.push((current_pos, current_offset, current_kind.unwrap()));
        }
        self.offsets = merged;
    }

    fn add_offset_orig(&mut self, position: usize, offset: i32, kind: ProcessedKind) {
        // Sort and merge overlapping/adjacent offsets
        self.offsets.push((position, offset, kind.clone()));
        self.offsets.sort_by_key(|k| k.0);

        let mut merged = Vec::new();
        let mut current_pos = 0;
        let mut current_offset = 0;
        let mut current_kind = kind;

        for (pos, offs, offset_kind) in self.offsets.drain(..) {
            match pos.cmp(&current_pos) {
                std::cmp::Ordering::Equal => {
                    current_offset += offs;
                }
                std::cmp::Ordering::Greater => {
                    if current_offset != 0 {
                        merged.push((current_pos, current_offset, current_kind));
                    }
                    current_pos = pos;
                    current_offset = offs;
                    current_kind = offset_kind;
                }
                std::cmp::Ordering::Less => {
                    // Skip overlapping offsets
                    continue;
                }
            }
        }

        if current_offset != 0 {
            merged.push((current_pos, current_offset, current_kind));
        }
        self.offsets = merged;
    }

    /// Map a source position back to its original location
    pub(crate) fn map_position(&self, pos: usize) -> usize {
        let mut offset = 0;

        if pos <= self.offsets.first().map(|(p, _, _)| *p).unwrap_or_default() {
            return pos;
        }

        // Apply cumulative offsets up to this position
        for (offset_pos, delta, _kind) in &self.offsets {
            if pos <= *offset_pos {
                break;
            }
            offset += -1 * delta;
        }

        usize::try_from(i32::try_from(pos).unwrap_or_default() + offset).unwrap_or_default()
    }
}

parser!(
    pub(crate) grammar InlinePreprocessor(document_attributes: &DocumentAttributes, state: &ParserState) for str {
        pub rule run() -> ProcessedContent
            = content:inlines()+ {
                ProcessedContent {
                    text: content.join(""),
                    passthroughs: state.passthroughs.borrow().clone(),
                    attributes: state.attributes.borrow().clone(),
                    source_map: state.source_map.borrow().clone(),
                }
            }

        rule inlines() -> String = quiet!{
            passthrough() / attribute_reference() / unprocessed_text()
        } / expected!("inlines parser failed")

        rule attribute_reference() -> String
            = start:position() "{" attribute_name:attribute_name() "}" {
                let location = state.tracker.borrow_mut().calculate_location(start, attribute_name, 2);
                let mut attributes = state.attributes.borrow_mut();
                if let Some(value) = document_attributes.get(&attribute_name) {
                    match value {
                        AttributeValue::String(value) => {
                            state.source_map.borrow_mut().add_offset(
                                location.absolute_start,
                                i32::try_from(value.len()).expect("failed to convert attribute value length to i32") - i32::try_from(location.absolute_end - location.absolute_start).expect("failed to convert attribute reference length to i32"),
                                ProcessedKind::Attribute
                            );
                            attributes.insert(state.source_map.borrow().offsets.len(), location);
                            value.to_string()
                        },
                        _ => {
                            // TODO(nlopes): do we need to handle other types?
                            // For non-string attributes, keep original text
                            format!("{{{attribute_name}}}")
                        }
                    }
                } else {
                    // TODO(nlopes): do we need to handle other types?
                    // For non-string attributes, keep original text
                    format!("{{{attribute_name}}}")
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
            = start:position() "+" content:$("+" / [^'+']+) "+" {
                let location = state.tracker.borrow_mut().calculate_location(start, content, 2);
                state.passthroughs.borrow_mut().push(Pass {
                    text: Some(content.to_string()),
                    // We add SpecialChars here for single and double but we don't do
                    // anything with them, only the converter does.
                    substitutions: vec![Substitution::SpecialChars].into_iter().collect(),
                    location: location.clone(),
                    kind: PassthroughKind::Single,
                });
                let new_content = format!("\u{FFFD}\u{FFFD}\u{FFFD}{}\u{FFFD}\u{FFFD}\u{FFFD}", state.pass_found_count.get());
                state.source_map.borrow_mut().add_offset(
                    location.absolute_start,
                    i32::try_from(new_content.len()).expect("failed to convert attribute value length to i32") - i32::try_from(location.absolute_end - location.absolute_start).expect("failed to convert attribute reference length to i32"),
                    ProcessedKind::Passthrough
                );

                state.pass_found_count.set(state.pass_found_count.get() + 1);
                new_content
            }

        rule double_plus_passthrough() -> String
            = start:position() "++" content:$((!"++" [_])+) "++" {
                let location = state.tracker.borrow_mut().calculate_location(start, content, 4);
                state.passthroughs.borrow_mut().push(Pass {
                    text: Some(content.to_string()),
                    // We add SpecialChars here for single and double but we don't do
                    // anything with them, only the converter does.
                    substitutions: vec![Substitution::SpecialChars].into_iter().collect(),
                    location: location.clone(),
                    kind: PassthroughKind::Double,
                });
                let new_content = format!("\u{FFFD}\u{FFFD}\u{FFFD}{}\u{FFFD}\u{FFFD}\u{FFFD}", state.pass_found_count.get());
                state.source_map.borrow_mut().add_offset(
                    location.absolute_start,
                    i32::try_from(new_content.len()).expect("failed to convert attribute value length to i32") - i32::try_from(location.absolute_end - location.absolute_start).expect("failed to convert attribute reference length to i32"),
                    ProcessedKind::Passthrough
                );

                state.pass_found_count.set(state.pass_found_count.get() + 1);
                new_content
            }

        rule triple_plus_passthrough() -> String
            = start:position() "+++" content:$((!"+++" [_])+) "+++" {
                let location = state.tracker.borrow_mut().calculate_location(start, content, 6);
                state.passthroughs.borrow_mut().push(Pass {
                    text: Some(content.to_string()),
                    substitutions: HashSet::new(),
                    location: location.clone(),
                    kind: PassthroughKind::Triple,
                });
                let new_content = format!("\u{FFFD}\u{FFFD}\u{FFFD}{}\u{FFFD}\u{FFFD}\u{FFFD}", state.pass_found_count.get());
                state.source_map.borrow_mut().add_offset(
                    location.absolute_start,
                    i32::try_from(new_content.len()).expect("failed to convert attribute value length to i32") - i32::try_from(location.absolute_end - location.absolute_start).expect("failed to convert attribute reference length to i32"),
                    ProcessedKind::Passthrough
                );

                state.pass_found_count.set(state.pass_found_count.get() + 1);
                new_content
            }

        rule pass_macro() -> String
            = start:position() "pass:" substitutions:substitutions() "[" content:$([^']']*) "]" end:position!() {
                let location = state.tracker.borrow_mut().calculate_location_from_start_end(start, end);
                let content = if substitutions.contains(&Substitution::Attributes) {
                    InlinePreprocessor::attribute_reference_substitutions(content, document_attributes, state).expect("failed to process attribute references inside pass macro")
                } else {
                    content.to_string()
                };
                state.passthroughs.borrow_mut().push(Pass {
                    text: Some(content.to_string()),
                    substitutions,
                    location: location.clone(),
                    kind: PassthroughKind::Macro,
                });
                let new_content = format!("\u{FFFD}\u{FFFD}\u{FFFD}{}\u{FFFD}\u{FFFD}\u{FFFD}", state.pass_found_count.get());
                state.source_map.borrow_mut().add_offset(
                    location.absolute_start,
                    i32::try_from(content.len()).expect("failed to convert passthrough macro content length to i32") - i32::try_from(location.absolute_end - location.absolute_start).expect("failed to convert attribute reference length to i32"),
                    ProcessedKind::Passthrough
                );
                state.pass_found_count.set(state.pass_found_count.get() + 1);
                new_content
            }

        rule substitutions() -> HashSet<Substitution>
            = subs:$(substitution_value() ** ",") {
                subs.split(",").map(|s| Substitution::from(s.trim())).collect()
            }

        rule substitution_value() -> &'input str
            = $(['a'..='z' | 'A'..='Z' | '0'..='9']+)

        rule unprocessed_text() -> String
            = text:$((!(passthrough_pattern() / attribute_reference_pattern()) [_])+) {
                state.tracker.borrow_mut().advance(text);
                text.to_string()
            }

        rule attribute_reference_pattern() = "{" attribute_name_pattern() "}"

        rule attribute_name_pattern() = ['a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_']+

        rule passthrough_pattern() =
            "+++" (!("+++") [_])+ "+++" /
            "++" (!("++") [_])+ "++" /
            "+" ("+" / [^'+']+) "+" /
            "pass:" substitutions()? "[" [^']']* "]"

        pub rule attribute_reference_substitutions() -> String
            = content:(attribute_reference_content() / unprocessed_text_content())+ {
                content.join("")
            }

        rule attribute_reference_content() -> String
            = "{" attribute_name:attribute_name() "}" {
                if let Some(value) = document_attributes.get(&attribute_name) {
                    match value {
                        AttributeValue::String(value) => {
                            value.to_string()
                        },
                        _ => {
                            // TODO(nlopes): do we need to handle other types?
                            // For non-string attributes, keep original text
                            format!("{{{attribute_name}}}")
                        }
                    }
                } else {
                    // TODO(nlopes): do we need to handle other types?
                    // For non-string attributes, keep original text
                    format!("{{{attribute_name}}}")
                }
            }

        rule unprocessed_text_content() -> String
            = text:$((!(passthrough_pattern() / attribute_reference_pattern()) [_])+) {
                text.to_string()
            }

        rule ANY() = [_]

        rule position() -> Position = { state.tracker.borrow().get_position() }
    }
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DocumentAttributes;

    fn setup_attributes() -> DocumentAttributes {
        let mut attributes = DocumentAttributes::default();
        attributes.insert(
            "s".to_string(),
            AttributeValue::String("link:/nonono".to_string()),
        );
        attributes.insert(
            "version".to_string(),
            AttributeValue::String("1.0".to_string()),
        );
        attributes.insert(
            "title".to_string(),
            AttributeValue::String("My Title".to_string()),
        );
        attributes
    }

    fn setup_state() -> ParserState {
        ParserState {
            pass_found_count: Cell::new(0),
            passthroughs: RefCell::new(Vec::new()),
            attributes: RefCell::new(HashMap::new()),
            tracker: RefCell::new(PositionTracker::new()),
            source_map: RefCell::new(SourceMap::default()),
        }
    }

    #[test]
    fn test_preprocess_inline_passthrough_single() {
        let attributes = setup_attributes();
        let state = setup_state();
        let input = "+hello+";
        let result = InlinePreprocessor::run(input, &attributes, &state).unwrap();
        assert_eq!(
            result.text,
            "\u{FFFD}\u{FFFD}\u{FFFD}0\u{FFFD}\u{FFFD}\u{FFFD}"
        );
        assert_eq!(state.pass_found_count.get(), 1);
        let passthroughs = state.passthroughs.into_inner();
        assert_eq!(passthroughs.len(), 1);
        assert_eq!(passthroughs[0].text, Some("hello".to_string()));
        assert_eq!(passthroughs[0].kind, PassthroughKind::Single);
    }

    #[test]
    fn test_preprocess_inline_passthrough_double() {
        let attributes = setup_attributes();
        let state = setup_state();
        let input = "++hello++";
        let result = InlinePreprocessor::run(input, &attributes, &state).unwrap();
        assert_eq!(
            result.text,
            "\u{FFFD}\u{FFFD}\u{FFFD}0\u{FFFD}\u{FFFD}\u{FFFD}"
        );
        assert_eq!(result.passthroughs.len(), 1);
        assert_eq!(result.passthroughs[0].text, Some("hello".to_string()));
        assert_eq!(result.passthroughs[0].kind, PassthroughKind::Double);
    }

    #[test]
    fn test_preprocess_inline_passthrough_triple() {
        let attributes = setup_attributes();
        let state = setup_state();
        let input = "+++hello+++";
        let result = InlinePreprocessor::run(input, &attributes, &state).unwrap();
        assert_eq!(
            result.text,
            "\u{FFFD}\u{FFFD}\u{FFFD}0\u{FFFD}\u{FFFD}\u{FFFD}"
        );
        assert_eq!(result.passthroughs.len(), 1);
        assert_eq!(result.passthroughs[0].text, Some("hello".to_string()));
        assert_eq!(result.passthroughs[0].kind, PassthroughKind::Triple);
    }

    #[test]
    fn test_preprocess_inline_passthrough_single_plus() {
        let attributes = setup_attributes();
        let state = setup_state();
        let input = "+hello+ world+";
        let result = InlinePreprocessor::run(input, &attributes, &state).unwrap();
        assert_eq!(
            result.text,
            "\u{FFFD}\u{FFFD}\u{FFFD}0\u{FFFD}\u{FFFD}\u{FFFD} world+"
        );
        assert_eq!(result.passthroughs.len(), 1);
        assert_eq!(result.passthroughs[0].text, Some("hello".to_string()));
        assert_eq!(result.passthroughs[0].kind, PassthroughKind::Single);
    }

    #[test]
    fn test_preprocess_inline_passthrough_multiple() {
        let attributes = setup_attributes();
        let state = setup_state();
        let input = "Something\n\nHere is some +*bold*+ text and ++**more bold**++ text.";
        //                 SomethingNNHere is some +*bold*+ text and ++**more bold**++ text.
        //                 0123456789012345678901234567890123456789012345678901234567890123456
        //                          1         2         3         4         5         6
        //                                         ^^^^^^^^          ^^^^^^^^^^^^^^^^^
        //                 Here is some +*bold*+ text and ++**more bold**++ text.
        //                 123456789012345678901234567890123456789012345678901234
        //                          1         2         3         4         5
        //                              ^^^^^^^^          ^^^^^^^^^^^^^^^^^
        let result = InlinePreprocessor::run(input, &attributes, &state).unwrap();

        // Verify processed text has placeholders
        assert_eq!(
            result.text,
            "Something\n\nHere is some \u{FFFD}\u{FFFD}\u{FFFD}0\u{FFFD}\u{FFFD}\u{FFFD} text and \u{FFFD}\u{FFFD}\u{FFFD}1\u{FFFD}\u{FFFD}\u{FFFD} text."
        );

        // Verify passthroughs were captured
        assert_eq!(result.passthroughs.len(), 2);

        // Check first passthrough
        assert_eq!(result.passthroughs[0].text.as_ref().unwrap(), "*bold*");
        assert_eq!(result.passthroughs[0].location.absolute_start, 24);
        assert_eq!(result.passthroughs[0].location.absolute_end, 32);
        assert_eq!(result.passthroughs[0].location.start.line, 3);
        assert_eq!(result.passthroughs[0].location.start.column, 14);
        assert_eq!(result.passthroughs[0].location.end.line, 3);
        assert_eq!(result.passthroughs[0].location.end.column, 22);

        // Check second passthrough
        assert_eq!(
            result.passthroughs[1].text.as_ref().unwrap(),
            "**more bold**"
        );

        assert_eq!(result.passthroughs[1].location.absolute_start, 42);
        assert_eq!(result.passthroughs[1].location.absolute_end, 59);
        assert_eq!(result.passthroughs[1].location.start.line, 3);
        assert_eq!(result.passthroughs[1].location.start.column, 32);
        assert_eq!(result.passthroughs[1].location.end.line, 3);
        assert_eq!(result.passthroughs[1].location.end.column, 49);
    }

    #[test]
    fn test_preprocess_attribute_in_link() {
        let attributes = setup_attributes();
        let state = setup_state();
        let input = "The {s}[syntax page] provides complete stuff.";

        let result = InlinePreprocessor::run(input, &attributes, &state).unwrap();

        assert_eq!(
            result.text,
            "The link:/nonono[syntax page] provides complete stuff."
        );

        // Check that source positions are mapped correctly
        // Original:  "The {s}[syntax page]..."
        //             01234567890123456789012
        // Processed: "The link:/nonono[syntax page]..."
        let pos = result.source_map.map_position(16); // Position after "{s}"
        assert_eq!(pos, 7); // Should map to end of "link:/nonono"
    }

    #[test]
    fn test_preprocess_inline_in_attributes() {
        let attributes = setup_attributes();
        let state = setup_state();

        // Test block title with attribute reference
        let input = "Version {version} of {title}";
        //                 0123456789012345678901234567
        //                 {version} -> 1.0 (-6 chars)
        //                 {title} -> My Title (+1 char)
        let result = InlinePreprocessor::run(input, &attributes, &state).unwrap();

        assert_eq!(result.text, "Version 1.0 of My Title");

        // Original:  "Version {version} of {title}"
        //             0123456789012345678901234567
        // Processed: "Version 1.0 of My Title"

        // Verify position mapping:
        // Position 8 in original (start of {version})
        // should map to position 8 in processed (start of "1.0")
        let pos = result.source_map.map_position(8);
        assert_eq!(pos, 8); // Same position since it's before any changes

        // Position 19 in original should map considering the change in length
        let pos = result.source_map.map_position(15);
        assert_eq!(pos, 21); // Maps to position in "of"
    }

    #[test]
    fn test_preprocess_complex_example() {
        let attributes = setup_attributes();
        let state = setup_state();

        // Complex example with attribute in link and passthrough
        let input = "Check the {s}[syntax page] and +this {s} won't expand+ for details.";
        //                 0123456789012345678901234
        //                           ^
        //                           {s} expands to link:/nonono (+9 chars)
        let result = InlinePreprocessor::run(input, &attributes, &state).unwrap();

        assert_eq!(
            result.text,
            "Check the link:/nonono[syntax page] and \u{FFFD}\u{FFFD}\u{FFFD}0\u{FFFD}\u{FFFD}\u{FFFD} for details."
        );

        // Verify passthrough was captured and preserved
        assert_eq!(result.passthroughs.len(), 1);
        assert_eq!(
            result.passthroughs[0].text.as_ref().unwrap(),
            "this {s} won't expand"
        );

        // Verify source mapping
        let pos = result.source_map.map_position(10); // Start of {s}
        assert_eq!(pos, 10); // Should map to start of "link:/nonono"
    }

    #[test]
    fn test_nested_passthrough_with_nested_attributes() {
        let state = setup_state();
        let mut attributes = setup_attributes();
        // Add nested attributes
        attributes.insert("nested1".into(), AttributeValue::String("{version}".into()));
        attributes.insert("nested2".into(), AttributeValue::String("{nested1}".into()));

        // Test passthrough containing attribute that references another attribute
        let input = "Here is a +special {nested2} value+ to test.";
        let result = InlinePreprocessor::run(input, &attributes, &state).unwrap();

        // Verify the passthrough preserved the unexpanded attribute
        assert_eq!(
            result.text,
            "Here is a \u{FFFD}\u{FFFD}\u{FFFD}0\u{FFFD}\u{FFFD}\u{FFFD} to test."
        );

        // Verify passthrough content preserved original text without expansion
        assert_eq!(result.passthroughs.len(), 1);
        assert_eq!(
            result.passthroughs[0].text.as_ref().unwrap(),
            "special {nested2} value"
        );

        // Verify source positions for debugging
        let start_pos = result.passthroughs[0].location.absolute_start;
        let end_pos = result.passthroughs[0].location.absolute_end;
        assert_eq!(start_pos, 10); // Start of passthrough content
        assert_eq!(end_pos, 35); // End of passthrough content
    }

    #[test]
    fn test_line_breaks() {
        let state = setup_state();
        let attributes = setup_attributes();

        let input = "This is a test +\nwith a line break.";
        //                 012345678901234567890123456789012345678
        //                 0         1         2         3         4
        let result = InlinePreprocessor::run(input, &attributes, &state).unwrap();
        assert_eq!(result.text, "This is a test +\nwith a line break.");

        // Verify no passthroughs were captured
        assert!(result.passthroughs.is_empty());
    }

    #[test]
    fn test_section_with_passthrough() {
        let attributes = setup_attributes();
        let state = setup_state();
        let input = "= Document Title\nHello +<h1>+World+</h1>+ of +<u>+Gemini+</u>+";
        //                 012345678901234567890123456789012345678901234567890123456789012
        //                 0         1         2         3         4         5         6
        let result = InlinePreprocessor::run(input, &attributes, &state).unwrap();
        assert_eq!(
            result.text,
            "= Document Title\nHello \u{FFFD}\u{FFFD}\u{FFFD}0\u{FFFD}\u{FFFD}\u{FFFD}World\u{FFFD}\u{FFFD}\u{FFFD}1\u{FFFD}\u{FFFD}\u{FFFD} of \u{FFFD}\u{FFFD}\u{FFFD}2\u{FFFD}\u{FFFD}\u{FFFD}Gemini\u{FFFD}\u{FFFD}\u{FFFD}3\u{FFFD}\u{FFFD}\u{FFFD}"
        );

        // Verify passthrough was captured
        assert_eq!(result.passthroughs.len(), 4);

        let first_pass = &result.passthroughs[0];
        let second_pass = &result.passthroughs[1];

        // Check passthrough content preserved original text
        assert_eq!(first_pass.text.as_ref().unwrap(), "<h1>");
        assert_eq!(second_pass.text.as_ref().unwrap(), "</h1>");

        // Verify substitutions were captured
        assert!(first_pass
            .substitutions
            .contains(&Substitution::SpecialChars));

        // Check positions
        assert_eq!(first_pass.location.absolute_start, 23); // Start of pass macro
        assert_eq!(first_pass.location.absolute_end, 29); // End of pass macro content including brackets

        // Verify substitutions were captured
        assert!(second_pass
            .substitutions
            .contains(&Substitution::SpecialChars));

        // Check positions
        assert_eq!(second_pass.location.absolute_start, 34); // Start of pass macro
        assert_eq!(second_pass.location.absolute_end, 41); // End of pass macro content including brackets
    }

    #[test]
    fn test_pass_macro_with_mixed_content() {
        let state = setup_state();
        let mut attributes = setup_attributes();
        // Add docname attribute
        attributes.insert("docname".into(), AttributeValue::String("test-doc".into()));

        let input = "The text pass:q,a[<u>underline _{docname}_</u>] is underlined.";
        //                 01234567890123456789012345678901234567890123456789012345678901
        //                 0         1         2         3         4         5         6
        //                          ^start of pass        ^docname
        //                "The text FFF0FFF is underlined."
        //                "the text <u>underline _test-doc_</u> is underlined."
        let result = InlinePreprocessor::run(input, &attributes, &state).unwrap();
        assert_eq!(
            result.text,
            "The text \u{FFFD}\u{FFFD}\u{FFFD}0\u{FFFD}\u{FFFD}\u{FFFD} is underlined."
        );

        // Verify passthrough was captured
        assert_eq!(result.passthroughs.len(), 1);

        let pass = &result.passthroughs[0];

        // Check passthrough content preserved original text
        assert_eq!(pass.text.as_ref().unwrap(), "<u>underline _test-doc_</u>");

        // Verify substitutions were captured
        assert!(pass.substitutions.contains(&Substitution::Quotes)); // 'q'
        assert!(pass.substitutions.contains(&Substitution::Attributes)); // 'a'

        // Check positions
        assert_eq!(pass.location.absolute_start, 9); // Start of pass macro
        assert_eq!(pass.location.absolute_end, 47); // End of pass macro content including brackets

        // Test position mapping
        let pos = result.source_map.map_position(9); // Start of pass macro
        assert_eq!(pos, 9); // Position unchanged before placeholder

        let pos = result.source_map.map_position(24); // After pass macro
        assert_eq!(pos, 35); // Position after placeholder
    }

    #[test]
    fn test_all_passthroughs_with_attribute() {
        let state = setup_state();
        let mut attributes = setup_attributes();
        attributes.insert("meh".into(), AttributeValue::String("1.0".into()));

        let input = "1 +2+, ++3++ {meh} and +++4+++ are all numbers.";
        //                 012345678901234567890123456789012345678901234567890123456789012345678901234567890123456
        //                 0         1         2         3         4         5         6         7         8
        //                 1 FFFFFFFFF0FFFFFFFFF, FFFFFFFFF1FFFFFFFFF 1.0 and FFFFFFFFF2FFFFFFFFF are all numbers.

        let result = InlinePreprocessor::run(input, &attributes, &state).unwrap();
        assert_eq!(
            result.text,
            "1 \u{FFFD}\u{FFFD}\u{FFFD}0\u{FFFD}\u{FFFD}\u{FFFD}, \u{FFFD}\u{FFFD}\u{FFFD}1\u{FFFD}\u{FFFD}\u{FFFD} 1.0 and \u{FFFD}\u{FFFD}\u{FFFD}2\u{FFFD}\u{FFFD}\u{FFFD} are all numbers."
        );

        assert_eq!(result.passthroughs.len(), 3);
        assert_eq!(result.passthroughs[0].text.as_ref().unwrap(), "2");
        assert_eq!(result.passthroughs[1].text.as_ref().unwrap(), "3");
        assert_eq!(result.passthroughs[2].text.as_ref().unwrap(), "4");

        dbg!(&result);

        assert_eq!(result.source_map.map_position(3), 3);
        assert_eq!(result.source_map.map_position(24), 3);
    }
}
