//! The data models for the `AsciiDoc` document.
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

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
    SubscriptText(SubscriptText),
    SuperscriptText(SuperscriptText),
    InlineLineBreak(Location),
    Macro(InlineMacro),
}

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

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Pass {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "HashSet::is_empty")]
    pub substitutions: HashSet<Substitution>,
    pub location: Location,
}

#[derive(Clone, Debug, Hash, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Substitution {
    SpecialChars,
    Attributes,
    Replacements,
    Macros,
    PostReplacements,
    Normal,
    Verbatim,
    Quotes,
}

impl From<&str> for Substitution {
    fn from(value: &str) -> Self {
        match value {
            "specialchars" | "c" => Substitution::SpecialChars,
            "attributes" | "a" => Substitution::Attributes,
            "replacements" | "r" => Substitution::Replacements,
            "macros" | "m" => Substitution::Macros,
            "post_replacements" | "p" => Substitution::PostReplacements,
            "normal" | "n" => Substitution::Normal,
            "verbatim" | "v" => Substitution::Verbatim,
            "quotes" | "q" => Substitution::Quotes,
            unknown => unimplemented!("{unknown:?}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Icon {
    pub target: String,
    pub attributes: HashMap<AttributeName, Option<String>>,
    pub location: Location,
}

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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Url {
    pub target: String,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub attributes: HashMap<AttributeName, Option<String>>,
    pub location: Location,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Button {
    pub label: String,
    pub location: Location,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Menu {
    pub target: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<String>,
    pub location: Location,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Keyboard {
    pub keys: Vec<Key>,
    pub location: Location,
}

// TODO(nlopes): this could perhaps be an enum instead with the allowed keys
pub type Key = String;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Autolink {
    pub url: String,
    pub location: Location,
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
pub struct SubscriptText {
    pub role: Option<Role>,
    pub content: Vec<InlineNode>,
    pub location: Location,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SuperscriptText {
    pub role: Option<Role>,
    pub content: Vec<InlineNode>,
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
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub attributes: HashMap<AttributeName, Option<String>>,
    pub location: Location,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Audio {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub source: AudioSource,
    #[serde(default, skip_serializing_if = "is_default_metadata")]
    pub metadata: BlockMetadata,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub attributes: HashMap<AttributeName, Option<String>>,
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
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub attributes: HashMap<AttributeName, Option<String>>,
    pub location: Location,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Image {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DescriptionList {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "is_default_metadata")]
    pub metadata: BlockMetadata,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub attributes: HashMap<AttributeName, Option<String>>,
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
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub attributes: HashMap<AttributeName, Option<String>>,
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
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub attributes: HashMap<AttributeName, Option<String>>,
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
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub attributes: HashMap<AttributeName, Option<String>>,
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
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub attributes: HashMap<AttributeName, Option<String>>,
    pub title: String,
    pub level: SectionLevel,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub content: Vec<Block>,
    pub location: Location,
}

#[derive(Debug, Default, Clone, Hash, Eq, PartialEq, Deserialize)]
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

#[derive(Debug, Default, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
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
