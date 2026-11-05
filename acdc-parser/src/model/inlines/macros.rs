use std::{fmt, num::NonZeroUsize};

use serde::Serialize;

use crate::{
    AttributeName, AttributeValue, ContentAttribute, ElementAttributes, InlineMetadata, InlineNode,
    Location, ReservedAttribute, Source, StemNotation, Substitution,
    model::metadata::{InlineMetadataView, MacroAttributes},
};

pub const ICON_SIZES: &[&str] = &["1x", "2x", "3x", "4x", "5x", "lg", "fw"];

/// A `Pass` represents a passthrough macro in a document.
///
/// ```compile_fail,E0616
/// use acdc_parser::{Location, Pass, PassthroughKind};
/// let mut value = Pass::new(
///     Some("text"), Vec::new(), PassthroughKind::Macro, Location::default(),
/// );
/// value.text = Some("changed");
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Pass<'a> {
    pub(crate) attribute_fragments: Box<[crate::model::AttributeInlineFragment]>,
    pub(crate) text: Option<&'a str>,
    pub(crate) substitutions: Vec<Substitution>,
    pub(crate) location: Location,
    pub(crate) kind: PassthroughKind,
}

impl<'a> Pass<'a> {
    /// Construct a passthrough value without processing its text or substitutions.
    #[must_use]
    pub fn new(
        text: Option<&'a str>,
        substitutions: Vec<Substitution>,
        kind: PassthroughKind,
        location: Location,
    ) -> Self {
        Self {
            attribute_fragments: Box::default(),
            text,
            substitutions,
            location,
            kind,
        }
    }

    /// Replace optional text without processing it or replaying earlier attribute values.
    pub fn replace_text(&mut self, text: Option<&'a str>) -> Option<&'a str> {
        self.attribute_fragments = Box::default();
        std::mem::replace(&mut self.text, text)
    }

    /// Replace the ordered substitutions without processing the text.
    /// Earlier attribute values are not reapplied.
    pub fn replace_substitutions(&mut self, substitutions: Vec<Substitution>) -> Vec<Substitution> {
        self.attribute_fragments = Box::default();
        std::mem::replace(&mut self.substitutions, substitutions)
    }

    /// Transfer the requested substitutions, leaving none.
    pub fn take_substitutions(&mut self) -> Vec<Substitution> {
        self.replace_substitutions(Vec::new())
    }

    /// Replace the passthrough form without interpreting text or reconstructing delimiters.
    pub fn set_kind(&mut self, kind: PassthroughKind) {
        self.attribute_fragments = Box::default();
        self.kind = kind;
    }

    /// Replace this node's recorded source span.
    pub fn set_location(&mut self, location: Location) {
        self.location = location;
    }
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
///
/// ```compile_fail,E0616
/// use acdc_parser::{Footnote, Location};
/// let mut note = Footnote::new(Vec::new(), 1, Location::default());
/// note.content = Vec::new();
/// ```
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Footnote<'a> {
    /// Keep discarded named bodies available for conflict diagnostics.
    pub(crate) definition_source: Option<&'a str>,
    /// Capture the body before later substitutions; consumed after source mapping.
    pub(crate) registration_substitutions: Option<crate::model::substitution::SubstitutionPlan>,
    pub(crate) id: Option<&'a str>,
    pub(crate) content: Vec<InlineNode<'a>>,
    pub(crate) number: u32,
    pub(crate) location: Location,
}

impl<'a> Footnote<'a> {
    /// Construct an anonymous note with prepared content and an assigned number.
    /// This does not register or renumber notes in a document.
    #[must_use]
    pub fn new(content: Vec<InlineNode<'a>>, number: u32, location: Location) -> Self {
        Self {
            definition_source: None,
            registration_substitutions: None,
            id: None,
            content,
            number,
            location,
        }
    }

    /// Replace the named key without registering a definition or resolving references.
    pub fn replace_id(&mut self, id: Option<&'a str>) -> Option<&'a str> {
        self.clear_registration_source();
        std::mem::replace(&mut self.id, id)
    }

    /// Replace prepared content without parsing it or updating registered definitions.
    pub fn replace_content(&mut self, content: Vec<InlineNode<'a>>) -> Vec<InlineNode<'a>> {
        self.clear_registration_source();
        std::mem::replace(&mut self.content, content)
    }

    /// Transfer the retained body, leaving this occurrence empty.
    pub fn take_content(&mut self) -> Vec<InlineNode<'a>> {
        self.replace_content(Vec::new())
    }

    /// Append a prepared inline without registering nested content.
    pub fn push_inline(&mut self, inline: InlineNode<'a>) {
        self.clear_registration_source();
        self.content.push(inline);
    }

    /// Replace the assigned number without renumbering other occurrences or catalogs.
    pub fn set_number(&mut self, number: u32) {
        self.number = number;
    }

    /// Replace the occurrence span, keeping the locations within its prepared body.
    pub fn set_location(&mut self, location: Location) {
        self.location = location;
    }

    fn clear_registration_source(&mut self) {
        self.definition_source = None;
        self.registration_substitutions = None;
    }
}

/// A `Button` represents an inline button in a document.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Button<'a> {
    pub label: ContentAttribute<'a>,
    pub location: Location,
}

/// A `Menu` represents an inline menu in a document.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Menu<'a> {
    pub target: ContentAttribute<'a>,
    pub items: Vec<ContentAttribute<'a>>,
    pub location: Location,
}

/// A `Keyboard` represents an inline keyboard shortcut in a document.
#[derive(Clone, Debug, PartialEq)]
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

/// One key label in a keyboard shortcut.
pub type Key<'a> = ContentAttribute<'a>;

/// A `CrossReference` represents an inline cross-reference (xref) in a document.
///
/// Equality and debug output omit parser-only state.
#[derive(Clone)]
#[non_exhaustive]
pub struct CrossReference<'a> {
    /// The effective link target. A resolved natural reference contains the
    /// matching ID; references to fully included sources contain the fragment ID.
    /// An empty local target addresses the document top. Unresolved references
    /// retain their reference text or fragment ID.
    pub target: &'a str,
    /// Whether `target` addresses this document, even when its ID is missing.
    /// Set this together with `target` when changing a reference destination.
    pub target_is_local: bool,
    pub text: Vec<InlineNode<'a>>,
    pub location: Location,
    pub xrefstyle: XrefStyle,
    pub caption_label: XrefCaptionLabel<'a>,
    signifier: XrefSignifier<'a>,
    pub(crate) section_signifiers: Option<&'a [XrefSignifier<'a>; REFSIG_ATTRIBUTES.len()]>,
    pub metadata: InlineMetadata<'a>,
    pub(crate) macro_attributes: MacroAttributes<'a>,
    pub(crate) caption_label_snapshot_id: Option<NonZeroUsize>,
    pub(crate) resolve_natural_target: bool,
    pub(crate) source_syntax: XrefSourceSyntax,
}

impl<'a> CrossReference<'a> {
    /// Creates a new `CrossReference` with the given target. Bare IDs default
    /// to local targets; paths with extensions or schemes default to external.
    /// Set `target_is_local` explicitly for an ID that resembles a filename.
    #[must_use]
    pub fn new(target: &'a str, location: Location) -> Self {
        Self {
            target,
            target_is_local: Self::is_local_target(target),
            text: Vec::new(),
            location,
            xrefstyle: XrefStyle::Default,
            caption_label: XrefCaptionLabel::AtTarget,
            signifier: XrefSignifier::Standard,
            section_signifiers: None,
            metadata: InlineMetadata::default(),
            macro_attributes: MacroAttributes::default(),
            caption_label_snapshot_id: None,
            resolve_natural_target: false,
            source_syntax: XrefSourceSyntax::Literal,
        }
    }

    /// Scalar controls and extensions, excluding identity, roles, options, and xref style.
    #[must_use]
    pub fn attributes(&self) -> &ElementAttributes<'a> {
        &self.macro_attributes.attributes
    }

    /// Set a scalar control or extension attribute.
    ///
    /// # Errors
    /// Rejects `xrefstyle`, `id`, `role`/`roles`, and `opts`/`options`.
    /// Update their dedicated fields instead.
    pub fn set_attribute(
        &mut self,
        name: AttributeName<'a>,
        value: AttributeValue<'a>,
    ) -> Result<(), ReservedAttribute> {
        self.macro_attributes.set(name, value, &["xrefstyle"])
    }

    /// Remove a scalar control or extension attribute.
    ///
    /// # Errors
    /// Rejects the dedicated names listed by [`Self::set_attribute`].
    pub fn remove_attribute(
        &mut self,
        name: &str,
    ) -> Result<Option<AttributeValue<'a>>, ReservedAttribute> {
        self.macro_attributes.remove(name, &["xrefstyle"])
    }

    /// Option tokens in their retained order.
    #[must_use]
    pub fn options(&self) -> &[&'a str] {
        &self.macro_attributes.options
    }

    /// Edit option tokens without creating attribute-map aliases.
    pub fn options_mut(&mut self) -> &mut Vec<&'a str> {
        &mut self.macro_attributes.options
    }

    pub(crate) fn metadata_view(&self) -> InlineMetadataView<'_, 'a> {
        InlineMetadataView {
            attributes: &self.macro_attributes.attributes,
            roles: &self.metadata.roles,
            options: &self.macro_attributes.options,
            id: &self.metadata.id,
        }
    }

    pub(crate) fn is_local_target(target: &str) -> bool {
        match target.split_once('#') {
            Some((path, _)) => path.is_empty(),
            None => !target.contains(['.', ':']),
        }
    }

    /// Sets the cross-reference display text as inline nodes.
    #[must_use]
    pub fn with_text(mut self, text: Vec<InlineNode<'a>>) -> Self {
        self.text = text;
        self
    }

    /// The word before a section number, using the attributes at this reference's
    /// source position unless overridden by [`Self::set_signifier`].
    #[must_use]
    pub const fn signifier(&self) -> XrefSignifier<'a> {
        self.signifier
    }

    /// Override the section signifier, including after subsequent calls to
    /// [`Document::renumber_sections`](crate::Document::renumber_sections).
    pub fn set_signifier(&mut self, signifier: XrefSignifier<'a>) {
        self.signifier = signifier;
        self.section_signifiers = None;
    }

    pub(crate) fn refresh_signifier(&mut self, name: Option<&str>) {
        let Some(signifiers) = self.section_signifiers else {
            return;
        };
        self.signifier = name
            .and_then(|name| {
                REFSIG_ATTRIBUTES
                    .iter()
                    .position(|(candidate, _)| *candidate == name)
            })
            .and_then(|index| signifiers.get(index))
            .copied()
            .unwrap_or_default();
    }
}

impl fmt::Debug for CrossReference<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CrossReference")
            .field("target", &self.target)
            .field("target_is_local", &self.target_is_local)
            .field("text", &self.text)
            .field("location", &self.location)
            .field("xrefstyle", &self.xrefstyle)
            .field("caption_label", &self.caption_label)
            .field("signifier", &self.signifier)
            .field("metadata", &self.metadata)
            .field("macro_attributes", &self.macro_attributes)
            .finish()
    }
}

impl PartialEq for CrossReference<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.target == other.target
            && self.target_is_local == other.target_is_local
            && self.text == other.text
            && self.location == other.location
            && self.xrefstyle == other.xrefstyle
            && self.caption_label == other.caption_label
            && self.signifier == other.signifier
            && self.metadata == other.metadata
            && self.macro_attributes == other.macro_attributes
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum XrefSourceSyntax {
    Literal,
    Shorthand,
    Macro,
    /// The forced local reference already contains an ID, including punctuation.
    LocalFragment,
}

impl XrefSourceSyntax {
    pub(crate) fn target_is_local(self, target: &str) -> bool {
        // Compatibility mode and forced fragments treat the entire target as an ID.
        // Otherwise a colon is ID punctuation; only a path before # or a macro
        // filename selects another document. Shorthand permits dots in local IDs.
        if matches!(self, Self::Literal | Self::LocalFragment) {
            return true;
        }
        match target.split_once('#') {
            Some((path, _)) => path.is_empty(),
            None => {
                matches!(self, Self::Shorthand)
                    || !target.rsplit('/').next().unwrap_or(target).contains('.')
            }
        }
    }
}

/// The word that introduces a section number in an automatic cross-reference.
///
/// Asciidoctor reads it from `<name>-refsig` where the reference is written,
/// so a change part-way through a document applies to the references after it.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub enum XrefSignifier<'a> {
    /// Use the standard word for the section's category. This is the default
    /// for references built outside the parser.
    #[default]
    Standard,
    /// Use this word before the number. Parsed references record `<name>-refsig`
    /// at their source position. An empty value still leaves a space before
    /// the number, as Asciidoctor does.
    AtReference(&'a str),
    /// Show the number alone. Parsed references select this when `<name>-refsig`
    /// was unset at their source position.
    Omitted,
}

// Capture every category before the target is known, and retain them so that
// renumbering can select a different category without losing source-position values.
pub(crate) const REFSIG_ATTRIBUTES: [(&str, &str); 11] = [
    ("part", "part-refsig"),
    ("chapter", "chapter-refsig"),
    ("section", "section-refsig"),
    ("appendix", "appendix-refsig"),
    ("preface", "preface-refsig"),
    ("abstract", "abstract-refsig"),
    ("dedication", "dedication-refsig"),
    ("colophon", "colophon-refsig"),
    ("glossary", "glossary-refsig"),
    ("bibliography", "bibliography-refsig"),
    ("index", "index-refsig"),
];

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
///
/// Selected styles fall back to [`Self::Basic`] when the target has no number
/// or custom caption prefix. Explicit reference text takes precedence.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub enum XrefStyle {
    /// Use the target title as written, without automatic emphasis.
    #[default]
    Default,
    /// Use the target title, emphasizing chapter and appendix titles.
    Basic,
    /// Use only the target's caption label and number or custom prefix.
    Short,
    /// Use the caption prefix followed by the target title.
    Full,
}

impl XrefStyle {
    pub(crate) fn attribute_value(self) -> Option<&'static str> {
        match self {
            Self::Default => None,
            Self::Basic => Some("basic"),
            Self::Short => Some("short"),
            Self::Full => Some("full"),
        }
    }

    pub(crate) fn from_attribute(value: Option<&str>) -> Self {
        match value {
            None => Self::Default,
            Some("short") => Self::Short,
            Some("full") => Self::Full,
            _ => Self::Basic,
        }
    }
}

/// An `Autolink` represents an inline autolink in a document.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Autolink<'a> {
    pub url: Source<'a>,
    /// Whether the autolink was written with angle brackets (e.g., `<user@example.com>`).
    /// Email autolinks retain these brackets in their visible text.
    pub bracketed: bool,
    pub location: Location,
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
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Stem<'a> {
    pub(crate) content: &'a str,
    pub(crate) notation: StemNotation,
    pub(crate) location: Location,
}

impl<'a> Stem<'a> {
    /// Construct a protected formula without interpreting its text.
    #[must_use]
    pub fn new(content: &'a str, notation: StemNotation, location: Location) -> Self {
        Self {
            content,
            notation,
            location,
        }
    }

    /// Replace formula text without changing its notation or source span.
    pub fn replace_text(&mut self, content: &'a str) -> &'a str {
        std::mem::replace(&mut self.content, content)
    }

    /// Replace the notation without translating the formula text.
    pub fn replace_notation(&mut self, notation: StemNotation) -> StemNotation {
        std::mem::replace(&mut self.notation, notation)
    }

    /// Replace this node's recorded source span.
    pub fn set_location(&mut self, location: Location) {
        self.location = location;
    }
}

/// The kind of index term, encoding both visibility and structure.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub enum IndexTermKind<'a> {
    /// A single term that is visible in the document and included in the index.
    Flow(Vec<InlineNode<'a>>),
    /// Hidden from output, supports hierarchical entries.
    Concealed {
        /// The primary inline label.
        term: Vec<InlineNode<'a>>,
        /// The secondary inline label, if present.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        secondary: Option<Vec<InlineNode<'a>>>,
        /// The tertiary inline label, if present.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tertiary: Option<Vec<InlineNode<'a>>>,
    },
}

/// A relationship from an index entry to another index term.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum IndexTermRelationship<'a> {
    /// Readers should use the target term instead of this entry.
    See {
        /// The replacement index term.
        target: Vec<InlineNode<'a>>,
    },
    /// Readers can also consult the related terms.
    SeeAlso {
        /// The related index terms.
        targets: Vec<Vec<InlineNode<'a>>>,
    },
}

/// An `IndexTerm` represents an index term in a document.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct IndexTerm<'a> {
    /// The kind and content of this index term.
    pub kind: IndexTermKind<'a>,
    /// The relationship from this entry to other index terms.
    pub relationship: Option<IndexTermRelationship<'a>>,
    /// Resolved catalog labels at registration, before later substitutions.
    pub(crate) catalog: Option<Box<IndexTerm<'a>>>,
    /// Substitutions selected when the term is registered.
    pub(crate) catalog_substitutions: Option<crate::model::substitution::SubstitutionPlan>,
    pub metadata: InlineMetadata<'a>,
    pub(crate) macro_attributes: MacroAttributes<'a>,
    pub location: Location,
}

impl<'a> IndexTerm<'a> {
    /// Construct an index occurrence without controls or registration snapshots.
    #[must_use]
    pub fn new(kind: IndexTermKind<'a>, location: Location) -> Self {
        Self {
            kind,
            relationship: None,
            catalog: None,
            catalog_substitutions: None,
            metadata: InlineMetadata::default(),
            macro_attributes: MacroAttributes::default(),
            location,
        }
    }

    /// Scalar controls and extensions, excluding identity, roles, options, and labels.
    #[must_use]
    pub fn attributes(&self) -> &ElementAttributes<'a> {
        &self.macro_attributes.attributes
    }

    /// Set a scalar control or extension attribute.
    ///
    /// # Errors
    /// Rejects `terms`, `see`, `see-also`, `id`, `role`/`roles`, and `opts`/`options`.
    /// Update their dedicated fields instead.
    pub fn set_attribute(
        &mut self,
        name: AttributeName<'a>,
        value: AttributeValue<'a>,
    ) -> Result<(), ReservedAttribute> {
        self.macro_attributes
            .set(name, value, &["terms", "see", "see-also"])
    }

    /// Remove a scalar control or extension attribute.
    ///
    /// # Errors
    /// Rejects the dedicated names listed by [`Self::set_attribute`].
    pub fn remove_attribute(
        &mut self,
        name: &str,
    ) -> Result<Option<AttributeValue<'a>>, ReservedAttribute> {
        self.macro_attributes
            .remove(name, &["terms", "see", "see-also"])
    }

    /// Option tokens in their retained order.
    #[must_use]
    pub fn options(&self) -> &[&'a str] {
        &self.macro_attributes.options
    }

    /// Edit option tokens without creating attribute-map aliases.
    pub fn options_mut(&mut self) -> &mut Vec<&'a str> {
        &mut self.macro_attributes.options
    }

    pub(crate) fn metadata_view(&self) -> InlineMetadataView<'_, 'a> {
        InlineMetadataView {
            attributes: &self.macro_attributes.attributes,
            roles: &self.metadata.roles,
            options: &self.macro_attributes.options,
            id: &self.metadata.id,
        }
    }

    pub(crate) fn for_each_label_mut(&mut self, mut visit: impl FnMut(&mut Vec<InlineNode<'a>>)) {
        match &mut self.kind {
            IndexTermKind::Flow(term) => visit(term),
            IndexTermKind::Concealed {
                term,
                secondary,
                tertiary,
            } => {
                visit(term);
                if let Some(secondary) = secondary {
                    visit(secondary);
                }
                if let Some(tertiary) = tertiary {
                    visit(tertiary);
                }
            }
        }
        match &mut self.relationship {
            Some(IndexTermRelationship::See { target }) => visit(target),
            Some(IndexTermRelationship::SeeAlso { targets }) => {
                for target in targets {
                    visit(target);
                }
            }
            None => {}
        }
    }

    /// Returns the labels registered in the index catalog.
    ///
    /// These can differ from the displayed term when substitutions run after
    /// index registration. The returned entry is not another occurrence.
    #[must_use]
    pub fn catalog_entry(&self) -> &Self {
        self.catalog.as_deref().unwrap_or(self)
    }

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

#[cfg(test)]
mod tests {
    use crate::{Document, InlineMacro, Options, grammar::walk_document_inline_nodes_mut, parse};

    use super::*;

    #[test]
    fn signifier_snapshots_are_not_part_of_the_value_contract()
    -> Result<(), Box<dyn std::error::Error>> {
        let parsed = parse(
            include_str!("../../../fixtures/tests/xref_signifier_renumber.adoc"),
            &Options::default(),
        )?;
        let mut document = Document {
            blocks: parsed.document_storage().blocks.clone(),
            ..Document::default()
        };
        let mut xrefs = Vec::new();
        walk_document_inline_nodes_mut(&mut document, &mut |inline| {
            if let InlineNode::Macro(InlineMacro::CrossReference(xref)) = inline {
                xrefs.push(xref.clone());
            }
        });
        assert_ne!(xrefs, []);
        for original in xrefs {
            assert!(original.section_signifiers.is_some());
            let mut explicit = original.clone();
            explicit.set_signifier(original.signifier());
            assert!(explicit.section_signifiers.is_none());
            assert_eq!(original, explicit);
            assert_eq!(format!("{original:?}"), format!("{explicit:?}"));
            assert_eq!(
                serde_json::to_value(&original)?,
                serde_json::to_value(&explicit)?
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod passthrough_edit_tests {
    use super::*;
    use crate::{ast::PassRef, model::AttributeInlineFragment};

    fn expanded() -> Pass<'static> {
        Pass {
            attribute_fragments: vec![AttributeInlineFragment {
                range: 0..4,
                substitutions: vec![Substitution::Quotes].into(),
                source: None,
            }]
            .into(),
            text: Some("text"),
            substitutions: vec![Substitution::Attributes],
            location: Location::default(),
            kind: PassthroughKind::AttributeRef,
        }
    }

    #[test]
    fn selected_copy_and_span_edits_keep_declaration_fragments() {
        let original = expanded();
        let mut copy = Pass::from(PassRef::from(&original));
        assert_eq!(copy.attribute_fragments, original.attribute_fragments);
        copy.set_location(Location::point(crate::Position::new(3, 4)));
        assert_eq!(copy.attribute_fragments, original.attribute_fragments);
        assert_eq!(copy.text, original.text);
        assert_eq!(copy.substitutions, original.substitutions);
        assert_eq!(copy.kind, original.kind);
    }

    #[test]
    fn replacing_text_does_not_reuse_ranges_into_the_old_text() {
        let mut pass = expanded();
        assert_eq!(pass.replace_text(Some("λ")), Some("text"));
        assert!(pass.attribute_fragments.is_empty());
        assert_eq!(pass.text, Some("λ"));
        assert_eq!(pass.substitutions, [Substitution::Attributes]);
    }

    #[test]
    fn explicit_interpretation_edits_do_not_replay_declaration_profiles() {
        let mut pass = expanded();
        assert_eq!(
            pass.replace_substitutions(vec![Substitution::SpecialChars]),
            [Substitution::Attributes]
        );
        assert!(pass.attribute_fragments.is_empty());
        assert_eq!(pass.text, Some("text"));
        let mut pass = expanded();
        assert_eq!(pass.take_substitutions(), [Substitution::Attributes]);
        assert!(pass.attribute_fragments.is_empty());
        assert_eq!(pass.substitutions, []);
        let mut pass = expanded();
        pass.set_kind(PassthroughKind::Macro);
        assert!(pass.attribute_fragments.is_empty());
        assert_eq!(pass.text, Some("text"));
    }
}

#[cfg(test)]
mod prepared_note_tests {
    use super::Footnote;
    use crate::{InlineNode, Location, Plain, model::substitution::SubstitutionPlan};

    fn pending() -> Footnote<'static> {
        let mut note = Footnote::new(Vec::new(), 1, Location::default());
        note.definition_source = Some("old body");
        note.registration_substitutions = Some(SubstitutionPlan::default());
        note
    }

    #[test]
    fn selected_note_copies_retain_pending_registration_snapshots() {
        let note = pending();
        let copied = Footnote::from(crate::ast::FootnoteRef::from(&note));
        assert_eq!(copied, note);
        assert_eq!(copied.definition_source, Some("old body"));
        assert!(copied.registration_substitutions.is_some());
    }

    #[test]
    fn prepared_note_edits_discard_old_registration_sources() {
        let mut note = pending();
        note.replace_content(Vec::new());
        assert!(note.definition_source.is_none());
        assert!(note.registration_substitutions.is_none());
        let mut note = pending();
        note.take_content();
        assert!(note.definition_source.is_none());
        assert!(note.registration_substitutions.is_none());
        let mut note = pending();
        note.push_inline(InlineNode::PlainText(Plain::new(
            "new",
            Location::default(),
        )));
        assert!(note.definition_source.is_none());
        assert!(note.registration_substitutions.is_none());
        let mut note = pending();
        note.replace_id(Some("new"));
        assert!(note.definition_source.is_none());
        assert!(note.registration_substitutions.is_none());
    }
}
