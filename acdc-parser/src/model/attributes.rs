use std::borrow::Cow;

use rustc_hash::FxHashMap;
use serde::{
    Serialize,
    ser::{SerializeMap, Serializer},
};

pub const MAX_TOC_LEVELS: u8 = 5;
pub const MAX_SECTION_LEVELS: u8 = 5;

/// Strip surrounding single or double quotes from a string.
///
/// Attribute values in `AsciiDoc` can be quoted with either single or double quotes.
/// This function strips the outermost matching quotes from both ends.
#[must_use]
pub fn strip_quotes(s: &str) -> &str {
    s.trim_start_matches(['"', '\''])
        .trim_end_matches(['"', '\''])
}

/// Internal shared implementation for both document and element attributes.
///
/// This type is not exported directly. Use `DocumentAttributes` for document-level
/// attributes or `ElementAttributes` for element-level attributes.
#[derive(Debug, PartialEq, Clone)]
struct AttributeMap<'a> {
    /// All attributes including defaults
    all: FxHashMap<AttributeName<'a>, AttributeValue<'a>>,
    /// Only explicitly set attributes (not defaults) - used for serialization
    explicit: FxHashMap<AttributeName<'a>, AttributeValue<'a>>,
}

impl Default for AttributeMap<'_> {
    fn default() -> Self {
        use std::sync::LazyLock;
        // Cache the built map so each `default()` call pays only a hashmap
        // clone (pre-sized buckets, trivial `Cow::Borrowed` copies) instead
        // of re-hashing the ~80 entries every time. The `FxHashMap` type
        // is deliberately confined to this file — `constants.rs` only
        // exposes the raw entry slice.
        static DEFAULTS: LazyLock<FxHashMap<AttributeName<'static>, AttributeValue<'static>>> =
            LazyLock::new(|| {
                crate::constants::DEFAULT_ATTRIBUTE_ENTRIES
                    .iter()
                    .cloned()
                    .collect()
            });
        AttributeMap {
            all: DEFAULTS.clone(),
            explicit: FxHashMap::default(), // Defaults are not explicit
        }
    }
}

impl<'a> AttributeMap<'a> {
    fn empty() -> Self {
        AttributeMap {
            all: FxHashMap::default(),
            explicit: FxHashMap::default(),
        }
    }

    fn iter(&self) -> impl Iterator<Item = (&AttributeName<'a>, &AttributeValue<'a>)> {
        self.all.iter()
    }

    fn is_empty(&self) -> bool {
        // We only consider explicit attributes for emptiness because defaults are always
        // present.
        self.explicit.is_empty()
    }

    fn insert(&mut self, name: AttributeName<'a>, value: AttributeValue<'a>) {
        if !self.contains_key(&name) {
            self.all.insert(name.clone(), value.clone());
            self.explicit.insert(name, value); // Track as explicit
        }
    }

    fn set(&mut self, name: AttributeName<'a>, value: AttributeValue<'a>) {
        self.all.insert(name.clone(), value.clone());
        self.explicit.insert(name, value); // Track as explicit
    }

    fn get(&self, name: &str) -> Option<&AttributeValue<'a>> {
        self.all.get(name)
    }

    fn contains_key(&self, name: &str) -> bool {
        self.all.contains_key(name)
    }

    fn remove(&mut self, name: &str) -> Option<AttributeValue<'a>> {
        self.explicit.remove(name);
        self.all.remove(name)
    }

    fn merge(&mut self, other: AttributeMap<'a>) {
        for (key, value) in other.all {
            self.insert(key, value);
        }
    }
}

impl Serialize for AttributeMap<'_> {
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

/// Validate bounded attributes and emit warnings for out-of-range values.
///
/// Some attributes like `sectnumlevels` and `toclevels` have valid ranges.
/// This function emits a warning if the value is outside the valid range.
fn validate_bounded_attribute(key: &str, value: &AttributeValue<'_>) {
    let AttributeValue::String(s) = value else {
        return;
    };

    match key {
        "sectnumlevels" => {
            if let Ok(level) = s.parse::<u8>()
                && level > MAX_SECTION_LEVELS
            {
                tracing::warn!(
                    attribute = "sectnumlevels",
                    value = level,
                    "sectnumlevels must be between 0 and {MAX_SECTION_LEVELS}, got {level}. \
                         Values above {MAX_SECTION_LEVELS} will be treated as {MAX_SECTION_LEVELS}."
                );
            }
        }
        "toclevels" => {
            if let Ok(level) = s.parse::<u8>()
                && level > MAX_TOC_LEVELS
            {
                tracing::warn!(
                    attribute = "toclevels",
                    value = level,
                    "toclevels must be between 0 and {MAX_TOC_LEVELS}, got {level}. \
                         Values above {MAX_TOC_LEVELS} will be treated as {MAX_TOC_LEVELS}."
                );
            }
        }
        _ => {}
    }
}

/// Document-level attributes with universal defaults.
///
/// These attributes apply to the entire document and include defaults for
/// admonition captions, TOC settings, structural settings, etc.
///
/// Use `DocumentAttributes::default()` to get a map with universal defaults applied.
#[derive(Debug, PartialEq, Clone, Default)]
pub struct DocumentAttributes<'a>(AttributeMap<'a>);

impl<'a> DocumentAttributes<'a> {
    /// Create an empty `DocumentAttributes` without default attributes.
    /// Used for lightweight parsing contexts (e.g., quotes-only) where
    /// document attributes aren't needed.
    pub(crate) fn empty() -> Self {
        Self(AttributeMap::empty())
    }

    /// Iterate over all attributes.
    pub fn iter(&self) -> impl Iterator<Item = (&AttributeName<'a>, &AttributeValue<'a>)> {
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
    pub fn insert(&mut self, name: AttributeName<'a>, value: AttributeValue<'a>) {
        validate_bounded_attribute(&name, &value);
        self.0.insert(name, value);
    }

    /// Set an attribute, overwriting any existing value.
    pub fn set(&mut self, name: AttributeName<'a>, value: AttributeValue<'a>) {
        validate_bounded_attribute(&name, &value);
        self.0.set(name, value);
    }

    /// Get an attribute value by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&AttributeValue<'a>> {
        self.0.get(name)
    }

    /// Check if an attribute exists.
    #[must_use]
    pub fn contains_key(&self, name: &str) -> bool {
        self.0.contains_key(name)
    }

    /// Remove an attribute by name.
    pub fn remove(&mut self, name: &str) -> Option<AttributeValue<'a>> {
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
    pub fn get_string(&self, name: &str) -> Option<Cow<'a, str>> {
        self.get(name).and_then(|v| match v {
            AttributeValue::String(s) => Some(match s {
                Cow::Borrowed(b) => Cow::Borrowed(strip_quotes(b)),
                Cow::Owned(o) => Cow::Owned(strip_quotes(o).to_string()),
            }),
            AttributeValue::None | AttributeValue::Bool(_) => None,
        })
    }

    /// Clone the attributes into an independent `'static` copy. Used by
    /// converters that cache document attributes on a processor whose
    /// lifetime is independent of the document being rendered.
    #[must_use]
    pub fn to_static(&self) -> DocumentAttributes<'static> {
        self.clone().into_static()
    }

    /// Consume the attributes, producing an independent `'static` copy.
    #[must_use]
    pub fn into_static(self) -> DocumentAttributes<'static> {
        let convert_map = |map: FxHashMap<AttributeName<'a>, AttributeValue<'a>>| -> FxHashMap<AttributeName<'static>, AttributeValue<'static>> {
            map.into_iter()
                .map(|(k, v)| {
                    let key: AttributeName<'static> = Cow::Owned(k.into_owned());
                    let val = match v {
                        AttributeValue::String(s) => AttributeValue::String(Cow::Owned(s.into_owned())),
                        AttributeValue::Bool(b) => AttributeValue::Bool(b),
                        AttributeValue::None => AttributeValue::None,
                    };
                    (key, val)
                })
                .collect()
        };
        DocumentAttributes(AttributeMap {
            all: convert_map(self.0.all),
            explicit: convert_map(self.0.explicit),
        })
    }
}

impl Serialize for DocumentAttributes<'_> {
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
pub struct ElementAttributes<'a>(AttributeMap<'a>);

impl Default for ElementAttributes<'_> {
    fn default() -> Self {
        ElementAttributes(AttributeMap::empty())
    }
}

impl<'a> ElementAttributes<'a> {
    /// Iterate over all attributes.
    pub fn iter(&self) -> impl Iterator<Item = (&AttributeName<'a>, &AttributeValue<'a>)> {
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
    pub fn insert(&mut self, name: AttributeName<'a>, value: AttributeValue<'a>) {
        self.0.insert(name, value);
    }

    /// Set an attribute, overwriting any existing value.
    pub fn set(&mut self, name: AttributeName<'a>, value: AttributeValue<'a>) {
        self.0.set(name, value);
    }

    /// Get an attribute value by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&AttributeValue<'a>> {
        self.0.get(name)
    }

    /// Check if an attribute exists.
    #[must_use]
    pub fn contains_key(&self, name: &str) -> bool {
        self.0.contains_key(name)
    }

    /// Remove an attribute by name.
    pub fn remove(&mut self, name: &str) -> Option<AttributeValue<'a>> {
        self.0.remove(name)
    }

    /// Merge another attribute map into this one.
    pub fn merge(&mut self, other: Self) {
        self.0.merge(other.0);
    }

    /// Convert all borrowed content to owned, producing `'static` lifetime attributes.
    #[must_use]
    pub fn into_static(self) -> ElementAttributes<'static> {
        let convert_map = |map: FxHashMap<AttributeName<'a>, AttributeValue<'a>>| -> FxHashMap<AttributeName<'static>, AttributeValue<'static>> {
            map.into_iter()
                .map(|(k, v)| {
                    let key: AttributeName<'static> = Cow::Owned(k.into_owned());
                    let val = match v {
                        AttributeValue::String(s) => AttributeValue::String(Cow::Owned(s.into_owned())),
                        AttributeValue::Bool(b) => AttributeValue::Bool(b),
                        AttributeValue::None => AttributeValue::None,
                    };
                    (key, val)
                })
                .collect()
        };
        ElementAttributes(AttributeMap {
            all: convert_map(self.0.all),
            explicit: convert_map(self.0.explicit),
        })
    }

    /// Get a string attribute value as an owned `String`.
    ///
    /// Strips surrounding quotes from the value if present.
    #[must_use]
    pub fn get_string(&self, name: &str) -> Option<Cow<'a, str>> {
        self.get(name).and_then(|v| match v {
            AttributeValue::String(s) => Some(match s {
                Cow::Borrowed(b) => Cow::Borrowed(strip_quotes(b)),
                Cow::Owned(o) => Cow::Owned(strip_quotes(o).to_string()),
            }),
            AttributeValue::None | AttributeValue::Bool(_) => None,
        })
    }
}

impl Serialize for ElementAttributes<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

/// An `AttributeName` represents the name of an attribute in a document.
pub type AttributeName<'a> = Cow<'a, str>;

/// An `AttributeValue` represents the value of an attribute in a document.
///
/// An attribute value can be a string, a boolean, or nothing
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum AttributeValue<'a> {
    /// A string attribute value.
    String(Cow<'a, str>),
    /// A boolean attribute value. `false` means it is unset.
    Bool(bool),
    /// No value (or it was unset)
    None,
}

impl std::fmt::Display for AttributeValue<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AttributeValue::String(value) => write!(f, "{value}"),
            AttributeValue::Bool(value) => write!(f, "{value}"),
            AttributeValue::None => write!(f, "null"),
        }
    }
}

impl<'a> From<&'a str> for AttributeValue<'a> {
    fn from(value: &'a str) -> Self {
        AttributeValue::String(Cow::Borrowed(value))
    }
}

impl From<String> for AttributeValue<'_> {
    fn from(value: String) -> Self {
        AttributeValue::String(Cow::Owned(value))
    }
}

impl From<bool> for AttributeValue<'_> {
    fn from(value: bool) -> Self {
        AttributeValue::Bool(value)
    }
}

impl From<()> for AttributeValue<'_> {
    fn from((): ()) -> Self {
        AttributeValue::None
    }
}
