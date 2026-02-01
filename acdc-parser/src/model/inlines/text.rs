use serde::{
    Serialize,
    ser::{SerializeMap, Serializer},
};

use crate::{Location, Role, Substitution};

use super::InlineNode;

/// The form of an inline formatting element (how it was expressed in the source)
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Form {
    Constrained,
    Unconstrained,
}

/// A `Subscript` represents a subscript section of text in a document.
#[derive(Clone, Debug, PartialEq)]
pub struct Subscript {
    pub role: Option<Role>,
    pub id: Option<String>,
    pub form: Form,
    pub content: Vec<InlineNode>,
    pub location: Location,
}

/// A `Superscript` represents a superscript section of text in a document.
#[derive(Clone, Debug, PartialEq)]
pub struct Superscript {
    pub role: Option<Role>,
    pub id: Option<String>,
    pub form: Form,
    pub content: Vec<InlineNode>,
    pub location: Location,
}

/// A `CurvedQuotation` represents a curved quotation section of text in a document.
#[derive(Clone, Debug, PartialEq)]
pub struct CurvedQuotation {
    pub role: Option<Role>,
    pub id: Option<String>,
    pub form: Form,
    pub content: Vec<InlineNode>,
    pub location: Location,
}

/// A `CurvedApostrophe` represents a curved apostrophe section of text in a document.
#[derive(Clone, Debug, PartialEq)]
pub struct CurvedApostrophe {
    pub role: Option<Role>,
    pub id: Option<String>,
    pub form: Form,
    pub content: Vec<InlineNode>,
    pub location: Location,
}

/// A `StandaloneCurvedApostrophe` represents a standalone curved apostrophe character.
#[derive(Clone, Debug, PartialEq)]
pub struct StandaloneCurvedApostrophe {
    pub location: Location,
}

/// A `Monospace` represents a monospace section of text in a document.
#[derive(Clone, Debug, PartialEq)]
pub struct Monospace {
    pub role: Option<Role>,
    pub id: Option<String>,
    pub form: Form,
    pub content: Vec<InlineNode>,
    pub location: Location,
}

/// A `Highlight` represents a highlighted section of text in a document.
#[derive(Clone, Debug, PartialEq)]
pub struct Highlight {
    pub role: Option<Role>,
    pub id: Option<String>,
    pub form: Form,
    pub content: Vec<InlineNode>,
    pub location: Location,
}

/// A `Bold` represents a bold section of text in a document.
#[derive(Clone, Debug, PartialEq)]
pub struct Bold {
    pub role: Option<Role>,
    pub id: Option<String>,
    pub form: Form,
    pub content: Vec<InlineNode>,
    pub location: Location,
}

/// An `Italic` represents an italic section of text in a document.
#[derive(Clone, Debug, PartialEq)]
pub struct Italic {
    pub role: Option<Role>,
    pub id: Option<String>,
    pub form: Form,
    pub content: Vec<InlineNode>,
    pub location: Location,
}

/// A `LineBreak` represents a line break (inline).
#[derive(Clone, Debug, PartialEq)]
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
#[derive(Clone, Debug, PartialEq)]
pub struct Plain {
    pub content: String,
    pub location: Location,
    /// True if content originated from an escaped pattern (e.g., `\^2^`).
    /// When true, the converter should not re-parse for quotes.
    pub escaped: bool,
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
/// must be rendered as they are (e.g: "\<h1>" should not end up being a \<h1> tag, it
/// should be "\<h1>" text in html, very likely \&lt;h1\&gt;).
#[derive(Clone, Debug, PartialEq)]
pub struct Raw {
    pub content: String,
    pub location: Location,
    /// The substitutions to apply when rendering this node.
    ///
    /// Carries the passthrough's own substitution list (minus Quotes, which are
    /// already handled during parsing). The converter uses these subs directly
    /// instead of the enclosing block's substitutions. An empty vec means no
    /// substitutions (raw output), as with `+++text+++` and `pass:[text]`.
    pub subs: Vec<Substitution>,
}

/// A `Verbatim` represents verbatim text section in a document.
///
/// This is the most basic form of text in a document and it should note that its contents
/// must be rendered as they are (e.g: "\<h1>" should not end up being a \<h1> tag, it
/// should be "\<h1>" text in html, very likely \&lt;h1\&gt;).
///
/// It is similar to `Raw`, but is intended for use in contexts where verbatim text is
/// used, and some substitutions are done, namely converting callouts.
#[derive(Clone, Debug, PartialEq)]
pub struct Verbatim {
    pub content: String,
    pub location: Location,
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

/// The kind of callout reference marker (how it was expressed in the source).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CalloutRefKind {
    /// Explicit callout: `<1>`, `<2>`, etc. - the number was specified directly.
    Explicit,
    /// Auto-numbered callout: `<.>` - the number was resolved automatically.
    Auto,
}

/// A `CalloutRef` represents a callout reference marker within verbatim content.
///
/// Callout references appear at the end of lines in source/listing blocks and
/// link to explanatory text in a subsequent callout list.
///
/// # Examples
///
/// ```asciidoc
/// [source,ruby]
/// ----
/// def main <1>
///   puts 'hello' <.>
/// end
/// ----
/// <1> Defines the main function
/// <.> Prints a greeting
/// ```
///
/// The `<1>` marker creates an `Explicit` callout ref, while `<.>` creates an
/// `Auto` callout ref that gets resolved to the next available number.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CalloutRef {
    /// The kind of callout (explicit number vs auto-numbered).
    pub kind: CalloutRefKind,
    /// The resolved callout number (1-indexed).
    pub number: usize,
    /// Source location of this callout reference.
    pub location: Location,
}

impl CalloutRef {
    /// Creates a new explicit callout reference with the given number.
    #[must_use]
    pub fn explicit(number: usize, location: Location) -> Self {
        Self {
            kind: CalloutRefKind::Explicit,
            number,
            location,
        }
    }

    /// Creates a new auto-numbered callout reference with the resolved number.
    #[must_use]
    pub fn auto(number: usize, location: Location) -> Self {
        Self {
            kind: CalloutRefKind::Auto,
            number,
            location,
        }
    }
}

impl Serialize for CalloutRef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(Some(5))?;
        state.serialize_entry("name", "callout_reference")?;
        state.serialize_entry("type", "inline")?;
        state.serialize_entry("variant", &self.kind)?;
        state.serialize_entry("number", &self.number)?;
        state.serialize_entry("location", &self.location)?;
        state.end()
    }
}
