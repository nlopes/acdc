use std::collections::{HashMap, HashSet};

use pest::Parser;
use pest_derive::Parser;
use tracing::instrument;

use crate::{
    AttributeValue, DocumentAttributes, Error, Location, Pass, PassthroughKind, Position,
    Substitution,
};

#[derive(Parser)]
#[grammar = "../grammar/inline_preprocessor.pest"]
struct InlinePreprocessorParser;

#[derive(Debug)]
pub(crate) struct InlinePreprocessor {
    attributes: DocumentAttributes,
    source_map: SourceMap,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct SourceMap {
    pub(crate) offsets: Vec<(usize, i32, ProcessedKind)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProcessedKind {
    Attribute,
    Passthrough,
}

impl SourceMap {
    fn add_offset(&mut self, position: usize, offset: i32, kind: ProcessedKind) {
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

#[derive(Debug)]
pub(crate) struct ProcessedContent {
    pub text: String,
    pub passthroughs: Vec<Pass>,
    pub attributes: HashMap<usize, Location>,
    pub(crate) source_map: SourceMap,
}

impl InlinePreprocessor {
    pub(crate) fn new(attributes: DocumentAttributes) -> Self {
        Self {
            attributes,
            source_map: SourceMap::default(),
        }
    }

    /// Process a span of text before inline parsing
    #[instrument(skip(text))]
    pub(crate) fn process(
        &mut self,
        text: &str,
        start_position: usize,
    ) -> Result<ProcessedContent, Error> {
        let mut result = String::with_capacity(text.len());
        let mut passthroughs = Vec::new();
        let mut attributes = HashMap::new();

        let pairs = InlinePreprocessorParser::parse(Rule::preprocessed_text, text)
            .map_err(|e| Error::Parse(format!("Invalid inline text: {e}")))?;
        let mut pass_found_count = 0;
        for pair in pairs.flatten() {
            match pair.as_rule() {
                Rule::attr_ref => {
                    let attr_name = pair.clone().into_inner().next().unwrap().as_str();
                    if let Some(value) = self.attributes.get(attr_name) {
                        match value {
                            AttributeValue::String(s) => {
                                let attr_span = pair.as_span();
                                self.source_map.add_offset(
                                    start_position + attr_span.start(),
                                    i32::try_from(s.len()).unwrap_or_default()
                                        - i32::try_from(attr_span.as_str().len())
                                            .unwrap_or_default(),
                                    ProcessedKind::Attribute,
                                );
                                result.push_str(s);
                                attributes.insert(
                                    self.source_map.offsets.len(),
                                    Location::from_pair(&pair),
                                );
                            }
                            _ => {
                                // TODO(nlopes): do we need to handle other types?
                                // For non-string attributes, keep original text
                                result.push_str(pair.as_str());
                            }
                        }
                    } else {
                        // Keep unresolved references as-is
                        result.push_str(pair.as_str());
                    }
                }
                Rule::single_plus_passthrough
                | Rule::double_plus_passthrough
                | Rule::triple_plus_passthrough
                | Rule::pass_macro => {
                    let span = pair.as_span();
                    let pass = self.create_passthrough(pair, start_position)?;
                    let len = span.end_pos().line_col().1 - span.start_pos().line_col().1;

                    // Insert placeholder
                    result.push_str(&format!(
                        "\u{FFFD}\u{FFFD}\u{FFFD}{pass_found_count}\u{FFFD}\u{FFFD}\u{FFFD}"
                    ));

                    self.source_map.add_offset(
                        start_position + span.start(),
                        0 - i32::try_from(len).unwrap_or(0) + 1, // +1 here to account for the placeholder
                        ProcessedKind::Passthrough,
                    );

                    pass_found_count += 1;
                    passthroughs.push(pass);
                }
                Rule::unprocessed_text => {
                    result.push_str(pair.as_str());
                }
                _ => {}
            }
        }

        Ok(ProcessedContent {
            text: result,
            passthroughs,
            attributes,
            source_map: self.source_map.clone(),
        })
    }

    /// Create a Pass instance from a passthrough rule match
    fn create_passthrough(
        &mut self,
        pair: pest::iterators::Pair<Rule>,
        start_position: usize,
    ) -> Result<Pass, Error> {
        let mut substitutions = HashSet::new();
        let span = pair.as_span();
        let span_start = span.start_pos().line_col();
        let span_end = span.end_pos().line_col();

        let rule = pair.as_rule();

        match rule {
            Rule::single_plus_passthrough
            | Rule::double_plus_passthrough
            | Rule::triple_plus_passthrough => {
                let (content, marker_size) = match rule {
                    Rule::single_plus_passthrough => {
                        (&pair.as_str()[1..pair.as_str().len() - 1], 1)
                    }
                    Rule::double_plus_passthrough => {
                        (&pair.as_str()[2..pair.as_str().len() - 2], 2)
                    }
                    Rule::triple_plus_passthrough => {
                        (&pair.as_str()[3..pair.as_str().len() - 3], 3)
                    }
                    _ => unreachable!(),
                };
                let location = Location {
                    absolute_start: span.start_pos().pos() + start_position,
                    absolute_end: span.end_pos().pos() + start_position,
                    start: Position {
                        line: span_start.0,
                        column: span_start.1,
                    },
                    end: Position {
                        line: span_end.0,
                        column: span_end.1,
                    },
                };
                Ok(Pass {
                    text: Some(content.to_string()),
                    kind: match marker_size {
                        1 => PassthroughKind::Single,
                        2 => PassthroughKind::Double,
                        3 => PassthroughKind::Triple,
                        _ => unreachable!(),
                    },
                    // We add SpecialChars here for single and double but we don't do
                    // anything with them, only the converter does.
                    substitutions: if marker_size < 3 {
                        vec![Substitution::SpecialChars].into_iter().collect()
                    } else {
                        HashSet::new()
                    },
                    location,
                })
            }
            Rule::pass_macro => {
                let mut inner = pair.into_inner();
                let subs: Option<Vec<Substitution>> = inner.next().map(|p| {
                    p.as_str()
                        .split(',')
                        .map(|s| Substitution::from(s.trim()))
                        .collect()
                });
                let text = if let Some(content_pair) = inner.next() {
                    let content = content_pair.as_str().to_string();
                    if let Some(subs) = &subs {
                        if subs.contains(&Substitution::Attributes) {
                            // Process any attribute references in the content
                            let processed = self.process(
                                &content,
                                content_pair.as_span().start() + start_position,
                            )?;
                            Some(processed.text)
                        } else {
                            Some(content)
                        }
                    } else {
                        return Err(Error::Parse("Pass macro content missing".to_string()));
                    }
                } else {
                    None
                };
                substitutions.extend(subs.unwrap_or_default());
                let location = Location {
                    absolute_start: span.start_pos().pos() + start_position,
                    absolute_end: span.end_pos().pos() + start_position,
                    start: Position {
                        line: span_start.0,
                        column: span_start.1,
                    },
                    end: Position {
                        line: span_end.0,
                        column: span_end.1,
                    },
                };

                Ok(Pass {
                    text,
                    substitutions,
                    location,
                    kind: PassthroughKind::Macro,
                })
            }
            _ => Err(Error::Parse("Invalid passthrough type".to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DocumentAttributes;

    fn setup_attributes() -> DocumentAttributes {
        let mut attrs = DocumentAttributes::default();
        attrs.insert("s".into(), AttributeValue::String("link:/nonono".into()));
        attrs.insert("version".into(), AttributeValue::String("1.0".into()));
        attrs.insert("title".into(), AttributeValue::String("My Title".into()));
        attrs
    }

    #[test]
    fn test_preprocess_attribute_in_link() {
        let attrs = setup_attributes();
        let mut preprocessor = InlinePreprocessor::new(attrs);

        let input = "The {s}[syntax page] provides complete stuff.";
        let start_pos = 0;

        let result = preprocessor.process(input, start_pos).unwrap();

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
    fn test_preprocess_inline_passthrough() {
        let attrs = setup_attributes();
        let mut preprocessor = InlinePreprocessor::new(attrs);

        let input = "Something\n\nHere is some +*bold*+ text and ++**more bold**++ text.";
        //                 0123456789012345678901234567890123456789012345678901234567890123456
        //                                           ^^^^^^^^          ^^^^^^^^^^^^^^^^^
        let start_pos = 0;

        let result = preprocessor.process(input, start_pos).unwrap();

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

        // Check second passthrough
        assert_eq!(
            result.passthroughs[1].text.as_ref().unwrap(),
            "**more bold**"
        );

        assert_eq!(result.passthroughs[1].location.absolute_start, 42);
        assert_eq!(result.passthroughs[1].location.absolute_end, 59);
    }

    #[test]
    fn test_preprocess_inline_in_attributes() {
        let attrs = setup_attributes();
        let mut preprocessor = InlinePreprocessor::new(attrs);

        // Test block title with attribute reference
        let input = "Version {version} of {title}";
        //                 0123456789012345678901234567
        //                 {version} -> 1.0 (-6 chars)
        //                 {title} -> My Title (+1 char)
        let start_pos = 0;
        let result = preprocessor.process(input, start_pos).unwrap();

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

        // Test nested attribute reference
        let input = ".{title} v{version}";
        let result = preprocessor.process(input, 0).unwrap();
        assert_eq!(result.text, ".My Title v1.0");
    }

    #[test]
    fn test_preprocess_complex_example() {
        let attrs = setup_attributes();
        let mut preprocessor = InlinePreprocessor::new(attrs.clone());

        // Complex example with attribute in link and passthrough
        let input = "Check the {s}[syntax page] and +this {s} won't expand+ for details.";
        //                 0123456789012345678901234
        //                           ^
        //                           {s} expands to link:/nonono (+9 chars)
        let result = preprocessor.process(input, 0).unwrap();

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
        let mut attrs = setup_attributes();
        // Add nested attributes
        attrs.insert("nested1".into(), AttributeValue::String("{version}".into()));
        attrs.insert("nested2".into(), AttributeValue::String("{nested1}".into()));

        let mut preprocessor = InlinePreprocessor::new(attrs);

        // Test passthrough containing attribute that references another attribute
        let input = "Here is a +special {nested2} value+ to test.";
        let result = preprocessor.process(input, 0).unwrap();

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
    fn test_pass_macro_with_mixed_content() {
        let mut attrs = setup_attributes();
        // Add docname attribute
        attrs.insert("docname".into(), AttributeValue::String("test-doc".into()));

        let mut preprocessor = InlinePreprocessor::new(attrs);
        let input = "The text pass:q,a[<u>underline _{docname}_</u>] is underlined.";
        //                 01234567890123456789012345678901234567890123456789012345678901
        //                          ^start of pass        ^docname
        //                "The text F is underlined."
        let result = preprocessor.process(input, 0).unwrap();

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
        assert_eq!(pos, 61); // Position after placeholder
    }

    #[test]
    fn test_line_breaks() {
        let attrs = setup_attributes();
        let mut preprocessor = InlinePreprocessor::new(attrs);

        let input = "This is a test +\nwith a line break.";
        //                 012345678901234567890123456789012345678
        //                 0         1         2         3         4
        let start_pos = 0;

        let result = preprocessor.process(input, start_pos).unwrap();

        assert_eq!(result.text, "This is a test +\nwith a line break.");

        // Verify no passthroughs were captured
        assert!(result.passthroughs.is_empty());
    }

    #[test]
    fn test_section_with_passthrough() {
        let attrs = setup_attributes();
        let mut preprocessor = InlinePreprocessor::new(attrs);

        let input = "= Document Title\nHello +<h1>+World+</h1>+ of +<u>+Gemini+</u>+";
        //                 012345678901234567890123456789012345678901234567890123456789012
        //                 0         1         2         3         4         5         6
        let start_pos = 0;

        let result = preprocessor.process(input, start_pos).unwrap();

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
}
