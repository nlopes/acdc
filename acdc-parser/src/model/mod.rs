//! The data models for the `AsciiDoc` document.
use std::{fmt::Display, str::FromStr};

use serde::{
    Deserialize, Serialize,
    de::{self, Deserializer, MapAccess, Visitor},
    ser::{SerializeMap, Serializer},
};

use crate::{Error, SourceLocation};

mod attributes;
mod inlines;
mod location;
mod section;
mod substitution;

pub use attributes::{AttributeName, AttributeValue, DocumentAttributes, ElementAttributes};
pub use inlines::*;
pub use location::*;
pub use section::*;
pub use substitution::*;

/// A `Document` represents the root of an `AsciiDoc` document.
#[derive(Default, Debug, PartialEq, Deserialize)]
pub struct Document {
    pub(crate) name: String,
    pub(crate) r#type: String,
    #[serde(default)]
    pub header: Option<Header>,
    #[serde(default, skip_serializing_if = "DocumentAttributes::is_empty")]
    pub attributes: DocumentAttributes,
    #[serde(default)]
    pub blocks: Vec<Block>,
    #[serde(skip)]
    pub footnotes: Vec<Footnote>,
    #[serde(skip)]
    pub toc_entries: Vec<TocEntry>,
    pub location: Location,
}

type Subtitle = Vec<InlineNode>;

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

/// A `BlockMetadata` represents the metadata of a block in a document.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BlockMetadata {
    #[serde(default, skip_serializing_if = "ElementAttributes::is_empty")]
    pub attributes: ElementAttributes,
    #[serde(default, skip_serializing)]
    pub positional_attributes: Vec<String>,
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
    pub fn move_positional_attributes_to_attributes(&mut self) {
        for positional_attribute in self.positional_attributes.drain(..) {
            self.attributes
                .insert(positional_attribute, AttributeValue::None);
        }
    }
    pub fn set_attributes(&mut self, attributes: ElementAttributes) {
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
            && self.positional_attributes.is_empty()
    }

    #[tracing::instrument(level = "debug")]
    pub fn merge(&mut self, other: &BlockMetadata) {
        self.attributes.merge(other.attributes.clone());
        self.positional_attributes
            .extend(other.positional_attributes.clone());
        self.roles.extend(other.roles.clone());
        self.options.extend(other.options.clone());
        if self.style.is_none() {
            self.style.clone_from(&other.style);
        }
        if self.id.is_none() {
            self.id.clone_from(&other.id);
        }
        self.anchors.extend(other.anchors.clone());
    }
}

/// A `TocEntry` represents a table of contents entry.
///
/// This is collected during parsing from Section.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TocEntry {
    /// Unique identifier for this section (used for anchor links)
    pub id: String,
    /// Title of the section
    pub title: Vec<InlineNode>,
    /// Section level (1 for top-level, 2 for subsection, etc.)
    pub level: u8,
    /// Optional cross-reference label (from `[[id,xreflabel]]` syntax)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub xreflabel: Option<String>,
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
        }
    }
}

/// A `DocumentAttribute` represents a document attribute in a document.
///
/// A document attribute is a key-value pair that can be used to set metadata in a
/// document.
#[derive(Clone, Debug, PartialEq)]
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
pub struct DiscreteHeader {
    pub metadata: BlockMetadata,
    //#[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub title: Vec<InlineNode>,
    pub level: u8,
    pub location: Location,
}

/// A `ThematicBreak` represents a thematic break in a document.
#[derive(Clone, Default, Debug, PartialEq)]
pub struct ThematicBreak {
    pub anchors: Vec<Anchor>,
    pub title: Vec<InlineNode>,
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
pub struct PageBreak {
    pub title: Vec<InlineNode>,
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

/// An `Audio` represents an audio block in a document.
#[derive(Clone, Debug, PartialEq)]
pub struct Audio {
    pub title: Vec<InlineNode>,
    pub source: Source,
    pub metadata: BlockMetadata,
    pub location: Location,
}

/// A `Video` represents a video block in a document.
#[derive(Clone, Debug, PartialEq)]
pub struct Video {
    pub title: Vec<InlineNode>,
    pub sources: Vec<Source>,
    pub metadata: BlockMetadata,
    pub location: Location,
}

/// An `Image` represents an image block in a document.
#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    pub title: Vec<InlineNode>,
    pub source: Source,
    pub metadata: BlockMetadata,
    pub location: Location,
}

/// A `TableOfContents` represents a table of contents block.
#[derive(Clone, Debug, PartialEq)]
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

/// A `Source` represents the source of content (images, audio, video, etc.).
///
/// This type distinguishes between filesystem paths, URLs, and simple names (like icon names).
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    /// A filesystem path
    Path(std::path::PathBuf),
    /// A URL
    Url(url::Url),
    /// A simple name (used for example in menu macros or icon names)
    Name(String),
}

impl Source {
    /// Get the filename from the source.
    ///
    /// For paths, this returns the file name component. For URLs, it returns the last path
    /// segment. For names, it returns the name itself.
    #[must_use]
    pub fn get_filename(&self) -> Option<&str> {
        match self {
            Source::Path(path) => path.file_name().and_then(|os_str| os_str.to_str()),
            Source::Url(url) => url
                .path_segments()
                .and_then(std::iter::Iterator::last)
                .filter(|s| !s.is_empty()),
            Source::Name(name) => Some(name.as_str()),
        }
    }
}

impl FromStr for Source {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        // Try to parse as URL first
        if value.starts_with("http://")
            || value.starts_with("https://")
            || value.starts_with("ftp://")
            || value.starts_with("irc://")
            || value.starts_with("mailto:")
        {
            url::Url::parse(value).map(Source::Url).map_err(|e| {
                Error::Parse(
                    Box::new(SourceLocation {
                        file: None,
                        positioning: crate::Positioning::Position(Position::default()),
                    }),
                    format!("invalid URL: {e}"),
                )
            })
        } else if value.contains('/') || value.contains('\\') || value.contains('.') {
            // Contains path separators - treat as filesystem path or contains a dot which
            // might indicate a filename with extension
            Ok(Source::Path(std::path::PathBuf::from(value)))
        } else {
            // Contains special characters or spaces - treat as a name
            Ok(Source::Name(value.to_string()))
        }
    }
}

impl Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Source::Path(path) => write!(f, "{}", path.display()),
            Source::Url(url) => {
                // The url crate normalizes domain-only URLs by adding a trailing slash
                // (e.g., "https://example.com" -> "https://example.com/").
                // Strip it to match asciidoctor's output behavior.
                let url_str = url.as_str();
                if url.path() == "/" && !url_str.ends_with("://") {
                    write!(f, "{}", url_str.trim_end_matches('/'))
                } else {
                    write!(f, "{url}")
                }
            }
            Source::Name(name) => write!(f, "{name}"),
        }
    }
}

/// A `DescriptionList` represents a description list in a document.
#[derive(Clone, Debug, PartialEq)]
pub struct DescriptionList {
    pub title: Vec<InlineNode>,
    pub metadata: BlockMetadata,
    pub items: Vec<DescriptionListItem>,
    pub location: Location,
}

/// A `DescriptionListItem` represents a description list item in a document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DescriptionListItem {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub anchors: Vec<Anchor>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub term: Vec<InlineNode>,
    pub delimiter: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub principal_text: Vec<InlineNode>,
    pub description: Vec<Block>,
    pub location: Location,
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

/// A `CalloutList` represents a callout list in a document.
///
/// Callout lists are used to annotate code blocks with numbered references.
#[derive(Clone, Debug, PartialEq)]
pub struct CalloutList {
    pub title: Vec<InlineNode>,
    pub metadata: BlockMetadata,
    pub items: Vec<ListItem>,
    pub location: Location,
}

pub type ListLevel = u8;

/// A `ListItemCheckedStatus` represents the checked status of a list item.
#[derive(Clone, Debug, PartialEq)]
pub enum ListItemCheckedStatus {
    Checked,
    Unchecked,
}

/// A `ListItem` represents a list item in a document.
///
/// List items have principal text (inline content immediately after the marker) and
/// optionally attached blocks (via continuation or nesting). This matches Asciidoctor's
/// AST structure where principal text renders as bare `<p>` and attached blocks render
/// with their full wrapper divs.
#[derive(Clone, Debug, PartialEq)]
pub struct ListItem {
    pub level: ListLevel,
    pub marker: String,
    pub checked: Option<ListItemCheckedStatus>,
    /// Principal text - inline content that appears immediately after the list marker
    pub principal: Vec<InlineNode>,
    /// Attached blocks - blocks attached via continuation (+) or nesting
    pub blocks: Vec<Block>,
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
#[derive(Clone, Debug, PartialEq)]
pub struct Admonition {
    pub metadata: BlockMetadata,
    pub variant: AdmonitionVariant,
    pub blocks: Vec<Block>,
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

impl Display for AdmonitionVariant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AdmonitionVariant::Note => write!(f, "note"),
            AdmonitionVariant::Tip => write!(f, "tip"),
            AdmonitionVariant::Important => write!(f, "important"),
            AdmonitionVariant::Caution => write!(f, "caution"),
            AdmonitionVariant::Warning => write!(f, "warning"),
        }
    }
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
            _ => Err(Error::Parse(
                Box::new(SourceLocation {
                    file: None,
                    positioning: crate::Positioning::Position(Position::default()),
                }),
                format!("unknown admonition variant: {variant}"),
            )),
        }
    }
}

/// Notation type for mathematical expressions.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StemContent {
    pub content: String,
    pub notation: StemNotation,
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

/// Horizontal alignment for table cells
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HorizontalAlignment {
    #[default]
    Left,
    Center,
    Right,
}

/// Vertical alignment for table cells
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VerticalAlignment {
    #[default]
    Top,
    Middle,
    Bottom,
}

/// Column width specification
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColumnWidth {
    /// Proportional width (e.g., 1, 2, 3 - relative to other columns)
    Proportional(u32),
    /// Percentage width (e.g., 15%, 30%, 55%)
    Percentage(u32),
    /// Auto-width - content determines width (~)
    Auto,
}

impl Default for ColumnWidth {
    fn default() -> Self {
        ColumnWidth::Proportional(1)
    }
}

/// Column content style
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColumnStyle {
    /// `AsciiDoc` block content (a) - supports lists, blocks, macros
    #[serde(rename = "asciidoc")]
    AsciiDoc,
    /// Default paragraph-level markup (d)
    #[default]
    Default,
    /// Emphasis/italic (e)
    Emphasis,
    /// Header styling (h)
    Header,
    /// Literal block text (l)
    Literal,
    /// Monospace font (m)
    Monospace,
    /// Strong/bold (s)
    Strong,
}

/// Column format specification for table formatting
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ColumnFormat {
    #[serde(default, skip_serializing_if = "is_default_halign")]
    pub halign: HorizontalAlignment,
    #[serde(default, skip_serializing_if = "is_default_valign")]
    pub valign: VerticalAlignment,
    #[serde(default, skip_serializing_if = "is_default_width")]
    pub width: ColumnWidth,
    #[serde(default, skip_serializing_if = "is_default_style")]
    pub style: ColumnStyle,
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_default_halign(h: &HorizontalAlignment) -> bool {
    *h == HorizontalAlignment::default()
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_default_valign(v: &VerticalAlignment) -> bool {
    *v == VerticalAlignment::default()
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_default_width(w: &ColumnWidth) -> bool {
    *w == ColumnWidth::default()
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_default_style(s: &ColumnStyle) -> bool {
    *s == ColumnStyle::default()
}

fn are_all_columns_default(specs: &[ColumnFormat]) -> bool {
    specs.iter().all(|s| *s == ColumnFormat::default())
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Table {
    pub header: Option<TableRow>,
    pub footer: Option<TableRow>,
    pub rows: Vec<TableRow>,
    /// Column format specification for each column (alignment, width, style)
    /// Skipped if all columns have default format
    #[serde(default, skip_serializing_if = "are_all_columns_default")]
    pub columns: Vec<ColumnFormat>,
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

impl Serialize for Admonition {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(None)?;
        state.serialize_entry("name", "admonition")?;
        state.serialize_entry("type", "block")?;
        state.serialize_entry("variant", &self.variant)?;
        if !self.metadata.is_default() {
            state.serialize_entry("metadata", &self.metadata)?;
        }
        if !self.title.is_empty() {
            state.serialize_entry("title", &self.title)?;
        }

        if !self.blocks.is_empty() {
            state.serialize_entry("blocks", &self.blocks)?;
        }
        state.serialize_entry("location", &self.location)?;
        state.end()
    }
}

impl Serialize for Audio {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(None)?;
        state.serialize_entry("name", "audio")?;
        state.serialize_entry("type", "block")?;
        state.serialize_entry("form", "macro")?;
        if !self.metadata.is_default() {
            state.serialize_entry("metadata", &self.metadata)?;
        }
        if !self.title.is_empty() {
            state.serialize_entry("title", &self.title)?;
        }
        state.serialize_entry("source", &self.source)?;
        state.serialize_entry("location", &self.location)?;
        state.end()
    }
}

impl Serialize for Image {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(None)?;
        state.serialize_entry("name", "image")?;
        state.serialize_entry("type", "block")?;
        state.serialize_entry("form", "macro")?;
        if !self.metadata.is_default() {
            state.serialize_entry("metadata", &self.metadata)?;
        }
        if !self.title.is_empty() {
            state.serialize_entry("title", &self.title)?;
        }
        state.serialize_entry("source", &self.source)?;
        state.serialize_entry("location", &self.location)?;
        state.end()
    }
}

impl Serialize for Video {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(None)?;
        state.serialize_entry("name", "video")?;
        state.serialize_entry("type", "block")?;
        state.serialize_entry("form", "macro")?;
        if !self.metadata.is_default() {
            state.serialize_entry("metadata", &self.metadata)?;
        }
        if !self.title.is_empty() {
            state.serialize_entry("title", &self.title)?;
        }
        if !self.sources.is_empty() {
            state.serialize_entry("sources", &self.sources)?;
        }
        state.serialize_entry("location", &self.location)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for Audio {
    fn deserialize<D>(deserializer: D) -> Result<Audio, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "lowercase")]
        enum Field {
            Metadata,
            Title,
            Source,
            Location,
        }

        struct AudioVisitor;

        impl<'de> Visitor<'de> for AudioVisitor {
            type Value = Audio;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("struct Audio")
            }

            fn visit_map<V>(self, mut map: V) -> Result<Audio, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut metadata = None;
                let mut title = None;
                let mut source = None;
                let mut location = None;

                while let Some(key) = map.next_key()? {
                    match key {
                        Field::Metadata => {
                            metadata = Some(map.next_value()?);
                        }
                        Field::Title => {
                            title = Some(map.next_value()?);
                        }
                        Field::Source => {
                            source = Some(map.next_value()?);
                        }
                        Field::Location => {
                            location = Some(map.next_value()?);
                        }
                    }
                }

                Ok(Audio {
                    title: title.unwrap_or_default(),
                    source: source.ok_or_else(|| serde::de::Error::missing_field("source"))?,
                    metadata: metadata.unwrap_or_default(),
                    location: location
                        .ok_or_else(|| serde::de::Error::missing_field("location"))?,
                })
            }
        }

        deserializer.deserialize_struct(
            "Audio",
            &["metadata", "title", "source", "location"],
            AudioVisitor,
        )
    }
}

impl<'de> Deserialize<'de> for Image {
    fn deserialize<D>(deserializer: D) -> Result<Image, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "lowercase")]
        enum Field {
            Metadata,
            Title,
            Source,
            Location,
        }

        struct ImageVisitor;

        impl<'de> Visitor<'de> for ImageVisitor {
            type Value = Image;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("struct Image")
            }

            fn visit_map<V>(self, mut map: V) -> Result<Image, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut metadata = None;
                let mut title = None;
                let mut source = None;
                let mut location = None;

                while let Some(key) = map.next_key()? {
                    match key {
                        Field::Metadata => {
                            metadata = Some(map.next_value()?);
                        }
                        Field::Title => {
                            title = Some(map.next_value()?);
                        }
                        Field::Source => {
                            source = Some(map.next_value()?);
                        }
                        Field::Location => {
                            location = Some(map.next_value()?);
                        }
                    }
                }

                Ok(Image {
                    title: title.unwrap_or_default(),
                    source: source.ok_or_else(|| serde::de::Error::missing_field("source"))?,
                    metadata: metadata.unwrap_or_default(),
                    location: location
                        .ok_or_else(|| serde::de::Error::missing_field("location"))?,
                })
            }
        }

        deserializer.deserialize_struct(
            "Image",
            &["metadata", "title", "source", "location"],
            ImageVisitor,
        )
    }
}

// Video uses "sources" (plural)
impl<'de> Deserialize<'de> for Video {
    fn deserialize<D>(deserializer: D) -> Result<Video, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "lowercase")]
        enum Field {
            Metadata,
            Title,
            Sources,
            Location,
        }

        struct VideoVisitor;

        impl<'de> Visitor<'de> for VideoVisitor {
            type Value = Video;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("struct Video")
            }

            fn visit_map<V>(self, mut map: V) -> Result<Video, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut metadata = None;
                let mut title = None;
                let mut sources = None;
                let mut location = None;

                while let Some(key) = map.next_key()? {
                    match key {
                        Field::Metadata => metadata = Some(map.next_value()?),
                        Field::Title => title = Some(map.next_value()?),
                        Field::Sources => sources = Some(map.next_value()?),
                        Field::Location => location = Some(map.next_value()?),
                    }
                }

                Ok(Video {
                    title: title.unwrap_or_default(),
                    sources: sources.unwrap_or_default(),
                    metadata: metadata.unwrap_or_default(),
                    location: location
                        .ok_or_else(|| serde::de::Error::missing_field("location"))?,
                })
            }
        }

        deserializer.deserialize_struct(
            "Video",
            &["metadata", "title", "sources", "location"],
            VideoVisitor,
        )
    }
}

impl Serialize for Source {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(None)?;
        match self {
            Source::Path(path) => {
                state.serialize_entry("type", "path")?;
                state.serialize_entry("value", &path.display().to_string())?;
            }
            Source::Url(url) => {
                state.serialize_entry("type", "url")?;
                state.serialize_entry("value", url.as_str())?;
            }
            Source::Name(name) => {
                state.serialize_entry("type", "name")?;
                state.serialize_entry("value", name)?;
            }
        }
        state.end()
    }
}

impl<'de> Deserialize<'de> for Source {
    fn deserialize<D>(deserializer: D) -> Result<Source, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct SourceVisitor;

        impl<'de> Visitor<'de> for SourceVisitor {
            type Value = Source;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a Source object with type and value fields")
            }

            fn visit_map<V>(self, mut map: V) -> Result<Source, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut source_type: Option<String> = None;
                let mut value: Option<String> = None;

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "type" => {
                            if source_type.is_some() {
                                return Err(de::Error::duplicate_field("type"));
                            }
                            source_type = Some(map.next_value()?);
                        }
                        "value" => {
                            if value.is_some() {
                                return Err(de::Error::duplicate_field("value"));
                            }
                            value = Some(map.next_value()?);
                        }
                        _ => {
                            let _ = map.next_value::<de::IgnoredAny>()?;
                        }
                    }
                }

                let source_type = source_type.ok_or_else(|| de::Error::missing_field("type"))?;
                let value = value.ok_or_else(|| de::Error::missing_field("value"))?;

                match source_type.as_str() {
                    "path" => Ok(Source::Path(std::path::PathBuf::from(value))),
                    "url" => url::Url::parse(&value)
                        .map(Source::Url)
                        .map_err(|e| de::Error::custom(format!("invalid URL: {e}"))),
                    "name" => Ok(Source::Name(value)),
                    _ => Err(de::Error::custom(format!(
                        "unexpected source type: {source_type}"
                    ))),
                }
            }
        }

        deserializer.deserialize_map(SourceVisitor)
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

macro_rules! impl_list_serialize {
    ($type:ty, $variant:literal, with_marker) => {
        impl Serialize for $type {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                let mut state = serializer.serialize_map(None)?;
                state.serialize_entry("name", "list")?;
                state.serialize_entry("type", "block")?;
                state.serialize_entry("variant", $variant)?;
                state.serialize_entry("marker", &self.marker)?;
                if !self.title.is_empty() {
                    state.serialize_entry("title", &self.title)?;
                }
                if !self.metadata.is_default() {
                    state.serialize_entry("metadata", &self.metadata)?;
                }
                state.serialize_entry("items", &self.items)?;
                state.serialize_entry("location", &self.location)?;
                state.end()
            }
        }
    };
    ($type:ty, $variant:literal) => {
        impl Serialize for $type {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                let mut state = serializer.serialize_map(None)?;
                state.serialize_entry("name", "list")?;
                state.serialize_entry("type", "block")?;
                state.serialize_entry("variant", $variant)?;
                if !self.title.is_empty() {
                    state.serialize_entry("title", &self.title)?;
                }
                if !self.metadata.is_default() {
                    state.serialize_entry("metadata", &self.metadata)?;
                }
                state.serialize_entry("items", &self.items)?;
                state.serialize_entry("location", &self.location)?;
                state.end()
            }
        }
    };
}

impl_list_serialize!(UnorderedList, "unordered", with_marker);
impl_list_serialize!(OrderedList, "ordered", with_marker);
impl_list_serialize!(CalloutList, "callout");

impl Serialize for DescriptionList {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(None)?;
        state.serialize_entry("name", "dlist")?;
        state.serialize_entry("type", "block")?;
        if !self.title.is_empty() {
            state.serialize_entry("title", &self.title)?;
        }
        if !self.metadata.is_default() {
            state.serialize_entry("metadata", &self.metadata)?;
        }
        state.serialize_entry("items", &self.items)?;
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

// =============================================================================
// Block Deserialization Infrastructure
// =============================================================================

/// Raw field collector for Block deserialization.
/// Uses derived Deserialize to handle JSON field parsing, then dispatches to constructors.
#[derive(Default, Deserialize)]
#[serde(default)]
struct RawBlockFields {
    name: Option<String>,
    r#type: Option<String>,
    value: Option<String>,
    form: Option<String>,
    target: Option<String>,
    source: Option<Source>,
    sources: Option<Vec<Source>>,
    delimiter: Option<String>,
    reftext: Option<String>,
    id: Option<String>,
    title: Option<Vec<InlineNode>>,
    anchors: Option<Vec<Anchor>>,
    level: Option<SectionLevel>,
    metadata: Option<BlockMetadata>,
    variant: Option<String>,
    content: Option<serde_json::Value>,
    notation: Option<serde_json::Value>,
    blocks: Option<serde_json::Value>,
    inlines: Option<Vec<InlineNode>>,
    marker: Option<String>,
    items: Option<serde_json::Value>,
    location: Option<Location>,
}

/// Helper to parse `Vec<Block>` from `serde_json::Value`
fn parse_blocks<E: de::Error>(value: Option<serde_json::Value>) -> Result<Vec<Block>, E> {
    match value {
        Some(serde_json::Value::Array(arr)) => arr
            .into_iter()
            .map(|v| serde_json::from_value(v).map_err(E::custom))
            .collect(),
        Some(_) => Err(E::custom("blocks must be an array")),
        None => Ok(Vec::new()),
    }
}

/// Helper to require `Vec<Block>` from `serde_json::Value`
fn require_blocks<E: de::Error>(value: Option<serde_json::Value>) -> Result<Vec<Block>, E> {
    match value {
        Some(serde_json::Value::Array(arr)) => arr
            .into_iter()
            .map(|v| serde_json::from_value(v).map_err(E::custom))
            .collect(),
        Some(_) => Err(E::custom("blocks must be an array")),
        None => Err(E::missing_field("blocks")),
    }
}

/// Helper to parse `Vec<ListItem>` from `serde_json::Value`
fn parse_list_items<E: de::Error>(value: Option<serde_json::Value>) -> Result<Vec<ListItem>, E> {
    match value {
        Some(serde_json::Value::Array(arr)) => arr
            .into_iter()
            .map(|v| serde_json::from_value(v).map_err(E::custom))
            .collect(),
        Some(_) => Err(E::custom("items must be an array")),
        None => Err(E::missing_field("items")),
    }
}

/// Helper to parse `Vec<DescriptionListItem>` from `serde_json::Value`
fn parse_dlist_items<E: de::Error>(
    value: Option<serde_json::Value>,
) -> Result<Vec<DescriptionListItem>, E> {
    match value {
        Some(serde_json::Value::Array(arr)) => arr
            .into_iter()
            .map(|v| serde_json::from_value(v).map_err(E::custom))
            .collect(),
        Some(_) => Err(E::custom("items must be an array")),
        None => Err(E::missing_field("items")),
    }
}

// -----------------------------------------------------------------------------
// Per-variant Block constructors
// -----------------------------------------------------------------------------

fn construct_section<E: de::Error>(raw: RawBlockFields) -> Result<Block, E> {
    Ok(Block::Section(Section {
        metadata: raw.metadata.unwrap_or_default(),
        title: raw.title.unwrap_or_default(),
        level: raw.level.ok_or_else(|| E::missing_field("level"))?,
        content: parse_blocks(raw.blocks)?,
        location: raw.location.ok_or_else(|| E::missing_field("location"))?,
    }))
}

fn construct_paragraph<E: de::Error>(raw: RawBlockFields) -> Result<Block, E> {
    Ok(Block::Paragraph(Paragraph {
        metadata: raw.metadata.unwrap_or_default(),
        title: raw.title.unwrap_or_default(),
        content: raw.inlines.ok_or_else(|| E::missing_field("inlines"))?,
        location: raw.location.ok_or_else(|| E::missing_field("location"))?,
    }))
}

fn construct_image<E: de::Error>(raw: RawBlockFields) -> Result<Block, E> {
    let form = raw.form.ok_or_else(|| E::missing_field("form"))?;
    if form != "macro" {
        return Err(E::custom(format!("unexpected form: {form}")));
    }
    Ok(Block::Image(Image {
        title: raw.title.unwrap_or_default(),
        source: raw.source.ok_or_else(|| E::missing_field("source"))?,
        metadata: raw.metadata.unwrap_or_default(),
        location: raw.location.ok_or_else(|| E::missing_field("location"))?,
    }))
}

fn construct_audio<E: de::Error>(raw: RawBlockFields) -> Result<Block, E> {
    let form = raw.form.ok_or_else(|| E::missing_field("form"))?;
    if form != "macro" {
        return Err(E::custom(format!("unexpected form: {form}")));
    }
    Ok(Block::Audio(Audio {
        title: raw.title.unwrap_or_default(),
        source: raw.source.ok_or_else(|| E::missing_field("source"))?,
        metadata: raw.metadata.unwrap_or_default(),
        location: raw.location.ok_or_else(|| E::missing_field("location"))?,
    }))
}

fn construct_video<E: de::Error>(raw: RawBlockFields) -> Result<Block, E> {
    let sources = if let Some(sources_value) = raw.sources {
        sources_value
    } else {
        // Fallback to simplified format with target
        let form = raw.form.ok_or_else(|| E::missing_field("form"))?;
        if form != "macro" {
            return Err(E::custom(format!("unexpected form: {form}")));
        }
        let target = raw.target.ok_or_else(|| E::missing_field("target"))?;
        let source = Source::from_str(&target).map_err(E::custom)?;
        vec![source]
    };
    Ok(Block::Video(Video {
        title: raw.title.unwrap_or_default(),
        sources,
        metadata: raw.metadata.unwrap_or_default(),
        location: raw.location.ok_or_else(|| E::missing_field("location"))?,
    }))
}

fn construct_break<E: de::Error>(raw: RawBlockFields) -> Result<Block, E> {
    let variant = raw.variant.ok_or_else(|| E::missing_field("variant"))?;
    let location = raw.location.ok_or_else(|| E::missing_field("location"))?;
    match variant.as_str() {
        "page" => Ok(Block::PageBreak(PageBreak {
            title: raw.title.unwrap_or_default(),
            metadata: raw.metadata.unwrap_or_default(),
            location,
        })),
        "thematic" => Ok(Block::ThematicBreak(ThematicBreak {
            title: raw.title.unwrap_or_default(),
            anchors: raw.anchors.unwrap_or_default(),
            location,
        })),
        _ => Err(E::custom(format!("unexpected 'break' variant: {variant}"))),
    }
}

fn construct_heading<E: de::Error>(raw: RawBlockFields) -> Result<Block, E> {
    Ok(Block::DiscreteHeader(DiscreteHeader {
        title: raw.title.unwrap_or_default(),
        level: raw.level.ok_or_else(|| E::missing_field("level"))?,
        metadata: raw.metadata.unwrap_or_default(),
        location: raw.location.ok_or_else(|| E::missing_field("location"))?,
    }))
}

fn construct_toc<E: de::Error>(raw: RawBlockFields) -> Result<Block, E> {
    Ok(Block::TableOfContents(TableOfContents {
        metadata: raw.metadata.unwrap_or_default(),
        location: raw.location.ok_or_else(|| E::missing_field("location"))?,
    }))
}

fn construct_admonition<E: de::Error>(raw: RawBlockFields) -> Result<Block, E> {
    let variant = raw.variant.ok_or_else(|| E::missing_field("variant"))?;
    Ok(Block::Admonition(Admonition {
        metadata: raw.metadata.unwrap_or_default(),
        variant: AdmonitionVariant::from_str(&variant).map_err(E::custom)?,
        blocks: require_blocks(raw.blocks)?,
        title: raw.title.unwrap_or_default(),
        location: raw.location.ok_or_else(|| E::missing_field("location"))?,
    }))
}

fn construct_dlist<E: de::Error>(raw: RawBlockFields) -> Result<Block, E> {
    Ok(Block::DescriptionList(DescriptionList {
        title: raw.title.unwrap_or_default(),
        metadata: raw.metadata.unwrap_or_default(),
        items: parse_dlist_items(raw.items)?,
        location: raw.location.ok_or_else(|| E::missing_field("location"))?,
    }))
}

fn construct_list<E: de::Error>(raw: RawBlockFields) -> Result<Block, E> {
    let variant = raw.variant.ok_or_else(|| E::missing_field("variant"))?;
    let location = raw.location.ok_or_else(|| E::missing_field("location"))?;
    let title = raw.title.unwrap_or_default();
    let metadata = raw.metadata.unwrap_or_default();
    let items = parse_list_items(raw.items)?;

    match variant.as_str() {
        "unordered" => Ok(Block::UnorderedList(UnorderedList {
            title,
            metadata,
            marker: raw.marker.ok_or_else(|| E::missing_field("marker"))?,
            items,
            location,
        })),
        "ordered" => Ok(Block::OrderedList(OrderedList {
            title,
            metadata,
            marker: raw.marker.ok_or_else(|| E::missing_field("marker"))?,
            items,
            location,
        })),
        "callout" => Ok(Block::CalloutList(CalloutList {
            title,
            metadata,
            items,
            location,
        })),
        _ => Err(E::custom(format!("unexpected 'list' variant: {variant}"))),
    }
}

fn construct_delimited<E: de::Error>(name: &str, raw: RawBlockFields) -> Result<Block, E> {
    let form = raw.form.ok_or_else(|| E::missing_field("form"))?;
    if form != "delimited" {
        return Err(E::custom(format!("unexpected form: {form}")));
    }
    let delimiter = raw.delimiter.ok_or_else(|| E::missing_field("delimiter"))?;
    let location = raw.location.ok_or_else(|| E::missing_field("location"))?;
    let metadata = raw.metadata.unwrap_or_default();
    let title = raw.title.unwrap_or_default();

    let inner = match name {
        "example" => DelimitedBlockType::DelimitedExample(require_blocks(raw.blocks)?),
        "sidebar" => DelimitedBlockType::DelimitedSidebar(require_blocks(raw.blocks)?),
        "open" => DelimitedBlockType::DelimitedOpen(require_blocks(raw.blocks)?),
        "quote" => DelimitedBlockType::DelimitedQuote(require_blocks(raw.blocks)?),
        "verse" => DelimitedBlockType::DelimitedVerse(
            raw.inlines.ok_or_else(|| E::missing_field("inlines"))?,
        ),
        "listing" => DelimitedBlockType::DelimitedListing(
            raw.inlines.ok_or_else(|| E::missing_field("inlines"))?,
        ),
        "literal" => DelimitedBlockType::DelimitedLiteral(
            raw.inlines.ok_or_else(|| E::missing_field("inlines"))?,
        ),
        "pass" => DelimitedBlockType::DelimitedPass(
            raw.inlines.ok_or_else(|| E::missing_field("inlines"))?,
        ),
        "stem" => {
            let serde_json::Value::String(content) =
                raw.content.ok_or_else(|| E::missing_field("content"))?
            else {
                return Err(E::custom("content must be a string"));
            };
            let notation = match raw.notation {
                Some(serde_json::Value::String(n)) => {
                    StemNotation::from_str(&n).map_err(E::custom)?
                }
                Some(
                    serde_json::Value::Null
                    | serde_json::Value::Bool(_)
                    | serde_json::Value::Number(_)
                    | serde_json::Value::Array(_)
                    | serde_json::Value::Object(_),
                )
                | None => StemNotation::Latexmath,
            };
            DelimitedBlockType::DelimitedStem(StemContent { content, notation })
        }
        "table" => {
            let table =
                serde_json::from_value(raw.content.ok_or_else(|| E::missing_field("content"))?)
                    .map_err(|e| {
                        tracing::error!("content must be compatible with `Table` type: {e}");
                        E::custom("content must be compatible with `Table` type")
                    })?;
            DelimitedBlockType::DelimitedTable(table)
        }
        _ => return Err(E::custom(format!("unexpected delimited block: {name}"))),
    };

    Ok(Block::DelimitedBlock(DelimitedBlock {
        metadata,
        inner,
        delimiter,
        title,
        location,
    }))
}

fn construct_document_attribute<E: de::Error>(name: &str, raw: RawBlockFields) -> Result<Block, E> {
    let value = if let Some(value) = raw.value {
        if value.is_empty() {
            AttributeValue::None
        } else if value.eq_ignore_ascii_case("true") {
            AttributeValue::Bool(true)
        } else if value.eq_ignore_ascii_case("false") {
            AttributeValue::Bool(false)
        } else {
            AttributeValue::String(value)
        }
    } else {
        AttributeValue::None
    };
    Ok(Block::DocumentAttribute(DocumentAttribute {
        name: name.to_string(),
        value,
        location: raw.location.ok_or_else(|| E::missing_field("location"))?,
    }))
}

/// Dispatch to the appropriate Block constructor based on name/type
fn dispatch_block<E: de::Error>(raw: RawBlockFields) -> Result<Block, E> {
    // Take ownership of name/type for dispatch, avoiding borrow issues
    let name = raw.name.clone().ok_or_else(|| E::missing_field("name"))?;
    let ty = raw.r#type.clone().ok_or_else(|| E::missing_field("type"))?;

    match (name.as_str(), ty.as_str()) {
        ("section", "block") => construct_section(raw),
        ("paragraph", "block") => construct_paragraph(raw),
        ("image", "block") => construct_image(raw),
        ("audio", "block") => construct_audio(raw),
        ("video", "block") => construct_video(raw),
        ("break", "block") => construct_break(raw),
        ("heading", "block") => construct_heading(raw),
        ("toc", "block") => construct_toc(raw),
        ("admonition", "block") => construct_admonition(raw),
        ("dlist", "block") => construct_dlist(raw),
        ("list", "block") => construct_list(raw),
        // Delimited blocks
        (
            "example" | "sidebar" | "open" | "quote" | "verse" | "listing" | "literal" | "pass"
            | "stem" | "table",
            "block",
        ) => construct_delimited(&name, raw),
        // Document attribute (type != "block")
        (_, "attribute") => construct_document_attribute(&name, raw),
        _ => Err(E::custom(format!(
            "unexpected name/type combination: {name}/{ty}"
        ))),
    }
}

impl<'de> Deserialize<'de> for Block {
    fn deserialize<D>(deserializer: D) -> Result<Block, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Deserialize into RawBlockFields using derived Deserialize, then dispatch
        let raw: RawBlockFields = RawBlockFields::deserialize(deserializer)?;
        dispatch_block(raw)
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
        if let Some(checked) = &self.checked {
            state.serialize_entry("checked", checked)?;
        }
        // The TCK doesn't contain level information for list items, so we don't serialize
        // it.
        //
        // Uncomment the line below if level information is added in the future.
        //
        // state.serialize_entry("level", &self.level)?;
        state.serialize_entry("principal", &self.principal)?;
        if !self.blocks.is_empty() {
            state.serialize_entry("blocks", &self.blocks)?;
        }
        state.serialize_entry("location", &self.location)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for ListItem {
    fn deserialize<D>(deserializer: D) -> Result<ListItem, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct MyStructVisitor;

        impl<'de> Visitor<'de> for MyStructVisitor {
            type Value = ListItem;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a struct representing MyStruct")
            }

            fn visit_map<V>(self, mut map: V) -> Result<ListItem, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut my_principal = None;
                let mut my_blocks = None;
                let mut my_checked = None;
                let mut my_location = None;
                let mut my_marker = None;

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "principal" => {
                            if my_principal.is_some() {
                                return Err(de::Error::duplicate_field("principal"));
                            }
                            my_principal = Some(map.next_value()?);
                        }
                        "blocks" => {
                            if my_blocks.is_some() {
                                return Err(de::Error::duplicate_field("blocks"));
                            }
                            my_blocks = Some(map.next_value()?);
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
                        "checked" => {
                            if my_checked.is_some() {
                                return Err(de::Error::duplicate_field("checked"));
                            }
                            my_checked = Some(map.next_value::<bool>()?);
                        }
                        _ => {
                            tracing::debug!(?key, "ignoring unexpected field in ListItem");
                            // Ignore any other fields
                            let _ = map.next_value::<de::IgnoredAny>()?;
                        }
                    }
                }
                let marker = my_marker.ok_or_else(|| de::Error::missing_field("marker"))?;
                let principal =
                    my_principal.ok_or_else(|| de::Error::missing_field("principal"))?;
                let blocks = my_blocks.unwrap_or_default();
                let level =
                    ListLevel::try_from(ListItem::parse_depth_from_marker(&marker).unwrap_or(1))
                        .map_err(|e| {
                            de::Error::custom(format!("invalid list item level from marker: {e}",))
                        })?;
                let checked = my_checked.map(|c| {
                    if c {
                        ListItemCheckedStatus::Checked
                    } else {
                        ListItemCheckedStatus::Unchecked
                    }
                });
                Ok(ListItem {
                    level,
                    marker,
                    location: my_location.ok_or_else(|| de::Error::missing_field("location"))?,
                    principal,
                    blocks,
                    checked,
                })
            }
        }
        deserializer.deserialize_map(MyStructVisitor)
    }
}

impl Serialize for ListItemCheckedStatus {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match &self {
            ListItemCheckedStatus::Checked => serializer.serialize_bool(true),
            ListItemCheckedStatus::Unchecked => serializer.serialize_bool(false),
        }
    }
}

impl<'de> Deserialize<'de> for ListItemCheckedStatus {
    fn deserialize<D>(deserializer: D) -> Result<ListItemCheckedStatus, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ListItemCheckedStatusVisitor;

        impl Visitor<'_> for ListItemCheckedStatusVisitor {
            type Value = ListItemCheckedStatus;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a boolean representing checked status")
            }

            fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                if v {
                    Ok(ListItemCheckedStatus::Checked)
                } else {
                    Ok(ListItemCheckedStatus::Unchecked)
                }
            }
        }

        deserializer.deserialize_bool(ListItemCheckedStatusVisitor)
    }
}
