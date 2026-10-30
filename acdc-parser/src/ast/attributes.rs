use crate::{
    Admonition, Audio, BlockMetadata, Bold, CalloutList, CrossReference, CurvedApostrophe,
    CurvedQuotation, DelimitedBlock, DescriptionList, DiscreteHeader, ElementAttributes, Header,
    Highlight, Icon, Image, IndexTerm, IndexTermRelationship, InlineImage, Italic, Link, Mailto,
    Monospace, OrderedList, PageBreak, Paragraph, Section, Subscript, Superscript, TableOfContents,
    ThematicBreak, UnorderedList, Url, Video,
};

use super::{
    AttributeValueRef, ContentListRef, ContentRef, ElementIdRef, SubstitutionSpecRef, TokensRef,
};

/// Named values on an element.
///
/// Reads cover titles, image alt, inline tooltips, IDs, styles, role/option tokens,
/// attribution, citation, authored captions, named reftext, substitution settings,
/// and scalar controls. Generated caption labels and anchor labels are separate.
/// Restricted owners expose only the fields they retain.
///
/// Dedicated fields supply their values even when the scalar map contains a
/// conflicting entry. An absent field stays absent; it does not reveal a stale
/// map value. Empty role/option collections are absent because the owner does not
/// distinguish an omitted collection from an explicitly empty one.
///
/// ```
/// use acdc_parser::{Location, Paragraph, ast::{AttributeValueRef, ParagraphRef}};
///
/// let mut paragraph = Paragraph::new(Vec::new(), Location::default());
/// paragraph.metadata.attributes.set("width".into(), String::from("120").into());
/// let attributes = ParagraphRef::from(&paragraph).attributes();
/// assert_eq!(attributes.get("width"), Some(AttributeValueRef::Text("120")));
/// assert!(!attributes.contains("title"));
/// ```
///
/// Attribute reads cannot outlive their owner:
///
/// ```compile_fail,E0597
/// use acdc_parser::{Location, Paragraph, ast::ParagraphRef};
///
/// let attributes = {
///     let paragraph = Paragraph::new(Vec::new(), Location::default());
///     ParagraphRef::from(&paragraph).attributes()
/// };
/// println!("{}", attributes.is_empty());
/// ```
#[derive(Clone, Copy, Debug)]
pub struct AttributesRef<'doc> {
    owner: Owner<'doc>,
}

#[derive(Clone, Copy, Debug)]
enum Owner<'doc> {
    Bold(&'doc Bold<'doc>),
    Italic(&'doc Italic<'doc>),
    Monospace(&'doc Monospace<'doc>),
    Highlight(&'doc Highlight<'doc>),
    Subscript(&'doc Subscript<'doc>),
    Superscript(&'doc Superscript<'doc>),
    CurvedQuotation(&'doc CurvedQuotation<'doc>),
    CurvedApostrophe(&'doc CurvedApostrophe<'doc>),

    Header(&'doc Header<'doc>),
    Paragraph(&'doc Paragraph<'doc>),
    Section(&'doc Section<'doc>),
    Delimited(&'doc DelimitedBlock<'doc>),
    Admonition(&'doc Admonition<'doc>),
    UnorderedList(&'doc UnorderedList<'doc>),
    OrderedList(&'doc OrderedList<'doc>),
    DescriptionList(&'doc DescriptionList<'doc>),
    CalloutList(&'doc CalloutList<'doc>),
    Image(&'doc Image<'doc>),
    InlineImage(&'doc InlineImage<'doc>),
    Icon(&'doc Icon<'doc>),
    CrossReference(&'doc CrossReference<'doc>),
    IndexTerm(&'doc IndexTerm<'doc>),
    Link(&'doc Link<'doc>),
    Url(&'doc Url<'doc>),
    Mailto(&'doc Mailto<'doc>),
    Audio(&'doc Audio<'doc>),
    Video(&'doc Video<'doc>),
    TableOfContents(&'doc TableOfContents<'doc>),
    DiscreteHeader(&'doc DiscreteHeader<'doc>),
    PageBreak(&'doc PageBreak<'doc>),
    ThematicBreak(&'doc ThematicBreak<'doc>),
}

#[derive(Clone, Copy)]
enum Field {
    Title,
    Id,
    Roles,
    Options,
    Style,
    Alt,
    XrefStyle,
    Terms,
    See,
    SeeAlso,
    Attribution,
    CiteTitle,
    Caption,
    Reftext,
    Substitutions,
}

impl Field {
    fn name(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Id => "id",
            Self::Roles => "role",
            Self::Options => "opts",
            Self::Style => "style",
            Self::Alt => "alt",
            Self::XrefStyle => "xrefstyle",
            Self::Terms => "terms",
            Self::See => "see",
            Self::SeeAlso => "see-also",
            Self::Attribution => "attribution",
            Self::CiteTitle => "citetitle",
            Self::Caption => "caption",
            Self::Reftext => "reftext",
            Self::Substitutions => "subs",
        }
    }
}

impl<'doc> AttributesRef<'doc> {
    pub(super) fn index_term(owner: &'doc IndexTerm<'doc>) -> Self {
        Self {
            owner: Owner::IndexTerm(owner),
        }
    }

    pub(super) fn cross_reference(owner: &'doc CrossReference<'doc>) -> Self {
        Self {
            owner: Owner::CrossReference(owner),
        }
    }

    pub(super) fn link(owner: &'doc Link<'doc>) -> Self {
        Self {
            owner: Owner::Link(owner),
        }
    }

    pub(super) fn url(owner: &'doc Url<'doc>) -> Self {
        Self {
            owner: Owner::Url(owner),
        }
    }

    pub(super) fn mailto(owner: &'doc Mailto<'doc>) -> Self {
        Self {
            owner: Owner::Mailto(owner),
        }
    }

    pub(super) fn bold(owner: &'doc Bold<'doc>) -> Self {
        Self {
            owner: Owner::Bold(owner),
        }
    }

    pub(super) fn italic(owner: &'doc Italic<'doc>) -> Self {
        Self {
            owner: Owner::Italic(owner),
        }
    }

    pub(super) fn monospace(owner: &'doc Monospace<'doc>) -> Self {
        Self {
            owner: Owner::Monospace(owner),
        }
    }

    pub(super) fn highlight(owner: &'doc Highlight<'doc>) -> Self {
        Self {
            owner: Owner::Highlight(owner),
        }
    }

    pub(super) fn subscript(owner: &'doc Subscript<'doc>) -> Self {
        Self {
            owner: Owner::Subscript(owner),
        }
    }

    pub(super) fn superscript(owner: &'doc Superscript<'doc>) -> Self {
        Self {
            owner: Owner::Superscript(owner),
        }
    }

    pub(super) fn curved_quotation(owner: &'doc CurvedQuotation<'doc>) -> Self {
        Self {
            owner: Owner::CurvedQuotation(owner),
        }
    }

    pub(super) fn curved_apostrophe(owner: &'doc CurvedApostrophe<'doc>) -> Self {
        Self {
            owner: Owner::CurvedApostrophe(owner),
        }
    }

    pub(super) fn audio(owner: &'doc Audio<'doc>) -> Self {
        Self {
            owner: Owner::Audio(owner),
        }
    }

    pub(super) fn video(owner: &'doc Video<'doc>) -> Self {
        Self {
            owner: Owner::Video(owner),
        }
    }

    pub(super) fn table_of_contents(owner: &'doc TableOfContents<'doc>) -> Self {
        Self {
            owner: Owner::TableOfContents(owner),
        }
    }

    pub(super) fn discrete_header(owner: &'doc DiscreteHeader<'doc>) -> Self {
        Self {
            owner: Owner::DiscreteHeader(owner),
        }
    }

    pub(super) fn page_break(owner: &'doc PageBreak<'doc>) -> Self {
        Self {
            owner: Owner::PageBreak(owner),
        }
    }

    pub(super) fn thematic_break(owner: &'doc ThematicBreak<'doc>) -> Self {
        Self {
            owner: Owner::ThematicBreak(owner),
        }
    }

    pub(super) fn header(owner: &'doc Header<'doc>) -> Self {
        Self {
            owner: Owner::Header(owner),
        }
    }

    pub(super) fn paragraph(owner: &'doc Paragraph<'doc>) -> Self {
        Self {
            owner: Owner::Paragraph(owner),
        }
    }

    pub(super) fn section(owner: &'doc Section<'doc>) -> Self {
        Self {
            owner: Owner::Section(owner),
        }
    }

    pub(super) fn delimited(owner: &'doc DelimitedBlock<'doc>) -> Self {
        Self {
            owner: Owner::Delimited(owner),
        }
    }

    pub(super) fn admonition(owner: &'doc Admonition<'doc>) -> Self {
        Self {
            owner: Owner::Admonition(owner),
        }
    }

    pub(super) fn unordered_list(owner: &'doc UnorderedList<'doc>) -> Self {
        Self {
            owner: Owner::UnorderedList(owner),
        }
    }

    pub(super) fn ordered_list(owner: &'doc OrderedList<'doc>) -> Self {
        Self {
            owner: Owner::OrderedList(owner),
        }
    }

    pub(super) fn description_list(owner: &'doc DescriptionList<'doc>) -> Self {
        Self {
            owner: Owner::DescriptionList(owner),
        }
    }

    pub(super) fn callout_list(owner: &'doc CalloutList<'doc>) -> Self {
        Self {
            owner: Owner::CalloutList(owner),
        }
    }

    pub(super) fn image(owner: &'doc Image<'doc>) -> Self {
        Self {
            owner: Owner::Image(owner),
        }
    }

    pub(super) fn icon(owner: &'doc Icon<'doc>) -> Self {
        Self {
            owner: Owner::Icon(owner),
        }
    }

    pub(super) fn inline_image(owner: &'doc InlineImage<'doc>) -> Self {
        Self {
            owner: Owner::InlineImage(owner),
        }
    }

    /// Read a value by name. `roles` aliases `role`; `options` aliases `opts`.
    ///
    /// `title` means the tooltip on an image, icon, explicit link, URL, or email
    /// macro, and the title on a block. Xrefs and index terms retain it as a
    /// scalar extension.
    /// Missing values return `None`; empty text/content, false and a
    /// retained valueless attribute remain present.
    #[must_use]
    pub fn get(self, name: &str) -> Option<AttributeValueRef<'doc>> {
        let name = crate::model::canonical_attribute_name(name);
        match self.field(name) {
            Some(field) => self.field_value(field),
            None => self
                .attribute_map()
                .and_then(|attributes| attributes.get(name))
                .map(AttributeValueRef::from),
        }
    }

    /// Whether a named value is present, using the same aliases as [`Self::get`].
    #[must_use]
    pub fn contains(self, name: &str) -> bool {
        self.get(name).is_some()
    }

    /// Iterate over present values once, using canonical names. Order is unspecified.
    pub fn iter(
        self,
    ) -> impl Iterator<Item = (&'doc str, AttributeValueRef<'doc>)> + Send + Sync + 'doc {
        let fields = self
            .fields()
            .iter()
            .filter_map(move |&field| self.field_value(field).map(|value| (field.name(), value)));
        let map_entries = self
            .attribute_map()
            .into_iter()
            .flat_map(ElementAttributes::iter)
            .filter(move |(name, _)| {
                self.field(crate::model::canonical_attribute_name(name))
                    .is_none()
            })
            .map(|(name, value)| (name.as_ref(), AttributeValueRef::from(value)));
        fields.chain(map_entries)
    }

    /// Whether this inventory has no present values.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.iter().next().is_none()
    }

    fn fields(self) -> &'static [Field] {
        use Field::{
            Alt, Attribution, Caption, CiteTitle, Id, Options, Reftext, Roles, Style,
            Substitutions, Title,
        };
        match self.owner {
            Owner::Bold(_)
            | Owner::Italic(_)
            | Owner::Monospace(_)
            | Owner::Highlight(_)
            | Owner::Subscript(_)
            | Owner::Superscript(_)
            | Owner::CurvedQuotation(_)
            | Owner::CurvedApostrophe(_) => &[Id, Roles],
            Owner::Image(_) => &[
                Title,
                Id,
                Roles,
                Options,
                Style,
                Alt,
                Attribution,
                CiteTitle,
                Caption,
                Reftext,
                Substitutions,
            ],
            Owner::InlineImage(_) | Owner::Icon(_) => &[Title, Id, Roles, Options, Alt],
            Owner::Link(_) | Owner::Url(_) | Owner::Mailto(_) => &[Title, Id, Roles, Options],
            Owner::CrossReference(_) => &[Id, Roles, Options, Field::XrefStyle],
            Owner::IndexTerm(_) => &[Id, Roles, Options, Field::Terms, Field::See, Field::SeeAlso],
            Owner::ThematicBreak(_) => &[Title, Id],
            Owner::TableOfContents(_) => &[
                Id,
                Roles,
                Options,
                Style,
                Attribution,
                CiteTitle,
                Caption,
                Reftext,
                Substitutions,
            ],
            Owner::Header(_)
            | Owner::Paragraph(_)
            | Owner::Section(_)
            | Owner::Delimited(_)
            | Owner::Admonition(_)
            | Owner::UnorderedList(_)
            | Owner::OrderedList(_)
            | Owner::DescriptionList(_)
            | Owner::CalloutList(_)
            | Owner::Audio(_)
            | Owner::Video(_)
            | Owner::DiscreteHeader(_)
            | Owner::PageBreak(_) => &[
                Title,
                Id,
                Roles,
                Options,
                Style,
                Attribution,
                CiteTitle,
                Caption,
                Reftext,
                Substitutions,
            ],
        }
    }

    fn field(self, name: &str) -> Option<Field> {
        self.fields()
            .iter()
            .copied()
            .find(|field| field.name() == name)
    }

    fn field_value(self, field: Field) -> Option<AttributeValueRef<'doc>> {
        match field {
            Field::Title => self.title().map(AttributeValueRef::Content),
            Field::Id => self.id().map(|id| AttributeValueRef::Text(id.as_str())),
            Field::Roles => {
                let tokens = self.roles();
                (!tokens.is_empty()).then_some(AttributeValueRef::Tokens(tokens))
            }
            Field::Options => {
                let tokens = self.options();
                (!tokens.is_empty()).then_some(AttributeValueRef::Tokens(tokens))
            }
            Field::Style => self.style().map(AttributeValueRef::Text),
            Field::Alt => self.alt().map(AttributeValueRef::Content),
            Field::Attribution => self
                .metadata()
                .and_then(|metadata| metadata.attribution.as_ref())
                .map(|content| AttributeValueRef::Content(ContentRef::from(content))),
            Field::CiteTitle => self
                .metadata()
                .and_then(|metadata| metadata.citetitle.as_ref())
                .map(|content| AttributeValueRef::Content(ContentRef::from(content))),
            Field::Caption => self
                .metadata()
                .and_then(BlockMetadata::caption_content)
                .map(|content| AttributeValueRef::Content(ContentRef::from(content))),
            Field::Reftext => self
                .metadata()
                .and_then(|metadata| metadata.reftext.as_ref())
                .map(|content| AttributeValueRef::Content(ContentRef::from(content))),
            Field::Substitutions => self
                .metadata()
                .and_then(|metadata| metadata.substitutions.as_ref())
                .map(|spec| AttributeValueRef::Substitutions(SubstitutionSpecRef::from(spec))),
            Field::Terms => {
                if let Owner::IndexTerm(node) = self.owner {
                    Some(AttributeValueRef::ContentList(ContentListRef::index_terms(
                        node,
                    )))
                } else {
                    None
                }
            }
            Field::See | Field::SeeAlso => {
                if let Owner::IndexTerm(node) = self.owner {
                    match (&node.relationship, field) {
                        (Some(IndexTermRelationship::See { target }), Field::See) => {
                            Some(AttributeValueRef::Content(ContentRef::new(target)))
                        }
                        (Some(IndexTermRelationship::SeeAlso { targets }), Field::SeeAlso) => {
                            Some(AttributeValueRef::ContentList(ContentListRef::new(targets)))
                        }
                        _ => None,
                    }
                } else {
                    None
                }
            }
            Field::XrefStyle => {
                if let Owner::CrossReference(node) = self.owner {
                    node.xrefstyle
                        .attribute_value()
                        .map(AttributeValueRef::Text)
                } else {
                    None
                }
            }
        }
    }

    fn metadata(self) -> Option<&'doc BlockMetadata<'doc>> {
        match self.owner {
            Owner::Header(node) => Some(&node.metadata),
            Owner::Paragraph(node) => Some(&node.metadata),
            Owner::Section(node) => Some(&node.metadata),
            Owner::Delimited(node) => Some(&node.metadata),
            Owner::Admonition(node) => Some(&node.metadata),
            Owner::UnorderedList(node) => Some(&node.metadata),
            Owner::OrderedList(node) => Some(&node.metadata),
            Owner::DescriptionList(node) => Some(&node.metadata),
            Owner::CalloutList(node) => Some(&node.metadata),
            Owner::Image(node) => Some(&node.metadata),
            Owner::Audio(node) => Some(&node.metadata),
            Owner::Video(node) => Some(&node.metadata),
            Owner::TableOfContents(node) => Some(&node.metadata),
            Owner::DiscreteHeader(node) => Some(&node.metadata),
            Owner::PageBreak(node) => Some(&node.metadata),
            Owner::CrossReference(_)
            | Owner::IndexTerm(_)
            | Owner::InlineImage(_)
            | Owner::Icon(_)
            | Owner::Link(_)
            | Owner::Url(_)
            | Owner::Mailto(_)
            | Owner::ThematicBreak(_)
            | Owner::Bold(_)
            | Owner::Italic(_)
            | Owner::Monospace(_)
            | Owner::Highlight(_)
            | Owner::Subscript(_)
            | Owner::Superscript(_)
            | Owner::CurvedQuotation(_)
            | Owner::CurvedApostrophe(_) => None,
        }
    }

    fn attribute_map(self) -> Option<&'doc ElementAttributes<'doc>> {
        if let Owner::IndexTerm(node) = self.owner {
            Some(node.attributes())
        } else if let Owner::CrossReference(node) = self.owner {
            Some(node.attributes())
        } else if let Owner::InlineImage(node) = self.owner {
            Some(node.attributes())
        } else if let Owner::Link(node) = self.owner {
            Some(node.attributes())
        } else if let Owner::Url(node) = self.owner {
            Some(node.attributes())
        } else if let Owner::Mailto(node) = self.owner {
            Some(node.attributes())
        } else if let Owner::Icon(node) = self.owner {
            Some(node.attributes())
        } else {
            self.metadata().map(|metadata| &metadata.attributes)
        }
    }

    fn title(self) -> Option<ContentRef<'doc>> {
        let title = match self.owner {
            Owner::Header(node) => &node.title,
            Owner::Paragraph(node) => &node.title,
            Owner::Section(node) => &node.title,
            Owner::Delimited(node) => &node.title,
            Owner::Admonition(node) => &node.title,
            Owner::UnorderedList(node) => &node.title,
            Owner::OrderedList(node) => &node.title,
            Owner::DescriptionList(node) => &node.title,
            Owner::CalloutList(node) => &node.title,
            Owner::Image(node) => &node.title,
            Owner::Audio(node) => &node.title,
            Owner::Video(node) => &node.title,
            Owner::DiscreteHeader(node) => &node.title,
            Owner::PageBreak(node) => &node.title,
            Owner::ThematicBreak(node) => &node.title,
            Owner::CrossReference(_)
            | Owner::IndexTerm(_)
            | Owner::TableOfContents(_)
            | Owner::Bold(_)
            | Owner::Italic(_)
            | Owner::Monospace(_)
            | Owner::Highlight(_)
            | Owner::Subscript(_)
            | Owner::Superscript(_)
            | Owner::CurvedQuotation(_)
            | Owner::CurvedApostrophe(_) => return None,
            Owner::InlineImage(node) => return node.tooltip.as_ref().map(ContentRef::from),
            Owner::Icon(node) => return node.tooltip.as_ref().map(ContentRef::from),
            Owner::Link(node) => return node.tooltip.as_ref().map(ContentRef::from),
            Owner::Url(node) => return node.tooltip.as_ref().map(ContentRef::from),
            Owner::Mailto(node) => return node.tooltip.as_ref().map(ContentRef::from),
        };
        title.content()
    }

    fn alt(self) -> Option<ContentRef<'doc>> {
        if let Owner::Image(node) = self.owner {
            node.alt.as_ref().map(ContentRef::from)
        } else if let Owner::InlineImage(node) = self.owner {
            node.alt.as_ref().map(ContentRef::from)
        } else if let Owner::Icon(node) = self.owner {
            node.alt.as_ref().map(ContentRef::from)
        } else {
            None
        }
    }

    pub(super) fn id(self) -> Option<ElementIdRef<'doc>> {
        match self.owner {
            Owner::Bold(node) => node.id.map(|id| ElementIdRef::new(id, &node.location)),
            Owner::Italic(node) => node.id.map(|id| ElementIdRef::new(id, &node.location)),
            Owner::Monospace(node) => node.id.map(|id| ElementIdRef::new(id, &node.location)),
            Owner::Highlight(node) => node.id.map(|id| ElementIdRef::new(id, &node.location)),
            Owner::Subscript(node) => node.id.map(|id| ElementIdRef::new(id, &node.location)),
            Owner::Superscript(node) => node.id.map(|id| ElementIdRef::new(id, &node.location)),
            Owner::CurvedQuotation(node) => node.id.map(|id| ElementIdRef::new(id, &node.location)),
            Owner::CurvedApostrophe(node) => {
                node.id.map(|id| ElementIdRef::new(id, &node.location))
            }
            Owner::InlineImage(node) => node.metadata.id.as_ref().map(ElementIdRef::from),
            Owner::Icon(node) => node.metadata.id.as_ref().map(ElementIdRef::from),
            Owner::CrossReference(node) => node.metadata.id.as_ref().map(ElementIdRef::from),
            Owner::IndexTerm(node) => node.metadata.id.as_ref().map(ElementIdRef::from),
            Owner::Link(node) => node.metadata.id.as_ref().map(ElementIdRef::from),
            Owner::Url(node) => node.metadata.id.as_ref().map(ElementIdRef::from),
            Owner::Mailto(node) => node.metadata.id.as_ref().map(ElementIdRef::from),
            Owner::ThematicBreak(node) => node
                .anchors
                .first()
                .map(|id| ElementIdRef::new(id.id, &id.location)),
            Owner::Header(_)
            | Owner::Paragraph(_)
            | Owner::Section(_)
            | Owner::Delimited(_)
            | Owner::Admonition(_)
            | Owner::UnorderedList(_)
            | Owner::OrderedList(_)
            | Owner::DescriptionList(_)
            | Owner::CalloutList(_)
            | Owner::Image(_)
            | Owner::Audio(_)
            | Owner::Video(_)
            | Owner::TableOfContents(_)
            | Owner::DiscreteHeader(_)
            | Owner::PageBreak(_) => self
                .metadata()
                .and_then(BlockMetadata::id_anchor)
                .map(|id| ElementIdRef::new(id.id, &id.location)),
        }
    }

    pub(super) fn roles(self) -> TokensRef<'doc> {
        match self.owner {
            Owner::Bold(node) => TokensRef::from_text(node.role.unwrap_or_default()),
            Owner::Italic(node) => TokensRef::from_text(node.role.unwrap_or_default()),
            Owner::Monospace(node) => TokensRef::from_text(node.role.unwrap_or_default()),
            Owner::Highlight(node) => TokensRef::from_text(node.role.unwrap_or_default()),
            Owner::Subscript(node) => TokensRef::from_text(node.role.unwrap_or_default()),
            Owner::Superscript(node) => TokensRef::from_text(node.role.unwrap_or_default()),
            Owner::CurvedQuotation(node) => TokensRef::from_text(node.role.unwrap_or_default()),
            Owner::CurvedApostrophe(node) => TokensRef::from_text(node.role.unwrap_or_default()),
            Owner::InlineImage(node) => TokensRef::new(&node.metadata.roles),
            Owner::Icon(node) => TokensRef::new(&node.metadata.roles),
            Owner::CrossReference(node) => TokensRef::new(&node.metadata.roles),
            Owner::IndexTerm(node) => TokensRef::new(&node.metadata.roles),
            Owner::Link(node) => TokensRef::new(&node.metadata.roles),
            Owner::Url(node) => TokensRef::new(&node.metadata.roles),
            Owner::Mailto(node) => TokensRef::new(&node.metadata.roles),
            Owner::ThematicBreak(_) => TokensRef::new(&[]),
            Owner::Header(_)
            | Owner::Paragraph(_)
            | Owner::Section(_)
            | Owner::Delimited(_)
            | Owner::Admonition(_)
            | Owner::UnorderedList(_)
            | Owner::OrderedList(_)
            | Owner::DescriptionList(_)
            | Owner::CalloutList(_)
            | Owner::Image(_)
            | Owner::Audio(_)
            | Owner::Video(_)
            | Owner::TableOfContents(_)
            | Owner::DiscreteHeader(_)
            | Owner::PageBreak(_) => TokensRef::new(
                self.metadata()
                    .map(|metadata| metadata.roles.as_slice())
                    .unwrap_or_default(),
            ),
        }
    }

    pub(super) fn options(self) -> TokensRef<'doc> {
        let options = if let Owner::IndexTerm(node) = self.owner {
            node.options()
        } else if let Owner::CrossReference(node) = self.owner {
            node.options()
        } else if let Owner::InlineImage(node) = self.owner {
            node.options()
        } else if let Owner::Link(node) = self.owner {
            node.options()
        } else if let Owner::Url(node) = self.owner {
            node.options()
        } else if let Owner::Mailto(node) = self.owner {
            node.options()
        } else if let Owner::Icon(node) = self.owner {
            node.options()
        } else {
            self.metadata()
                .map(|metadata| metadata.options.as_slice())
                .unwrap_or_default()
        };
        TokensRef::new(options)
    }

    pub(super) fn style(self) -> Option<&'doc str> {
        self.metadata().and_then(|metadata| metadata.style)
    }
}
