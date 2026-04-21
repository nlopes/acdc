//! Block metadata types for `AsciiDoc` documents.

use serde::Serialize;

use super::anchor::Anchor;
use super::attributes::{AttributeValue, ElementAttributes};
use super::attribution::{Attribution, CiteTitle};
use super::location::Location;
use super::substitution::SubstitutionSpec;

pub type Role<'a> = &'a str;

/// A `BlockMetadata` represents the metadata of a block in a document.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[non_exhaustive]
pub struct BlockMetadata<'a> {
    #[serde(default, skip_serializing_if = "ElementAttributes::is_empty")]
    pub attributes: ElementAttributes<'a>,
    /// Parser intermediate state: positional attrs from `[foo,bar,baz]` that
    /// haven't yet been routed to named slots (style/width/height/…) or
    /// merged into `attributes`. Grammar rules drain this before the block
    /// is finalised; external consumers never see non-empty values.
    #[serde(default, skip_serializing)]
    pub(crate) positional_attributes: Vec<&'a str>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub roles: Vec<Role<'a>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<&'a str>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<&'a str>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<Anchor<'a>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub anchors: Vec<Anchor<'a>>,
    /// Substitutions to apply to block content.
    ///
    /// - `None`: Use block-type defaults (VERBATIM for listing/literal, NORMAL for paragraphs)
    /// - `Some(Explicit([]))`: No substitutions (equivalent to `subs=none`)
    /// - `Some(Explicit(list))`: Use the explicit list of substitutions
    /// - `Some(Modifiers(ops))`: Apply modifier operations to block-type defaults
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub substitutions: Option<SubstitutionSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribution: Option<Attribution<'a>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub citetitle: Option<CiteTitle<'a>>,
    #[serde(skip)]
    pub location: Option<Location>,
}

impl<'a> BlockMetadata<'a> {
    /// Create a new block metadata with default values.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the attributes.
    #[must_use]
    pub fn with_attributes(mut self, attributes: ElementAttributes<'a>) -> Self {
        self.attributes = attributes;
        self
    }

    /// Set the options.
    #[must_use]
    pub fn with_options(mut self, options: Vec<&'a str>) -> Self {
        self.options = options;
        self
    }

    /// Set the roles.
    #[must_use]
    pub fn with_roles(mut self, roles: Vec<Role<'a>>) -> Self {
        self.roles = roles;
        self
    }

    /// Set the style.
    #[must_use]
    pub fn with_style(mut self, style: Option<&'a str>) -> Self {
        self.style = style;
        self
    }

    /// Set the ID.
    #[must_use]
    pub fn with_id(mut self, id: Option<Anchor<'a>>) -> Self {
        self.id = id;
        self
    }

    pub(crate) fn move_positional_attributes_to_attributes(&mut self) {
        for positional_attribute in self.positional_attributes.drain(..) {
            self.attributes.insert(
                std::borrow::Cow::Borrowed(positional_attribute),
                AttributeValue::None,
            );
        }
    }

    #[must_use]
    pub fn is_default(&self) -> bool {
        self.roles.is_empty()
            && self.options.is_empty()
            && self.style.is_none()
            && self.id.is_none()
            && self.anchors.is_empty()
            && self.attributes.is_empty()
            && self.positional_attributes.is_empty()
            && self.substitutions.is_none()
            && self.attribution.is_none()
            && self.citetitle.is_none()
    }

    #[tracing::instrument(level = "debug")]
    pub(crate) fn merge(&mut self, other: &BlockMetadata<'a>) {
        self.attributes.merge(other.attributes.clone());
        self.positional_attributes
            .extend(other.positional_attributes.clone());
        self.roles.extend(other.roles.clone());
        self.options.extend(other.options.clone());
        if self.style.is_none() {
            self.style.clone_from(&other.style);
        }
        if self.id.is_none() {
            self.id.clone_from(&other.id);
        }
        self.anchors.extend(other.anchors.clone());
        if self.substitutions.is_none() {
            self.substitutions.clone_from(&other.substitutions);
        }
        if self.attribution.is_none() {
            self.attribution.clone_from(&other.attribution);
        }
        if self.citetitle.is_none() {
            self.citetitle.clone_from(&other.citetitle);
        }
    }
}
