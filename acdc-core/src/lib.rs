use std::collections::HashMap;

use serde::{
    ser::{SerializeMap, SerializeSeq, Serializer},
    Deserialize, Serialize,
};

mod config;
pub use config::*;

#[derive(Debug, Default, PartialEq, Deserialize)]
pub struct DocumentAttributes(
    #[serde(skip_serializing_if = "HashMap::is_empty")] HashMap<AttributeName, AttributeValue>,
);

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Toc {
    Auto,
    Left,
    Right,
    Macro,
    Preamble,
}

impl Serialize for DocumentAttributes {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // We serialize the attributes as a sequence of key-value pairs.
        let mut state = serializer.serialize_map(Some(self.0.len()))?;
        for (key, value) in &self.0 {
            if key == "toc" && value == &AttributeValue::Bool(true) {
                state.serialize_entry(key, &AttributeValue::String(String::new()))?;
                continue;
            }
            state.serialize_entry(key, value)?;
        }
        state.end()
    }
}

impl DocumentAttributes {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn insert(&mut self, name: AttributeName, value: AttributeValue) {
        self.0.insert(name, value);
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&AttributeValue> {
        self.0.get(name)
    }

    #[must_use]
    pub fn contains_key(&self, name: &str) -> bool {
        self.0.contains_key(name)
    }
}

/// An `AttributeName` represents the name of an attribute in a document.
pub type AttributeName = String;

/// An `AttributeValue` represents the value of an attribute in a document.
///
/// An attribute value can be a string or a boolean.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AttributeValue {
    /// A string attribute value.
    String(String),
    /// A boolean attribute value. `false` means it is unset.
    Bool(bool),
}

/// A `Location` represents a location in a document.
#[derive(Debug, Default, Clone, Hash, Eq, PartialEq, Deserialize)]
pub struct Location {
    /// The start position of the location.
    pub start: Position,
    /// The end position of the location.
    pub end: Position,
}

impl Location {
    #[must_use]
    pub fn from_pair<R: pest::RuleType>(pair: &pest::iterators::Pair<R>) -> Self {
        let mut location = Location::default();
        let start = pair.as_span().start_pos();
        let end = pair.as_span().end_pos();
        location.set_start_from_pos(&start);
        location.set_end_from_pos(&end);
        location
    }

    pub fn set_start_from_pos(&mut self, start: &pest::Position) {
        let (line, column) = start.line_col();
        self.start.line = line;
        self.start.column = column;
    }

    pub fn set_end_from_pos(&mut self, end: &pest::Position) {
        let (line, column) = end.line_col();
        self.end.line = line;
        self.end.column = column - 1;
    }

    pub fn shift(&mut self, parent: Option<&Location>) {
        if let Some(parent) = parent {
            if parent.start.line == 0 {
                return;
            }
            self.start.line += parent.start.line;
            self.end.line += parent.start.line;
        }
    }

    /// Shifts the location inline. We subtract 1 from the line number of the start and
    /// end to account for the fact that inlines are always in the same line as the
    /// parent calling the parsing function.
    pub fn shift_inline(&mut self, parent: Option<&Location>) {
        if let Some(parent) = parent {
            if parent.start.line != 0 {
                self.start.line += parent.start.line - 1;
                self.end.line += parent.start.line - 1;
            }
            if parent.start.column != 0 {
                self.start.column += parent.start.column - 1;
                self.end.column += parent.start.column - 1;
            }
        }
    }

    pub fn shift_start(&mut self, parent: Option<&Location>) {
        if let Some(parent) = parent {
            if parent.start.line == 0 {
                return;
            }
            self.start.line += parent.start.line - 1;
        }
    }

    pub fn shift_end(&mut self, parent: Option<&Location>) {
        if let Some(parent) = parent {
            if parent.start.line == 0 {
                return;
            }
            self.end.line += parent.start.line - 1;
        }
    }

    pub fn shift_line_column(&mut self, line: usize, column: usize) {
        self.start.line += line - 1;
        self.end.line += line - 1;
        self.start.column += column - 1;
        self.end.column += column - 1;
    }
}

// We need to implement `Serialize` because I prefer our current `Location` struct to the
// `asciidoc` `ASG` definition.
//
// We serialize `Location` into the ASG format, which is a sequence of two elements: the
// start and end positions as an array.
impl Serialize for Location {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_seq(Some(2))?;
        state.serialize_element(&self.start)?;
        state.serialize_element(&self.end)?;
        state.end()
    }
}

/// A `Position` represents a position in a document.
#[derive(Debug, Default, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct Position {
    /// The line number of the position.
    pub line: usize,
    /// The column number of the position.
    #[serde(rename = "col")]
    pub column: usize,
}

/// A `Substitution` represents a substitution in a passthrough macro.
#[derive(Clone, Debug, Hash, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Substitution {
    SpecialChars,
    Attributes,
    Replacements,
    Macros,
    PostReplacements,
    Normal,
    Verbatim,
    Quotes,
    Callouts,
}

impl From<&str> for Substitution {
    fn from(value: &str) -> Self {
        match value {
            "specialchars" | "c" => Substitution::SpecialChars,
            "attributes" | "a" => Substitution::Attributes,
            "replacements" | "r" => Substitution::Replacements,
            "macros" | "m" => Substitution::Macros,
            "post_replacements" | "p" => Substitution::PostReplacements,
            "normal" | "n" => Substitution::Normal,
            "verbatim" | "v" => Substitution::Verbatim,
            "quotes" | "q" => Substitution::Quotes,
            "callouts" => Substitution::Callouts,
            unknown => unimplemented!("{unknown:?}"),
        }
    }
}

pub const BASIC: &[Substitution] = &[Substitution::SpecialChars];
pub const HEADER: &[Substitution] = &[Substitution::SpecialChars, Substitution::Attributes];
pub const NORMAL: &[Substitution] = &[
    Substitution::SpecialChars,
    Substitution::Attributes,
    Substitution::Quotes,
    Substitution::Replacements,
    Substitution::Macros,
    Substitution::PostReplacements,
];
pub const REFTEXT: &[Substitution] = &[
    Substitution::SpecialChars,
    Substitution::Quotes,
    Substitution::Replacements,
];
pub const VERBATIM: &[Substitution] = &[Substitution::SpecialChars, Substitution::Callouts];

impl Substitute for &str {}
impl Substitute for String {}

pub trait Substitute: ToString {
    fn substitute(
        &self,
        substitutions: &[Substitution],
        attributes: &DocumentAttributes,
    ) -> String {
        let mut text = self.to_string();
        for substitution in substitutions {
            match substitution {
                Substitution::SpecialChars => {
                    text = Self::substitute_special_chars(&text);
                }
                Substitution::Attributes => {
                    // TODO(nlopes): this check is probably not needed and doesn't
                    // actually change performance at all
                    if text.contains('{') {
                        text = Self::substitute_attributes(&text, attributes);
                    }
                }
                Substitution::Quotes => {
                    text = Self::substitute_quotes(&text);
                }
                Substitution::Replacements => {
                    text = Self::substitute_replacements(&text);
                }
                Substitution::Macros => {
                    text = Self::substitute_macros(&text);
                }
                Substitution::PostReplacements => {
                    text = Self::substitute_post_replacements(&text);
                }
                Substitution::Callouts => {
                    text = Self::substitute_callouts(&text);
                }

                // TODO(nlopes): for the two below, should this be how I do it? 🤔
                Substitution::Normal => {
                    self.substitute(NORMAL, attributes);
                }
                Substitution::Verbatim => {
                    self.substitute(VERBATIM, attributes);
                }
            }
        }
        text
    }

    #[must_use]
    fn substitute_special_chars(text: &str) -> String {
        text.to_string()
    }

    /**
    Given a text and a set of attributes, resolve the attribute references in the text.

    The attribute references are in the form of {name}.
     */
    #[must_use]
    fn substitute_attributes(text: &str, attributes: &DocumentAttributes) -> String {
        let mut result = String::with_capacity(text.len());
        let mut i: usize = 0;

        while i < text.len() {
            if text[i..].starts_with('{') {
                if let Some(end_brace) = text[i + 1..].find('}') {
                    let attr_name = &text[i + 1..i + 1 + end_brace];
                    match attributes.get(attr_name) {
                        Some(AttributeValue::Bool(true)) => {
                            result.push_str("");
                        }
                        Some(AttributeValue::String(attr_value)) => {
                            result.push_str(attr_value);
                        }
                        _ => {
                            // If the attribute is not found, we return the attribute reference as is.
                            result.push('{');
                            result.push_str(attr_name);
                            result.push('}');
                        }
                    }
                    i += end_brace + 2;
                } else {
                    result.push_str(&text[i..=i]);
                    i += 1;
                }
            } else {
                result.push_str(&text[i..=i]);
                i += 1;
            }
        }

        result
    }

    #[must_use]
    fn substitute_quotes(text: &str) -> String {
        text.to_string()
    }

    #[must_use]
    fn substitute_replacements(text: &str) -> String {
        text.to_string()
    }

    #[must_use]
    fn substitute_macros(text: &str) -> String {
        text.to_string()
    }

    #[must_use]
    fn substitute_post_replacements(text: &str) -> String {
        text.to_string()
    }

    #[must_use]
    fn substitute_callouts(text: &str) -> String {
        text.to_string()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn test_resolve_attribute_references() {
        // These two are attributes we add to the attributes map.
        let attribute_weight = AttributeValue::String(String::from("weight"));
        let attribute_mass = AttributeValue::String(String::from("mass"));

        // This one is an attribute we do NOT add to the attributes map so it can never be
        // resolved.
        let attribute_volume_repeat = String::from("value {attribute_volume}");

        let mut attributes = DocumentAttributes(HashMap::new());
        attributes.insert("weight".to_string(), attribute_weight.clone());
        attributes.insert("mass".to_string(), attribute_mass.clone());

        // Resolve an attribute that is in the attributes map.
        let value = "{weight}";
        let resolved = value.substitute(HEADER, &attributes);
        assert_eq!(resolved, "weight".to_string());

        // Resolve two attributes that are in the attributes map.
        let value = "{weight} {mass}";
        let resolved = value.substitute(HEADER, &attributes);
        assert_eq!(resolved, "weight mass".to_string());

        // Resolve without attributes in the map
        let value = "value {attribute_volume}";
        let resolved = value.substitute(HEADER, &attributes);
        assert_eq!(resolved, attribute_volume_repeat);
    }
}
