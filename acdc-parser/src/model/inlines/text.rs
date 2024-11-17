use acdc_core::Location;
use serde::{
    ser::{SerializeMap, Serializer},
    Deserialize, Serialize,
};

use crate::model::Role;

use super::InlineNode;

/// A `SubscriptText` represents a subscript section of text in a document.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct SubscriptText {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    #[serde(rename = "inlines")]
    pub content: Vec<InlineNode>,
    pub location: Location,
}

/// A `SuperscriptText` represents a superscript section of text in a document.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct SuperscriptText {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    #[serde(rename = "inlines")]
    pub content: Vec<InlineNode>,
    pub location: Location,
}

/// A `MonospaceText` represents a monospace section of text in a document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MonospaceText {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    #[serde(rename = "inlines")]
    pub content: Vec<InlineNode>,
    pub location: Location,
}

/// A `HighlightText` represents a highlighted section of text in a document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HighlightText {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    #[serde(rename = "inlines")]
    pub content: Vec<InlineNode>,
    pub location: Location,
}

/// A `BoldText` represents a bold section of text in a document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BoldText {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    #[serde(rename = "inlines")]
    pub content: Vec<InlineNode>,
    pub location: Location,
}

/// An `ItalicText` represents an italic section of text in a document.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct ItalicText {
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

/// A `PlainText` represents a plain text section in a document.
///
/// This is the most basic form of text in a document.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct PlainText {
    #[serde(rename = "value")]
    pub content: String,
    pub location: Location,
}

impl Serialize for PlainText {
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

impl Serialize for ItalicText {
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

impl Serialize for SuperscriptText {
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

impl Serialize for SubscriptText {
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
