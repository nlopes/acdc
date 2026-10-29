use crate::{
    Admonition, Audio, BlockMetadata, CalloutList, DelimitedBlock, DescriptionList, DiscreteHeader,
    ElementAttributes, Header, Image, InlineImage, OrderedList, PageBreak, Paragraph, Section,
    TableOfContents, ThematicBreak, UnorderedList, Video,
};

use super::{AttributeValueRef, ContentRef, ElementIdRef, TokensRef};

/// Named values on an element.
///
/// Reads cover titles, image alt and tooltips, IDs, styles, role/option tokens,
/// and scalar controls. Quote/caption content and substitution settings use
/// dedicated accessors or the owner API. Restricted owners expose only the fields
/// they retain.
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
        }
    }
}

impl<'doc> AttributesRef<'doc> {
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

    pub(super) fn inline_image(owner: &'doc InlineImage<'doc>) -> Self {
        Self {
            owner: Owner::InlineImage(owner),
        }
    }

    /// Read a value by name. `roles` aliases `role`; `options` aliases `opts`.
    ///
    /// `title` means the tooltip on an inline image and the title on a block.
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
        use Field::{Alt, Id, Options, Roles, Style, Title};
        match self.owner {
            Owner::Image(_) => &[Title, Id, Roles, Options, Style, Alt],
            Owner::InlineImage(_) => &[Title, Id, Roles, Options, Alt],
            Owner::ThematicBreak(_) => &[Title, Id],
            Owner::TableOfContents(_) => &[Id, Roles, Options, Style],
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
            | Owner::PageBreak(_) => &[Title, Id, Roles, Options, Style],
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
            Owner::InlineImage(_) | Owner::ThematicBreak(_) => None,
        }
    }

    fn attribute_map(self) -> Option<&'doc ElementAttributes<'doc>> {
        if let Owner::InlineImage(node) = self.owner {
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
            Owner::TableOfContents(_) => return None,
            Owner::InlineImage(node) => return node.tooltip.as_ref().map(ContentRef::from),
        };
        title.content()
    }

    fn alt(self) -> Option<ContentRef<'doc>> {
        if let Owner::Image(node) = self.owner {
            node.alt.as_ref().map(ContentRef::from)
        } else if let Owner::InlineImage(node) = self.owner {
            node.alt.as_ref().map(ContentRef::from)
        } else {
            None
        }
    }

    pub(super) fn id(self) -> Option<ElementIdRef<'doc>> {
        if let Owner::InlineImage(node) = self.owner {
            return node.metadata.id.as_ref().map(ElementIdRef::from);
        }
        let anchor = if let Owner::ThematicBreak(node) = self.owner {
            node.anchors.first()
        } else {
            self.metadata().and_then(BlockMetadata::id_anchor)
        };
        anchor.map(|id| ElementIdRef::new(id.id, &id.location))
    }

    pub(super) fn roles(self) -> TokensRef<'doc> {
        let roles = if let Owner::InlineImage(node) = self.owner {
            node.metadata.roles.as_slice()
        } else {
            self.metadata()
                .map(|metadata| metadata.roles.as_slice())
                .unwrap_or_default()
        };
        TokensRef::new(roles)
    }

    pub(super) fn options(self) -> TokensRef<'doc> {
        let options = if let Owner::InlineImage(node) = self.owner {
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
