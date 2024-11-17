use acdc_core::Location;
use serde::{
    ser::{SerializeMap, Serializer},
    Deserialize, Serialize,
};

use crate::model::Role;

use super::InlineNode;

/// A `Subscript` represents a subscript section of text in a document.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Subscript {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    #[serde(rename = "inlines")]
    pub content: Vec<InlineNode>,
    pub location: Location,
}

/// A `Superscript` represents a superscript section of text in a document.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Superscript {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    #[serde(rename = "inlines")]
    pub content: Vec<InlineNode>,
    pub location: Location,
}

/// A `Monospace` represents a monospace section of text in a document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Monospace {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    #[serde(rename = "inlines")]
    pub content: Vec<InlineNode>,
    pub location: Location,
}

/// A `Highlight` represents a highlighted section of text in a document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Highlight {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    #[serde(rename = "inlines")]
    pub content: Vec<InlineNode>,
    pub location: Location,
}

/// A `Bold` represents a bold section of text in a document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bold {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    #[serde(rename = "inlines")]
    pub content: Vec<InlineNode>,
    pub location: Location,
}

/// An `Italic` represents an italic section of text in a document.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Italic {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    #[serde(rename = "inlines")]
    pub content: Vec<InlineNode>,
    pub location: Location,
}

/// A `LineBreak` represents a line break (inline).
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct LineBreak {
    pub location: Location,
}

impl Serialize for LineBreak {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(Some(3))?;
        state.serialize_entry("name", "linebreak")?;
        state.serialize_entry("type", "string")?;
        state.serialize_entry("location", &self.location)?;
        state.end()
    }
}

/// A `Plain` represents a plain text section in a document.
///
/// This is the most basic form of text in a document.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Plain {
    #[serde(rename = "value")]
    pub content: String,
    pub location: Location,
}

impl Serialize for Plain {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(Some(4))?;
        state.serialize_entry("name", "text")?;
        state.serialize_entry("type", "string")?;
        state.serialize_entry("value", &self.content)?;
        state.serialize_entry("location", &self.location)?;
        state.end()
    }
}

impl Serialize for Italic {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(Some(4))?;
        state.serialize_entry("name", "span")?;
        state.serialize_entry("type", "inline")?;
        state.serialize_entry("variant", "emphasis")?;
        state.serialize_entry("form", "constrained")?;
        state.serialize_entry("inlines", &self.content)?;
        state.serialize_entry("location", &self.location)?;
        state.end()
    }
}

impl Serialize for Superscript {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(Some(4))?;
        state.serialize_entry("name", "span")?;
        state.serialize_entry("type", "inline")?;
        state.serialize_entry("variant", "superscript")?;
        state.serialize_entry("form", "unconstrained")?;
        state.serialize_entry("inlines", &self.content)?;
        state.serialize_entry("location", &self.location)?;
        state.end()
    }
}

impl Serialize for Subscript {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(Some(4))?;
        state.serialize_entry("name", "span")?;
        state.serialize_entry("type", "inline")?;
        state.serialize_entry("variant", "subscript")?;
        state.serialize_entry("form", "unconstrained")?;
        state.serialize_entry("inlines", &self.content)?;
        state.serialize_entry("location", &self.location)?;
        state.end()
    }
}
