//! The data models for the `AsciiDoc` document.
use std::{collections::HashMap, path::Path};

use serde::{
    ser::{SerializeSeq, Serializer},
    Deserialize, Serialize,
};

use crate::Error;

#[derive(Default, Debug, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub(crate) name: String,
    pub(crate) r#type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header: Option<Header>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub attributes: HashMap<AttributeName, AttributeValue>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<Block>,
    pub location: Location,
}

type Subtitle = String;

#[derive(Default, Debug, PartialEq, Serialize, Deserialize)]
pub struct Title {
    pub(crate) name: String,
    pub(crate) r#type: String,
    #[serde(rename = "value")]
    pub title: String,
    pub location: Location,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Header {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<Title>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<Subtitle>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub authors: Vec<Author>,
    pub location: Location,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Author {
    pub first_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub middle_name: Option<String>,
    pub last_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AttributeEntry {
    pub name: Option<AttributeName>,
    pub value: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Anchor {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub xreflabel: Option<String>,
    pub location: Location,
}

pub type Role = String;

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

#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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

impl BlockExt for Block {
    fn set_metadata(&mut self, metadata: BlockMetadata) {
        match self {
            Block::DiscreteHeader(_header) => {}
            Block::DocumentAttribute(_attr) => {}
            Block::ThematicBreak(_thematic_break) => {}
            Block::PageBreak(page_break) => page_break.metadata = metadata,
            Block::UnorderedList(unordered_list) => unordered_list.metadata = metadata,
            Block::OrderedList(ordered_list) => ordered_list.metadata = metadata,
            Block::DescriptionList(description_list) => description_list.metadata = metadata,
            Block::Section(section) => section.metadata = metadata,
            Block::DelimitedBlock(delimited_block) => delimited_block.metadata = metadata,
            Block::Paragraph(paragraph) => paragraph.metadata = metadata,
            Block::Image(image) => image.metadata = metadata,
            Block::Audio(audio) => audio.metadata = metadata,
            Block::Video(video) => video.metadata = metadata,
        }
    }

    fn set_attributes(&mut self, attributes: Vec<AttributeEntry>) {
        match self {
            Block::DiscreteHeader(_header) => {}
            Block::DocumentAttribute(_attr) => {}
            Block::ThematicBreak(_thematic_break) => {}
            Block::PageBreak(page_break) => page_break.attributes = attributes,
            Block::UnorderedList(unordered_list) => unordered_list.attributes = attributes,
            Block::OrderedList(ordered_list) => ordered_list.attributes = attributes,
            Block::DescriptionList(description_list) => description_list.attributes = attributes,
            Block::Section(section) => section.attributes = attributes,
            Block::DelimitedBlock(delimited_block) => delimited_block.attributes = attributes,
            Block::Paragraph(paragraph) => paragraph.attributes = attributes,
            Block::Image(image) => image.attributes = attributes,
            Block::Audio(audio) => audio.attributes = attributes,
            Block::Video(video) => video.attributes = attributes,
        }
    }

    fn set_anchors(&mut self, anchors: Vec<Anchor>) {
        match self {
            Block::DiscreteHeader(header) => header.anchors = anchors,
            Block::DocumentAttribute(_attr) => {}
            Block::ThematicBreak(thematic_break) => thematic_break.anchors = anchors,
            Block::PageBreak(page_break) => page_break.metadata.anchors = anchors,
            Block::UnorderedList(unordered_list) => unordered_list.metadata.anchors = anchors,
            Block::OrderedList(ordered_list) => ordered_list.metadata.anchors = anchors,
            Block::DescriptionList(description_list) => description_list.metadata.anchors = anchors,
            Block::Section(section) => section.metadata.anchors = anchors,
            Block::DelimitedBlock(delimited_block) => delimited_block.metadata.anchors = anchors,
            Block::Paragraph(paragraph) => paragraph.metadata.anchors = anchors,
            Block::Image(image) => image.metadata.anchors = anchors,
            Block::Audio(audio) => audio.metadata.anchors = anchors,
            Block::Video(video) => video.metadata.anchors = anchors,
        }
    }

    fn set_title(&mut self, title: String) {
        match self {
            Block::DiscreteHeader(header) => header.title = title,
            Block::DocumentAttribute(_attr) => {}
            Block::ThematicBreak(thematic_break) => thematic_break.title = Some(title),
            Block::PageBreak(page_break) => page_break.title = Some(title),
            Block::UnorderedList(unordered_list) => unordered_list.title = Some(title),
            Block::OrderedList(ordered_list) => ordered_list.title = Some(title),
            Block::DescriptionList(description_list) => description_list.title = Some(title),
            Block::Section(section) => section.title = title,
            Block::DelimitedBlock(delimited_block) => delimited_block.title = Some(title),
            Block::Paragraph(paragraph) => paragraph.title = Some(title),
            Block::Image(image) => image.title = Some(title),
            Block::Audio(audio) => audio.title = Some(title),
            Block::Video(video) => video.title = Some(title),
        }
    }

    #[must_use]
    fn set_location(&mut self, location: Location) {
        match self {
            Block::DiscreteHeader(header) => header.location = location,
            Block::DocumentAttribute(attr) => attr.location = location,
            Block::ThematicBreak(thematic_break) => thematic_break.location = location,
            Block::PageBreak(page_break) => page_break.location = location,
            Block::UnorderedList(unordered_list) => unordered_list.location = location,
            Block::OrderedList(ordered_list) => ordered_list.location = location,
            Block::DescriptionList(description_list) => description_list.location = location,
            Block::Section(section) => section.location = location,
            Block::DelimitedBlock(delimited_block) => delimited_block.location = location,
            Block::Paragraph(paragraph) => paragraph.location = location,
            Block::Image(image) => image.location = location,
            Block::Audio(audio) => audio.location = location,
            Block::Video(video) => video.location = location,
        }
    }
}

pub(crate) trait BlockExt {
    fn set_location(&mut self, location: Location);
    fn set_anchors(&mut self, anchor: Vec<Anchor>);
    fn set_title(&mut self, title: String);
    fn set_attributes(&mut self, attributes: Vec<AttributeEntry>);
    fn set_metadata(&mut self, metadata: BlockMetadata);
}

impl std::fmt::Display for Block {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Block::DiscreteHeader(_) => write!(f, "DiscreteHeader"),
            Block::DocumentAttribute(_) => write!(f, "DocumentAttribute"),
            Block::ThematicBreak(_) => write!(f, "ThematicBreak"),
            Block::PageBreak(_) => write!(f, "PageBreak"),
            Block::UnorderedList(_) => write!(f, "UnorderedList"),
            Block::OrderedList(_) => write!(f, "OrderedList"),
            Block::DescriptionList(_) => write!(f, "DescriptionList"),
            Block::Section(_) => write!(f, "Section"),
            Block::DelimitedBlock(_) => write!(f, "DelimitedBlock"),
            Block::Paragraph(_) => write!(f, "Paragraph"),
            Block::Image(_) => write!(f, "Image"),
            Block::Audio(_) => write!(f, "Audio"),
            Block::Video(_) => write!(f, "Video"),
        }
    }
}

pub type AttributeName = String;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AttributeValue {
    String(String),
    Bool(bool),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DocumentAttribute {
    pub name: AttributeName,
    pub value: AttributeValue,
    pub location: Location,
}

#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum InlineNode {
    PlainText(PlainText),
    BoldText(BoldText),
    ItalicText(ItalicText),
    MonospaceText(MonospaceText),
    HighlightText(HighlightText),
    InlineLineBreak(Location),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DiscreteHeader {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub anchors: Vec<Anchor>,
    pub title: String,
    pub level: u8,
    pub location: Location,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MonospaceText {
    pub role: Option<Role>,
    pub content: Vec<InlineNode>,
    pub location: Location,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HighlightText {
    pub role: Option<Role>,
    pub content: Vec<InlineNode>,
    pub location: Location,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BoldText {
    pub role: Option<Role>,
    pub content: Vec<InlineNode>,
    pub location: Location,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ItalicText {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    pub content: Vec<InlineNode>,
    pub location: Location,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlainText {
    pub content: String,
    pub location: Location,
}

#[derive(Clone, Default, Debug, PartialEq, Serialize, Deserialize)]
pub struct ThematicBreak {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub anchors: Vec<Anchor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub location: Location,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PageBreak {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "is_default_metadata")]
    pub metadata: BlockMetadata,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<AttributeEntry>,
    pub location: Location,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Audio {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub source: AudioSource,
    #[serde(default, skip_serializing_if = "is_default_metadata")]
    pub metadata: BlockMetadata,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<AttributeEntry>,
    pub location: Location,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Video {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<VideoSource>,
    #[serde(default, skip_serializing_if = "is_default_metadata")]
    pub metadata: BlockMetadata,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<AttributeEntry>,
    pub location: Location,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Image {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub source: ImageSource,
    #[serde(default, skip_serializing_if = "is_default_metadata")]
    pub metadata: BlockMetadata,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<AttributeEntry>,
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DescriptionList {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "is_default_metadata")]
    pub metadata: BlockMetadata,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<AttributeEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<DescriptionListItem>,
    pub location: Location,
}

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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UnorderedList {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "is_default_metadata")]
    pub metadata: BlockMetadata,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<AttributeEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<ListItem>,
    pub location: Location,
}

pub type OrderedList = UnorderedList;
pub type ListLevel = u8;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ListItem {
    // TODO(nlopes): missing anchors
    pub level: ListLevel,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub content: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Paragraph {
    #[serde(default, skip_serializing_if = "is_default_metadata")]
    pub metadata: BlockMetadata,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<AttributeEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DelimitedBlock {
    #[serde(default, skip_serializing_if = "is_default_metadata")]
    pub metadata: BlockMetadata,
    pub inner: DelimitedBlockType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<AttributeEntry>,
    pub location: Location,
}

#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum DelimitedBlockType {
    DelimitedComment(String),
    DelimitedExample(Vec<Block>),
    DelimitedListing(String),
    DelimitedLiteral(String),
    DelimitedOpen(Vec<Block>),
    DelimitedSidebar(Vec<Block>),
    DelimitedTable(String),
    DelimitedPass(String),
    DelimitedQuote(Vec<Block>),
}

pub type SectionLevel = u8;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Section {
    #[serde(default, skip_serializing_if = "is_default_metadata")]
    pub metadata: BlockMetadata,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<AttributeEntry>,
    pub title: String,
    pub level: SectionLevel,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub content: Vec<Block>,
    pub location: Location,
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Deserialize)]
pub struct Location {
    pub start: Position,
    pub end: Position,
}

// We need to implement `Serialize` because I prefer our current `Location` struct to the
// `asciidoc` `ASG` definition.
//
// We serialize `Location` into the ASG format, which is a sequence of two elements: the
// start and end positions as an array.
impl Serialize for Location {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_seq(Some(2))?;
        state.serialize_element(&self.start)?;
        state.serialize_element(&self.end)?;
        state.end()
    }
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub line: usize,
    #[serde(rename = "col")]
    pub column: usize,
}

/// The `Parser` trait defines the interface for parsing `AsciiDoc` documents.
pub trait Parser {
    /// Parse the input string and return a Document.
    ///
    /// # Arguments
    ///
    /// * `input` - A string slice that holds the input to be parsed.
    ///
    /// # Returns
    ///
    /// A `Document` if the `input` was successfully parsed, or an `Error` if the input
    /// could not be parsed.
    ///
    /// # Errors
    ///
    /// Returns an `Error` if the input string cannot be parsed.
    fn parse(&self, input: &str) -> Result<Document, Error>;

    /// Parse the file in `file_path` and return a Document.
    ///
    /// # Arguments
    ///
    /// * `file_path` - A file path that holds the input to be parsed.
    ///
    /// # Returns
    ///
    /// A `Document` if the `file_path` was successfully parsed, or an `Error` if the
    /// input could not be parsed.
    ///
    /// # Errors
    ///
    /// Returns an `Error` if the input from `file_path` cannot be parsed.
    fn parse_file<P: AsRef<Path>>(&self, file_path: P) -> Result<Document, Error>;
}
