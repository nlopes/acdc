use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::{ElementAttributes, InlineNode, Location, Source, Substitution};

/// A `Pass` represents a passthrough macro in a document.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Pass {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "HashSet::is_empty")]
    pub substitutions: HashSet<Substitution>,
    pub location: Location,
    #[serde(skip)]
    pub kind: PassthroughKind,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default)]
pub enum PassthroughKind {
    #[default]
    Single,
    Double,
    Triple,
    Macro,
}

/// An `Icon` represents an inline icon in a document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Icon {
    pub target: Source,
    pub attributes: ElementAttributes,
    pub location: Location,
}

/// A `Link` represents an inline link in a document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Link {
    // We don't serialize the text here because it's already serialized in the attributes
    // (that's how it's represented in the ASG)
    #[serde(skip_serializing)]
    pub text: Option<String>,
    pub target: Source,
    pub attributes: ElementAttributes,
    pub location: Location,
}

/// An `Url` represents an inline URL in a document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Url {
    // We don't serialize the text here because it's already serialized in the attributes
    // (that's how it's represented in the ASG)
    #[serde(skip_serializing)]
    pub text: Vec<InlineNode>,
    pub target: Source,
    pub attributes: ElementAttributes,
    pub location: Location,
}

/// A `Button` represents an inline button in a document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Button {
    pub label: String,
    pub location: Location,
}

/// A `Menu` represents an inline menu in a document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Menu {
    pub target: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<String>,
    pub location: Location,
}

/// A `Keyboard` represents an inline keyboard shortcut in a document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Keyboard {
    pub keys: Vec<Key>,
    pub location: Location,
}

// TODO(nlopes): this could perhaps be an enum instead with the allowed keys
pub type Key = String;

/// An `Autolink` represents an inline autolink in a document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Autolink {
    pub url: Source,
    pub location: Location,
}
