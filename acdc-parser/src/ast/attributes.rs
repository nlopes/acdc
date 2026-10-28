use crate::{
    BlockMetadata, DelimitedBlock, ElementAttributes, Image, InlineImage, Paragraph, Section, Title,
};

use super::{AttributeValueRef, ContentRef, ElementIdRef, TokensRef};

/// Named values on an element.
///
/// Reads cover titles, image alt and tooltips, IDs, styles, role/option tokens,
/// and scalar controls. Other block content and substitution settings remain
/// available through the owner API.
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
    Paragraph(&'doc Paragraph<'doc>),
    Section(&'doc Section<'doc>),
    Table(&'doc DelimitedBlock<'doc>),
    Image(&'doc Image<'doc>),
    InlineImage(&'doc InlineImage<'doc>),
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

    pub(super) fn table(owner: &'doc DelimitedBlock<'doc>) -> Self {
        Self {
            owner: Owner::Table(owner),
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
            None => self.attribute_map().get(name).map(AttributeValueRef::from),
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
            .iter()
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
            Owner::Paragraph(_) | Owner::Section(_) | Owner::Table(_) => {
                &[Title, Id, Roles, Options, Style]
            }
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

    fn block(self) -> Option<(&'doc BlockMetadata<'doc>, &'doc Title<'doc>)> {
        match self.owner {
            Owner::Paragraph(node) => Some((&node.metadata, &node.title)),
            Owner::Section(node) => Some((&node.metadata, &node.title)),
            Owner::Table(node) => Some((&node.metadata, &node.title)),
            Owner::Image(node) => Some((&node.metadata, &node.title)),
            Owner::InlineImage(_) => None,
        }
    }

    fn attribute_map(self) -> &'doc ElementAttributes<'doc> {
        match self.owner {
            Owner::Paragraph(node) => &node.metadata.attributes,
            Owner::Section(node) => &node.metadata.attributes,
            Owner::Table(node) => &node.metadata.attributes,
            Owner::Image(node) => &node.metadata.attributes,
            Owner::InlineImage(node) => node.attributes(),
        }
    }

    fn title(self) -> Option<ContentRef<'doc>> {
        match self.owner {
            Owner::InlineImage(node) => node.tooltip.as_ref().map(ContentRef::from),
            Owner::Paragraph(_) | Owner::Section(_) | Owner::Table(_) | Owner::Image(_) => {
                self.block().and_then(|(_, title)| title.content())
            }
        }
    }

    fn alt(self) -> Option<ContentRef<'doc>> {
        match self.owner {
            Owner::Image(node) => node.alt.as_ref().map(ContentRef::from),
            Owner::InlineImage(node) => node.alt.as_ref().map(ContentRef::from),
            Owner::Paragraph(_) | Owner::Section(_) | Owner::Table(_) => None,
        }
    }

    pub(super) fn id(self) -> Option<ElementIdRef<'doc>> {
        match self.owner {
            Owner::InlineImage(node) => node.metadata.id.as_ref().map(ElementIdRef::from),
            Owner::Paragraph(_) | Owner::Section(_) | Owner::Table(_) | Owner::Image(_) => self
                .block()
                .and_then(|(metadata, _)| metadata.id_anchor())
                .map(|id| ElementIdRef::new(id.id, &id.location)),
        }
    }

    pub(super) fn roles(self) -> TokensRef<'doc> {
        let roles = match self.owner {
            Owner::Paragraph(node) => &node.metadata.roles,
            Owner::Section(node) => &node.metadata.roles,
            Owner::Table(node) => &node.metadata.roles,
            Owner::Image(node) => &node.metadata.roles,
            Owner::InlineImage(node) => &node.metadata.roles,
        };
        TokensRef::new(roles)
    }

    pub(super) fn options(self) -> TokensRef<'doc> {
        let options = match self.owner {
            Owner::Paragraph(node) => node.metadata.options.as_slice(),
            Owner::Section(node) => node.metadata.options.as_slice(),
            Owner::Table(node) => node.metadata.options.as_slice(),
            Owner::Image(node) => node.metadata.options.as_slice(),
            Owner::InlineImage(node) => node.options(),
        };
        TokensRef::new(options)
    }

    pub(super) fn style(self) -> Option<&'doc str> {
        self.block().and_then(|(metadata, _)| metadata.style)
    }
}
