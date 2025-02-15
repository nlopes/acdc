use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
};

use peg::parser;

use crate::{
    grammar::PositionTracker, AttributeValue, DocumentAttributes, Location, Pass, PassthroughKind,
    Position, Substitution,
};

// The parser state for the inline preprocessor.
//
// It should use internal mutability to allow the parser to modify the state.
#[derive(Debug)]
pub(crate) struct InlinePreprocessorParserState {
    pub(crate) pass_found_count: Cell<usize>,
    pub(crate) passthroughs: RefCell<Vec<Pass>>,
    pub(crate) attributes: RefCell<HashMap<usize, Location>>,
    pub(crate) tracker: RefCell<PositionTracker>,
    pub(crate) source_map: RefCell<SourceMap>,
}

impl InlinePreprocessorParserState {
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
    pub(crate) fn map_position(&self, pos: usize) -> usize {
        let signed_pos = i32::try_from(pos).expect("could not convert pos to i32");

        // The adjustment is the total number of characters removed/added during preprocessing.
        //
        // For example, if we have a passthrough like `+a+`: the original text is 3 characters long,
        // but the processed text is 7 characters long (FFF0FFF). So the adjustment is 7 - 3 = 4.
        let mut adjustment: i32 = 0;

        for rep in &self.replacements {
            let rep_absolute_start = i32::try_from(rep.absolute_start)
                .expect("could not convert rep.absolute_start to i32");
            let rep_absolute_end =
                i32::try_from(rep.absolute_end).expect("could not convert rep.absolute_end to i32");
            let rep_processed_end = i32::try_from(rep.processed_end)
                .expect("could not convert rep.processed_end to i32");

            // If our position is less than or equal to the start of the replacement, then
            // we can break and return whatever is in adjusted (which is likely to be pos).
            if signed_pos <= rep_absolute_start {
                break;
            }

            // If this matches, we're within a passthrough, attribute or pass macro.
            //
            // This usually means we can simply return because it's "easy" to calculate
            // the position.
            if signed_pos < rep_processed_end {
                // pos is within this replacement.
                match rep.kind {
                    ProcessedKind::Attribute => {
                        // All inserted characters map to the left-most original character.
                        return rep.absolute_start;
                    }
                    ProcessedKind::Passthrough => {
                        if signed_pos >= rep_absolute_end {
                            // if we're here, it means we need to return the end of the
                            // passthrough (our position, even though within a passthrough
                            // is at a position *after* the original end of the
                            // passthrough
                            return rep.absolute_end - 1;
                        }

                        // If we're here, it means we're within the passthrough and it's
                        // safe to just subtract the adjustment.
                        return usize::try_from(signed_pos - adjustment)
                            .expect("could not convert back to usize within passthrough");
                    }
                }
            }

            // adjust the total of characters removed/added during preprocessing.
            adjustment += rep_processed_end - rep_absolute_end;
        }

        // If we're here, it means we're not within any replacement, so we can just
        // subtract the adjustment.
        usize::try_from(signed_pos - adjustment)
            .expect("could not convert back to usize post all replacements")
    }
}

parser!(
    pub(crate) grammar inline_preprocessing(document_attributes: &DocumentAttributes, state: &InlinePreprocessorParserState) for str {
        pub rule run() -> ProcessedContent
            = content:inlines()+ {
                ProcessedContent {
                    text: content.join(""),
                    passthroughs: state.passthroughs.borrow().clone(),
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
                match document_attributes.get(attribute_name) {
                    Some(AttributeValue::String(value)) => {
                        state.source_map.borrow_mut().add_replacement(
                            location.absolute_start,
                            location.absolute_end,
                            value.chars().count(),
                            ProcessedKind::Attribute,
                        );
                        attributes.insert(state.source_map.borrow().replacements.len(), location);
                        value.to_string()
                    },
                    _ => {
                        // TODO(nlopes): do we need to handle other types?
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
                let location = state.tracker.borrow_mut().calculate_location(start, content, 6);
                state.passthroughs.borrow_mut().push(Pass {
                    text: Some(content.to_string()),
                    substitutions: HashSet::new(),
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
            = start:position() "pass:" substitutions:substitutions() "[" content:$([^']']*) "]" end:position!() {
                let location = state.tracker.borrow_mut().calculate_location_from_start_end(start, end);
                let content = if substitutions.contains(&Substitution::Attributes) {
                    inline_preprocessing::attribute_reference_substitutions(content, document_attributes, state).expect("failed to process attribute references inside pass macro")
                } else {
                    content.to_string()
                };
                state.passthroughs.borrow_mut().push(Pass {
                    text: Some(content.to_string()),
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

        rule substitutions() -> HashSet<Substitution>
            = subs:$(substitution_value() ** ",") {
                subs.split(',').map(|s| Substitution::from(s.trim())).collect()
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
                match document_attributes.get(attribute_name) {
                    Some(AttributeValue::String(value)) => value.to_string(),
                        // TODO(nlopes): do we need to handle other types?
                        // For non-string attributes, keep original text
                    _ => format!("{{{attribute_name}}}"),
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

    fn setup_state() -> InlinePreprocessorParserState {
        InlinePreprocessorParserState {
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
        let result = inline_preprocessing::run(input, &attributes, &state).unwrap();
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
        let result = inline_preprocessing::run(input, &attributes, &state).unwrap();
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
        let result = inline_preprocessing::run(input, &attributes, &state).unwrap();
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
        let result = inline_preprocessing::run(input, &attributes, &state).unwrap();
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
        let result = inline_preprocessing::run(input, &attributes, &state).unwrap();

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

        let result = inline_preprocessing::run(input, &attributes, &state).unwrap();

        assert_eq!(
            result.text,
            "The link:/nonono[syntax page] provides complete stuff."
        );

        // Check that source positions are mapped correctly
        // Original:  "The {s}[syntax page] provides complete stuff."
        //             012345678901234567890123456789012345678901234567890123
        // Processed: "The link:/nonono[syntax page] provides complete stuff."
        assert_eq!(result.source_map.map_position(15), 4); // This is still within the attribute so map it to the beginning.
        assert_eq!(result.source_map.map_position(16), 7); // This is after the attribute so map it to where it should be.
        assert_eq!(result.source_map.map_position(30), 21); // This is the `p` from `provides`.
    }

    #[test]
    fn test_preprocess_inline_in_attributes() {
        let attributes = setup_attributes();
        let state = setup_state();

        // Test block title with attribute reference
        let input = "Version {version} of {title}";
        //                 0123456789012345678901234567
        //                 Version 1.0 of My Title
        //                 {version} -> 1.0 (-6 chars)
        //                 {title} -> My Title (+1 char)
        let result = inline_preprocessing::run(input, &attributes, &state).unwrap();

        assert_eq!(result.text, "Version 1.0 of My Title");

        // Original:  "Version {version} of {title}"
        //             0123456789012345678901234567
        // Processed: "Version 1.0 of My Title"

        // Position 8 in original (start of {version}) should map to position 8 in
        // processed (start of "1.0")
        assert_eq!(result.source_map.map_position(8), 8);
        assert_eq!(result.source_map.map_position(15), 21);
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
        let result = inline_preprocessing::run(input, &attributes, &state).unwrap();

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
        let result = inline_preprocessing::run(input, &attributes, &state).unwrap();

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
        let result = inline_preprocessing::run(input, &attributes, &state).unwrap();
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
        let result = inline_preprocessing::run(input, &attributes, &state).unwrap();
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
        let result = inline_preprocessing::run(input, &attributes, &state).unwrap();
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

        assert_eq!(result.source_map.map_position(9), 9); // Start of pass macro
        assert_eq!(result.source_map.map_position(24), 55);
    }

    #[test]
    fn test_all_passthroughs_with_attribute() {
        let state = setup_state();
        let mut attributes = setup_attributes();
        attributes.insert("meh".into(), AttributeValue::String("1.0".into()));

        let input = "1 +2+, ++3++ {meh} and +++4+++ are all numbers.";
        //                 012345678901234567890123456789012345678901234567890123456789012345678901234567890123456
        //                 0         1         2         3         4         5         6         7         8
        //                 1 FFF0FFF, FFF1FFF 1.0 and FFF2FFF are all numbers.

        let result = inline_preprocessing::run(input, &attributes, &state).unwrap();
        assert_eq!(
            result.text,
            "1 \u{FFFD}\u{FFFD}\u{FFFD}0\u{FFFD}\u{FFFD}\u{FFFD}, \u{FFFD}\u{FFFD}\u{FFFD}1\u{FFFD}\u{FFFD}\u{FFFD} 1.0 and \u{FFFD}\u{FFFD}\u{FFFD}2\u{FFFD}\u{FFFD}\u{FFFD} are all numbers."
        );

        assert_eq!(result.passthroughs.len(), 3);
        assert_eq!(result.passthroughs[0].text.as_ref().unwrap(), "2");
        assert_eq!(result.passthroughs[1].text.as_ref().unwrap(), "3");
        assert_eq!(result.passthroughs[2].text.as_ref().unwrap(), "4");

        assert_eq!(result.source_map.map_position(2), 2);
        // 5 is the 0 within FFF0FFF, which corresponds to the +2+ macro: I believe it should map to the end of the macro.
        assert_eq!(result.source_map.map_position(5), 4);
        // 24 is the FFF in passthrough 2, therefore it should map to position 10
        assert_eq!(result.source_map.map_position(24), 20);
        // 48 is the n in "and", therefore it should map to position 19
        assert_eq!(result.source_map.map_position(48), 44);
    }
}
