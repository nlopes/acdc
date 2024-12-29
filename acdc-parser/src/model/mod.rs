//! The data models for the `AsciiDoc` document.
use std::{collections::HashMap, str::FromStr};

use serde::{
    de::{self, Deserializer, MapAccess, Visitor},
    ser::{SerializeMap, Serializer},
    Deserialize, Serialize,
};

use crate::Error;

mod document_attributes;
mod inlines;
mod location;
mod substitution;

pub use document_attributes::*;
pub use inlines::*;
pub use location::*;
pub use substitution::*;

/// A `Document` represents the root of an `AsciiDoc` document.
#[derive(Default, Debug, PartialEq, Deserialize)]
pub struct Document {
    pub(crate) name: String,
    pub(crate) r#type: String,
    #[serde(default)]
    pub header: Option<Header>,
    #[serde(default)]
    pub attributes: DocumentAttributes,
    #[serde(default)]
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

// TODO: we could and probably should just use a `AttributeValue` instead
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct OptionalAttributeValue(
    #[serde(default, skip_serializing_if = "Option::is_none")] pub Option<String>,
);

impl<'de> Deserialize<'de> for OptionalAttributeValue {
    fn deserialize<D>(deserializer: D) -> Result<OptionalAttributeValue, D::Error>
    where
        D: Deserializer<'de>,
    {
        match Option::<String>::deserialize(deserializer)? {
            Some(value) => {
                if value.as_str() == "null" {
                    Ok(OptionalAttributeValue(None))
                } else {
                    Ok(OptionalAttributeValue(Some(value)))
                }
            }
            None => Ok(OptionalAttributeValue(None)),
        }
    }
}

/// A `BlockMetadata` represents the metadata of a block in a document.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BlockMetadata {
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub attributes: HashMap<AttributeName, OptionalAttributeValue>,
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

impl BlockMetadata {
    pub fn set_attributes(&mut self, attributes: HashMap<AttributeName, OptionalAttributeValue>) {
        self.attributes = attributes;
    }

    #[must_use]
    pub fn is_default(&self) -> bool {
        self.roles.is_empty()
            && self.options.is_empty()
            && self.style.is_none()
            && self.id.is_none()
            && self.anchors.is_empty()
            && self.attributes.is_empty()
    }
}

/// A `Block` represents a block in a document.
///
/// A block is a structural element in a document that can contain other blocks.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Block {
    TableOfContents(TableOfContents),
    Admonition(Admonition),
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
    pub location: Location,
}

/// A `TableOfContents` represents a table of contents block.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TableOfContents {
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
#[derive(Clone, Debug, PartialEq)]
pub struct UnorderedList {
    pub title: Vec<InlineNode>,
    pub metadata: BlockMetadata,
    pub items: Vec<ListItem>,
    pub marker: String,
    pub location: Location,
}

/// An `OrderedList` represents an ordered list in a document.
#[derive(Clone, Debug, PartialEq)]
pub struct OrderedList {
    pub title: Vec<InlineNode>,
    pub metadata: BlockMetadata,
    pub items: Vec<ListItem>,
    pub marker: String,
    pub location: Location,
}
pub type ListLevel = u8;

/// A `ListItem` represents a list item in a document.
#[derive(Clone, Debug, PartialEq)]
pub struct ListItem {
    // TODO(nlopes): missing anchors
    pub level: ListLevel,
    pub marker: String,
    pub checked: Option<bool>,
    pub content: Vec<InlineNode>,
    pub location: Location,
}

/// A `Paragraph` represents a paragraph in a document.
#[derive(Clone, Debug, PartialEq)]
pub struct Paragraph {
    pub metadata: BlockMetadata,
    pub title: Vec<InlineNode>,
    pub content: Vec<InlineNode>,
    pub location: Location,
}

fn is_default_metadata(metadata: &BlockMetadata) -> bool {
    metadata.is_default()
}

/// A `DelimitedBlock` represents a delimited block in a document.
#[derive(Clone, Debug, PartialEq)]
pub struct DelimitedBlock {
    pub metadata: BlockMetadata,
    pub inner: DelimitedBlockType,
    pub delimiter: String,
    pub title: Vec<InlineNode>,
    pub location: Location,
}

/// An `Admonition` represents an admonition in a document.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Admonition {
    #[serde(default, skip_serializing_if = "is_default_metadata")]
    pub metadata: BlockMetadata,
    pub variant: AdmonitionVariant,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<Block>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub title: Vec<InlineNode>,
    pub location: Location,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AdmonitionVariant {
    Note,
    Tip,
    Important,
    Caution,
    Warning,
}

impl FromStr for AdmonitionVariant {
    type Err = Error;

    fn from_str(variant: &str) -> Result<Self, Self::Err> {
        match variant {
            "NOTE" | "note" => Ok(AdmonitionVariant::Note),
            "TIP" | "tip" => Ok(AdmonitionVariant::Tip),
            "IMPORTANT" | "important" => Ok(AdmonitionVariant::Important),
            "CAUTION" | "caution" => Ok(AdmonitionVariant::Caution),
            "WARNING" | "warning" => Ok(AdmonitionVariant::Warning),
            _ => Err(Error::Parse(format!(
                "unknown admonition variant: {variant}"
            ))),
        }
    }
}

/// A `DelimitedBlockType` represents the type of a delimited block in a document.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DelimitedBlockType {
    DelimitedComment(Vec<InlineNode>),
    DelimitedExample(Vec<Block>),
    DelimitedListing(Vec<InlineNode>),
    DelimitedLiteral(Vec<InlineNode>),
    DelimitedOpen(Vec<Block>),
    DelimitedSidebar(Vec<Block>),
    DelimitedTable(Table),
    DelimitedPass(Vec<InlineNode>),
    DelimitedQuote(Vec<Block>),
    DelimitedVerse(Vec<InlineNode>),
}

impl DelimitedBlockType {
    fn name(&self) -> &'static str {
        match self {
            DelimitedBlockType::DelimitedComment(_) => "comment",
            DelimitedBlockType::DelimitedExample(_) => "example",
            DelimitedBlockType::DelimitedListing(_) => "listing",
            DelimitedBlockType::DelimitedLiteral(_) => "literal",
            DelimitedBlockType::DelimitedOpen(_) => "open",
            DelimitedBlockType::DelimitedSidebar(_) => "sidebar",
            DelimitedBlockType::DelimitedTable(_) => "table",
            DelimitedBlockType::DelimitedPass(_) => "pass",
            DelimitedBlockType::DelimitedQuote(_) => "quote",
            DelimitedBlockType::DelimitedVerse(_) => "verse",
        }
    }
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
    pub metadata: BlockMetadata,
    pub title: Vec<InlineNode>,
    pub level: SectionLevel,
    pub content: Vec<Block>,
    pub location: Location,
}

impl Serialize for Document {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(None)?;
        state.serialize_entry("name", "document")?;
        state.serialize_entry("type", "block")?;
        if let Some(header) = &self.header {
            state.serialize_entry("header", header)?;
            state.serialize_entry("attributes", &self.attributes)?;
        } else if !self.attributes.is_empty() {
            state.serialize_entry("attributes", &self.attributes)?;
        }
        if !self.blocks.is_empty() {
            state.serialize_entry("blocks", &self.blocks)?;
        }
        state.serialize_entry("location", &self.location)?;
        state.end()
    }
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
        if !self.content.is_empty() {
            state.serialize_entry("blocks", &self.content)?;
        }
        state.serialize_entry("location", &self.location)?;
        state.end()
    }
}

impl Serialize for DelimitedBlock {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(None)?;
        state.serialize_entry("name", self.inner.name())?;
        state.serialize_entry("type", "block")?;
        state.serialize_entry("form", "delimited")?;
        state.serialize_entry("delimiter", &self.delimiter)?;
        if !is_default_metadata(&self.metadata) {
            state.serialize_entry("metadata", &self.metadata)?;
        }

        match &self.inner {
            /* TODO(nlopes): missing stem */
            DelimitedBlockType::DelimitedListing(inner)
            | DelimitedBlockType::DelimitedLiteral(inner)
            | DelimitedBlockType::DelimitedPass(inner)
            | DelimitedBlockType::DelimitedVerse(inner) => {
                state.serialize_entry("inlines", &inner)?;
            }
            inner => {
                state.serialize_entry("blocks", &inner)?;
            }
        }
        if !self.title.is_empty() {
            state.serialize_entry("title", &self.title)?;
        }
        state.serialize_entry("location", &self.location)?;
        state.end()
    }
}

impl Serialize for UnorderedList {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(None)?;
        state.serialize_entry("name", "list")?;
        state.serialize_entry("type", "block")?;
        state.serialize_entry("variant", "unordered")?;
        state.serialize_entry("marker", &self.marker)?;
        if !self.title.is_empty() {
            state.serialize_entry("title", &self.title)?;
        }
        if !is_default_metadata(&self.metadata) {
            state.serialize_entry("metadata", &self.metadata)?;
        }
        state.serialize_entry("items", &self.items)?;
        state.serialize_entry("location", &self.location)?;
        state.end()
    }
}

impl Serialize for OrderedList {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(None)?;
        state.serialize_entry("name", "list")?;
        state.serialize_entry("type", "block")?;
        state.serialize_entry("variant", "ordered")?;
        state.serialize_entry("marker", &self.marker)?;
        if !self.title.is_empty() {
            state.serialize_entry("title", &self.title)?;
        }
        if !is_default_metadata(&self.metadata) {
            state.serialize_entry("metadata", &self.metadata)?;
        }
        state.serialize_entry("items", &self.items)?;
        state.serialize_entry("location", &self.location)?;
        state.end()
    }
}

impl Serialize for ListItem {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(None)?;
        state.serialize_entry("name", "listItem")?;
        state.serialize_entry("type", "block")?;
        state.serialize_entry("marker", &self.marker)?;
        state.serialize_entry("principal", &self.content)?;
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

impl Serialize for Paragraph {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(None)?;
        state.serialize_entry("name", "paragraph")?;
        state.serialize_entry("type", "block")?;
        if !self.title.is_empty() {
            state.serialize_entry("title", &self.title)?;
        }
        state.serialize_entry("inlines", &self.content)?;
        if !self.metadata.is_default() {
            state.serialize_entry("metadata", &self.metadata)?;
        }
        state.serialize_entry("location", &self.location)?;
        state.end()
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
                let mut my_id = None;
                let mut my_title = None;
                let mut my_level = None;
                let mut my_metadata = None;
                let mut my_location = None;
                let mut my_ref_text = None;
                let mut my_form = None;
                let mut my_target = None;
                let mut my_variant = None;
                let mut my_anchors = None;
                let mut my_marker = None;
                let mut my_blocks = None;
                let mut my_items = None;
                let mut my_inlines = None;
                let mut my_content: Option<serde_json::Value> = None;
                let mut my_delimiter = None;

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

                        "form" => {
                            if my_form.is_some() {
                                return Err(de::Error::duplicate_field("form"));
                            }
                            my_form = Some(map.next_value::<String>()?);
                        }
                        "target" => {
                            if my_target.is_some() {
                                return Err(de::Error::duplicate_field("target"));
                            }
                            my_target = Some(map.next_value::<String>()?);
                        }
                        "delimiter" => {
                            if my_delimiter.is_some() {
                                return Err(de::Error::duplicate_field("delimiter"));
                            }
                            my_delimiter = Some(map.next_value::<String>()?);
                        }
                        "reftext" => {
                            if my_ref_text.is_some() {
                                return Err(de::Error::duplicate_field("reftext"));
                            }
                            my_ref_text = Some(map.next_value::<String>()?);
                        }
                        "id" => {
                            if my_id.is_some() {
                                return Err(de::Error::duplicate_field("id"));
                            }
                            my_id = Some(map.next_value::<String>()?);
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
                        "variant" => {
                            if my_variant.is_some() {
                                return Err(de::Error::duplicate_field("variant"));
                            }
                            my_variant = Some(map.next_value::<String>()?);
                        }
                        "content" => {
                            if my_content.is_some() {
                                return Err(de::Error::duplicate_field("content"));
                            }
                            my_content = Some(map.next_value()?);
                        }
                        "blocks" => {
                            if my_blocks.is_some() {
                                return Err(de::Error::duplicate_field("blocks"));
                            }
                            my_blocks = Some(map.next_value()?);
                        }
                        "inlines" => {
                            if my_inlines.is_some() {
                                return Err(de::Error::duplicate_field("inlines"));
                            }
                            my_inlines = Some(map.next_value()?);
                        }
                        "marker" => {
                            if my_marker.is_some() {
                                return Err(de::Error::duplicate_field("marker"));
                            }
                            my_marker = Some(map.next_value::<String>()?);
                        }
                        "items" => {
                            if my_items.is_some() {
                                return Err(de::Error::duplicate_field("items"));
                            }
                            my_items = Some(map.next_value()?);
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
                let my_title = my_title.unwrap_or_else(Vec::new);
                let my_anchors = my_anchors.unwrap_or_else(Vec::new);
                let my_metadata = my_metadata.unwrap_or_else(BlockMetadata::default);
                let my_location =
                    my_location.ok_or_else(|| de::Error::missing_field("location"))?;

                match (my_name.as_str(), my_type.as_str()) {
                    ("section", "block") => {
                        let my_level = my_level.ok_or_else(|| de::Error::missing_field("level"))?;
                        let my_blocks =
                            match my_blocks.ok_or_else(|| de::Error::missing_field("blocks"))? {
                                serde_json::Value::Array(a) => a
                                    .into_iter()
                                    .map(|v| serde_json::from_value(v).map_err(de::Error::custom))
                                    .collect::<Result<Vec<Block>, _>>()?,
                                _ => return Err(de::Error::custom("blocks must be an array")),
                            };
                        Ok(Block::Section(Section {
                            metadata: my_metadata,
                            title: my_title,
                            level: my_level,
                            content: my_blocks,
                            location: my_location,
                        }))
                    }
                    ("paragraph", "block") => {
                        let my_inlines =
                            my_inlines.ok_or_else(|| de::Error::missing_field("inlines"))?;
                        Ok(Block::Paragraph(Paragraph {
                            metadata: my_metadata,
                            title: my_title,
                            content: my_inlines,
                            location: my_location,
                        }))
                    }
                    ("image", "block") => {
                        let my_form = my_form.ok_or_else(|| de::Error::missing_field("form"))?;
                        if my_form != "macro" {
                            return Err(de::Error::custom(format!("unexpected form: {my_form}")));
                        }
                        Ok(Block::Image(Image {
                            title: my_title,
                            // TODO(nlopes): this should be figured out if url or path
                            source: ImageSource::Path(
                                my_target.ok_or_else(|| de::Error::missing_field("target"))?,
                            ),
                            metadata: my_metadata,
                            location: my_location,
                        }))
                    }
                    ("audio", "block") => {
                        let my_form = my_form.ok_or_else(|| de::Error::missing_field("form"))?;
                        if my_form != "macro" {
                            return Err(de::Error::custom(format!("unexpected form: {my_form}")));
                        }
                        Ok(Block::Audio(Audio {
                            title: my_title,
                            source: AudioSource::Path(
                                my_target.ok_or_else(|| de::Error::missing_field("target"))?,
                            ),
                            metadata: my_metadata,
                            location: my_location,
                        }))
                    }
                    ("video", "block") => {
                        let my_form = my_form.ok_or_else(|| de::Error::missing_field("form"))?;
                        if my_form != "macro" {
                            return Err(de::Error::custom(format!("unexpected form: {my_form}")));
                        }
                        Ok(Block::Video(Video {
                            title: my_title,
                            sources: vec![VideoSource::Path(
                                my_target.ok_or_else(|| de::Error::missing_field("target"))?,
                            )],
                            metadata: my_metadata,
                            location: my_location,
                        }))
                    }
                    ("break", "block") => {
                        let my_variant =
                            my_variant.ok_or_else(|| de::Error::missing_field("variant"))?;
                        match my_variant.as_str() {
                            "page" => Ok(Block::PageBreak(PageBreak {
                                title: my_title,
                                metadata: my_metadata,
                                location: my_location,
                            })),
                            "thematic" => Ok(Block::ThematicBreak(ThematicBreak {
                                title: my_title,
                                anchors: my_anchors,
                                location: my_location,
                            })),
                            _ => Err(de::Error::custom(format!(
                                "unexpected 'break' variant: {my_variant}",
                            ))),
                        }
                    }
                    ("heading", "block") => Ok(Block::DiscreteHeader(DiscreteHeader {
                        title: my_title,
                        level: my_level.ok_or_else(|| de::Error::missing_field("level"))?,
                        anchors: my_anchors, // TODO: this should be in metadata instead?
                        location: my_location,
                    })),
                    ("example", "block") => {
                        let my_form = my_form.ok_or_else(|| de::Error::missing_field("form"))?;
                        if my_form != "delimited" {
                            return Err(de::Error::custom(format!("unexpected form: {my_form}")));
                        }
                        let my_delimiter =
                            my_delimiter.ok_or_else(|| de::Error::missing_field("delimiter"))?;
                        let my_blocks =
                            match my_blocks.ok_or_else(|| de::Error::missing_field("blocks"))? {
                                serde_json::Value::Array(a) => a
                                    .into_iter()
                                    .map(|v| serde_json::from_value(v).map_err(de::Error::custom))
                                    .collect::<Result<Vec<Block>, _>>()?,
                                _ => return Err(de::Error::custom("blocks must be an array")),
                            };
                        Ok(Block::DelimitedBlock(DelimitedBlock {
                            metadata: my_metadata,
                            inner: DelimitedBlockType::DelimitedExample(my_blocks),
                            title: my_title,
                            delimiter: my_delimiter,
                            location: my_location,
                        }))
                    }
                    ("sidebar", "block") => {
                        let my_form = my_form.ok_or_else(|| de::Error::missing_field("form"))?;
                        if my_form != "delimited" {
                            return Err(de::Error::custom(format!("unexpected form: {my_form}")));
                        }
                        let my_delimiter =
                            my_delimiter.ok_or_else(|| de::Error::missing_field("delimiter"))?;
                        let my_blocks =
                            match my_blocks.ok_or_else(|| de::Error::missing_field("blocks"))? {
                                serde_json::Value::Array(a) => a
                                    .into_iter()
                                    .map(|v| serde_json::from_value(v).map_err(de::Error::custom))
                                    .collect::<Result<Vec<Block>, _>>()?,
                                _ => return Err(de::Error::custom("blocks must be an array")),
                            };
                        Ok(Block::DelimitedBlock(DelimitedBlock {
                            metadata: my_metadata,
                            inner: DelimitedBlockType::DelimitedSidebar(my_blocks),
                            delimiter: my_delimiter,
                            title: my_title,
                            location: my_location,
                        }))
                    }
                    ("open", "block") => {
                        let my_form = my_form.ok_or_else(|| de::Error::missing_field("form"))?;
                        if my_form != "delimited" {
                            return Err(de::Error::custom(format!("unexpected form: {my_form}")));
                        }
                        let my_delimiter =
                            my_delimiter.ok_or_else(|| de::Error::missing_field("delimiter"))?;
                        let my_blocks =
                            match my_blocks.ok_or_else(|| de::Error::missing_field("blocks"))? {
                                serde_json::Value::Array(a) => a
                                    .into_iter()
                                    .map(|v| serde_json::from_value(v).map_err(de::Error::custom))
                                    .collect::<Result<Vec<Block>, _>>()?,
                                _ => return Err(de::Error::custom("blocks must be an array")),
                            };
                        Ok(Block::DelimitedBlock(DelimitedBlock {
                            metadata: my_metadata,
                            inner: DelimitedBlockType::DelimitedOpen(my_blocks),
                            title: my_title,
                            delimiter: my_delimiter,
                            location: my_location,
                        }))
                    }
                    ("quote", "block") => {
                        let my_form = my_form.ok_or_else(|| de::Error::missing_field("form"))?;
                        if my_form != "delimited" {
                            return Err(de::Error::custom(format!("unexpected form: {my_form}")));
                        }
                        let my_delimiter =
                            my_delimiter.ok_or_else(|| de::Error::missing_field("delimiter"))?;
                        let my_blocks =
                            match my_blocks.ok_or_else(|| de::Error::missing_field("blocks"))? {
                                serde_json::Value::Array(a) => a
                                    .into_iter()
                                    .map(|v| serde_json::from_value(v).map_err(de::Error::custom))
                                    .collect::<Result<Vec<Block>, _>>()?,
                                _ => return Err(de::Error::custom("blocks must be an array")),
                            };
                        Ok(Block::DelimitedBlock(DelimitedBlock {
                            metadata: my_metadata,
                            inner: DelimitedBlockType::DelimitedQuote(my_blocks),
                            title: my_title,
                            delimiter: my_delimiter,
                            location: my_location,
                        }))
                    }
                    ("verse", "block") => {
                        let my_form = my_form.ok_or_else(|| de::Error::missing_field("form"))?;
                        if my_form != "delimited" {
                            return Err(de::Error::custom(format!("unexpected form: {my_form}")));
                        }
                        let my_delimiter =
                            my_delimiter.ok_or_else(|| de::Error::missing_field("delimiter"))?;
                        let my_inlines =
                            my_inlines.ok_or_else(|| de::Error::missing_field("inlines"))?;
                        Ok(Block::DelimitedBlock(DelimitedBlock {
                            metadata: my_metadata,
                            inner: DelimitedBlockType::DelimitedVerse(my_inlines),
                            title: my_title,
                            delimiter: my_delimiter,
                            location: my_location,
                        }))
                    }
                    ("listing", "block") => {
                        let my_form = my_form.ok_or_else(|| de::Error::missing_field("form"))?;
                        if my_form != "delimited" {
                            return Err(de::Error::custom(format!("unexpected form: {my_form}")));
                        }
                        let my_delimiter =
                            my_delimiter.ok_or_else(|| de::Error::missing_field("delimiter"))?;
                        let my_inlines =
                            my_inlines.ok_or_else(|| de::Error::missing_field("inlines"))?;
                        Ok(Block::DelimitedBlock(DelimitedBlock {
                            metadata: my_metadata,
                            inner: DelimitedBlockType::DelimitedListing(my_inlines),
                            title: my_title,
                            delimiter: my_delimiter,
                            location: my_location,
                        }))
                    }
                    ("literal", "block") => {
                        let my_form = my_form.ok_or_else(|| de::Error::missing_field("form"))?;
                        if my_form != "delimited" {
                            return Err(de::Error::custom(format!("unexpected form: {my_form}")));
                        }
                        let my_delimiter =
                            my_delimiter.ok_or_else(|| de::Error::missing_field("delimiter"))?;
                        let my_inlines =
                            my_inlines.ok_or_else(|| de::Error::missing_field("inlines"))?;
                        Ok(Block::DelimitedBlock(DelimitedBlock {
                            metadata: my_metadata,
                            inner: DelimitedBlockType::DelimitedLiteral(my_inlines),
                            title: my_title,
                            delimiter: my_delimiter,
                            location: my_location,
                        }))
                    }
                    ("pass", "block") => {
                        let my_form = my_form.ok_or_else(|| de::Error::missing_field("form"))?;
                        if my_form != "delimited" {
                            return Err(de::Error::custom(format!("unexpected form: {my_form}")));
                        }
                        let my_delimiter =
                            my_delimiter.ok_or_else(|| de::Error::missing_field("delimiter"))?;
                        let my_inlines =
                            my_inlines.ok_or_else(|| de::Error::missing_field("inlines"))?;
                        Ok(Block::DelimitedBlock(DelimitedBlock {
                            metadata: my_metadata,
                            inner: DelimitedBlockType::DelimitedPass(my_inlines),
                            title: my_title,
                            delimiter: my_delimiter,
                            location: my_location,
                        }))
                    }
                    ("table", "block") => {
                        let my_form = my_form.ok_or_else(|| de::Error::missing_field("form"))?;
                        if my_form != "delimited" {
                            return Err(de::Error::custom(format!("unexpected form: {my_form}")));
                        }
                        let my_delimiter =
                            my_delimiter.ok_or_else(|| de::Error::missing_field("delimiter"))?;
                        let inner = DelimitedBlockType::DelimitedTable(
                            serde_json::from_value(
                                my_content.ok_or_else(|| de::Error::missing_field("content"))?,
                            )
                            .map_err(|_| {
                                tracing::error!("content must be compatible with `Table` type");
                                de::Error::custom("content must be compatible with `Table` type")
                            })?,
                        );
                        Ok(Block::DelimitedBlock(DelimitedBlock {
                            metadata: my_metadata,
                            inner,
                            title: my_title,
                            delimiter: my_delimiter,
                            location: my_location,
                        }))
                    }
                    ("dlist", "block") => {
                        let _my_marker = my_marker.unwrap_or_else(String::new); // TODO: what is this marker?
                        Ok(Block::DescriptionList(DescriptionList {
                            title: my_title,
                            metadata: my_metadata,
                            items: match my_items
                                .ok_or_else(|| de::Error::missing_field("items"))?
                            {
                                serde_json::Value::Array(a) => a
                                    .into_iter()
                                    .map(|v| serde_json::from_value(v).map_err(de::Error::custom))
                                    .collect::<Result<Vec<DescriptionListItem>, _>>()?,
                                _ => return Err(de::Error::custom("items must be an array")),
                            },
                            location: my_location,
                        }))
                    }
                    ("list", "block") => {
                        let my_variant =
                            my_variant.ok_or_else(|| de::Error::missing_field("variant"))?;
                        let my_marker =
                            my_marker.ok_or_else(|| de::Error::missing_field("marker"))?;
                        match my_variant.as_str() {
                            "unordered" => Ok(Block::UnorderedList(UnorderedList {
                                title: my_title,
                                metadata: my_metadata,
                                marker: my_marker,
                                items: match my_items
                                    .ok_or_else(|| de::Error::missing_field("items"))?
                                {
                                    serde_json::Value::Array(a) => a
                                        .into_iter()
                                        .map(|v| {
                                            serde_json::from_value(v).map_err(de::Error::custom)
                                        })
                                        .collect::<Result<Vec<ListItem>, _>>()?,
                                    _ => return Err(de::Error::custom("items must be an array")),
                                },
                                location: my_location,
                            })),
                            "ordered" => Ok(Block::OrderedList(OrderedList {
                                title: my_title,
                                metadata: my_metadata,
                                marker: my_marker,
                                items: match my_items
                                    .ok_or_else(|| de::Error::missing_field("items"))?
                                {
                                    serde_json::Value::Array(a) => a
                                        .into_iter()
                                        .map(|v| {
                                            serde_json::from_value(v).map_err(de::Error::custom)
                                        })
                                        .collect::<Result<Vec<ListItem>, _>>()?,
                                    _ => return Err(de::Error::custom("items must be an array")),
                                },
                                location: my_location,
                            })),
                            "callout" => todo!("callout list"),
                            _ => Err(de::Error::custom(format!(
                                "unexpected 'list' variant: {my_variant}",
                            ))),
                        }
                    }
                    ("admonition", "block") => {
                        let my_variant =
                            my_variant.ok_or_else(|| de::Error::missing_field("variant"))?;
                        let my_blocks =
                            match my_blocks.ok_or_else(|| de::Error::missing_field("blocks"))? {
                                serde_json::Value::Array(a) => a
                                    .into_iter()
                                    .map(|v| serde_json::from_value(v).map_err(de::Error::custom))
                                    .collect::<Result<Vec<Block>, _>>()?,
                                _ => return Err(de::Error::custom("blocks must be an array")),
                            };
                        Ok(Block::Admonition(Admonition {
                            metadata: my_metadata,
                            variant: AdmonitionVariant::from_str(my_variant.as_str())
                                .map_err(de::Error::custom)?,
                            blocks: my_blocks,
                            title: my_title,
                            location: my_location,
                        }))
                    }
                    _ => Err(de::Error::custom(format!(
                        "unexpected name/type combination: {my_name}/{my_type}",
                    ))),
                }
            }
        }

        deserializer.deserialize_map(MyStructVisitor)
    }
}

impl<'de> Deserialize<'de> for ListItem {
    #[allow(clippy::too_many_lines)]
    fn deserialize<D>(deserializer: D) -> Result<ListItem, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ListItemVisitor;

        impl<'de> Visitor<'de> for ListItemVisitor {
            type Value = ListItem;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a struct representing ListItem")
            }

            fn visit_map<V>(self, mut map: V) -> Result<ListItem, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut my_name = None;
                let mut my_type = None;
                let mut my_marker = None;
                let mut my_principal: Option<Vec<InlineNode>> = None;
                let mut my_location = None;
                let mut my_checked = None;

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
                        "principal" => {
                            if my_principal.is_some() {
                                return Err(de::Error::duplicate_field("principal"));
                            }
                            my_principal = Some(map.next_value()?);
                        }
                        "checked" => {
                            if my_checked.is_some() {
                                return Err(de::Error::duplicate_field("marker"));
                            }
                            my_checked = Some(map.next_value::<Option<bool>>()?);
                        }
                        "marker" => {
                            if my_marker.is_some() {
                                return Err(de::Error::duplicate_field("marker"));
                            }
                            my_marker = Some(map.next_value::<String>()?);
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
                let my_marker = my_marker.ok_or_else(|| de::Error::missing_field("marker"))?;
                let my_location =
                    my_location.ok_or_else(|| de::Error::missing_field("location"))?;
                let my_principal =
                    my_principal.ok_or_else(|| de::Error::missing_field("principal"))?;

                if my_name != "listItem" {
                    return Err(de::Error::custom(format!("unexpected name: {my_name}")));
                }
                if my_type != "block" {
                    return Err(de::Error::custom(format!("unexpected type: {my_type}")));
                }

                // Calculate the level of depth of the list item from the marker
                let level =
                    ListLevel::try_from(ListItem::parse_depth_from_marker(&my_marker).unwrap_or(1))
                        .map_err(de::Error::custom)?;
                let my_checked = my_checked.unwrap_or(None);

                Ok(ListItem {
                    marker: my_marker,
                    content: my_principal,
                    location: my_location,
                    checked: my_checked,
                    level,
                })
            }
        }
        deserializer.deserialize_map(ListItemVisitor)
    }
}
