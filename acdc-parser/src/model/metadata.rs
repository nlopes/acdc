//! Block metadata types for `AsciiDoc` documents.

use serde::Serialize;

use super::anchor::Anchor;
use super::attributes::{AttributeValue, ElementAttributes};
use super::attribution::{Attribution, CiteTitle};
use super::location::Location;
#[cfg(feature = "pre-spec-subs")]
use super::substitution::SubstitutionSpec;

pub type Role<'a> = &'a str;

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct PositionalAttribute<'a> {
    pub(crate) value: &'a str,
    pub(crate) substitutions: bool,
    pub(crate) location: Option<Location>,
}

/// A `BlockMetadata` represents the metadata of a block in a document.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[non_exhaustive]
pub struct BlockMetadata<'a> {
    #[serde(default, skip_serializing_if = "ElementAttributes::is_empty")]
    pub attributes: ElementAttributes<'a>,
    /// Parser intermediate state: positional attrs from `[foo,bar,baz]` that
    /// have not yet been routed to context-specific slots or merged into
    /// `attributes`. Grammar rules drain these before finalising the block.
    #[serde(default, skip_serializing)]
    pub(crate) positional_attributes: Vec<PositionalAttribute<'a>>,
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
    /// Substitutions to apply to block content. Only present when the
    /// `pre-spec-subs` feature is enabled; the draft `AsciiDoc` spec is
    /// dropping the substitution model in favour of an inline parsing
    /// grammar, so this field is feature-gated to reflect that.
    ///
    /// - `None`: Use block-type defaults (VERBATIM for listing/literal, NORMAL for paragraphs)
    /// - `Some(Explicit([]))`: No substitutions (equivalent to `subs=none`)
    /// - `Some(Explicit(list))`: Use the explicit list of substitutions
    /// - `Some(Modifiers(ops))`: Apply modifier operations to block-type defaults
    #[cfg(feature = "pre-spec-subs")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub substitutions: Option<SubstitutionSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribution: Option<Attribution<'a>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub citetitle: Option<CiteTitle<'a>>,
    #[serde(default, skip_serializing)]
    pub(crate) attribution_substitutions: bool,
    #[serde(default, skip_serializing)]
    pub(crate) citetitle_substitutions: bool,
    #[serde(skip)]
    pub location: Option<Location>,
}

impl<'a> BlockMetadata<'a> {
    /// Create a new block metadata with default values.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The anchor that defines this block's id: the explicit `id` (`[#id]`),
    /// otherwise the first `[[id]]` anchor. `None` when the block has no id.
    pub(crate) fn id_anchor(&self) -> Option<&Anchor<'a>> {
        self.id.as_ref().or_else(|| self.anchors.first())
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
            if !positional_attribute.value.is_empty() {
                self.attributes.insert(
                    std::borrow::Cow::Borrowed(positional_attribute.value),
                    AttributeValue::None,
                );
            }
        }
    }

    pub(crate) fn overlay_positional_attributes(&mut self, other: &[PositionalAttribute<'a>]) {
        if self.positional_attributes.len() < other.len() {
            self.positional_attributes
                .resize(other.len(), PositionalAttribute::default());
        }
        for (slot, attribute) in other.iter().enumerate() {
            if let Some(destination) = self.positional_attributes.get_mut(slot) {
                destination.clone_from(attribute);
            }
        }
    }

    #[must_use]
    pub fn is_default(&self) -> bool {
        #[cfg(feature = "pre-spec-subs")]
        let subs_default = self.substitutions.is_none();
        #[cfg(not(feature = "pre-spec-subs"))]
        let subs_default = true;
        self.roles.is_empty()
            && self.options.is_empty()
            && self.style.is_none()
            && self.id.is_none()
            && self.anchors.is_empty()
            && self.attributes.is_empty()
            && self.positional_attributes.is_empty()
            && subs_default
            && self.attribution.is_none()
            && self.citetitle.is_none()
    }

    #[tracing::instrument(level = "debug")]
    pub(crate) fn merge(&mut self, other: &BlockMetadata<'a>) {
        for (name, value) in other.attributes.iter() {
            self.attributes.set(name.clone(), value.clone());
        }
        self.overlay_positional_attributes(&other.positional_attributes);
        if !other.roles.is_empty() {
            self.roles.clone_from(&other.roles);
        }
        if !other.options.is_empty() {
            self.options.clone_from(&other.options);
        }
        if other.style.is_some() {
            self.style.clone_from(&other.style);
        }
        if other.id.is_some() {
            self.id.clone_from(&other.id);
        }
        self.anchors.extend(other.anchors.clone());
        #[cfg(feature = "pre-spec-subs")]
        if other.substitutions.is_some() {
            self.substitutions.clone_from(&other.substitutions);
        }
        if other.attribution.is_some() {
            self.attribution.clone_from(&other.attribution);
            self.attribution_substitutions = other.attribution_substitutions;
        }
        if other.citetitle.is_some() {
            self.citetitle.clone_from(&other.citetitle);
            self.citetitle_substitutions = other.citetitle_substitutions;
        }
    }
}
