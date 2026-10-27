//! Block metadata types for `AsciiDoc` documents.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::CompatibilityMode;

use super::{
    DocumentAttribute,
    anchor::Anchor,
    attributes::{AttributeValue, ElementAttributes},
    attribution::{Attribution, CiteTitle},
    caption::Caption,
    content_attribute::ContentAttribute,
    location::Location,
    substitution::{Substitution, SubstitutionSpec},
};

pub type Role<'a> = &'a str;

/// Order in the expanded input, independent of remapped source locations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct AttributePosition {
    pub(crate) list_start: usize,
    pub(crate) slot: usize,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct AttributePositions<'a> {
    pub(crate) first: Option<AttributePosition>,
    pub(crate) named: BTreeMap<&'a str, AttributePosition>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct PositionalAttribute<'a> {
    pub(crate) position: Option<AttributePosition>,
    pub(crate) value: &'a str,
    pub(crate) quoted_empty: bool,
    pub(crate) location: Option<Location>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct DocumentAttributeEvents<'a>(Vec<DocumentAttribute<'a>>);

/// A `BlockMetadata` represents the metadata of a block in a document.
///
/// Equality includes the values and empty slots from [`Self::positional_attributes`],
/// but not their source locations. The block's own [`Self::location`] still participates.
#[derive(Clone, Debug, Default, Serialize)]
#[non_exhaustive]
pub struct BlockMetadata<'a> {
    #[serde(skip)]
    pub(crate) attribute_positions: Option<Box<AttributePositions<'a>>>,
    // Store uncommon parser events separately to keep each block's metadata small.
    #[serde(skip)]
    pub(crate) document_attributes: Option<Box<DocumentAttributeEvents<'a>>>,
    /// Caption behavior resolved from the document attributes in effect at this block's
    /// source position, and the ordinal assigned to it. `None` means caller-built metadata
    /// or a block that takes no caption.
    #[serde(
        skip_serializing_if = "caption_has_no_content",
        serialize_with = "serialize_caption_content"
    )]
    pub caption: Option<Caption<'a>>,
    #[serde(skip)]
    pub(crate) alt_source: Option<Box<super::content_attribute::PendingContentAttribute<'a>>>,
    #[serde(skip)]
    pub(crate) first_source: Option<Box<super::content_attribute::PendingContentAttribute<'a>>>,
    #[serde(default, skip_serializing_if = "ElementAttributes::is_empty")]
    pub attributes: ElementAttributes<'a>,
    /// Explicit reference label, interpreted independently of the block body.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reftext: Option<ContentAttribute<'a>>,
    #[serde(skip)]
    // Key into parser-owned input, cleared before returning the AST.
    pub(crate) reftext_source: Option<usize>,
    /// Parser working slots, consumed as attributes are routed to their block context.
    #[serde(default, skip_serializing)]
    pub(crate) positional_attributes: Vec<PositionalAttribute<'a>>,
    // Preserve the original slots before routing consumes working slots.
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
    /// Substitution settings for block content, available in every build.
    ///
    /// - `None`: Use block-type defaults (VERBATIM for listing/literal, NORMAL for paragraphs)
    /// - `Some(Source(entries))`: Preserve authored entries; resolve against block defaults as needed
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

impl PartialEq for BlockMetadata<'_> {
    fn eq(&self, other: &Self) -> bool {
        let Self {
            attribute_positions: _,
            document_attributes,
            caption,
            alt_source: _,
            first_source: _,
            attributes,
            reftext,
            reftext_source: _,
            positional_attributes: _,
            retained_positional_attributes: _,
            roles,
            options,
            style,
            id,
            anchors,
            substitutions,
            attribution,
            citetitle,
            location,
        } = self;
        document_attributes == &other.document_attributes
            && caption == &other.caption
            && attributes == &other.attributes
            && reftext == &other.reftext
            && roles == &other.roles
            && options == &other.options
            && style == &other.style
            && id == &other.id
            && anchors == &other.anchors
            && substitutions == &other.substitutions
            && attribution == &other.attribution
            && citetitle == &other.citetitle
            && location == &other.location
            && self
                .positional_attributes()
                .eq(other.positional_attributes())
    }
}

impl<'a> BlockMetadata<'a> {
    /// Explicit caption content, independent of generated labels and counters.
    #[must_use]
    pub fn caption_content(&self) -> Option<&super::ContentAttribute<'a>> {
        match &self.caption {
            Some(Caption::CustomContent(content)) => Some(content),
            _ => None,
        }
    }

    /// Create a new block metadata with default values.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether a substitution is enabled after applying this block's `subs` setting.
    ///
    /// `defaults` contains individual substitutions, such as [`crate::VERBATIM`]
    /// for source blocks. An explicit setting overrides or modifies these defaults.
    #[must_use]
    pub fn uses_substitution(
        &self,
        substitution: &Substitution,
        defaults: &[Substitution],
    ) -> bool {
        if let Some(spec) = &self.substitutions {
            return spec.contains(substitution, defaults);
        }
        defaults.contains(substitution)
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
    /// Consecutive lists merge by slot. Named entries leave earlier positional values
    /// intact. Asciidoctor mode lets an empty position clear an earlier value; Draft
    /// keeps the earlier value. Earlier trailing slots remain in both modes.
    ///
    /// Values remain available after context-specific routing. This positional view
    /// is not added to JSON. Markdown fence languages do not add positional slots.
    ///
    /// This is a snapshot of the parsed, merged slots. Changing [`Self::attributes`]
    /// or [`Self::style`] afterward does not update it. For example, after parsing
    /// `[source,rust]`, setting the named `language` attribute to `python` leaves
    /// positional slot 0 as `Some("rust")`. Metadata created with [`Self::new`] or
    /// [`Self::default`] has no positional slots; setting attributes does not add them.
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

    fn retain_positional_attributes(&mut self) {
        self.retained_positional_attributes
            .get_or_insert_with(|| self.positional_attributes.clone());
    }

    pub(crate) fn take_next_positional_attribute(&mut self) -> Option<PositionalAttribute<'a>> {
        self.retain_positional_attributes();
        if self.positional_attributes.is_empty() {
            None
        } else {
            Some(self.positional_attributes.remove(0))
        }
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

    pub(crate) fn overlay_positional_attributes(
        &mut self,
        other: &BlockMetadata<'a>,
        mode: CompatibilityMode,
    ) {
        if self.retained_positional_attributes.is_some()
            || other.retained_positional_attributes.is_some()
        {
            self.retain_positional_attributes();
            if let Some(retained) = &mut self.retained_positional_attributes {
                overlay_positional_slots(retained, other.raw_positional_attributes(), mode);
            }
        }
        overlay_positional_slots(
            &mut self.positional_attributes,
            &other.positional_attributes,
            mode,
        );
    }

    /// Whether this metadata serializes to nothing.
    ///
    /// This gates `skip_serializing_if` on every block's `metadata` field, so it covers only
    /// the serialized fields. Only authored caption content is serialized; generated
    /// caption labels and ordinals do not make metadata nonempty.
    #[must_use]
    pub fn is_default(&self) -> bool {
        self.roles.is_empty()
            && self.options.is_empty()
            && self.style.is_none()
            && self.id.is_none()
            && self.anchors.is_empty()
            && self.attributes.is_empty()
            && self.reftext.is_none()
            && self.positional_attributes.is_empty()
            && self.substitutions.is_none()
            && caption_has_no_content(&self.caption)
            && self.attribution.is_none()
            && self.citetitle.is_none()
    }
}

fn overlay_positional_slots<'a>(
    destination: &mut Vec<PositionalAttribute<'a>>,
    source: &[PositionalAttribute<'a>],
    mode: CompatibilityMode,
) {
    if destination.len() < source.len() {
        destination.resize(source.len(), PositionalAttribute::default());
    }
    for (destination, attribute) in destination.iter_mut().zip(source) {
        if (attribute.position.is_some() || !attribute.value.is_empty())
            && (mode == CompatibilityMode::Asciidoctor || !attribute.value.is_empty())
        {
            destination.clone_from(attribute);
        }
    }
}

#[expect(clippy::ref_option, reason = "Serde passes a reference to the field")]
fn caption_has_no_content(caption: &Option<Caption<'_>>) -> bool {
    !matches!(caption, Some(Caption::CustomContent(_)))
}

#[expect(clippy::ref_option, reason = "Serde passes a reference to the field")]
fn serialize_caption_content<S: serde::Serializer>(
    caption: &Option<Caption<'_>>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    if let Some(Caption::CustomContent(content)) = caption {
        content.serialize(serializer)
    } else {
        serializer.serialize_none()
    }
}
