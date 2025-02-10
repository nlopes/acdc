use std::collections::BTreeMap;

use serde::{
    de::Deserializer,
    ser::{SerializeMap, Serializer},
    Deserialize, Serialize,
};

#[derive(Debug, Default, PartialEq, Clone)]
pub struct Document(BTreeMap<AttributeName, AttributeValue>);
pub type Element = Document;

impl Serialize for Document {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // We serialize the attributes as a sequence of key-value pairs.
        let mut state = serializer.serialize_map(Some(self.0.len()))?;
        for (key, value) in &self.0 {
            if key == "toc" && value == &AttributeValue::Bool(true) {
                state.serialize_entry(key, "")?;
                continue;
            }
            state.serialize_entry(key, value)?;
        }
        state.end()
    }
}

impl<'de> Deserialize<'de> for Document {
    fn deserialize<D>(deserializer: D) -> Result<Document, D::Error>
    where
        D: Deserializer<'de>,
    {
        let pairs = BTreeMap::deserialize(deserializer).unwrap_or_default();
        Ok(Document(pairs))
    }
}

impl Document {
    pub fn iter(&self) -> impl Iterator<Item = (&AttributeName, &AttributeValue)> {
        self.0.iter()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    // Insert a new attribute into the document.
    //
    // NOTE: This will *NOT* overwrite an existing attribute with the same name.
    pub fn insert(&mut self, name: AttributeName, value: AttributeValue) {
        if !self.contains_key(&name) {
            self.0.insert(name, value);
        }
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
/// An attribute value can be a string, a boolean, or nothing
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AttributeValue {
    /// A string attribute value.
    String(String),
    /// A boolean attribute value. `false` means it is unset.
    Bool(bool),
    /// No value (or it was unset)
    None,

    /// A list of inline elements - used for the inline preprocessor only!
    Inlines(Vec<crate::InlineNode>),
}

impl std::fmt::Display for AttributeValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AttributeValue::String(value) => write!(f, "{value}"),
            AttributeValue::Bool(value) => write!(f, "{value}"),
            AttributeValue::None => write!(f, "null"),
            AttributeValue::Inlines(_) => unreachable!(),
        }
    }
}
