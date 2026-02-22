//! The data models for the `AsciiDoc` document.
use std::{fmt::Display, str::FromStr, string::ToString};

use serde::{
    Serialize,
    ser::{SerializeMap, Serializer},
};

mod admonition;
mod anchor;
mod attributes;
mod attribution;
mod inlines;
mod lists;
mod location;
mod media;
mod metadata;
mod section;
pub(crate) mod substitution;
mod tables;
mod title;

pub use admonition::{Admonition, AdmonitionVariant};
pub use anchor::{Anchor, TocEntry, UNNUMBERED_SECTION_STYLES};
pub use attributes::{
    AttributeName, AttributeValue, DocumentAttributes, ElementAttributes, MAX_SECTION_LEVELS,
    MAX_TOC_LEVELS,
};
pub use attribution::{Attribution, CiteTitle};
pub use inlines::*;
pub use lists::{
    CalloutList, CalloutListItem, DescriptionList, DescriptionListItem, ListItem,
    ListItemCheckedStatus, ListLevel, OrderedList, UnorderedList,
};
pub use location::*;
pub use media::{Audio, Image, Source, SourceUrl, Video};
pub use metadata::{BlockMetadata, Role};
pub use section::*;
pub use substitution::*;
pub use tables::{
    ColumnFormat, ColumnStyle, ColumnWidth, HorizontalAlignment, Table, TableColumn, TableRow,
    VerticalAlignment,
};
pub use title::{Subtitle, Title};

/// A `Document` represents the root of an `AsciiDoc` document.
#[derive(Default, Debug, PartialEq)]
#[non_exhaustive]
pub struct Document {
    pub(crate) name: String,
    pub(crate) r#type: String,
    pub header: Option<Header>,
    pub attributes: DocumentAttributes,
    pub blocks: Vec<Block>,
    pub footnotes: Vec<Footnote>,
    pub toc_entries: Vec<TocEntry>,
    pub location: Location,
}

/// A `Header` represents the header of a document.
///
/// The header contains the title, subtitle, authors, and optional metadata
/// (such as ID and roles) that can be applied to the document title.
#[derive(Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct Header {
    #[serde(skip_serializing_if = "BlockMetadata::is_default")]
    pub metadata: BlockMetadata,
    #[serde(skip_serializing_if = "Title::is_empty")]
    pub title: Title,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<Subtitle>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub authors: Vec<Author>,
    pub location: Location,
}

/// An `Author` represents the author of a document.
#[derive(Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct Author {
    #[serde(rename = "firstname")]
    pub first_name: String,
    #[serde(skip_serializing_if = "Option::is_none", rename = "middlename")]
    pub middle_name: Option<String>,
    #[serde(rename = "lastname")]
    pub last_name: String,
    pub initials: String,
    #[serde(skip_serializing_if = "Option::is_none", rename = "address")]
    pub email: Option<String>,
}

impl Header {
    /// Create a new header with the given title and location.
    #[must_use]
    pub fn new(title: Title, location: Location) -> Self {
        Self {
            metadata: BlockMetadata::default(),
            title,
            subtitle: None,
            authors: Vec::new(),
            location,
        }
    }

    /// Set the metadata.
    #[must_use]
    pub fn with_metadata(mut self, metadata: BlockMetadata) -> Self {
        self.metadata = metadata;
        self
    }

    /// Set the subtitle.
    #[must_use]
    pub fn with_subtitle(mut self, subtitle: Subtitle) -> Self {
        self.subtitle = Some(subtitle);
        self
    }

    /// Set the authors.
    #[must_use]
    pub fn with_authors(mut self, authors: Vec<Author>) -> Self {
        self.authors = authors;
        self
    }
}

impl Author {
    /// Create a new author with the given names and initials.
    #[must_use]
    pub fn new(first_name: &str, middle_name: Option<&str>, last_name: Option<&str>) -> Self {
        let initials = Self::generate_initials(first_name, middle_name, last_name);
        let last_name = last_name.unwrap_or_default().to_string();
        Self {
            first_name: first_name.to_string(),
            middle_name: middle_name.map(ToString::to_string),
            last_name,
            initials,
            email: None,
        }
    }

    /// Set the email address.
    #[must_use]
    pub fn with_email(mut self, email: String) -> Self {
        self.email = Some(email);
        self
    }

    /// Generate initials from first, optional middle, and last name parts
    fn generate_initials(first: &str, middle: Option<&str>, last: Option<&str>) -> String {
        let first_initial = first.chars().next().unwrap_or_default().to_string();
        let middle_initial = middle
            .map(|m| m.chars().next().unwrap_or_default().to_string())
            .unwrap_or_default();
        let last_initial = last
            .map(|m| m.chars().next().unwrap_or_default().to_string())
            .unwrap_or_default();
        first_initial + &middle_initial + &last_initial
    }
}

/// A single-line comment in a document.
///
/// Line comments begin with `//` and continue to end of line.
/// They act as block boundaries but produce no output.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Comment {
    pub content: String,
    pub location: Location,
}

/// A `Block` represents a block in a document.
///
/// A block is a structural element in a document that can contain other blocks.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Block {
    TableOfContents(TableOfContents),
    // TODO(nlopes): we shouldn't have an admonition type here, instead it should be
    // picked up from the style attribute from the block metadata.
    //
    // The main one that would need changing is the Paragraph and the Delimited Example
    // blocks, where we currently use this but don't need to.
    Admonition(Admonition),
    DiscreteHeader(DiscreteHeader),
    DocumentAttribute(DocumentAttribute),
    ThematicBreak(ThematicBreak),
    PageBreak(PageBreak),
    UnorderedList(UnorderedList),
    OrderedList(OrderedList),
    CalloutList(CalloutList),
    DescriptionList(DescriptionList),
    Section(Section),
    DelimitedBlock(DelimitedBlock),
    Paragraph(Paragraph),
    Image(Image),
    Audio(Audio),
    Video(Video),
    Comment(Comment),
}

impl Locateable for Block {
    fn location(&self) -> &Location {
        match self {
            Block::Section(s) => &s.location,
            Block::Paragraph(p) => &p.location,
            Block::UnorderedList(l) => &l.location,
            Block::OrderedList(l) => &l.location,
            Block::DescriptionList(l) => &l.location,
            Block::CalloutList(l) => &l.location,
            Block::DelimitedBlock(d) => &d.location,
            Block::Admonition(a) => &a.location,
            Block::TableOfContents(t) => &t.location,
            Block::DiscreteHeader(h) => &h.location,
            Block::DocumentAttribute(a) => &a.location,
            Block::ThematicBreak(tb) => &tb.location,
            Block::PageBreak(pb) => &pb.location,
            Block::Image(i) => &i.location,
            Block::Audio(a) => &a.location,
            Block::Video(v) => &v.location,
            Block::Comment(c) => &c.location,
        }
    }
}

/// A `DocumentAttribute` represents a document attribute in a document.
///
/// A document attribute is a key-value pair that can be used to set metadata in a
/// document.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct DocumentAttribute {
    pub name: AttributeName,
    pub value: AttributeValue,
    pub location: Location,
}

impl Serialize for DocumentAttribute {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(None)?;
        state.serialize_entry("name", &self.name)?;
        state.serialize_entry("type", "attribute")?;
        state.serialize_entry("value", &self.value)?;
        state.serialize_entry("location", &self.location)?;
        state.end()
    }
}

/// A `DiscreteHeader` represents a discrete header in a document.
///
/// Discrete headings are useful for making headings inside of other blocks, like a
/// sidebar.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct DiscreteHeader {
    pub metadata: BlockMetadata,
    pub title: Title,
    pub level: u8,
    pub location: Location,
}

/// A `ThematicBreak` represents a thematic break in a document.
#[derive(Clone, Default, Debug, PartialEq)]
#[non_exhaustive]
pub struct ThematicBreak {
    pub anchors: Vec<Anchor>,
    pub title: Title,
    pub location: Location,
}

impl Serialize for ThematicBreak {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(None)?;
        state.serialize_entry("name", "break")?;
        state.serialize_entry("type", "block")?;
        state.serialize_entry("variant", "thematic")?;
        if !self.anchors.is_empty() {
            state.serialize_entry("anchors", &self.anchors)?;
        }
        if !self.title.is_empty() {
            state.serialize_entry("title", &self.title)?;
        }
        state.serialize_entry("location", &self.location)?;
        state.end()
    }
}

/// A `PageBreak` represents a page break in a document.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct PageBreak {
    pub title: Title,
    pub metadata: BlockMetadata,
    pub location: Location,
}

impl Serialize for PageBreak {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(None)?;
        state.serialize_entry("name", "break")?;
        state.serialize_entry("type", "block")?;
        state.serialize_entry("variant", "page")?;
        if !self.title.is_empty() {
            state.serialize_entry("title", &self.title)?;
        }
        if !self.metadata.is_default() {
            state.serialize_entry("metadata", &self.metadata)?;
        }
        state.serialize_entry("location", &self.location)?;
        state.end()
    }
}

impl Serialize for Comment {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(None)?;
        state.serialize_entry("name", "comment")?;
        state.serialize_entry("type", "block")?;
        if !self.content.is_empty() {
            state.serialize_entry("content", &self.content)?;
        }
        state.serialize_entry("location", &self.location)?;
        state.end()
    }
}

/// A `TableOfContents` represents a table of contents block.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct TableOfContents {
    pub metadata: BlockMetadata,
    pub location: Location,
}

impl Serialize for TableOfContents {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(None)?;
        state.serialize_entry("name", "toc")?;
        state.serialize_entry("type", "block")?;
        if !self.metadata.is_default() {
            state.serialize_entry("metadata", &self.metadata)?;
        }
        state.serialize_entry("location", &self.location)?;
        state.end()
    }
}

/// A `Paragraph` represents a paragraph in a document.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Paragraph {
    pub metadata: BlockMetadata,
    pub title: Title,
    pub content: Vec<InlineNode>,
    pub location: Location,
}

impl Paragraph {
    /// Create a new paragraph with the given content and location.
    #[must_use]
    pub fn new(content: Vec<InlineNode>, location: Location) -> Self {
        Self {
            metadata: BlockMetadata::default(),
            title: Title::default(),
            content,
            location,
        }
    }

    /// Set the metadata.
    #[must_use]
    pub fn with_metadata(mut self, metadata: BlockMetadata) -> Self {
        self.metadata = metadata;
        self
    }

    /// Set the title.
    #[must_use]
    pub fn with_title(mut self, title: Title) -> Self {
        self.title = title;
        self
    }
}

/// A `DelimitedBlock` represents a delimited block in a document.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct DelimitedBlock {
    pub metadata: BlockMetadata,
    pub inner: DelimitedBlockType,
    pub delimiter: String,
    pub title: Title,
    pub location: Location,
}

impl DelimitedBlock {
    /// Create a new delimited block.
    #[must_use]
    pub fn new(inner: DelimitedBlockType, delimiter: String, location: Location) -> Self {
        Self {
            metadata: BlockMetadata::default(),
            inner,
            delimiter,
            title: Title::default(),
            location,
        }
    }

    /// Set the metadata.
    #[must_use]
    pub fn with_metadata(mut self, metadata: BlockMetadata) -> Self {
        self.metadata = metadata;
        self
    }

    /// Set the title.
    #[must_use]
    pub fn with_title(mut self, title: Title) -> Self {
        self.title = title;
        self
    }
}

/// Notation type for mathematical expressions.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum StemNotation {
    Latexmath,
    Asciimath,
}

impl Display for StemNotation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StemNotation::Latexmath => write!(f, "latexmath"),
            StemNotation::Asciimath => write!(f, "asciimath"),
        }
    }
}

impl FromStr for StemNotation {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "latexmath" => Ok(Self::Latexmath),
            "asciimath" => Ok(Self::Asciimath),
            _ => Err(format!("unknown stem notation: {s}")),
        }
    }
}

/// Content of a stem block with math notation.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct StemContent {
    pub content: String,
    pub notation: StemNotation,
}

impl StemContent {
    /// Create a new stem content with the given content and notation.
    #[must_use]
    pub fn new(content: String, notation: StemNotation) -> Self {
        Self { content, notation }
    }
}

/// The inner content type of a delimited block.
///
/// Each variant wraps the content appropriate for that block type:
/// - **Verbatim content** (`Vec<InlineNode>`): `DelimitedListing`, `DelimitedLiteral`,
///   `DelimitedPass`, `DelimitedVerse`, `DelimitedComment` - preserves whitespace/formatting
/// - **Compound content** (`Vec<Block>`): `DelimitedExample`, `DelimitedOpen`,
///   `DelimitedSidebar`, `DelimitedQuote` - can contain nested blocks
/// - **Structured content**: `DelimitedTable(Table)`, `DelimitedStem(StemContent)`
///
/// # Accessing Content
///
/// Use pattern matching to extract the inner content:
///
/// ```
/// # use acdc_parser::{DelimitedBlockType, Block, InlineNode};
/// fn process_block(block_type: &DelimitedBlockType) {
///     match block_type {
///         DelimitedBlockType::DelimitedListing(inlines) => {
///             // Handle listing content (source code, etc.)
///         }
///         DelimitedBlockType::DelimitedExample(blocks) => {
///             // Handle example with nested blocks
///         }
///         DelimitedBlockType::DelimitedTable(table) => {
///             // Access table.rows, table.header, etc.
///         }
///         // ... other variants
///         _ => {}
///     }
/// }
/// ```
///
/// # Note on Variant Names
///
/// Variants are prefixed with `Delimited` to disambiguate from potential future
/// non-delimited block types and to make pattern matching more explicit.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum DelimitedBlockType {
    /// Comment block content (not rendered in output).
    DelimitedComment(Vec<InlineNode>),
    /// Example block - can contain nested blocks, admonitions, etc.
    DelimitedExample(Vec<Block>),
    /// Listing block - typically source code with syntax highlighting.
    DelimitedListing(Vec<InlineNode>),
    /// Literal block - preformatted text rendered verbatim.
    DelimitedLiteral(Vec<InlineNode>),
    /// Open block - generic container for nested blocks.
    DelimitedOpen(Vec<Block>),
    /// Sidebar block - supplementary content in a styled container.
    DelimitedSidebar(Vec<Block>),
    /// Table block - structured tabular data.
    DelimitedTable(Table),
    /// Passthrough block - content passed directly to output without processing.
    DelimitedPass(Vec<InlineNode>),
    /// Quote block - blockquote with optional attribution.
    DelimitedQuote(Vec<Block>),
    /// Verse block - poetry/lyrics preserving line breaks.
    DelimitedVerse(Vec<InlineNode>),
    /// STEM (math) block - LaTeX or `AsciiMath` notation.
    DelimitedStem(StemContent),
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
            DelimitedBlockType::DelimitedStem(_) => "stem",
        }
    }
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
            // We serialize the attributes even if they're empty because that's what the
            // TCK expects (odd but true)
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
        if !self.metadata.is_default() {
            state.serialize_entry("metadata", &self.metadata)?;
        }

        match &self.inner {
            DelimitedBlockType::DelimitedStem(stem) => {
                state.serialize_entry("content", &stem.content)?;
                state.serialize_entry("notation", &stem.notation)?;
            }
            DelimitedBlockType::DelimitedListing(inner)
            | DelimitedBlockType::DelimitedLiteral(inner)
            | DelimitedBlockType::DelimitedPass(inner)
            | DelimitedBlockType::DelimitedVerse(inner) => {
                state.serialize_entry("inlines", &inner)?;
            }
            DelimitedBlockType::DelimitedTable(inner) => {
                state.serialize_entry("content", &inner)?;
            }
            inner @ (DelimitedBlockType::DelimitedComment(_)
            | DelimitedBlockType::DelimitedExample(_)
            | DelimitedBlockType::DelimitedOpen(_)
            | DelimitedBlockType::DelimitedQuote(_)
            | DelimitedBlockType::DelimitedSidebar(_)) => {
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

impl Serialize for DiscreteHeader {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(None)?;
        state.serialize_entry("name", "heading")?;
        state.serialize_entry("type", "block")?;
        if !self.title.is_empty() {
            state.serialize_entry("title", &self.title)?;
        }
        state.serialize_entry("level", &self.level)?;
        if !self.metadata.is_default() {
            state.serialize_entry("metadata", &self.metadata)?;
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
