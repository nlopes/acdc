//! Block metadata types for `AsciiDoc` documents.

use serde::Serialize;

use super::anchor::Anchor;
use super::attributes::{AttributeValue, ElementAttributes};
use super::attribution::{Attribution, CiteTitle};
use super::caption::Caption;
#[cfg(feature = "pre-spec-subs")]
use super::substitution::SubstitutionSpec;
use super::{DocumentAttribute, location::Location};

pub type Role<'a> = &'a str;

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct PositionalAttribute<'a> {
    pub(crate) value: &'a str,
    pub(crate) substitutions: bool,
    pub(crate) location: Option<Location>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct DocumentAttributeEvents<'a>(Vec<DocumentAttribute<'a>>);

/// A `BlockMetadata` represents the metadata of a block in a document.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[non_exhaustive]
pub struct BlockMetadata<'a> {
    // Store uncommon parser events separately to keep each block's metadata small.
    #[serde(skip)]
    pub(crate) document_attributes: Option<Box<DocumentAttributeEvents<'a>>>,
    /// Caption behavior resolved from the document attributes in effect at this block's
    /// source position, and the ordinal assigned to it. `None` means caller-built metadata
    /// or a block that takes no caption.
    #[serde(skip)]
    pub caption: Option<Caption<'a>>,
    #[serde(default, skip_serializing_if = "ElementAttributes::is_empty")]
    pub attributes: ElementAttributes<'a>,
    /// Parser working slots, consumed as attributes are routed to their block context.
    #[serde(default, skip_serializing)]
    pub(crate) positional_attributes: Vec<PositionalAttribute<'a>>,
    // Preserve the original slots before routing consumes or inserts working slots.
    #[serde(skip)]
    pub(crate) retained_positional_attributes: Option<Vec<PositionalAttribute<'a>>>,
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

    /// Return a non-empty retained positional attribute by its zero-based slot.
    ///
    /// Returns `None` when the slot is empty or absent. See
    /// [`Self::positional_attributes`] for ordering, substitution, and merge behavior.
    #[must_use]
    pub fn positional_attribute(&self, index: usize) -> Option<&'a str> {
        self.raw_positional_attributes()
            .get(index)
            .map(|attribute| attribute.value)
            .filter(|value| !value.is_empty())
    }

    /// Iterate over retained positional slots after the first attribute-list entry.
    ///
    /// The first entry is usually a block style, or alt text or a poster in a media
    /// macro. It is excluded even when parsing consumes or clears [`Self::style`].
    ///
    /// For block attribute lists, surrounding quotes are removed and document-attribute
    /// references are expanded; inline markup remains literal. Empty strings (quoted
    /// or unquoted) and gaps before later positional values yield `None`. For example,
    /// `[tikz,target=x,svg]` yields `[None, Some("svg")]`; trailing named attributes do
    /// not extend the iterator.
    /// Consecutive lists merge by slot: later slots replace earlier ones, including
    /// empty slots, while earlier trailing slots remain.
    ///
    /// Values remain available after context-specific routing. This positional view
    /// is not added to JSON. Markdown fence languages do not add positional slots.
    #[must_use]
    pub fn positional_attributes(&self) -> impl ExactSizeIterator<Item = Option<&'a str>> + '_ {
        self.raw_positional_attributes()
            .iter()
            .map(|attribute| (!attribute.value.is_empty()).then_some(attribute.value))
    }

    pub(crate) fn raw_positional_attributes(&self) -> &[PositionalAttribute<'a>] {
        self.retained_positional_attributes
            .as_deref()
            .unwrap_or(&self.positional_attributes)
    }

    pub(crate) fn retain_positional_attributes(&mut self) {
        self.retained_positional_attributes
            .get_or_insert_with(|| self.positional_attributes.clone());
    }

    pub(crate) fn move_positional_attributes_to_attributes(&mut self) {
        let positional_attributes = std::mem::take(&mut self.positional_attributes);
        if self.style != Some("source") {
            for attribute in &positional_attributes {
                if !attribute.value.is_empty() {
                    self.attributes.insert(
                        std::borrow::Cow::Borrowed(attribute.value),
                        AttributeValue::None,
                    );
                }
            }
        }
        // Moving at finalization avoids cloning slots that routing did not consume.
        if self.retained_positional_attributes.is_none() {
            self.retained_positional_attributes = Some(positional_attributes);
        }
    }

    pub(crate) fn has_document_attributes(&self) -> bool {
        self.document_attributes.is_some()
    }

    pub(crate) fn push_document_attribute(&mut self, attribute: DocumentAttribute<'a>) {
        self.document_attributes
            .get_or_insert_with(Default::default)
            .0
            .push(attribute);
    }

    pub(crate) fn take_document_attributes(&mut self) -> Vec<DocumentAttribute<'a>> {
        self.document_attributes
            .take()
            .map_or_else(Vec::new, |attributes| attributes.0)
    }

    pub(crate) fn append_document_attributes(
        &mut self,
        mut attributes: Vec<DocumentAttribute<'a>>,
    ) {
        if attributes.is_empty() {
            return;
        }
        self.document_attributes
            .get_or_insert_with(Default::default)
            .0
            .append(&mut attributes);
    }

    pub(crate) fn overlay_positional_attributes(&mut self, other: &[PositionalAttribute<'a>]) {
        overlay_positional_slots(&mut self.positional_attributes, other);
    }

    /// Whether this metadata serializes to nothing.
    ///
    /// This gates `skip_serializing_if` on every block's `metadata` field, so it covers only
    /// the serialized fields. `caption` is deliberately excluded: it carries `#[serde(skip)]`,
    /// so counting it would make a block emit an empty `metadata` object.
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
        if let Some(attributes) = &other.document_attributes {
            self.document_attributes
                .get_or_insert_with(Default::default)
                .0
                .extend(attributes.0.iter().cloned());
        }
        for (name, value) in other.attributes.iter() {
            self.attributes.set(name.clone(), value.clone());
        }
        if self.retained_positional_attributes.is_some()
            || other.retained_positional_attributes.is_some()
        {
            self.retain_positional_attributes();
            if let Some(retained) = &mut self.retained_positional_attributes {
                overlay_positional_slots(retained, other.raw_positional_attributes());
            }
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

fn overlay_positional_slots<'a>(
    destination: &mut Vec<PositionalAttribute<'a>>,
    source: &[PositionalAttribute<'a>],
) {
    if destination.len() < source.len() {
        destination.resize(source.len(), PositionalAttribute::default());
    }
    for (destination, attribute) in destination.iter_mut().zip(source) {
        destination.clone_from(attribute);
    }
}
