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

    fn serialize_explicit<S>(
        &self,
        serializer: S,
        include: impl Fn(&AttributeName<'_>, &AttributeValue<'_>) -> bool,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut entries: Vec<_> = self
            .explicit
            .iter()
            .filter(|(key, value)| include(key, value))
            .collect();
        entries.sort_by_key(|(key, _)| *key);

        let mut state = serializer.serialize_map(Some(entries.len()))?;
        for (key, value) in entries {
            match value {
                AttributeValue::Bool(true) if key == "toc" => {
                    state.serialize_entry(key, "")?;
                }
                AttributeValue::Bool(true) => {
                    state.serialize_entry(key, &true)?;
                }
                AttributeValue::Bool(false) | AttributeValue::String(_) | AttributeValue::None => {
                    state.serialize_entry(key, value)?;
                }
            }
        }
        state.end()
    }
}

impl Serialize for AttributeMap<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Only serialize explicitly set attributes, not defaults.
        self.serialize_explicit(serializer, |_, _| true)
    }
}

/// Whether a stored value counts as set: an explicit string, or boolean `true`.
///
/// Values stored as `false` or no-value read as unset. Single definition of
/// attribute truthiness for [`DocumentAttributes::is_set`], [`DocumentAttributes::get`],
/// and serialization.
fn is_truthy(value: &AttributeValue<'_>) -> bool {
    matches!(
        value,
        AttributeValue::String(_) | AttributeValue::Bool(true)
    )
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
#[derive(Debug, PartialEq, Clone)]
pub struct DocumentAttributes<'a> {
    attributes: AttributeMap<'a>,
    defaults_enabled: bool,
}

impl Default for DocumentAttributes<'_> {
    fn default() -> Self {
        Self {
            attributes: AttributeMap::default(),
            defaults_enabled: true,
        }
    }
}

impl<'a> DocumentAttributes<'a> {
    /// Create an empty `DocumentAttributes` without default attributes.
    /// Used for lightweight parsing contexts (e.g., quotes-only) where
    /// document attributes aren't needed.
    pub(crate) fn empty() -> Self {
        Self {
            attributes: AttributeMap::empty(),
            defaults_enabled: false,
        }
    }

    /// Iterate over stored attributes.
    ///
    /// This does not include the `max-include-depth` fallback synthesized by
    /// [`Self::get`].
    pub fn iter(&self) -> impl Iterator<Item = (&AttributeName<'a>, &AttributeValue<'a>)> {
        self.attributes.iter()
    }

    /// Check whether no attributes have been set explicitly.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.attributes.is_empty()
    }

    /// Insert a new attribute.
    ///
    /// NOTE: This will *NOT* overwrite an existing attribute with the same name.
    pub fn insert(&mut self, name: AttributeName<'a>, value: AttributeValue<'a>) {
        validate_bounded_attribute(&name, &value);
        self.attributes.insert(name, value);
    }

    /// Set an attribute, overwriting any existing value.
    pub fn set(&mut self, name: AttributeName<'a>, value: AttributeValue<'a>) {
        validate_bounded_attribute(&name, &value);
        self.attributes.set(name, value);
    }

    /// Whether `name` is set to a truthy value: an explicit string, or boolean
    /// `true`. Absent attributes and those set to `false` / none are not
    /// considered set. Reads the raw stored value, so it is not affected by the
    /// `max-include-depth` default synthesized by [`Self::get`].
    pub(crate) fn is_set(&self, name: &str) -> bool {
        self.attributes.get(name).is_some_and(is_truthy)
    }

    /// Whether document input or parser options explicitly set a truthy value.
    pub(crate) fn is_explicitly_set(&self, name: &str) -> bool {
        self.attributes.explicit.get(name).is_some_and(is_truthy)
    }

    /// Get an attribute value by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&AttributeValue<'a>> {
        let stored = self.attributes.get(name);
        // A truthy stored value always wins, so the common lookup pays one probe
        // and no name comparison.
        if stored.is_some_and(is_truthy) {
            return stored;
        }
        // `max-include-depth` resolves to the built-in default unless it is set
        // to an explicit value.
        if self.defaults_enabled && name == crate::constants::MAX_INCLUDE_DEPTH_ATTR {
            return Some(&crate::constants::DEFAULT_MAX_INCLUDE_DEPTH_VALUE);
        }
        stored
    }

    /// Check whether an attribute is stored.
    ///
    /// This does not consider the `max-include-depth` fallback synthesized by
    /// [`Self::get`].
    #[must_use]
    pub fn contains_key(&self, name: &str) -> bool {
        self.attributes.contains_key(name)
    }

    /// Remove an attribute by name.
    pub fn remove(&mut self, name: &str) -> Option<AttributeValue<'a>> {
        self.attributes.remove(name)
    }

    /// Merge another attribute map into this one.
    pub fn merge(&mut self, other: Self) {
        self.attributes.merge(other.attributes);
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
        let Self {
            attributes,
            defaults_enabled,
        } = self;
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
        DocumentAttributes {
            attributes: AttributeMap {
                all: convert_map(attributes.all),
                explicit: convert_map(attributes.explicit),
            },
            defaults_enabled,
        }
    }
}

impl Serialize for DocumentAttributes<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.attributes
            .serialize_explicit(serializer, |key, value| {
                // An unset marker stored for `max-include-depth` asks for the built-in
                // default that `get` synthesizes, so it is not a caller-supplied value.
                let asks_for_synthesized_default = self.defaults_enabled
                    && key == crate::constants::MAX_INCLUDE_DEPTH_ATTR
                    && !is_truthy(value);
                !asks_for_synthesized_default
            })
    }
}

#[cfg(test)]
mod document_attribute_tests {
    use super::*;
    use crate::constants::MAX_INCLUDE_DEPTH_ATTR;
    use serde_json::json;

    #[test]
    fn max_include_depth_default_is_visible_only_through_get() -> Result<(), serde_json::Error> {
        let attributes = DocumentAttributes::default();

        assert_eq!(
            attributes.get_string(MAX_INCLUDE_DEPTH_ATTR).as_deref(),
            Some("64")
        );
        assert!(!attributes.contains_key(MAX_INCLUDE_DEPTH_ATTR));
        assert!(
            attributes
                .iter()
                .all(|(name, _)| name != MAX_INCLUDE_DEPTH_ATTR)
        );
        assert!(attributes.is_empty());
        assert_eq!(serde_json::to_value(&attributes)?, json!({}));

        let static_attributes = attributes.into_static();
        assert_eq!(
            static_attributes
                .get_string(MAX_INCLUDE_DEPTH_ATTR)
                .as_deref(),
            Some("64")
        );
        Ok(())
    }

    #[test]
    fn empty_document_attributes_do_not_synthesize_defaults() -> Result<(), serde_json::Error> {
        let attributes = DocumentAttributes::empty();

        assert_eq!(attributes.get(MAX_INCLUDE_DEPTH_ATTR), None);
        assert!(!attributes.contains_key(MAX_INCLUDE_DEPTH_ATTR));
        assert_eq!(attributes.iter().count(), 0);
        assert!(attributes.is_empty());
        assert_eq!(serde_json::to_value(&attributes)?, json!({}));

        let static_attributes = attributes.into_static();
        assert_eq!(static_attributes.get(MAX_INCLUDE_DEPTH_ATTR), None);
        Ok(())
    }

    #[test]
    fn explicit_max_include_depth_uses_normal_map_semantics() -> Result<(), serde_json::Error> {
        let mut attributes = DocumentAttributes::default();
        attributes.set(MAX_INCLUDE_DEPTH_ATTR.into(), "8".into());

        assert_eq!(
            attributes.get_string(MAX_INCLUDE_DEPTH_ATTR).as_deref(),
            Some("8")
        );
        assert!(attributes.contains_key(MAX_INCLUDE_DEPTH_ATTR));
        assert_eq!(
            attributes
                .iter()
                .find(|(name, _)| name.as_ref() == MAX_INCLUDE_DEPTH_ATTR)
                .map(|(_, value)| value),
            Some(&AttributeValue::String("8".into()))
        );
        assert!(!attributes.is_empty());
        assert_eq!(
            serde_json::to_value(&attributes)?,
            json!({ "max-include-depth": "8" })
        );
        Ok(())
    }

    #[test]
    fn unset_max_include_depth_values_are_not_serialized() -> Result<(), serde_json::Error> {
        for value in [AttributeValue::Bool(false), AttributeValue::None] {
            let mut attributes = DocumentAttributes::default();
            attributes.set(MAX_INCLUDE_DEPTH_ATTR.into(), value.clone());

            assert_eq!(
                attributes.get_string(MAX_INCLUDE_DEPTH_ATTR).as_deref(),
                Some("64")
            );
            assert!(attributes.contains_key(MAX_INCLUDE_DEPTH_ATTR));
            assert_eq!(
                attributes
                    .iter()
                    .find(|(name, _)| name.as_ref() == MAX_INCLUDE_DEPTH_ATTR)
                    .map(|(_, stored)| stored),
                Some(&value)
            );
            assert!(!attributes.is_empty());
            assert_eq!(serde_json::to_value(&attributes)?, json!({}));
        }
        Ok(())
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
