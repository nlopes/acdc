//! The data models for the `AsciiDoc` document.
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use acdc_core::{AttributeName, AttributeValue, DocumentAttributes, Location, Substitution};
use serde::{Deserialize, Serialize};

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

/// A `Title` represents the title of a document.
#[derive(Default, Debug, PartialEq, Serialize, Deserialize)]
pub struct Title {
    pub(crate) name: String,
    pub(crate) r#type: String,
    #[serde(rename = "value")]
    pub title: Vec<InlineNode>,
    pub location: Location,
}

/// A `Header` represents the header of a document.
///
/// The header contains the title, subtitle, and authors
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

/// An `Author` represents the author of a document.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Author {
    pub first_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub middle_name: Option<String>,
    pub last_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
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

/// A `DocumentAttribute` represents a document attribute in a document.
///
/// A document attribute is a key-value pair that can be used to set metadata in a
/// document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DocumentAttribute {
    pub name: AttributeName,
    pub value: AttributeValue,
    pub location: Location,
}

/// An `InlineNode` represents an inline node in a document.
///
/// An inline node is a structural element in a document that can contain other inline
/// nodes and are only valid within a paragraph (a leaf).
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum InlineNode {
    PlainText(PlainText),
    BoldText(BoldText),
    ItalicText(ItalicText),
    MonospaceText(MonospaceText),
    HighlightText(HighlightText),
    SubscriptText(SubscriptText),
    SuperscriptText(SuperscriptText),
    InlineLineBreak(Location),
    Macro(InlineMacro),
}

/// An `InlineMacro` represents an inline macro in a document.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum InlineMacro {
    Icon(Icon),
    Image(Box<Image>),
    Keyboard(Keyboard),
    Button(Button),
    Menu(Menu),
    Url(Url),
    Link(Link),
    Autolink(Autolink),
    Pass(Pass),
}

/// A `Pass` represents a passthrough macro in a document.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Pass {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "HashSet::is_empty")]
    pub substitutions: HashSet<Substitution>,
    pub location: Location,
}

/// An `Icon` represents an inline icon in a document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Icon {
    pub target: String,
    pub attributes: HashMap<AttributeName, Option<String>>,
    pub location: Location,
}

/// A `Link` represents an inline link in a document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Link {
    pub target: LinkTarget,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub attributes: HashMap<AttributeName, Option<String>>,
    pub location: Location,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum LinkTarget {
    Url(String),
    Path(PathBuf),
}

/// An `Url` represents an inline URL in a document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Url {
    pub target: String,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub attributes: HashMap<AttributeName, Option<String>>,
    pub location: Location,
}

/// A `Button` represents an inline button in a document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Button {
    pub label: String,
    pub location: Location,
}

/// A `Menu` represents an inline menu in a document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Menu {
    pub target: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<String>,
    pub location: Location,
}

/// A `Keyboard` represents an inline keyboard shortcut in a document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Keyboard {
    pub keys: Vec<Key>,
    pub location: Location,
}

// TODO(nlopes): this could perhaps be an enum instead with the allowed keys
pub type Key = String;

/// An `Autolink` represents an inline autolink in a document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Autolink {
    pub url: String,
    pub location: Location,
}

/// A `DiscreteHeader` represents a discrete header in a document.
///
/// Discrete headings are useful for making headings inside of other blocks, like a
/// sidebar.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DiscreteHeader {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub anchors: Vec<Anchor>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub title: Vec<InlineNode>,
    pub level: u8,
    pub location: Location,
}

/// A `SubscriptText` represents a subscript section of text in a document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SubscriptText {
    pub role: Option<Role>,
    pub content: Vec<InlineNode>,
    pub location: Location,
}

/// A `SuperscriptText` represents a superscript section of text in a document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SuperscriptText {
    pub role: Option<Role>,
    pub content: Vec<InlineNode>,
    pub location: Location,
}

/// A `MonospaceText` represents a monospace section of text in a document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MonospaceText {
    pub role: Option<Role>,
    pub content: Vec<InlineNode>,
    pub location: Location,
}

/// A `HighlightText` represents a highlighted section of text in a document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HighlightText {
    pub role: Option<Role>,
    pub content: Vec<InlineNode>,
    pub location: Location,
}

/// A `BoldText` represents a bold section of text in a document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BoldText {
    pub role: Option<Role>,
    pub content: Vec<InlineNode>,
    pub location: Location,
}

/// An `ItalicText` represents an italic section of text in a document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ItalicText {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    pub content: Vec<InlineNode>,
    pub location: Location,
}

/// A `PlainText` represents a plain text section in a document.
///
/// This is the most basic form of text in a document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlainText {
    pub content: String,
    pub location: Location,
}

/// A `ThematicBreak` represents a thematic break in a document.
#[derive(Clone, Default, Debug, PartialEq, Serialize, Deserialize)]
pub struct ThematicBreak {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub anchors: Vec<Anchor>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub title: Vec<InlineNode>,
    pub location: Location,
}

/// A `PageBreak` represents a page break in a document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Section {
    #[serde(default, skip_serializing_if = "is_default_metadata")]
    pub metadata: BlockMetadata,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub attributes: HashMap<AttributeName, Option<String>>,
    pub title: Vec<InlineNode>,
    pub level: SectionLevel,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub content: Vec<Block>,
    pub location: Location,
}
