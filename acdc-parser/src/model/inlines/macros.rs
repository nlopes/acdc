use std::{collections::HashSet, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::{ElementAttributes, Location, Substitution};

/// A `Pass` represents a passthrough macro in a document.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Pass {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "HashSet::is_empty")]
    pub substitutions: HashSet<Substitution>,
    pub location: Location,
}

/// An `Icon` represents an inline icon in a document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Icon {
    pub target: String,
    pub attributes: ElementAttributes,
    pub location: Location,
}

/// A `Link` represents an inline link in a document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Link {
    pub target: LinkTarget,
    pub attributes: ElementAttributes,
    pub location: Location,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum LinkTarget {
    Url(String),
    Path(PathBuf),
}

/// An `Url` represents an inline URL in a document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Url {
    pub target: String,
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
    pub url: String,
    pub location: Location,
}
