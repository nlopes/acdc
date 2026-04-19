//! The data models for the `AsciiDoc` document.
use std::{fmt::Display, str::FromStr, string::ToString};

use bumpalo::Bump;
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
    MAX_TOC_LEVELS, strip_quotes,
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
pub struct Document<'a> {
    pub header: Option<Header<'a>>,
    pub attributes: DocumentAttributes<'a>,
    pub blocks: Vec<Block<'a>>,
    pub footnotes: Vec<Footnote<'a>>,
    pub toc_entries: Vec<TocEntry<'a>>,
    pub location: Location,
}

/// A `Header` represents the header of a document.
///
/// The header contains the title, subtitle, authors, and optional metadata
/// (such as ID and roles) that can be applied to the document title.
#[derive(Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct Header<'a> {
    #[serde(skip_serializing_if = "BlockMetadata::is_default")]
    pub metadata: BlockMetadata<'a>,
    #[serde(skip_serializing_if = "Title::is_empty")]
    pub title: Title<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<Subtitle<'a>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub authors: Vec<Author<'a>>,
    pub location: Location,
}

/// An `Author` represents the author of a document.
#[derive(Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct Author<'a> {
    #[serde(rename = "firstname")]
    pub first_name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none", rename = "middlename")]
    pub middle_name: Option<&'a str>,
    #[serde(rename = "lastname")]
    pub last_name: &'a str,
    pub initials: &'a str,
    #[serde(skip_serializing_if = "Option::is_none", rename = "address")]
    pub email: Option<&'a str>,
}

impl<'a> Header<'a> {
    /// Create a new header with the given title and location.
    #[must_use]
    pub fn new(title: Title<'a>, location: Location) -> Self {
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
    pub fn with_metadata(mut self, metadata: BlockMetadata<'a>) -> Self {
        self.metadata = metadata;
        self
    }

    /// Set the subtitle.
    #[must_use]
    pub fn with_subtitle(mut self, subtitle: Subtitle<'a>) -> Self {
        self.subtitle = Some(subtitle);
        self
    }

    /// Set the authors.
    #[must_use]
    pub fn with_authors(mut self, authors: Vec<Author<'a>>) -> Self {
        self.authors = authors;
        self
    }
}

impl<'a> Author<'a> {
    /// Assemble an author from already-prepared name parts. No normalization
    /// or allocation happens — callers (tests, external consumers) are
    /// responsible for providing the display-ready strings they want.
    #[must_use]
    pub fn from_parts(
        first_name: &'a str,
        middle_name: Option<&'a str>,
        last_name: &'a str,
        initials: &'a str,
    ) -> Self {
        Self {
            first_name,
            middle_name,
            last_name,
            initials,
            email: None,
        }
    }

    /// Create a new author with the given names. Arena-allocates any
    /// underscore-normalized strings and computed initials.
    #[must_use]
    pub(crate) fn new(
        arena: &'a Bump,
        first_name: &'a str,
        middle_name: Option<&'a str>,
        last_name: Option<&'a str>,
    ) -> Self {
        let first_processed = first_name.replace('_', " ");
        let middle_processed = middle_name.map(|m| m.replace('_', " "));
        let last_processed = last_name.map(|l| l.replace('_', " "));

        let initials = Self::generate_initials(
            &first_processed,
            middle_processed.as_deref(),
            last_processed.as_deref(),
        );

        Self {
            first_name: if first_processed == first_name {
                first_name
            } else {
                arena.alloc_str(&first_processed)
            },
            middle_name: middle_name
                .zip(middle_processed.as_ref())
                .map(|(orig, proc)| {
                    if proc == orig {
                        orig
                    } else {
                        &*arena.alloc_str(proc)
                    }
                }),
            last_name: last_name
                .zip(last_processed.as_ref())
                .map_or("", |(orig, proc)| {
                    if proc == orig {
                        orig
                    } else {
                        arena.alloc_str(proc)
                    }
                }),
            initials: arena.alloc_str(&initials),
            email: None,
        }
    }

    /// Set the email address.
    #[must_use]
    pub fn with_email(mut self, email: &'a str) -> Self {
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
pub struct Comment<'a> {
    pub content: &'a str,
    pub location: Location,
}

/// A `Block` represents a block in a document.
///
/// A block is a structural element in a document that can contain other blocks.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Block<'a> {
    TableOfContents(TableOfContents<'a>),
    // TODO(nlopes): we shouldn't have an admonition type here, instead it should be
    // picked up from the style attribute from the block metadata.
    //
    // The main one that would need changing is the Paragraph and the Delimited Example
    // blocks, where we currently use this but don't need to.
    Admonition(Admonition<'a>),
    DiscreteHeader(DiscreteHeader<'a>),
    DocumentAttribute(DocumentAttribute<'a>),
    ThematicBreak(ThematicBreak<'a>),
    PageBreak(PageBreak<'a>),
    UnorderedList(UnorderedList<'a>),
    OrderedList(OrderedList<'a>),
    CalloutList(CalloutList<'a>),
    DescriptionList(DescriptionList<'a>),
    Section(Section<'a>),
    DelimitedBlock(DelimitedBlock<'a>),
    Paragraph(Paragraph<'a>),
    Image(Image<'a>),
    Audio(Audio<'a>),
    Video(Video<'a>),
    Comment(Comment<'a>),
}

impl Locateable for Block<'_> {
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
pub struct DocumentAttribute<'a> {
    pub name: AttributeName<'a>,
    pub value: AttributeValue<'a>,
    pub location: Location,
}

impl Serialize for DocumentAttribute<'_> {
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
pub struct DiscreteHeader<'a> {
    pub metadata: BlockMetadata<'a>,
    pub title: Title<'a>,
    pub level: u8,
    pub location: Location,
}

/// A `ThematicBreak` represents a thematic break in a document.
#[derive(Clone, Default, Debug, PartialEq)]
#[non_exhaustive]
pub struct ThematicBreak<'a> {
    pub anchors: Vec<Anchor<'a>>,
    pub title: Title<'a>,
    pub location: Location,
}

impl Serialize for ThematicBreak<'_> {
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
pub struct PageBreak<'a> {
    pub title: Title<'a>,
    pub metadata: BlockMetadata<'a>,
    pub location: Location,
}

impl Serialize for PageBreak<'_> {
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

impl Serialize for Comment<'_> {
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
pub struct TableOfContents<'a> {
    pub metadata: BlockMetadata<'a>,
    pub location: Location,
}

impl Serialize for TableOfContents<'_> {
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
pub struct Paragraph<'a> {
    pub metadata: BlockMetadata<'a>,
    pub title: Title<'a>,
    pub content: Vec<InlineNode<'a>>,
    pub location: Location,
}

impl<'a> Paragraph<'a> {
    /// Create a new paragraph with the given content and location.
    #[must_use]
    pub fn new(content: Vec<InlineNode<'a>>, location: Location) -> Self {
        Self {
            metadata: BlockMetadata::default(),
            title: Title::default(),
            content,
            location,
        }
    }

    /// Set the metadata.
    #[must_use]
    pub fn with_metadata(mut self, metadata: BlockMetadata<'a>) -> Self {
        self.metadata = metadata;
        self
    }

    /// Set the title.
    #[must_use]
    pub fn with_title(mut self, title: Title<'a>) -> Self {
        self.title = title;
        self
    }
}

/// A `DelimitedBlock` represents a delimited block in a document.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct DelimitedBlock<'a> {
    pub metadata: BlockMetadata<'a>,
    pub inner: DelimitedBlockType<'a>,
    pub delimiter: &'a str,
    pub title: Title<'a>,
    pub location: Location,
    pub open_delimiter_location: Option<Location>,
    pub close_delimiter_location: Option<Location>,
}

impl<'a> DelimitedBlock<'a> {
    /// Create a new delimited block.
    #[must_use]
    pub fn new(inner: DelimitedBlockType<'a>, delimiter: &'a str, location: Location) -> Self {
        Self {
            metadata: BlockMetadata::default(),
            inner,
            delimiter,
            title: Title::default(),
            location,
            open_delimiter_location: None,
            close_delimiter_location: None,
        }
    }

    /// Set the metadata.
    #[must_use]
    pub fn with_metadata(mut self, metadata: BlockMetadata<'a>) -> Self {
        self.metadata = metadata;
        self
    }

    /// Set the title.
    #[must_use]
    pub fn with_title(mut self, title: Title<'a>) -> Self {
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
pub struct StemContent<'a> {
    pub content: &'a str,
    pub notation: StemNotation,
}

impl<'a> StemContent<'a> {
    /// Create a new stem content with the given content and notation.
    #[must_use]
    pub fn new(content: &'a str, notation: StemNotation) -> Self {
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
pub enum DelimitedBlockType<'a> {
    /// Comment block content (not rendered in output).
    DelimitedComment(Vec<InlineNode<'a>>),
    /// Example block - can contain nested blocks, admonitions, etc.
    DelimitedExample(Vec<Block<'a>>),
    /// Listing block - typically source code with syntax highlighting.
    DelimitedListing(Vec<InlineNode<'a>>),
    /// Literal block - preformatted text rendered verbatim.
    DelimitedLiteral(Vec<InlineNode<'a>>),
    /// Open block - generic container for nested blocks.
    DelimitedOpen(Vec<Block<'a>>),
    /// Sidebar block - supplementary content in a styled container.
    DelimitedSidebar(Vec<Block<'a>>),
    /// Table block - structured tabular data.
    DelimitedTable(Table<'a>),
    /// Passthrough block - content passed directly to output without processing.
    DelimitedPass(Vec<InlineNode<'a>>),
    /// Quote block - blockquote with optional attribution.
    DelimitedQuote(Vec<Block<'a>>),
    /// Verse block - poetry/lyrics preserving line breaks.
    DelimitedVerse(Vec<InlineNode<'a>>),
    /// STEM (math) block - LaTeX or `AsciiMath` notation.
    DelimitedStem(StemContent<'a>),
}

impl DelimitedBlockType<'_> {
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

impl Serialize for Document<'_> {
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

impl Serialize for DelimitedBlock<'_> {
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

impl Serialize for DiscreteHeader<'_> {
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

impl Serialize for Paragraph<'_> {
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
