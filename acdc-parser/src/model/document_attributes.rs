use std::collections::HashMap;

use serde::{
    ser::{SerializeMap, Serializer},
    Deserialize, Serialize,
};

#[derive(Debug, Default, PartialEq, Deserialize)]
pub struct DocumentAttributes(
    #[serde(skip_serializing_if = "HashMap::is_empty")] HashMap<AttributeName, AttributeValue>,
);

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
