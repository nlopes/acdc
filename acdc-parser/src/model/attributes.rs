use rustc_hash::FxHashMap;
use serde::{
    Deserialize, Serialize,
    de::Deserializer,
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
                    | AttributeValue::None
                    | AttributeValue::Inlines(_)) => {
                        state.serialize_entry(key, value)?;
                    }
                }
            }
        }
        state.end()
    }
}

impl<'de> Deserialize<'de> for AttributeMap {
    fn deserialize<D>(deserializer: D) -> Result<AttributeMap, D::Error>
    where
        D: Deserializer<'de>,
    {
        let explicit = FxHashMap::deserialize(deserializer).unwrap_or_default();
        // When deserializing, explicit attributes are the only ones we have
        // Defaults will be added by DocumentAttributes::deserialize
        Ok(AttributeMap {
            all: explicit.clone(),
            explicit,
        })
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
}

impl Serialize for DocumentAttributes {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for DocumentAttributes {
    fn deserialize<D>(deserializer: D) -> Result<DocumentAttributes, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mut map = AttributeMap::deserialize(deserializer)?;

        // Re-apply defaults after deserialization
        // This ensures defaults are available at runtime even though they weren't serialized
        for (name, value) in crate::constants::default_attributes() {
            map.all
                .entry(name)
                .and_modify(|v| *v = value.clone())
                .or_insert(value.clone());
        }

        Ok(DocumentAttributes(map))
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
}

impl Serialize for ElementAttributes {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ElementAttributes {
    fn deserialize<D>(deserializer: D) -> Result<ElementAttributes, D::Error>
    where
        D: Deserializer<'de>,
    {
        AttributeMap::deserialize(deserializer).map(ElementAttributes)
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
            inlines @ AttributeValue::Inlines(_) => {
                tracing::error!(?inlines, "Attempted to display Inlines attribute value");
                Ok(())
            }
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
