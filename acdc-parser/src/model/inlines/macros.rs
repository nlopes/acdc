use serde::Serialize;

use crate::{ElementAttributes, InlineNode, Location, Source, StemNotation, Substitution};

pub const ICON_SIZES: &[&str] = &["1x", "2x", "3x", "4x", "5x", "lg", "fw"];

/// A `Pass` represents a passthrough macro in a document.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[non_exhaustive]
pub struct Pass<'a> {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<&'a str>,
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
    /// Character replacement attribute expanded as passthrough (e.g., `{plus}` → `+`).
    /// The location spans the `{attr}` reference, not delimiters.
    AttributeRef,
}

/// A `Footnote` represents an inline footnote in a document.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct Footnote<'a> {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<&'a str>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub content: Vec<InlineNode<'a>>,
    #[serde(skip)]
    pub number: u32,
    pub location: Location,
}

/// An `Icon` represents an inline icon in a document.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct Icon<'a> {
    pub target: Source<'a>,
    pub attributes: ElementAttributes<'a>,
    pub location: Location,
}

/// A `Link` represents an inline link in a document.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct Link<'a> {
    // We don't serialize the text here because it's already serialized in the attributes
    // (that's how it's represented in the ASG)
    #[serde(skip_serializing)]
    pub text: Option<&'a str>,
    pub target: Source<'a>,
    pub attributes: ElementAttributes<'a>,
    pub location: Location,
}

impl<'a> Link<'a> {
    /// Creates a new `Link` with the given target.
    #[must_use]
    pub fn new(target: Source<'a>, location: Location) -> Self {
        Self {
            text: None,
            target,
            attributes: ElementAttributes::default(),
            location,
        }
    }

    /// Sets the link text.
    #[must_use]
    pub fn with_text(mut self, text: Option<&'a str>) -> Self {
        self.text = text;
        self
    }

    /// Sets the link attributes.
    #[must_use]
    pub fn with_attributes(mut self, attributes: ElementAttributes<'a>) -> Self {
        self.attributes = attributes;
        self
    }
}

/// An `Url` represents an inline URL in a document.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct Url<'a> {
    #[serde(skip_serializing)]
    pub text: Vec<InlineNode<'a>>,
    pub target: Source<'a>,
    pub attributes: ElementAttributes<'a>,
    pub location: Location,
}

/// An `Mailto` represents an inline `mailto:` in a document.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct Mailto<'a> {
    #[serde(skip_serializing)]
    pub text: Vec<InlineNode<'a>>,
    pub target: Source<'a>,
    pub attributes: ElementAttributes<'a>,
    pub location: Location,
}

/// A `Button` represents an inline button in a document.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct Button<'a> {
    pub label: &'a str,
    pub location: Location,
}

/// A `Menu` represents an inline menu in a document.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct Menu<'a> {
    pub target: &'a str,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<&'a str>,
    pub location: Location,
}

/// A `Keyboard` represents an inline keyboard shortcut in a document.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct Keyboard<'a> {
    pub keys: Vec<Key<'a>>,
    pub location: Location,
}

impl<'a> Keyboard<'a> {
    /// Creates a new `Keyboard` with the given keys.
    #[must_use]
    pub fn new(keys: Vec<Key<'a>>, location: Location) -> Self {
        Self { keys, location }
    }
}

// TODO(nlopes): this could perhaps be an enum instead with the allowed keys
pub type Key<'a> = &'a str;

/// A `CrossReference` represents an inline cross-reference (xref) in a document.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct CrossReference<'a> {
    pub target: &'a str,
    #[serde(skip_serializing)]
    pub text: Vec<InlineNode<'a>>,
    pub location: Location,
}

impl<'a> CrossReference<'a> {
    /// Creates a new `CrossReference` with the given target.
    #[must_use]
    pub fn new(target: &'a str, location: Location) -> Self {
        Self {
            target,
            text: Vec::new(),
            location,
        }
    }

    /// Sets the cross-reference display text as inline nodes.
    #[must_use]
    pub fn with_text(mut self, text: Vec<InlineNode<'a>>) -> Self {
        self.text = text;
        self
    }
}

/// An `Autolink` represents an inline autolink in a document.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct Autolink<'a> {
    pub url: Source<'a>,
    /// Whether the autolink was written with angle brackets (e.g., `<user@example.com>`).
    /// When true, the renderer should preserve the brackets in the output.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bracketed: bool,
    pub location: Location,
}

/// A `Stem` represents an inline mathematical expression.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct Stem<'a> {
    pub content: &'a str,
    pub notation: StemNotation,
    pub location: Location,
}

/// The kind of index term, encoding both visibility and structure.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub enum IndexTermKind<'a> {
    /// Visible in output, single term only.
    Flow(&'a str),
    /// Hidden from output, supports hierarchical entries.
    Concealed {
        term: &'a str,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        secondary: Option<&'a str>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tertiary: Option<&'a str>,
    },
}

/// An `IndexTerm` represents an index term in a document.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct IndexTerm<'a> {
    /// The kind and content of this index term.
    pub kind: IndexTermKind<'a>,
    pub location: Location,
}

impl IndexTerm<'_> {
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
            IndexTermKind::Concealed { secondary, .. } => *secondary,
        }
    }

    /// Returns the tertiary term, if any.
    #[must_use]
    pub fn tertiary(&self) -> Option<&str> {
        match &self.kind {
            IndexTermKind::Flow(_) => None,
            IndexTermKind::Concealed { tertiary, .. } => *tertiary,
        }
    }

    /// Returns whether this term is visible in the output.
    #[must_use]
    pub fn is_visible(&self) -> bool {
        matches!(self.kind, IndexTermKind::Flow(_))
    }
}
