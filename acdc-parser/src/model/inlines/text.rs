use serde::{
    ser::{SerializeMap, Serializer},
    Deserialize, Serialize,
};

use crate::{Location, Role};

use super::InlineNode;

/// The form of an inline formatting element (how it was expressed in the source)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Form {
    Constrained,
    Unconstrained,
}

/// A `Subscript` represents a subscript section of text in a document.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Subscript {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    pub form: Form,
    #[serde(rename = "inlines")]
    pub content: Vec<InlineNode>,
    pub location: Location,
}

/// A `Superscript` represents a superscript section of text in a document.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Superscript {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    pub form: Form,
    #[serde(rename = "inlines")]
    pub content: Vec<InlineNode>,
    pub location: Location,
}

/// A `CurvedQuotation` represents a curved quotation section of text in a document.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct CurvedQuotation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    pub form: Form,
    #[serde(rename = "inlines")]
    pub content: Vec<InlineNode>,
    pub location: Location,
}

/// A `CurvedApostrophe` represents a curved apostrophe section of text in a document.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct CurvedApostrophe {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    pub form: Form,
    #[serde(rename = "inlines")]
    pub content: Vec<InlineNode>,
    pub location: Location,
}

/// A `StandaloneCurvedApostrophe` represents a standalone curved apostrophe character.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct StandaloneCurvedApostrophe {
    pub location: Location,
}

/// A `Monospace` represents a monospace section of text in a document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Monospace {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    pub form: Form,
    #[serde(rename = "inlines")]
    pub content: Vec<InlineNode>,
    pub location: Location,
}

/// A `Highlight` represents a highlighted section of text in a document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Highlight {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    pub form: Form,
    #[serde(rename = "inlines")]
    pub content: Vec<InlineNode>,
    pub location: Location,
}

/// A `Bold` represents a bold section of text in a document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bold {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    pub form: Form,
    #[serde(rename = "inlines")]
    pub content: Vec<InlineNode>,
    pub location: Location,
}

/// An `Italic` represents an italic section of text in a document.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Italic {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    pub form: Form,
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

/// A `Raw` represents a raw text section in a document.
///
/// This is the most basic form of text in a document and it should note that its contents
/// must be rendered as they are (e.g: "<h1>" should not end up being a <h1> tag, it
/// should be "<h1>" text in html, very likely &lt;h1&gt;).
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Raw {
    #[serde(rename = "value")]
    pub content: String,
    pub location: Location,
}

impl Serialize for Raw {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(Some(4))?;
        state.serialize_entry("name", "raw")?;
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
        state.serialize_entry("form", &self.form)?;
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
        state.serialize_entry("form", &self.form)?;
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
        state.serialize_entry("form", &self.form)?;
        state.serialize_entry("inlines", &self.content)?;
        state.serialize_entry("location", &self.location)?;
        state.end()
    }
}

impl Serialize for CurvedQuotation {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(Some(4))?;
        state.serialize_entry("name", "span")?;
        state.serialize_entry("type", "inline")?;
        state.serialize_entry("variant", "curved_quotation")?;
        state.serialize_entry("form", &self.form)?;
        state.serialize_entry("inlines", &self.content)?;
        state.serialize_entry("location", &self.location)?;
        state.end()
    }
}

impl Serialize for CurvedApostrophe {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(Some(4))?;
        state.serialize_entry("name", "span")?;
        state.serialize_entry("type", "inline")?;
        state.serialize_entry("variant", "curved_apostrophe")?;
        state.serialize_entry("form", &self.form)?;
        state.serialize_entry("inlines", &self.content)?;
        state.serialize_entry("location", &self.location)?;
        state.end()
    }
}

impl Serialize for StandaloneCurvedApostrophe {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(Some(3))?;
        state.serialize_entry("name", "curved_apostrophe")?;
        state.serialize_entry("type", "string")?;
        state.serialize_entry("location", &self.location)?;
        state.end()
    }
}
