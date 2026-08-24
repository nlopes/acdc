use std::{fmt, num::NonZeroUsize};

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
    #[serde(skip_serializing)]
    pub text: Vec<InlineNode<'a>>,
    pub target: Source<'a>,
    pub attributes: ElementAttributes<'a>,
    pub location: Location,
    #[serde(skip)]
    pub(crate) hide_uri_scheme: bool,
}

impl<'a> Link<'a> {
    /// Creates a new `Link` with the given target.
    #[must_use]
    pub fn new(target: Source<'a>, location: Location) -> Self {
        Self {
            text: Vec::new(),
            target,
            attributes: ElementAttributes::default(),
            location,
            hide_uri_scheme: false,
        }
    }

    /// Sets the link text as inline nodes.
    #[must_use]
    pub fn with_text(mut self, text: Vec<InlineNode<'a>>) -> Self {
        self.text = text;
        self
    }

    /// Sets the link attributes.
    #[must_use]
    pub fn with_attributes(mut self, attributes: ElementAttributes<'a>) -> Self {
        self.attributes = attributes;
        self
    }

    /// Whether fallback display text omits the target's URI scheme.
    #[must_use]
    pub fn hides_uri_scheme(&self) -> bool {
        self.hide_uri_scheme
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
    #[serde(skip)]
    pub(crate) hide_uri_scheme: bool,
}

impl Url<'_> {
    /// Whether fallback display text omits the target's URI scheme.
    #[must_use]
    pub fn hides_uri_scheme(&self) -> bool {
        self.hide_uri_scheme
    }
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
#[derive(Clone, Serialize)]
#[non_exhaustive]
pub struct CrossReference<'a> {
    pub target: &'a str,
    #[serde(skip_serializing)]
    pub text: Vec<InlineNode<'a>>,
    pub location: Location,
    #[serde(skip)]
    pub xrefstyle: XrefStyle,
    #[serde(skip)]
    pub caption_label: XrefCaptionLabel<'a>,
    #[serde(skip)]
    pub(crate) caption_label_snapshot_id: Option<NonZeroUsize>,
}

impl<'a> CrossReference<'a> {
    /// Creates a new `CrossReference` with the given target.
    #[must_use]
    pub fn new(target: &'a str, location: Location) -> Self {
        Self {
            target,
            text: Vec::new(),
            location,
            xrefstyle: XrefStyle::Basic,
            caption_label: XrefCaptionLabel::AtTarget,
            caption_label_snapshot_id: None,
        }
    }

    /// Sets the cross-reference display text as inline nodes.
    #[must_use]
    pub fn with_text(mut self, text: Vec<InlineNode<'a>>) -> Self {
        self.text = text;
        self
    }
}

impl fmt::Debug for CrossReference<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CrossReference")
            .field("target", &self.target)
            .field("text", &self.text)
            .field("location", &self.location)
            .field("xrefstyle", &self.xrefstyle)
            .field("caption_label", &self.caption_label)
            .finish()
    }
}

impl PartialEq for CrossReference<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.target == other.target
            && self.text == other.text
            && self.location == other.location
            && self.xrefstyle == other.xrefstyle
            && self.caption_label == other.caption_label
    }
}

/// Selects the label used by an automatic cross-reference to a numbered caption.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub enum XrefCaptionLabel<'a> {
    /// Use the caption label recorded on the target.
    #[default]
    AtTarget,
    /// Use the caption label active at the reference position.
    AtReference(&'a str),
    /// Omit the label and show only the caption number.
    NumberOnly,
}

/// The display style for an automatic cross-reference.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub enum XrefStyle {
    /// Use the target title without its caption prefix.
    #[default]
    Basic,
    /// Use only the target's caption label and number or custom prefix.
    Short,
    /// Use the caption prefix followed by the target title.
    Full,
}

impl XrefStyle {
    pub(crate) fn from_attribute(value: Option<&str>) -> Self {
        match value {
            Some("short") => Self::Short,
            Some("full") => Self::Full,
            _ => Self::Basic,
        }
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
    #[serde(skip)]
    pub(crate) hide_uri_scheme: bool,
}

impl Autolink<'_> {
    /// Whether fallback display text omits the target's URI scheme.
    #[must_use]
    pub fn hides_uri_scheme(&self) -> bool {
        self.hide_uri_scheme
    }
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
    /// A single term that is visible in the document and included in the index.
    Flow(Vec<InlineNode<'a>>),
    /// Hidden from output, supports hierarchical entries.
    Concealed {
        /// The fully substituted primary term.
        term: Vec<InlineNode<'a>>,
        /// The fully substituted secondary term, if present.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        secondary: Option<Vec<InlineNode<'a>>>,
        /// The fully substituted tertiary term, if present.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tertiary: Option<Vec<InlineNode<'a>>>,
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

impl<'a> IndexTerm<'a> {
    /// Returns the primary term.
    #[must_use]
    pub fn term(&self) -> &[InlineNode<'a>] {
        match &self.kind {
            IndexTermKind::Flow(term) | IndexTermKind::Concealed { term, .. } => term,
        }
    }

    /// Returns the secondary term, if any.
    #[must_use]
    pub fn secondary(&self) -> Option<&[InlineNode<'a>]> {
        match &self.kind {
            IndexTermKind::Flow(_) => None,
            IndexTermKind::Concealed { secondary, .. } => secondary.as_deref(),
        }
    }

    /// Returns the tertiary term, if any.
    #[must_use]
    pub fn tertiary(&self) -> Option<&[InlineNode<'a>]> {
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
