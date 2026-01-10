use rustc_hash::FxHashMap;
use serde::{
    Serialize,
    ser::{SerializeMap, Serializer},
};

/// Internal shared implementation for both document and element attributes.
///
/// This type is not exported directly. Use `DocumentAttributes` for document-level
/// attributes or `ElementAttributes` for element-level attributes.
#[derive(Debug, PartialEq, Clone)]
struct AttributeMap {
    /// All attributes including defaults
    all: FxHashMap<AttributeName, AttributeValue>,
    /// Only explicitly set attributes (not defaults) - used for serialization
    explicit: FxHashMap<AttributeName, AttributeValue>,
}

impl Default for AttributeMap {
    fn default() -> Self {
        AttributeMap {
            all: crate::constants::default_attributes(),
            explicit: FxHashMap::default(), // Defaults are not explicit
        }
    }
}

impl AttributeMap {
    fn empty() -> Self {
        AttributeMap {
            all: FxHashMap::default(),
            explicit: FxHashMap::default(),
        }
    }

    fn iter(&self) -> impl Iterator<Item = (&AttributeName, &AttributeValue)> {
        self.all.iter()
    }

    fn is_empty(&self) -> bool {
        // We only consider explicit attributes for emptiness because defaults are always
        // present.
        self.explicit.is_empty()
    }

    fn insert(&mut self, name: AttributeName, value: AttributeValue) {
        if !self.contains_key(&name) {
            self.all.insert(name.clone(), value.clone());
            self.explicit.insert(name, value); // Track as explicit
        }
    }

    fn set(&mut self, name: AttributeName, value: AttributeValue) {
        self.all.insert(name.clone(), value.clone());
        self.explicit.insert(name, value); // Track as explicit
    }

    fn get(&self, name: &str) -> Option<&AttributeValue> {
        self.all.get(name)
    }

    fn contains_key(&self, name: &str) -> bool {
        self.all.contains_key(name)
    }

    fn remove(&mut self, name: &str) -> Option<AttributeValue> {
        self.explicit.remove(name);
        self.all.remove(name)
    }

    fn merge(&mut self, other: AttributeMap) {
        for (key, value) in other.all {
            self.insert(key, value);
        }
    }
}

impl Serialize for AttributeMap {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Only serialize explicitly set attributes, not defaults
        let mut sorted_keys: Vec<_> = self.explicit.keys().collect();
        sorted_keys.sort();

        let mut state = serializer.serialize_map(Some(self.explicit.len()))?;
        for key in sorted_keys {
            if let Some(value) = &self.explicit.get(key) {
                match value {
                    AttributeValue::Bool(true) => {
                        if key == "toc" {
                            state.serialize_entry(key, "")?;
                        } else {
                            state.serialize_entry(key, &true)?;
                        }
                    }
                    value @ (AttributeValue::Bool(false)
                    | AttributeValue::String(_)
                    | AttributeValue::None) => {
                        state.serialize_entry(key, value)?;
                    }
                }
            }
        }
        state.end()
    }
}

/// Document-level attributes with universal defaults.
///
/// These attributes apply to the entire document and include defaults for
/// admonition captions, TOC settings, structural settings, etc.
///
/// Use `DocumentAttributes::default()` to get a map with universal defaults applied.
#[derive(Debug, PartialEq, Clone, Default)]
pub struct DocumentAttributes(AttributeMap);

impl DocumentAttributes {
    /// Iterate over all attributes.
    pub fn iter(&self) -> impl Iterator<Item = (&AttributeName, &AttributeValue)> {
        self.0.iter()
    }

    /// Check if the attribute map is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Insert a new attribute.
    ///
    /// NOTE: This will *NOT* overwrite an existing attribute with the same name.
    pub fn insert(&mut self, name: AttributeName, value: AttributeValue) {
        self.0.insert(name, value);
    }

    /// Set an attribute, overwriting any existing value.
    pub fn set(&mut self, name: AttributeName, value: AttributeValue) {
        self.0.set(name, value);
    }

    /// Get an attribute value by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&AttributeValue> {
        self.0.get(name)
    }

    /// Check if an attribute exists.
    #[must_use]
    pub fn contains_key(&self, name: &str) -> bool {
        self.0.contains_key(name)
    }

    /// Remove an attribute by name.
    pub fn remove(&mut self, name: &str) -> Option<AttributeValue> {
        self.0.remove(name)
    }

    /// Merge another attribute map into this one.
    pub fn merge(&mut self, other: Self) {
        self.0.merge(other.0);
    }

    /// Helper to get a string value.
    ///
    /// Strips surrounding quotes from the value if present (parser quirk workaround).
    #[must_use]
    pub fn get_string(&self, name: &str) -> Option<String> {
        self.get(name).and_then(|v| match v {
            AttributeValue::String(s) => {
                // Strip surrounding quotes if present (parser includes them for quoted values)
                let trimmed = s.trim_matches('"');
                Some(trimmed.to_string())
            }
            AttributeValue::None | AttributeValue::Bool(_) => None,
        })
    }
}

impl Serialize for DocumentAttributes {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

/// Element-level attributes (for blocks, sections, etc.).
///
/// These attributes are specific to individual elements and start empty.
///
/// Use `ElementAttributes::default()` to get an empty attribute map.
#[derive(Debug, PartialEq, Clone)]
pub struct ElementAttributes(AttributeMap);

impl Default for ElementAttributes {
    fn default() -> Self {
        ElementAttributes(AttributeMap::empty())
    }
}

impl ElementAttributes {
    /// Iterate over all attributes.
    pub fn iter(&self) -> impl Iterator<Item = (&AttributeName, &AttributeValue)> {
        self.0.iter()
    }

    /// Check if the attribute map is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Insert a new attribute.
    ///
    /// NOTE: This will *NOT* overwrite an existing attribute with the same name.
    pub fn insert(&mut self, name: AttributeName, value: AttributeValue) {
        self.0.insert(name, value);
    }

    /// Set an attribute, overwriting any existing value.
    pub fn set(&mut self, name: AttributeName, value: AttributeValue) {
        self.0.set(name, value);
    }

    /// Get an attribute value by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&AttributeValue> {
        self.0.get(name)
    }

    /// Check if an attribute exists.
    #[must_use]
    pub fn contains_key(&self, name: &str) -> bool {
        self.0.contains_key(name)
    }

    /// Remove an attribute by name.
    pub fn remove(&mut self, name: &str) -> Option<AttributeValue> {
        self.0.remove(name)
    }

    /// Merge another attribute map into this one.
    pub fn merge(&mut self, other: Self) {
        self.0.merge(other.0);
    }

    /// Helper to get a string value.
    ///
    /// Strips surrounding quotes from the value if present (parser quirk workaround).
    #[must_use]
    pub fn get_string(&self, name: &str) -> Option<String> {
        self.get(name).and_then(|v| match v {
            AttributeValue::String(s) => {
                // Strip surrounding quotes if present (parser includes them for quoted values)
                let trimmed = s.trim_matches('"');
                Some(trimmed.to_string())
            }
            AttributeValue::None | AttributeValue::Bool(_) => None,
        })
    }
}

impl Serialize for ElementAttributes {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

/// An `AttributeName` represents the name of an attribute in a document.
pub type AttributeName = String;

/// An `AttributeValue` represents the value of an attribute in a document.
///
/// An attribute value can be a string, a boolean, or nothing
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum AttributeValue {
    /// A string attribute value.
    String(String),
    /// A boolean attribute value. `false` means it is unset.
    Bool(bool),
    /// No value (or it was unset)
    None,
}

impl std::fmt::Display for AttributeValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AttributeValue::String(value) => write!(f, "{value}"),
            AttributeValue::Bool(value) => write!(f, "{value}"),
            AttributeValue::None => write!(f, "null"),
        }
    }
}

impl From<&str> for AttributeValue {
    fn from(value: &str) -> Self {
        AttributeValue::String(value.to_string())
    }
}

impl From<String> for AttributeValue {
    fn from(value: String) -> Self {
        AttributeValue::String(value)
    }
}

impl From<bool> for AttributeValue {
    fn from(value: bool) -> Self {
        AttributeValue::Bool(value)
    }
}

impl From<()> for AttributeValue {
    fn from((): ()) -> Self {
        AttributeValue::None
    }
}
