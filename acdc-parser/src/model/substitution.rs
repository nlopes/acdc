use serde::{Deserialize, Serialize};

use crate::{AttributeValue, DocumentAttributes};

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
