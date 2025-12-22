//! The data models for the `AsciiDoc` document.
use std::{fmt::Display, str::FromStr};

use serde::{
    Deserialize, Serialize,
    de::{self, Deserializer},
    ser::{SerializeMap, Serializer},
};

mod admonition;
mod anchor;
mod attributes;
mod inlines;
mod lists;
mod location;
mod media;
mod metadata;
mod section;
mod substitution;
mod tables;

pub use admonition::{Admonition, AdmonitionVariant};
pub use anchor::{Anchor, TocEntry};
pub use attributes::{AttributeName, AttributeValue, DocumentAttributes, ElementAttributes};
pub use inlines::*;
pub use lists::{
    CalloutList, DescriptionList, DescriptionListItem, ListItem, ListItemCheckedStatus, ListLevel,
    OrderedList, UnorderedList,
};
pub use location::*;
pub use media::{Audio, Image, Source, Video};
pub use metadata::{BlockMetadata, Role};
pub use section::*;
pub use substitution::*;
pub use tables::{
    ColumnFormat, ColumnStyle, ColumnWidth, HorizontalAlignment, Table, TableColumn, TableRow,
    VerticalAlignment,
};

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
/// The header contains the title, subtitle, authors, and optional metadata
/// (such as ID and roles) that can be applied to the document title.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Header {
    #[serde(default, skip_serializing_if = "BlockMetadata::is_default")]
    pub metadata: BlockMetadata,
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

/// A single-line comment in a document.
///
/// Line comments begin with `//` and continue to end of line.
/// They act as block boundaries but produce no output.
#[derive(Clone, Debug, PartialEq)]
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

fn construct_comment<E: de::Error>(raw: RawBlockFields) -> Result<Block, E> {
    let content = match raw.content {
        Some(serde_json::Value::String(s)) => s,
        Some(_) => return Err(E::custom("comment content must be a string")),
        None => String::new(),
    };
    Ok(Block::Comment(Comment {
        content,
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
        ("comment", "block") => construct_comment(raw),
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
