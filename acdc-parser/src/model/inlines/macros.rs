use serde::Serialize;

use crate::{ElementAttributes, InlineNode, Location, Source, StemNotation, Substitution};

pub const ICON_SIZES: &[&str] = &["1x", "2x", "3x", "4x", "5x", "lg", "fw"];

/// A `Pass` represents a passthrough macro in a document.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[non_exhaustive]
pub struct Pass {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub substitutions: Vec<Substitution>,
    pub location: Location,
    #[serde(skip)]
    pub kind: PassthroughKind,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Default)]
pub enum PassthroughKind {
    #[default]
    Single,
    Double,
    Triple,
    Macro,
}

/// A `Footnote` represents an inline footnote in a document.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct Footnote {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub content: Vec<InlineNode>,
    #[serde(skip)]
    pub number: u32,
    pub location: Location,
}

/// An `Icon` represents an inline icon in a document.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct Icon {
    pub target: Source,
    pub attributes: ElementAttributes,
    pub location: Location,
}

/// A `Link` represents an inline link in a document.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct Link {
    // We don't serialize the text here because it's already serialized in the attributes
    // (that's how it's represented in the ASG)
    #[serde(skip_serializing)]
    pub text: Option<String>,
    pub target: Source,
    pub attributes: ElementAttributes,
    pub location: Location,
}

impl Link {
    /// Creates a new `Link` with the given target.
    #[must_use]
    pub fn new(target: Source, location: Location) -> Self {
        Self {
            text: None,
            target,
            attributes: ElementAttributes::default(),
            location,
        }
    }

    /// Sets the link text.
    #[must_use]
    pub fn with_text(mut self, text: Option<String>) -> Self {
        self.text = text;
        self
    }

    /// Sets the link attributes.
    #[must_use]
    pub fn with_attributes(mut self, attributes: ElementAttributes) -> Self {
        self.attributes = attributes;
        self
    }
}

/// An `Url` represents an inline URL in a document.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct Url {
    // We don't serialize the text here because it's already serialized in the attributes
    // (that's how it's represented in the ASG)
    #[serde(skip_serializing)]
    pub text: Vec<InlineNode>,
    pub target: Source,
    pub attributes: ElementAttributes,
    pub location: Location,
}

/// An `Mailto` represents an inline `mailto:` in a document.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct Mailto {
    // We don't serialize the text here because it's already serialized in the attributes
    // (that's how it's represented in the ASG)
    #[serde(skip_serializing)]
    pub text: Vec<InlineNode>,
    pub target: Source,
    pub attributes: ElementAttributes,
    pub location: Location,
}

/// A `Button` represents an inline button in a document.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct Button {
    pub label: String,
    pub location: Location,
}

/// A `Menu` represents an inline menu in a document.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct Menu {
    pub target: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<String>,
    pub location: Location,
}

/// A `Keyboard` represents an inline keyboard shortcut in a document.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct Keyboard {
    pub keys: Vec<Key>,
    pub location: Location,
}

impl Keyboard {
    /// Creates a new `Keyboard` with the given keys.
    #[must_use]
    pub fn new(keys: Vec<Key>, location: Location) -> Self {
        Self { keys, location }
    }
}

// TODO(nlopes): this could perhaps be an enum instead with the allowed keys
pub type Key = String;

/// A `CrossReference` represents an inline cross-reference (xref) in a document.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct CrossReference {
    pub target: String,
    // We don't serialize the text here because it's serialized as "inlines" in the ASG
    #[serde(skip_serializing)]
    pub text: Vec<InlineNode>,
    pub location: Location,
}

impl CrossReference {
    /// Creates a new `CrossReference` with the given target.
    #[must_use]
    pub fn new(target: impl Into<String>, location: Location) -> Self {
        Self {
            target: target.into(),
            text: Vec::new(),
            location,
        }
    }

    /// Sets the cross-reference display text as inline nodes.
    #[must_use]
    pub fn with_text(mut self, text: Vec<InlineNode>) -> Self {
        self.text = text;
        self
    }
}

/// An `Autolink` represents an inline autolink in a document.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct Autolink {
    pub url: Source,
    /// Whether the autolink was written with angle brackets (e.g., `<user@example.com>`).
    /// When true, the renderer should preserve the brackets in the output.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bracketed: bool,
    pub location: Location,
}

/// A `Stem` represents an inline mathematical expression.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct Stem {
    pub content: String,
    pub notation: StemNotation,
    pub location: Location,
}

/// The kind of index term, encoding both visibility and structure.
///
/// This enum makes invalid states unrepresentable: flow terms can only have
/// a single term (no hierarchy), while concealed terms support up to three
/// hierarchical levels.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub enum IndexTermKind {
    /// Visible in output, single term only.
    ///
    /// Created by `((term))` or `indexterm2:[term]`.
    Flow(String),
    /// Hidden from output, supports hierarchical entries.
    ///
    /// Created by `(((term,secondary,tertiary)))` or `indexterm:[term,secondary,tertiary]`.
    Concealed {
        term: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        secondary: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tertiary: Option<String>,
    },
}

/// An `IndexTerm` represents an index term in a document.
///
/// Index terms can be either:
/// - **Flow terms** (visible): `((term))` or `indexterm2:[term]` - the term appears in the text
/// - **Concealed terms** (hidden): `(((term,secondary,tertiary)))` or `indexterm:[term,secondary,tertiary]`
///   - only appears in the index
///
/// Concealed terms support hierarchical entries with primary, secondary, and tertiary levels.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct IndexTerm {
    /// The kind and content of this index term.
    pub kind: IndexTermKind,
    pub location: Location,
}

impl IndexTerm {
    /// Returns the primary term.
    #[must_use]
    pub fn term(&self) -> &str {
        match &self.kind {
            IndexTermKind::Flow(term) | IndexTermKind::Concealed { term, .. } => term,
        }
    }

    /// Returns the secondary term, if any.
    #[must_use]
    pub fn secondary(&self) -> Option<&str> {
        match &self.kind {
            IndexTermKind::Flow(_) => None,
            IndexTermKind::Concealed { secondary, .. } => secondary.as_deref(),
        }
    }

    /// Returns the tertiary term, if any.
    #[must_use]
    pub fn tertiary(&self) -> Option<&str> {
        match &self.kind {
            IndexTermKind::Flow(_) => None,
            IndexTermKind::Concealed { tertiary, .. } => tertiary.as_deref(),
        }
    }

    /// Returns whether this term is visible in the output.
    #[must_use]
    pub fn is_visible(&self) -> bool {
        matches!(self.kind, IndexTermKind::Flow(_))
    }
}
