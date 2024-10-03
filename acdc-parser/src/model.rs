use serde::{Deserialize, Serialize};

use crate::Error;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub header: Option<Header>,
    pub content: Vec<Block>,
}

type Title = String;
type Subtitle = String;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Header {
    pub title: Option<Title>,
    pub subtitle: Option<Subtitle>,
    pub authors: Vec<Author>,
    pub revision: Option<Revision>,
    pub attributes: Vec<AttributeEntry>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Revision {
    pub number: String,
    pub date: Option<String>,
    pub remark: Option<String>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Author {
    pub first_name: String,
    pub middle_name: Option<String>,
    pub last_name: String,
    pub email: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AttributeEntry {
    pub name: Option<String>,
    pub value: Option<String>,
}

#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Block {
    UnorderedList(UnorderedList),
    OrderedList(OrderedList),
    Section(Section),
    DelimitedBlock(DelimitedBlock),
    Paragraph(Paragraph),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UnorderedList {
    pub title: Option<String>,
    pub items: Vec<ListItem>,
    pub location: Location,
}

pub type OrderedList = UnorderedList;
pub type ListLevel = u8;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ListItem {
    pub level: ListLevel,
    pub checked: Option<bool>,
    pub content: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Paragraph {
    pub roles: Vec<String>,
    pub attributes: Vec<AttributeEntry>,
    pub content: String,
    pub location: Location,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DelimitedBlock {
    pub inner: DelimitedBlockType,
    pub anchor: Option<String>,
    pub title: Option<String>,
    pub attributes: Vec<AttributeEntry>,
    pub location: Location,
}

#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum DelimitedBlockType {
    DelimitedComment(String),
    DelimitedExample(String),
    DelimitedListing(String),
    DelimitedLiteral(String),
    DelimitedOpen(String),
    DelimitedSidebar(String),
    DelimitedTable(String),
    DelimitedPass(String),
    DelimitedQuote(String),
}

pub type SectionLevel = u8;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Section {
    pub title: String,
    pub level: SectionLevel,
    pub content: Vec<Block>,
    pub location: Location,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct Location {
    pub start: Position,
    pub end: Position,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub line: usize,
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
}
