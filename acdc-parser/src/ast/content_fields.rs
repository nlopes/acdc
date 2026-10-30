use std::{array, iter::FusedIterator, option, slice};

use crate::{Anchor, BlockMetadata, ContentAttribute, IndexTermRelationship, InlineNode};

use super::{AnchorRef, BlockKind, ContentRef, InlineKind, InlineRef, StructuralRef};

/// The purpose of an owned inline-content field, independent of rendering policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ContentRole {
    Title,
    Subtitle,
    Body,
    Attribution,
    CiteTitle,
    Caption,
    Alt,
    Tooltip,
    ReferenceText,
    BibliographyLabel,
    ListPrincipal,
    DescriptionTerm,
    Span,
    LinkText,
    Footnote,
    ButtonLabel,
    KeyboardKey,
    MenuTarget,
    MenuItem,
    IndexPrimary,
    IndexSecondary,
    IndexTertiary,
    IndexSee,
    IndexSeeAlso,
}

/// The node that owns a content field. Catalog entries are not content owners.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum ContentOwnerRef<'doc> {
    Structural(StructuralRef<'doc>),
    Inline(InlineRef<'doc>),
    Anchor(AnchorRef<'doc>),
}

/// One retained content field, including an explicitly empty field.
///
/// Read source locations from the content's inlines or its owner. Empty fields
/// do not acquire a synthetic text node or source range.
///
/// Fields cannot outlive their parsed document:
///
/// ```compile_fail,E0597
/// use acdc_parser::{Options, parse};
/// let field = {
///     let parsed = parse("Paragraph.", &Options::default()).unwrap();
///     parsed.document_ref().content_occurrences().next().unwrap()
/// };
/// let _ = field.content();
/// ```
#[derive(Clone, Copy, Debug)]
pub struct ContentFieldRef<'doc> {
    owner: ContentOwnerRef<'doc>,
    role: ContentRole,
    content: ContentRef<'doc>,
}

impl<'doc> ContentFieldRef<'doc> {
    /// The typed owner of this field.
    #[must_use]
    pub fn owner(self) -> ContentOwnerRef<'doc> {
        self.owner
    }

    /// This field's purpose within its owner.
    #[must_use]
    pub fn role(self) -> ContentRole {
        self.role
    }

    /// The field's inline content, without rendering or catalog lookup.
    #[must_use]
    pub fn content(self) -> ContentRef<'doc> {
        self.content
    }
}

impl<'doc> StructuralRef<'doc> {
    /// Directly owned inline-content fields, without allocation or recursion.
    ///
    /// Titles precede body/alt content, then attribution, citation title, custom
    /// caption, named reference text, and anchor labels. Headers use title then
    /// subtitle; description items use term then principal text. Anchor labels
    /// retain anchor order and report the anchor as their owner.
    ///
    /// Absent fields are omitted; present empty fields remain. Child structures,
    /// scalar text such as formulas, and derived catalogs are separate reads.
    pub fn content_fields(
        self,
    ) -> impl FusedIterator<Item = ContentFieldRef<'doc>> + Send + Sync + 'doc {
        self.field_iter()
    }

    pub(super) fn field_iter(self) -> Fields<'doc> {
        use ContentRole::{Alt, Body, DescriptionTerm, ListPrincipal, Subtitle, Title};
        let owner = ContentOwnerRef::Structural(self);
        match self {
            Self::Header(header) => Fields::with_metadata(
                owner,
                (Title, header.title()),
                (Subtitle, header.subtitle()),
                Some(&header.node.metadata),
            ),
            Self::Block(block) => {
                let body = match block.kind() {
                    BlockKind::Paragraph(node) => (Body, Some(node.content())),
                    BlockKind::Listing(node) => (Body, Some(node.content())),
                    BlockKind::Literal(node) => (Body, Some(node.content())),
                    BlockKind::Passthrough(node) => (Body, Some(node.content())),
                    BlockKind::Verse(node) => (Body, Some(node.content())),
                    BlockKind::DelimitedComment(node) => (Body, Some(node.content())),
                    BlockKind::Image(node) => (Alt, node.alt()),
                    BlockKind::Section(_)
                    | BlockKind::Admonition(_)
                    | BlockKind::Example(_)
                    | BlockKind::Open(_)
                    | BlockKind::Sidebar(_)
                    | BlockKind::Quote(_)
                    | BlockKind::UnorderedList(_)
                    | BlockKind::OrderedList(_)
                    | BlockKind::DescriptionList(_)
                    | BlockKind::CalloutList(_)
                    | BlockKind::Table(_)
                    | BlockKind::TableOfContents(_)
                    | BlockKind::DiscreteHeader(_)
                    | BlockKind::DocumentAttribute(_)
                    | BlockKind::ThematicBreak(_)
                    | BlockKind::PageBreak(_)
                    | BlockKind::Audio(_)
                    | BlockKind::Video(_)
                    | BlockKind::Comment(_)
                    | BlockKind::Stem(_) => (Body, None),
                };
                let metadata = block
                    .element_attributes()
                    .and_then(super::AttributesRef::metadata);
                let mut fields =
                    Fields::with_metadata(owner, (Title, block.title()), body, metadata);
                if let BlockKind::ThematicBreak(node) = block.kind() {
                    fields.tail = Tail::anchors(None, &node.node.anchors);
                }
                fields
            }
            Self::ListItem(item) => Fields::one(owner, ListPrincipal, Some(item.principal())),
            Self::CalloutListItem(item) => {
                Fields::one(owner, ListPrincipal, Some(item.principal()))
            }
            Self::DescriptionListItem(item) => {
                let mut fields = Fields::pair(
                    owner,
                    (DescriptionTerm, Some(item.term())),
                    (ListPrincipal, Some(item.principal())),
                );
                fields.tail = Tail::anchors(None, &item.node.anchors);
                fields
            }
            Self::Document(_) | Self::TableRow(_) | Self::TableCell(_) => Fields::empty(owner),
        }
    }
}

impl<'doc> InlineRef<'doc> {
    /// Directly owned inline-content fields, without allocation or recursion.
    ///
    /// Images and icons yield alt then tooltip; hyperlinks yield text then
    /// tooltip. Menus yield the target then items. Index terms yield primary,
    /// secondary, tertiary, then selected relationship targets. Repeated fields
    /// retain their order and empty values. Span and footnote bodies occur once.
    /// Generated labels and catalog copies are excluded.
    pub fn content_fields(
        self,
    ) -> impl FusedIterator<Item = ContentFieldRef<'doc>> + Send + Sync + 'doc {
        self.field_iter()
    }

    pub(super) fn field_iter(self) -> Fields<'doc> {
        use ContentRole::{
            Alt, ButtonLabel, Footnote, IndexPrimary, IndexSecondary, IndexSee, IndexSeeAlso,
            IndexTertiary, KeyboardKey, LinkText, MenuItem, MenuTarget, Span, Tooltip,
        };
        let owner = ContentOwnerRef::Inline(self);
        match self.kind() {
            InlineKind::Bold(node) => Fields::one(owner, Span, Some(node.content())),
            InlineKind::Italic(node) => Fields::one(owner, Span, Some(node.content())),
            InlineKind::Monospace(node) => Fields::one(owner, Span, Some(node.content())),
            InlineKind::Highlight(node) => Fields::one(owner, Span, Some(node.content())),
            InlineKind::Subscript(node) => Fields::one(owner, Span, Some(node.content())),
            InlineKind::Superscript(node) => Fields::one(owner, Span, Some(node.content())),
            InlineKind::CurvedQuotation(node) => Fields::one(owner, Span, Some(node.content())),
            InlineKind::CurvedApostrophe(node) => Fields::one(owner, Span, Some(node.content())),
            InlineKind::Image(node) => {
                Fields::pair(owner, (Alt, node.alt()), (Tooltip, node.tooltip()))
            }
            InlineKind::Icon(node) => {
                Fields::pair(owner, (Alt, node.alt()), (Tooltip, node.tooltip()))
            }
            InlineKind::Link(node) => Fields::pair(
                owner,
                (LinkText, Some(node.text())),
                (Tooltip, node.tooltip()),
            ),
            InlineKind::Url(node) => Fields::pair(
                owner,
                (LinkText, Some(node.text())),
                (Tooltip, node.tooltip()),
            ),
            InlineKind::Mailto(node) => Fields::pair(
                owner,
                (LinkText, Some(node.text())),
                (Tooltip, node.tooltip()),
            ),
            InlineKind::CrossReference(node) => Fields::one(owner, LinkText, Some(node.text())),
            InlineKind::Footnote(node) => Fields::one(owner, Footnote, Some(node.content())),
            InlineKind::Anchor(node) => node.field_iter(),
            InlineKind::Button(node) => Fields::one(owner, ButtonLabel, Some(node.label())),
            InlineKind::Keyboard(node) => Fields {
                tail: Tail::Attributes(KeyboardKey, node.node.keys.iter()),
                ..Fields::empty(owner)
            },
            InlineKind::Menu(node) => Fields {
                tail: Tail::Attributes(MenuItem, node.node.items.iter()),
                ..Fields::one(owner, MenuTarget, Some(node.target()))
            },
            InlineKind::IndexTerm(node) => {
                let (see, tail) = match node.node.relationship.as_ref() {
                    Some(IndexTermRelationship::See { target }) => {
                        (Some(ContentRef::new(target)), Tail::Empty)
                    }
                    Some(IndexTermRelationship::SeeAlso { targets }) => {
                        (None, Tail::Nodes(IndexSeeAlso, targets.iter()))
                    }
                    None => (None, Tail::Empty),
                };
                Fields {
                    owner,
                    prefix: [
                        (IndexPrimary, Some(node.term())),
                        (IndexSecondary, node.secondary()),
                        (IndexTertiary, node.tertiary()),
                        (IndexSee, see),
                        (Span, None),
                        (Span, None),
                    ]
                    .into_iter(),
                    tail,
                }
            }
            InlineKind::Plain(_)
            | InlineKind::Raw(_)
            | InlineKind::Verbatim(_)
            | InlineKind::StandaloneCurvedApostrophe(_)
            | InlineKind::LineBreak(_)
            | InlineKind::Callout(_)
            | InlineKind::Autolink(_)
            | InlineKind::Pass(_)
            | InlineKind::Stem(_) => Fields::empty(owner),
        }
    }
}

impl<'doc> AnchorRef<'doc> {
    /// The retained reference label, or the authored bibliography entry label.
    /// Generated citation text is not another occurrence of the entry label.
    pub fn content_fields(
        self,
    ) -> impl FusedIterator<Item = ContentFieldRef<'doc>> + Send + Sync + 'doc {
        self.field_iter()
    }

    pub(super) fn field_iter(self) -> Fields<'doc> {
        let (role, content) = if self.is_bibliography() {
            (ContentRole::BibliographyLabel, self.bibliography_label())
        } else {
            (ContentRole::ReferenceText, self.reftext())
        };
        Fields::one(ContentOwnerRef::Anchor(self), role, content)
    }
}

#[derive(Debug)]
pub(super) struct Fields<'doc> {
    owner: ContentOwnerRef<'doc>,
    prefix: array::IntoIter<(ContentRole, Option<ContentRef<'doc>>), 6>,
    tail: Tail<'doc>,
}

impl<'doc> Fields<'doc> {
    fn empty(owner: ContentOwnerRef<'doc>) -> Self {
        Self::one(owner, ContentRole::Body, None)
    }

    fn one(
        owner: ContentOwnerRef<'doc>,
        role: ContentRole,
        content: Option<ContentRef<'doc>>,
    ) -> Self {
        Self::pair(owner, (role, content), (ContentRole::Body, None))
    }

    fn pair(
        owner: ContentOwnerRef<'doc>,
        first: (ContentRole, Option<ContentRef<'doc>>),
        second: (ContentRole, Option<ContentRef<'doc>>),
    ) -> Self {
        Self::with_metadata(owner, first, second, None)
    }

    fn with_metadata(
        owner: ContentOwnerRef<'doc>,
        first: (ContentRole, Option<ContentRef<'doc>>),
        second: (ContentRole, Option<ContentRef<'doc>>),
        metadata: Option<&'doc BlockMetadata<'doc>>,
    ) -> Self {
        use ContentRole::{Attribution, Caption, CiteTitle, ReferenceText};
        Self {
            owner,
            prefix: [
                first,
                second,
                (
                    Attribution,
                    metadata
                        .and_then(|m| m.attribution.as_ref())
                        .map(ContentRef::from),
                ),
                (
                    CiteTitle,
                    metadata
                        .and_then(|m| m.citetitle.as_ref())
                        .map(ContentRef::from),
                ),
                (
                    Caption,
                    metadata
                        .and_then(BlockMetadata::caption_content)
                        .map(ContentRef::from),
                ),
                (
                    ReferenceText,
                    metadata
                        .and_then(|m| m.reftext.as_ref())
                        .map(ContentRef::from),
                ),
            ]
            .into_iter(),
            tail: metadata.map_or(Tail::Empty, |m| Tail::anchors(m.id.as_ref(), &m.anchors)),
        }
    }
}

#[derive(Debug)]
enum Tail<'doc> {
    Empty,
    Attributes(ContentRole, slice::Iter<'doc, ContentAttribute<'doc>>),
    Nodes(ContentRole, slice::Iter<'doc, Vec<InlineNode<'doc>>>),
    Anchors(
        std::iter::Chain<option::IntoIter<&'doc Anchor<'doc>>, slice::Iter<'doc, Anchor<'doc>>>,
    ),
}

impl<'doc> Tail<'doc> {
    fn anchors(id: Option<&'doc Anchor<'doc>>, aliases: &'doc [Anchor<'doc>]) -> Self {
        Self::Anchors(id.into_iter().chain(aliases.iter()))
    }
}

impl<'doc> Iterator for Fields<'doc> {
    type Item = ContentFieldRef<'doc>;

    fn next(&mut self) -> Option<Self::Item> {
        for (role, content) in self.prefix.by_ref() {
            if let Some(content) = content {
                return Some(ContentFieldRef {
                    owner: self.owner,
                    role,
                    content,
                });
            }
        }
        let (role, content) = match &mut self.tail {
            Tail::Empty => return None,
            Tail::Attributes(role, values) => (*role, ContentRef::from(values.next()?)),
            Tail::Nodes(role, values) => (*role, ContentRef::new(values.next()?)),
            Tail::Anchors(anchors) => {
                return anchors.find_map(|anchor| AnchorRef::from(anchor).field_iter().next());
            }
        };
        Some(ContentFieldRef {
            owner: self.owner,
            role,
            content,
        })
    }
}

impl FusedIterator for Fields<'_> {}
