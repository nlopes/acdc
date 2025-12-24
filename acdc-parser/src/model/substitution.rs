use serde::{Deserialize, Serialize};

use crate::{AttributeValue, DocumentAttributes};

/// A `Substitution` represents a substitution in a passthrough macro.
#[derive(Clone, Debug, Hash, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
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
            "attributes" | "a" => Substitution::Attributes,
            "replacements" | "r" => Substitution::Replacements,
            "macros" | "m" => Substitution::Macros,
            "post_replacements" | "p" => Substitution::PostReplacements,
            "normal" | "n" => Substitution::Normal,
            "verbatim" | "v" => Substitution::Verbatim,
            "quotes" | "q" => Substitution::Quotes,
            "callouts" => Substitution::Callouts,
            "specialchars" | "c" | "" => Substitution::SpecialChars, // Empty substitution list defaults to special chars
            unknown => {
                tracing::warn!(substitution = %unknown, "unknown substitution type, using SpecialChars as default");
                Substitution::SpecialChars
            }
        }
    }
}

#[allow(dead_code)]
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
#[allow(dead_code)]
pub const REFTEXT: &[Substitution] = &[
    Substitution::SpecialChars,
    Substitution::Quotes,
    Substitution::Replacements,
];
pub const VERBATIM: &[Substitution] = &[Substitution::SpecialChars, Substitution::Callouts];

impl Substitute for &str {}
impl Substitute for String {}

pub(crate) trait Substitute: ToString {
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
                    text = Self::substitute_attributes(&text, attributes);
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
                // For the two below, should this be how I do it? 🤔 Not sure.
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
        let mut chars = text.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '{' {
                // Collect characters until we find '}'
                let mut attr_name = String::new();
                let mut found_closing_brace = false;

                while let Some(&next_ch) = chars.peek() {
                    if next_ch == '}' {
                        chars.next(); // consume the '}'
                        found_closing_brace = true;
                        break;
                    }
                    attr_name.push(next_ch);
                    chars.next();
                }

                if found_closing_brace {
                    match attributes.get(&attr_name) {
                        Some(AttributeValue::Bool(true)) => {
                            // Don't add anything for true boolean attributes
                        }
                        Some(AttributeValue::String(attr_value)) => {
                            result.push_str(attr_value);
                        }
                        _ => {
                            // If the attribute is not found, we return the attribute reference as is.
                            result.push('{');
                            result.push_str(&attr_name);
                            result.push('}');
                        }
                    }
                } else {
                    // No closing brace found, push the opening brace and the collected chars
                    result.push('{');
                    result.push_str(&attr_name);
                }
            } else {
                result.push(ch);
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
    use super::*;

    #[test]
    fn test_resolve_attribute_references() {
        // These two are attributes we add to the attributes map.
        let attribute_weight = AttributeValue::String(String::from("weight"));
        let attribute_mass = AttributeValue::String(String::from("mass"));

        // This one is an attribute we do NOT add to the attributes map so it can never be
        // resolved.
        let attribute_volume_repeat = String::from("value {attribute_volume}");

        let mut attributes = DocumentAttributes::default();
        attributes.insert("weight".into(), attribute_weight.clone());
        attributes.insert("mass".into(), attribute_mass.clone());

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

    #[test]
    fn test_utf8_boundary_handling() {
        // Regression test for fuzzer-found bug: UTF-8 multi-byte characters
        // should not cause panics during attribute substitution
        let attributes = DocumentAttributes::default();

        // Input with UTF-8 multi-byte character (Ô = 0xc3 0x94)
        let value = ":J::~\x01\x00\x00Ô";
        let resolved = value.substitute(HEADER, &attributes);
        // Should not panic and preserve the input
        assert_eq!(resolved, value);

        // Test with various UTF-8 characters and attribute-like patterns
        let value = "{attr}Ô{missing}日本語";
        let resolved = value.substitute(HEADER, &attributes);
        assert_eq!(resolved, "{attr}Ô{missing}日本語");

        // Test with multi-byte chars inside attribute name
        let value = "{attrÔ}test";
        let resolved = value.substitute(HEADER, &attributes);
        assert_eq!(resolved, "{attrÔ}test");
    }
}
