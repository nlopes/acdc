//! The data models for the `AsciiDoc` document.
use std::collections::HashMap;

use acdc_core::{AttributeName, AttributeValue, DocumentAttributes, Location};
use serde::{
    de::{self, Deserializer, MapAccess, Visitor},
    ser::{SerializeMap, Serializer},
    Deserialize, Serialize,
};

mod inlines;

pub use inlines::*;

/// A `Document` represents the root of an `AsciiDoc` document.
#[derive(Default, Debug, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub(crate) name: String,
    pub(crate) r#type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header: Option<Header>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub attributes: DocumentAttributes,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<Block>,
    pub location: Location,
}

type Subtitle = String;

/// A `Header` represents the header of a document.
///
/// The header contains the title, subtitle, and authors
#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Header {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub title: Vec<InlineNode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<Subtitle>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub authors: Vec<Author>,
    pub location: Location,
}

/// An `Author` represents the author of a document.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Author {
    #[serde(rename = "firstname")]
    pub first_name: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "middlename"
    )]
    pub middle_name: Option<String>,
    #[serde(rename = "lastname")]
    pub last_name: String,
    pub initials: String,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "address")]
    pub email: Option<String>,
}

/// An `AttributeEntry` represents an attribute entry in a document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AttributeEntry {
    pub name: Option<AttributeName>,
    pub value: Option<String>,
}

/// An `Anchor` represents an anchor in a document.
///
/// An anchor is a reference point in a document that can be linked to.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Anchor {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub xreflabel: Option<String>,
    pub location: Location,
}

pub type Role = String;

/// A `BlockMetadata` represents the metadata of a block in a document.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BlockMetadata {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub roles: Vec<Role>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<Anchor>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub anchors: Vec<Anchor>,
}

/// A `Block` represents a block in a document.
///
/// A block is a structural element in a document that can contain other blocks.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum Block {
    DiscreteHeader(DiscreteHeader),
    DocumentAttribute(DocumentAttribute),
    ThematicBreak(ThematicBreak),
    PageBreak(PageBreak),
    UnorderedList(UnorderedList),
    OrderedList(OrderedList),
    DescriptionList(DescriptionList),
    Section(Section),
    DelimitedBlock(DelimitedBlock),
    Paragraph(Paragraph),
    Image(Image),
    Audio(Audio),
    Video(Video),
}

/// A `DocumentAttribute` represents a document attribute in a document.
///
/// A document attribute is a key-value pair that can be used to set metadata in a
/// document.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DocumentAttribute {
    pub name: AttributeName,
    pub value: AttributeValue,
    pub location: Location,
}

/// A `DiscreteHeader` represents a discrete header in a document.
///
/// Discrete headings are useful for making headings inside of other blocks, like a
/// sidebar.
#[derive(Clone, Debug, PartialEq)]
pub struct DiscreteHeader {
    //#[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub anchors: Vec<Anchor>,
    //#[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub title: Vec<InlineNode>,
    pub level: u8,
    pub location: Location,
}

/// A `ThematicBreak` represents a thematic break in a document.
#[derive(Clone, Default, Debug, PartialEq, Serialize)]
pub struct ThematicBreak {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub anchors: Vec<Anchor>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub title: Vec<InlineNode>,
    pub location: Location,
}

/// A `PageBreak` represents a page break in a document.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PageBreak {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub title: Vec<InlineNode>,
    #[serde(default, skip_serializing_if = "is_default_metadata")]
    pub metadata: BlockMetadata,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub attributes: HashMap<AttributeName, Option<String>>,
    pub location: Location,
}

/// An `Audio` represents an audio block in a document.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Audio {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub title: Vec<InlineNode>,
    pub source: AudioSource,
    #[serde(default, skip_serializing_if = "is_default_metadata")]
    pub metadata: BlockMetadata,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub attributes: HashMap<AttributeName, Option<String>>,
    pub location: Location,
}

/// A `Video` represents a video block in a document.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Video {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub title: Vec<InlineNode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<VideoSource>,
    #[serde(default, skip_serializing_if = "is_default_metadata")]
    pub metadata: BlockMetadata,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub attributes: HashMap<AttributeName, Option<String>>,
    pub location: Location,
}

/// An `Image` represents an image block in a document.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Image {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub title: Vec<InlineNode>,
    pub source: ImageSource,
    #[serde(default, skip_serializing_if = "is_default_metadata")]
    pub metadata: BlockMetadata,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub attributes: HashMap<AttributeName, Option<String>>,
    pub location: Location,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum AudioSource {
    Path(String),
    Url(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum VideoSource {
    Path(String),
    Url(String),
}

// TODO(nlopes): this should use instead
//
// - Path(std::path::PathBuf)
// - Url(url::Url)
//
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ImageSource {
    Path(String),
    Url(String),
}

/// A `DescriptionList` represents a description list in a document.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DescriptionList {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub title: Vec<InlineNode>,
    #[serde(default, skip_serializing_if = "is_default_metadata")]
    pub metadata: BlockMetadata,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub attributes: HashMap<AttributeName, Option<String>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<DescriptionListItem>,
    pub location: Location,
}

/// A `DescriptionListItem` represents a description list item in a document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DescriptionListItem {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub anchors: Vec<Anchor>,
    pub term: String,
    pub delimiter: String,
    pub description: DescriptionListDescription,
    pub location: Location,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum DescriptionListDescription {
    Inline(String),
    Blocks(Vec<Block>),
}

/// A `UnorderedList` represents an unordered list in a document.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct UnorderedList {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub title: Vec<InlineNode>,
    #[serde(default, skip_serializing_if = "is_default_metadata")]
    pub metadata: BlockMetadata,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub attributes: HashMap<AttributeName, Option<String>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<ListItem>,
    pub location: Location,
}

/// An `OrderedList` represents an ordered list in a document.
pub type OrderedList = UnorderedList;
pub type ListLevel = u8;

/// A `ListItem` represents a list item in a document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ListItem {
    // TODO(nlopes): missing anchors
    pub level: ListLevel,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub content: Vec<String>,
}

/// A `Paragraph` represents a paragraph in a document.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Paragraph {
    #[serde(default, skip_serializing_if = "is_default_metadata")]
    pub metadata: BlockMetadata,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub attributes: HashMap<AttributeName, Option<String>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub title: Vec<InlineNode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub content: Vec<InlineNode>,
    pub location: Location,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub admonition: Option<String>,
}

fn is_default_metadata(metadata: &BlockMetadata) -> bool {
    metadata.roles.is_empty()
        && metadata.options.is_empty()
        && metadata.style.is_none()
        && metadata.id.is_none()
        && metadata.anchors.is_empty()
}

/// A `DelimitedBlock` represents a delimited block in a document.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DelimitedBlock {
    #[serde(default, skip_serializing_if = "is_default_metadata")]
    pub metadata: BlockMetadata,
    pub inner: DelimitedBlockType,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub title: Vec<InlineNode>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub attributes: HashMap<AttributeName, Option<String>>,
    pub location: Location,
}

/// A `DelimitedBlockType` represents the type of a delimited block in a document.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum DelimitedBlockType {
    DelimitedComment(String),
    DelimitedExample(Vec<Block>),
    DelimitedListing(String),
    DelimitedLiteral(String),
    DelimitedOpen(Vec<Block>),
    DelimitedSidebar(Vec<Block>),
    DelimitedTable(Table),
    DelimitedPass(String),
    DelimitedQuote(Vec<Block>),
}

/// A `SectionLevel` represents a section depth in a document.
pub type SectionLevel = u8;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Table {
    pub header: Option<TableRow>,
    pub footer: Option<TableRow>,
    pub rows: Vec<TableRow>,
    pub location: Location,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TableRow {
    pub columns: Vec<TableColumn>,
    //pub location: Location,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TableColumn {
    pub content: Vec<Block>,
    //pub location: Location,
}

/// A `Section` represents a section in a document.
#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    //#[serde(default, skip_serializing_if = "is_default_metadata")]
    pub metadata: BlockMetadata,
    //#[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub attributes: HashMap<AttributeName, Option<String>>,
    pub title: Vec<InlineNode>,
    pub level: SectionLevel,
    //#[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub content: Vec<Block>,
    pub location: Location,
}

impl Serialize for Section {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(None)?;
        state.serialize_entry("name", "section")?;
        state.serialize_entry("type", "block")?;
        state.serialize_entry("title", &self.title)?;
        state.serialize_entry("level", &self.level)?;
        if !is_default_metadata(&self.metadata) {
            state.serialize_entry("metadata", &self.metadata)?;
        }
        if !self.attributes.is_empty() {
            state.serialize_entry("attributes", &self.attributes)?;
        }
        if !self.content.is_empty() {
            state.serialize_entry("content", &self.content)?;
        }
        state.serialize_entry("location", &self.location)?;
        state.end()
    }
}

impl Serialize for DiscreteHeader {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(None)?;
        state.serialize_entry("name", "heading")?;
        if !self.title.is_empty() {
            state.serialize_entry("title", &self.title)?;
        }
        state.serialize_entry("level", &self.level)?;
        if !self.anchors.is_empty() {
            state.serialize_entry("anchors", &self.anchors)?;
        }
        state.serialize_entry("location", &self.location)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for DiscreteHeader {
    #[allow(clippy::too_many_lines)]
    fn deserialize<D>(deserializer: D) -> Result<DiscreteHeader, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct MyStructVisitor;

        impl<'de> Visitor<'de> for MyStructVisitor {
            type Value = DiscreteHeader;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a struct representing MyStruct")
            }

            fn visit_map<V>(self, mut map: V) -> Result<DiscreteHeader, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut my_name = None;
                let mut my_title = None;
                let mut my_level = None;
                let mut my_location = None;
                let mut my_anchors = None;

                // TODO(nlopes): need to deserialize the attributes!
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "name" => {
                            if my_name.is_some() {
                                return Err(de::Error::duplicate_field("name"));
                            }
                            my_name = Some(map.next_value::<String>()?);
                        }
                        "title" => {
                            if my_title.is_some() {
                                return Err(de::Error::duplicate_field("title"));
                            }
                            my_title = Some(map.next_value()?);
                        }
                        "level" => {
                            if my_level.is_some() {
                                return Err(de::Error::duplicate_field("level"));
                            }
                            my_level = Some(map.next_value::<SectionLevel>()?);
                        }
                        "anchors" => {
                            if my_anchors.is_some() {
                                return Err(de::Error::duplicate_field("anchors"));
                            }
                            my_anchors = Some(map.next_value()?);
                        }
                        "location" => {
                            if my_location.is_some() {
                                return Err(de::Error::duplicate_field("location"));
                            }
                            my_location = Some(map.next_value()?);
                        }
                        _ => {
                            // Ignore any other fields
                            let _ = map.next_value::<de::IgnoredAny>()?;
                        }
                    }
                }

                let my_name = my_name.ok_or_else(|| de::Error::missing_field("name"))?;
                let my_title = my_title.ok_or_else(|| de::Error::missing_field("title"))?;
                let my_level = my_level.ok_or_else(|| de::Error::missing_field("level"))?;
                let my_anchors = my_anchors.ok_or_else(|| de::Error::missing_field("anchors"))?;
                let my_location =
                    my_location.ok_or_else(|| de::Error::missing_field("location"))?;

                if my_name != "heading" {
                    return Err(de::Error::custom(format!("unexpected name: {}", my_name)));
                }
                Ok(DiscreteHeader {
                    title: my_title,
                    level: my_level,
                    anchors: my_anchors,
                    location: my_location,
                })
            }
        }

        deserializer.deserialize_map(MyStructVisitor)
    }
}

impl<'de> Deserialize<'de> for PageBreak {
    #[allow(clippy::too_many_lines)]
    fn deserialize<D>(deserializer: D) -> Result<PageBreak, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct MyStructVisitor;

        impl<'de> Visitor<'de> for MyStructVisitor {
            type Value = PageBreak;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a struct representing MyStruct")
            }

            fn visit_map<V>(self, mut map: V) -> Result<PageBreak, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut my_name = None;
                let mut my_type = None;
                let mut my_variant = None;
                let mut my_title = None;
                let mut my_metadata = None;
                let mut my_attributes = None;
                let mut my_location = None;

                // TODO(nlopes): need to deserialize the attributes!
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "name" => {
                            if my_name.is_some() {
                                return Err(de::Error::duplicate_field("name"));
                            }
                            my_name = Some(map.next_value::<String>()?);
                        }
                        "title" => {
                            if my_title.is_some() {
                                return Err(de::Error::duplicate_field("title"));
                            }
                            my_title = Some(map.next_value()?);
                        }
                        "type" => {
                            if my_type.is_some() {
                                return Err(de::Error::duplicate_field("type"));
                            }
                            my_type = Some(map.next_value::<String>()?);
                        }
                        "variant" => {
                            if my_variant.is_some() {
                                return Err(de::Error::duplicate_field("variant"));
                            }
                            my_variant = Some(map.next_value::<String>()?);
                        }
                        "metadata" => {
                            if my_metadata.is_some() {
                                return Err(de::Error::duplicate_field("metadata"));
                            }
                            my_metadata = Some(map.next_value()?);
                        }
                        "attributes" => {
                            if my_attributes.is_some() {
                                return Err(de::Error::duplicate_field("attributes"));
                            }
                            my_attributes = Some(map.next_value()?);
                        }
                        "location" => {
                            if my_location.is_some() {
                                return Err(de::Error::duplicate_field("location"));
                            }
                            my_location = Some(map.next_value()?);
                        }
                        _ => {
                            // Ignore any other fields
                            let _ = map.next_value::<de::IgnoredAny>()?;
                        }
                    }
                }

                let my_name = my_name.ok_or_else(|| de::Error::missing_field("name"))?;
                let my_type = my_type.ok_or_else(|| de::Error::missing_field("type"))?;
                let my_variant = my_variant.ok_or_else(|| de::Error::missing_field("variant"))?;
                let my_title = my_title.ok_or_else(|| de::Error::missing_field("title"))?;
                let my_metadata =
                    my_metadata.ok_or_else(|| de::Error::missing_field("metadata"))?;
                let my_attributes =
                    my_attributes.ok_or_else(|| de::Error::missing_field("attributes"))?;
                let my_location =
                    my_location.ok_or_else(|| de::Error::missing_field("location"))?;

                if my_name != "break" && my_type != "block" && my_variant != "page" {
                    return Err(de::Error::custom(format!(
                        "unexpected name/type/variant: {my_name}/{my_type}/{my_variant}",
                    )));
                }
                Ok(PageBreak {
                    title: my_title,
                    metadata: my_metadata,
                    attributes: my_attributes,
                    location: my_location,
                })
            }
        }
        deserializer.deserialize_map(MyStructVisitor)
    }
}

impl<'de> Deserialize<'de> for ThematicBreak {
    #[allow(clippy::too_many_lines)]
    fn deserialize<D>(deserializer: D) -> Result<ThematicBreak, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct MyStructVisitor;

        impl<'de> Visitor<'de> for MyStructVisitor {
            type Value = ThematicBreak;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a struct representing MyStruct")
            }

            fn visit_map<V>(self, mut map: V) -> Result<ThematicBreak, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut my_name = None;
                let mut my_type = None;
                let mut my_variant = None;
                let mut my_title = None;
                let mut my_location = None;
                let mut my_anchors = None;

                // TODO(nlopes): need to deserialize the attributes!
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "name" => {
                            if my_name.is_some() {
                                return Err(de::Error::duplicate_field("name"));
                            }
                            my_name = Some(map.next_value::<String>()?);
                        }
                        "type" => {
                            if my_type.is_some() {
                                return Err(de::Error::duplicate_field("type"));
                            }
                            my_type = Some(map.next_value::<String>()?);
                        }
                        "variant" => {
                            if my_variant.is_some() {
                                return Err(de::Error::duplicate_field("variant"));
                            }
                            my_variant = Some(map.next_value::<String>()?);
                        }
                        "title" => {
                            if my_title.is_some() {
                                return Err(de::Error::duplicate_field("title"));
                            }
                            my_title = Some(map.next_value()?);
                        }
                        "anchors" => {
                            if my_anchors.is_some() {
                                return Err(de::Error::duplicate_field("anchors"));
                            }
                            my_anchors = Some(map.next_value()?);
                        }
                        "location" => {
                            if my_location.is_some() {
                                return Err(de::Error::duplicate_field("location"));
                            }
                            my_location = Some(map.next_value()?);
                        }
                        _ => {
                            // Ignore any other fields
                            let _ = map.next_value::<de::IgnoredAny>()?;
                        }
                    }
                }

                let my_name = my_name.ok_or_else(|| de::Error::missing_field("name"))?;
                let my_type = my_type.ok_or_else(|| de::Error::missing_field("type"))?;
                let my_variant = my_variant.ok_or_else(|| de::Error::missing_field("variant"))?;
                let my_title = my_title.ok_or_else(|| de::Error::missing_field("title"))?;
                let my_anchors = my_anchors.ok_or_else(|| de::Error::missing_field("anchors"))?;
                let my_location =
                    my_location.ok_or_else(|| de::Error::missing_field("location"))?;

                if my_name != "break" && my_type != "block" && my_variant != "thematic" {
                    return Err(de::Error::custom(format!(
                        "unexpected name/type/variant: {my_name}/{my_type}/{my_variant}",
                    )));
                }
                Ok(ThematicBreak {
                    title: my_title,
                    anchors: my_anchors,
                    location: my_location,
                })
            }
        }

        deserializer.deserialize_map(MyStructVisitor)
    }
}

impl<'de> Deserialize<'de> for Section {
    #[allow(clippy::too_many_lines)]
    fn deserialize<D>(deserializer: D) -> Result<Section, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct MyStructVisitor;

        impl<'de> Visitor<'de> for MyStructVisitor {
            type Value = Section;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a struct representing MyStruct")
            }

            fn visit_map<V>(self, mut map: V) -> Result<Section, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut my_name = None;
                let mut my_type = None;
                let mut my_title = None;
                let mut my_level = None;
                let mut my_metadata = None;
                let mut my_location = None;
                let mut my_content = None;

                // TODO(nlopes): need to deserialize the attributes!
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "name" => {
                            if my_name.is_some() {
                                return Err(de::Error::duplicate_field("name"));
                            }
                            my_name = Some(map.next_value::<String>()?);
                        }
                        "type" => {
                            if my_type.is_some() {
                                return Err(de::Error::duplicate_field("type"));
                            }
                            my_type = Some(map.next_value::<String>()?);
                        }
                        "title" => {
                            if my_title.is_some() {
                                return Err(de::Error::duplicate_field("title"));
                            }
                            my_title = Some(map.next_value()?);
                        }
                        "level" => {
                            if my_level.is_some() {
                                return Err(de::Error::duplicate_field("level"));
                            }
                            my_level = Some(map.next_value::<SectionLevel>()?);
                        }
                        "metadata" => {
                            if my_metadata.is_some() {
                                return Err(de::Error::duplicate_field("metadata"));
                            }
                            my_metadata = Some(map.next_value()?);
                        }
                        "content" => {
                            if my_content.is_some() {
                                return Err(de::Error::duplicate_field("content"));
                            }
                            my_content = Some(map.next_value()?);
                        }
                        "location" => {
                            if my_location.is_some() {
                                return Err(de::Error::duplicate_field("location"));
                            }
                            my_location = Some(map.next_value()?);
                        }
                        _ => {
                            // Ignore any other fields
                            let _ = map.next_value::<de::IgnoredAny>()?;
                        }
                    }
                }

                let my_name = my_name.ok_or_else(|| de::Error::missing_field("name"))?;
                let my_type = my_type.ok_or_else(|| de::Error::missing_field("type"))?;
                let my_title = my_title.ok_or_else(|| de::Error::missing_field("title"))?;
                let my_level = my_level.ok_or_else(|| de::Error::missing_field("level"))?;
                let my_metadata =
                    my_metadata.ok_or_else(|| de::Error::missing_field("metadata"))?;
                let my_content = my_content.ok_or_else(|| de::Error::missing_field("content"))?;
                let my_location =
                    my_location.ok_or_else(|| de::Error::missing_field("location"))?;

                match (my_name.as_str(), my_type.as_str()) {
                    ("section", "block") => Ok(Section {
                        metadata: my_metadata,
                        attributes: Default::default(),
                        title: my_title,
                        level: my_level,
                        content: my_content,
                        location: my_location,
                    }),
                    _ => Err(de::Error::custom(format!(
                        "unexpected name/type combination: {}/{}",
                        my_name, my_type
                    ))),
                }
            }
        }

        deserializer.deserialize_map(MyStructVisitor)
    }
}

impl<'de> Deserialize<'de> for UnorderedList {
    #[allow(clippy::too_many_lines)]
    fn deserialize<D>(deserializer: D) -> Result<UnorderedList, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct MyStructVisitor;

        impl<'de> Visitor<'de> for MyStructVisitor {
            type Value = UnorderedList;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a struct representing MyStruct")
            }

            fn visit_map<V>(self, mut map: V) -> Result<UnorderedList, V::Error>
            where
                V: MapAccess<'de>,
            {
                todo!("implement deserialize for unorderedlist")
            }
        }

        deserializer.deserialize_map(MyStructVisitor)
    }
}

impl<'de> Deserialize<'de> for DocumentAttribute {
    #[allow(clippy::too_many_lines)]
    fn deserialize<D>(deserializer: D) -> Result<DocumentAttribute, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct MyStructVisitor;

        impl<'de> Visitor<'de> for MyStructVisitor {
            type Value = DocumentAttribute;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a struct representing MyStruct")
            }

            fn visit_map<V>(self, mut map: V) -> Result<DocumentAttribute, V::Error>
            where
                V: MapAccess<'de>,
            {
                todo!("implement deserialize for documentattribute")
            }
        }

        deserializer.deserialize_map(MyStructVisitor)
    }
}

impl<'de> Deserialize<'de> for DescriptionList {
    #[allow(clippy::too_many_lines)]
    fn deserialize<D>(deserializer: D) -> Result<DescriptionList, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct MyStructVisitor;

        impl<'de> Visitor<'de> for MyStructVisitor {
            type Value = DescriptionList;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a struct representing MyStruct")
            }

            fn visit_map<V>(self, mut map: V) -> Result<DescriptionList, V::Error>
            where
                V: MapAccess<'de>,
            {
                // TODO
                Ok(DescriptionList {
                    title: map.next_value()?,
                    metadata: map.next_value()?,
                    attributes: map.next_value()?,
                    items: map.next_value()?,
                    location: map.next_value()?,
                })
            }
        }

        deserializer.deserialize_map(MyStructVisitor)
    }
}

impl<'de> Deserialize<'de> for DelimitedBlock {
    #[allow(clippy::too_many_lines)]
    fn deserialize<D>(deserializer: D) -> Result<DelimitedBlock, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct MyStructVisitor;

        impl<'de> Visitor<'de> for MyStructVisitor {
            type Value = DelimitedBlock;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a struct representing MyStruct")
            }

            fn visit_map<V>(self, mut map: V) -> Result<DelimitedBlock, V::Error>
            where
                V: MapAccess<'de>,
            {
                // TODO
                Ok(DelimitedBlock {
                    metadata: map.next_value()?,
                    inner: map.next_value()?,
                    title: map.next_value()?,
                    attributes: map.next_value()?,
                    location: map.next_value()?,
                })
            }
        }

        deserializer.deserialize_map(MyStructVisitor)
    }
}

impl<'de> Deserialize<'de> for Paragraph {
    #[allow(clippy::too_many_lines)]
    fn deserialize<D>(deserializer: D) -> Result<Paragraph, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct MyStructVisitor;

        impl<'de> Visitor<'de> for MyStructVisitor {
            type Value = Paragraph;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a struct representing MyStruct")
            }

            fn visit_map<V>(self, mut map: V) -> Result<Paragraph, V::Error>
            where
                V: MapAccess<'de>,
            {
                // TODO
                Ok(Paragraph {
                    metadata: map.next_value()?,
                    attributes: map.next_value()?,
                    title: map.next_value()?,
                    content: map.next_value()?,
                    location: map.next_value()?,
                    admonition: map.next_value()?,
                })
            }
        }

        deserializer.deserialize_map(MyStructVisitor)
    }
}

impl<'de> Deserialize<'de> for Image {
    #[allow(clippy::too_many_lines)]
    fn deserialize<D>(deserializer: D) -> Result<Image, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct MyStructVisitor;

        impl<'de> Visitor<'de> for MyStructVisitor {
            type Value = Image;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a struct representing MyStruct")
            }

            fn visit_map<V>(self, mut map: V) -> Result<Image, V::Error>
            where
                V: MapAccess<'de>,
            {
                // TODO
                Ok(Image {
                    title: map.next_value()?,
                    source: map.next_value()?,
                    metadata: map.next_value()?,
                    attributes: map.next_value()?,
                    location: map.next_value()?,
                })
            }
        }

        deserializer.deserialize_map(MyStructVisitor)
    }
}

impl<'de> Deserialize<'de> for Audio {
    #[allow(clippy::too_many_lines)]
    fn deserialize<D>(deserializer: D) -> Result<Audio, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct MyStructVisitor;

        impl<'de> Visitor<'de> for MyStructVisitor {
            type Value = Audio;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a struct representing MyStruct")
            }

            fn visit_map<V>(self, mut map: V) -> Result<Audio, V::Error>
            where
                V: MapAccess<'de>,
            {
                // TODO
                Ok(Audio {
                    title: map.next_value()?,
                    source: map.next_value()?,
                    metadata: map.next_value()?,
                    attributes: map.next_value()?,
                    location: map.next_value()?,
                })
            }
        }

        deserializer.deserialize_map(MyStructVisitor)
    }
}

impl<'de> Deserialize<'de> for Video {
    #[allow(clippy::too_many_lines)]
    fn deserialize<D>(deserializer: D) -> Result<Video, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct MyStructVisitor;

        impl<'de> Visitor<'de> for MyStructVisitor {
            type Value = Video;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a struct representing MyStruct")
            }

            fn visit_map<V>(self, mut map: V) -> Result<Video, V::Error>
            where
                V: MapAccess<'de>,
            {
                // TODO
                Ok(Video {
                    title: map.next_value()?,
                    sources: map.next_value()?,
                    metadata: map.next_value()?,
                    attributes: map.next_value()?,
                    location: map.next_value()?,
                })
            }
        }

        deserializer.deserialize_map(MyStructVisitor)
    }
}

impl<'de> Deserialize<'de> for Block {
    #[allow(clippy::too_many_lines)]
    fn deserialize<D>(deserializer: D) -> Result<Block, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct MyStructVisitor;

        impl<'de> Visitor<'de> for MyStructVisitor {
            type Value = Block;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a struct representing MyStruct")
            }

            fn visit_map<V>(self, mut map: V) -> Result<Block, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut my_name = None;
                let mut my_type = None;
                let mut my_title = None;
                let mut my_level = None;
                let mut my_metadata = None;
                let mut my_location = None;
                let mut my_content: Option<serde_json::Value> = None;

                // TODO(nlopes): need to deserialize the attributes!
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "name" => {
                            if my_name.is_some() {
                                return Err(de::Error::duplicate_field("name"));
                            }
                            my_name = Some(map.next_value::<String>()?);
                        }
                        "type" => {
                            if my_type.is_some() {
                                return Err(de::Error::duplicate_field("type"));
                            }
                            my_type = Some(map.next_value::<String>()?);
                        }
                        "title" => {
                            if my_title.is_some() {
                                return Err(de::Error::duplicate_field("title"));
                            }
                            my_title = Some(map.next_value()?);
                        }
                        "level" => {
                            if my_level.is_some() {
                                return Err(de::Error::duplicate_field("level"));
                            }
                            my_level = Some(map.next_value::<SectionLevel>()?);
                        }
                        "metadata" => {
                            if my_metadata.is_some() {
                                return Err(de::Error::duplicate_field("metadata"));
                            }
                            my_metadata = Some(map.next_value()?);
                        }
                        "content" => {
                            if my_content.is_some() {
                                return Err(de::Error::duplicate_field("content"));
                            }
                            my_content = Some(map.next_value()?);
                        }
                        "location" => {
                            if my_location.is_some() {
                                return Err(de::Error::duplicate_field("location"));
                            }
                            my_location = Some(map.next_value()?);
                        }
                        _ => {
                            // Ignore any other fields
                            let _ = map.next_value::<de::IgnoredAny>()?;
                        }
                    }
                }

                let my_name = my_name.ok_or_else(|| de::Error::missing_field("name"))?;
                let my_type = my_type.ok_or_else(|| de::Error::missing_field("type"))?;
                let my_title = my_title.ok_or_else(|| de::Error::missing_field("title"))?;
                let my_level = my_level.ok_or_else(|| de::Error::missing_field("level"))?;
                let my_metadata =
                    my_metadata.ok_or_else(|| de::Error::missing_field("metadata"))?;
                let my_location =
                    my_location.ok_or_else(|| de::Error::missing_field("location"))?;

                match (my_name.as_str(), my_type.as_str()) {
                    ("section", "block") => {
                        let my_content =
                            match my_content.ok_or_else(|| de::Error::missing_field("content"))? {
                                serde_json::Value::Array(a) => a
                                    .into_iter()
                                    .map(|v| serde_json::from_value(v).map_err(de::Error::custom))
                                    .collect::<Result<Vec<Block>, _>>()?,
                                _ => return Err(de::Error::custom("content must be an array")),
                            };

                        Ok(Block::Section(Section {
                            metadata: my_metadata,
                            attributes: Default::default(),
                            title: my_title,
                            level: my_level,
                            content: my_content,
                            location: my_location,
                        }))
                    }
                    ("paragraph", "block") => {
                        let my_content =
                            match my_content.ok_or_else(|| de::Error::missing_field("content"))? {
                                serde_json::Value::Array(a) => a
                                    .into_iter()
                                    .map(|v| serde_json::from_value(v).map_err(de::Error::custom))
                                    .collect::<Result<Vec<InlineNode>, _>>()?,
                                _ => return Err(de::Error::custom("content must be an array")),
                            };

                        Ok(Block::Paragraph(Paragraph {
                            metadata: my_metadata,
                            attributes: Default::default(),
                            title: my_title,
                            content: my_content,
                            location: my_location,
                            admonition: None,
                        }))
                    }
                    _ => Err(de::Error::custom(format!(
                        "unexpected name/type combination: {}/{}",
                        my_name, my_type
                    ))),
                }
            }
        }

        deserializer.deserialize_map(MyStructVisitor)
    }
}
