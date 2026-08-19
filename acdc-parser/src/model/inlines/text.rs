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
pub struct Subscript<'a> {
    pub role: Option<Role<'a>>,
    pub id: Option<&'a str>,
    pub form: Form,
    pub content: Vec<InlineNode<'a>>,
    pub location: Location,
}

/// A `Superscript` represents a superscript section of text in a document.
#[derive(Clone, Debug, PartialEq)]
pub struct Superscript<'a> {
    pub role: Option<Role<'a>>,
    pub id: Option<&'a str>,
    pub form: Form,
    pub content: Vec<InlineNode<'a>>,
    pub location: Location,
}

/// A `CurvedQuotation` represents a curved quotation section of text in a document.
#[derive(Clone, Debug, PartialEq)]
pub struct CurvedQuotation<'a> {
    pub role: Option<Role<'a>>,
    pub id: Option<&'a str>,
    pub form: Form,
    pub content: Vec<InlineNode<'a>>,
    pub location: Location,
}

/// A `CurvedApostrophe` represents a curved apostrophe section of text in a document.
#[derive(Clone, Debug, PartialEq)]
pub struct CurvedApostrophe<'a> {
    pub role: Option<Role<'a>>,
    pub id: Option<&'a str>,
    pub form: Form,
    pub content: Vec<InlineNode<'a>>,
    pub location: Location,
}

/// A `StandaloneCurvedApostrophe` represents a standalone curved apostrophe character.
#[derive(Clone, Debug, PartialEq)]
pub struct StandaloneCurvedApostrophe {
    pub location: Location,
}

/// A `Monospace` represents a monospace section of text in a document.
#[derive(Clone, Debug, PartialEq)]
pub struct Monospace<'a> {
    pub role: Option<Role<'a>>,
    pub id: Option<&'a str>,
    pub form: Form,
    pub content: Vec<InlineNode<'a>>,
    pub location: Location,
}

/// A `Highlight` represents a highlighted section of text in a document.
#[derive(Clone, Debug, PartialEq)]
pub struct Highlight<'a> {
    pub role: Option<Role<'a>>,
    pub id: Option<&'a str>,
    pub form: Form,
    pub content: Vec<InlineNode<'a>>,
    pub location: Location,
}

/// A `Bold` represents a bold section of text in a document.
#[derive(Clone, Debug, PartialEq)]
pub struct Bold<'a> {
    pub role: Option<Role<'a>>,
    pub id: Option<&'a str>,
    pub form: Form,
    pub content: Vec<InlineNode<'a>>,
    pub location: Location,
}

/// An `Italic` represents an italic section of text in a document.
#[derive(Clone, Debug, PartialEq)]
pub struct Italic<'a> {
    pub role: Option<Role<'a>>,
    pub id: Option<&'a str>,
    pub form: Form,
    pub content: Vec<InlineNode<'a>>,
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

/// Ordinary inline text that inherits processing from its enclosing block.
///
/// Converters apply the enclosing block's substitutions and normal prose
/// whitespace rules. Use [`Raw`] for text whose passthrough defines its own
/// substitutions, or [`Verbatim`] for literal and listing content.
#[derive(Clone, Debug, PartialEq)]
pub struct Plain<'a> {
    pub content: &'a str,
    pub location: Location,
    /// Whether an escape prevented this text from being parsed as inline formatting.
    ///
    /// When set, quote substitution must not reinterpret the content.
    pub escaped: bool,
}

impl Serialize for Plain<'_> {
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

/// Inline passthrough text with its own substitution policy.
///
/// Unlike [`Plain`], this text does not inherit substitutions from its enclosing
/// block. Unlike [`Verbatim`], it is not implicitly literal or code content. An
/// empty [`Raw::subs`] preserves the passthrough content using the backend's raw
/// output behavior.
#[derive(Clone, Debug, PartialEq)]
pub struct Raw<'a> {
    pub content: &'a str,
    pub location: Location,
    /// The passthrough substitutions that the converter applies to this text.
    ///
    /// These replace, rather than extend, the enclosing block's substitutions.
    /// The parser resolves substitutions that create inline structure before it
    /// returns the AST, so parsed nodes normally retain only substitutions that
    /// require converter-specific rendering. An empty list means no further
    /// substitutions, as with `+++text+++` and `pass:[text]`.
    pub subs: Vec<Substitution>,
}

/// Text from a literal, listing, or source context.
///
/// Verbatim text inherits substitutions from its enclosing block like [`Plain`],
/// but converters use literal or code presentation and preserve source whitespace
/// unless a requested substitution transforms it. Parsed callout markers are
/// represented by separate [`CalloutRef`] nodes rather than remaining in this text.
#[derive(Clone, Debug, PartialEq)]
pub struct Verbatim<'a> {
    pub content: &'a str,
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
