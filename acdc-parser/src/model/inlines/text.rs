use serde::{
    Serialize,
    ser::{SerializeMap, Serializer},
};

use crate::{ElementId, Location, Role, Substitution};

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
    pub id: Option<ElementId<'a>>,
    pub form: Form,
    pub content: Vec<InlineNode<'a>>,
    pub location: Location,
}

/// A `Superscript` represents a superscript section of text in a document.
#[derive(Clone, Debug, PartialEq)]
pub struct Superscript<'a> {
    pub role: Option<Role<'a>>,
    pub id: Option<ElementId<'a>>,
    pub form: Form,
    pub content: Vec<InlineNode<'a>>,
    pub location: Location,
}

/// A `CurvedQuotation` represents a curved quotation section of text in a document.
#[derive(Clone, Debug, PartialEq)]
pub struct CurvedQuotation<'a> {
    pub role: Option<Role<'a>>,
    pub id: Option<ElementId<'a>>,
    pub form: Form,
    pub content: Vec<InlineNode<'a>>,
    pub location: Location,
}

/// A `CurvedApostrophe` represents a curved apostrophe section of text in a document.
#[derive(Clone, Debug, PartialEq)]
pub struct CurvedApostrophe<'a> {
    pub role: Option<Role<'a>>,
    pub id: Option<ElementId<'a>>,
    pub form: Form,
    pub content: Vec<InlineNode<'a>>,
    pub location: Location,
}

/// A `StandaloneCurvedApostrophe` represents a standalone curved apostrophe character.
#[derive(Clone, Debug, PartialEq)]
pub struct StandaloneCurvedApostrophe {
    pub(crate) location: Location,
}

impl StandaloneCurvedApostrophe {
    /// Construct a standalone typographic apostrophe.
    #[must_use]
    pub fn new(location: Location) -> Self {
        Self { location }
    }

    /// Replace this node's recorded source span.
    pub fn set_location(&mut self, location: Location) {
        self.location = location;
    }
}

/// A `Monospace` represents a monospace section of text in a document.
#[derive(Clone, Debug, PartialEq)]
pub struct Monospace<'a> {
    pub role: Option<Role<'a>>,
    pub id: Option<ElementId<'a>>,
    pub form: Form,
    pub content: Vec<InlineNode<'a>>,
    pub location: Location,
}

/// A `Highlight` represents a highlighted section of text in a document.
#[derive(Clone, Debug, PartialEq)]
pub struct Highlight<'a> {
    pub role: Option<Role<'a>>,
    pub id: Option<ElementId<'a>>,
    pub form: Form,
    pub content: Vec<InlineNode<'a>>,
    pub location: Location,
}

/// A `Bold` represents a bold section of text in a document.
#[derive(Clone, Debug, PartialEq)]
pub struct Bold<'a> {
    pub role: Option<Role<'a>>,
    pub id: Option<ElementId<'a>>,
    pub form: Form,
    pub content: Vec<InlineNode<'a>>,
    pub location: Location,
}

/// An `Italic` represents an italic section of text in a document.
#[derive(Clone, Debug, PartialEq)]
pub struct Italic<'a> {
    pub role: Option<Role<'a>>,
    pub id: Option<ElementId<'a>>,
    pub form: Form,
    pub content: Vec<InlineNode<'a>>,
    pub location: Location,
}

/// A `LineBreak` represents a line break (inline).
#[derive(Clone, Debug, PartialEq)]
pub struct LineBreak {
    pub(crate) location: Location,
}

impl LineBreak {
    /// Construct an explicit line break.
    #[must_use]
    pub fn new(location: Location) -> Self {
        Self { location }
    }

    /// Replace this node's recorded source span.
    pub fn set_location(&mut self, location: Location) {
        self.location = location;
    }
}

impl Serialize for LineBreak {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(Some(3))?;
        state.serialize_entry("name", "break")?;
        state.serialize_entry("type", "inline")?;
        state.serialize_entry("location", &self.location)?;
        state.end()
    }
}

/// Ordinary inline text that inherits processing from its enclosing block.
///
/// Converters apply the enclosing block's substitutions and normal prose
/// whitespace rules. Use [`Raw`] for text whose passthrough defines its own
/// substitutions, or [`Verbatim`] for literal and listing content.
///
/// ```compile_fail,E0616
/// use acdc_parser::{Location, Plain};
/// let mut plain = Plain::new("text", Location::default());
/// plain.content = "changed";
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct Plain<'a> {
    pub(crate) content: &'a str,
    pub(crate) location: Location,
    /// Whether an escape prevented this text from being parsed as inline formatting.
    ///
    /// When set, quote substitution must not reinterpret the content.
    pub(crate) escaped: bool,
}

impl<'a> Plain<'a> {
    /// Construct prepared ordinary text without parsing inline syntax.
    #[must_use]
    pub fn new(content: &'a str, location: Location) -> Self {
        Self {
            content,
            location,
            escaped: false,
        }
    }

    /// Set whether quote substitution must keep this text literal.
    #[must_use]
    pub fn with_escaped(mut self, escaped: bool) -> Self {
        self.set_escaped(escaped);
        self
    }

    /// Set whether quote substitution must keep this text literal.
    pub fn set_escaped(&mut self, escaped: bool) {
        self.escaped = escaped;
    }

    /// Replace prepared text, retaining its escape decision and source span.
    pub fn replace_text(&mut self, content: &'a str) -> &'a str {
        std::mem::replace(&mut self.content, content)
    }

    /// Replace this node's recorded source span.
    pub fn set_location(&mut self, location: Location) {
        self.location = location;
    }
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
/// empty substitution sequence preserves the passthrough content using the backend's raw
/// output behavior.
///
/// ```compile_fail,E0616
/// use acdc_parser::{Location, Raw};
/// let mut value = Raw::new("text", Vec::new(), Location::default());
/// value.content = "changed";
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct Raw<'a> {
    /// Identity on an attributed passthrough; absent on ordinary raw text.
    pub(crate) metadata: Option<Box<crate::InlineMetadata<'a>>>,
    pub(crate) content: &'a str,
    pub(crate) location: Location,
    /// The passthrough substitutions that the converter applies to this text, in order.
    ///
    /// These replace, rather than extend, the enclosing block's substitutions.
    /// The parser resolves substitutions that create inline structure before it
    /// returns the AST, so parsed nodes normally retain only substitutions that
    /// require converter-specific rendering. Repeated entries remain significant. An
    /// empty list means no further substitutions, as with `+++text+++` and `pass:[text]`.
    pub(crate) subs: Vec<Substitution>,
}

impl<'a> Raw<'a> {
    /// Construct prepared passthrough text with its remaining substitutions.
    #[must_use]
    pub fn new(content: &'a str, substitutions: Vec<Substitution>, location: Location) -> Self {
        Self {
            metadata: None,
            content,
            location,
            subs: substitutions,
        }
    }

    /// Replace prepared text, keeping its substitutions, identity and source span.
    pub fn replace_text(&mut self, content: &'a str) -> &'a str {
        std::mem::replace(&mut self.content, content)
    }

    /// Replace the ordered substitutions and return the previous sequence.
    pub fn replace_substitutions(&mut self, substitutions: Vec<Substitution>) -> Vec<Substitution> {
        std::mem::replace(&mut self.subs, substitutions)
    }

    /// Transfer the substitutions, leaving no remaining substitutions.
    pub fn take_substitutions(&mut self) -> Vec<Substitution> {
        std::mem::take(&mut self.subs)
    }

    /// Replace identity and roles, preserving absent versus present empty metadata.
    pub fn set_metadata(&mut self, metadata: Option<crate::InlineMetadata<'a>>) {
        self.metadata = metadata.map(Box::new);
    }

    /// Copy only the optional identity and role metadata.
    #[must_use]
    pub fn clone_metadata(&self) -> Option<crate::InlineMetadata<'a>> {
        self.metadata.as_deref().cloned()
    }

    /// Transfer identity and roles, leaving metadata absent.
    pub fn take_metadata(&mut self) -> Option<crate::InlineMetadata<'a>> {
        self.metadata.take().map(|metadata| *metadata)
    }

    /// Replace this node's recorded source span.
    pub fn set_location(&mut self, location: Location) {
        self.location = location;
    }
}

/// Text from a literal, listing, or source context.
///
/// Verbatim text inherits substitutions from its enclosing block like [`Plain`],
/// but converters use literal or code presentation and preserve source whitespace
/// unless a requested substitution transforms it. Parsed callout markers are
/// represented by separate [`CalloutRef`] nodes rather than remaining in this text.
#[derive(Clone, Debug, PartialEq)]
pub struct Verbatim<'a> {
    pub(crate) content: &'a str,
    pub(crate) location: Location,
}

impl<'a> Verbatim<'a> {
    /// Construct prepared literal or code text without parsing inline syntax.
    #[must_use]
    pub fn new(content: &'a str, location: Location) -> Self {
        Self { content, location }
    }

    /// Replace prepared literal text and return the previous text.
    pub fn replace_text(&mut self, content: &'a str) -> &'a str {
        std::mem::replace(&mut self.content, content)
    }

    /// Replace this node's recorded source span.
    pub fn set_location(&mut self, location: Location) {
        self.location = location;
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
