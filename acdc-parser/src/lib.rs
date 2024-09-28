mod error;
pub use error::{Detail as ErrorDetail, Error};

#[derive(Debug, PartialEq)]
pub struct Document {
    pub header: Option<Header>,
    pub content: Vec<Block>,
}

type Title = String;
type Subtitle = String;

#[derive(Debug, PartialEq)]
pub struct Header {
    pub title: Option<Title>,
    pub subtitle: Option<Subtitle>,
    pub authors: Vec<Author>,
    pub revision: Option<Revision>,
    pub attributes: Vec<AttributeEntry>,
}

#[derive(Debug, PartialEq)]
pub struct Revision {
    pub number: String,
    pub date: Option<String>,
    pub remark: Option<String>,
}

#[derive(Debug, PartialEq)]
pub struct Author {
    pub first_name: String,
    pub middle_name: Option<String>,
    pub last_name: String,
    pub email: Option<String>,
}

#[derive(Debug, PartialEq)]
pub struct AttributeEntry {
    pub name: String,
    pub value: Option<String>,
}

#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    Section(Section),
    DelimitedComment(String),
    DelimitedExample(String),
    DelimitedListing(String),
    DelimitedLiteral(String),
    DelimitedOpen(String),
    DelimitedSidebar(String),
    DelimitedTable(String),
    DelimitedPass(String),
    DelimitedQuote(String),
    Paragraph(String),
}

type SectionLevel = u8;

#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    pub title: String,
    pub level: SectionLevel,
    pub content: Vec<Block>,
    pub location: Location,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Location {
    pub start: Position,
    pub end: Position,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Position {
    pub line: usize,
    pub column: usize,
}

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
