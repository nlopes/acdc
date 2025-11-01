//! The data models for the `AsciiDoc` document.
use std::{fmt::Display, str::FromStr};

use serde::{
    Deserialize, Serialize,
    de::{self, Deserializer, MapAccess, Visitor},
    ser::{SerializeMap, Serializer},
};

use crate::Error;

mod attributes;
mod inlines;
mod location;
mod substitution;

pub use attributes::{
    AttributeName, AttributeValue, Document as DocumentAttributes, Element as ElementAttributes,
};
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
            url::Url::parse(value)
                .map(Source::Url)
                .map_err(|e| Error::Parse(format!("invalid URL: {e}")))
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
            Source::Url(url) => write!(f, "{url}"),
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
            _ => Err(Error::Parse(format!(
                "unknown admonition variant: {variant}"
            ))),
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

/// A `SafeId` represents a sanitised ID.
#[derive(Clone, Debug, PartialEq)]
pub enum SafeId {
    Modified(String),
    Unmodified(String),
}

impl Display for SafeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SafeId::Modified(id) => write!(f, "_{id}"),
            SafeId::Unmodified(id) => write!(f, "{id}"),
        }
    }
}

impl Section {
    /// Generate a section ID based on its title and metadata.
    ///
    /// This function checks if the section has an explicit ID in its metadata. If not, it
    /// generates an ID from the title by converting it to lowercase, replacing spaces and
    /// hyphens with underscores, and removing non-alphanumeric characters.
    #[must_use]
    pub fn generate_id(metadata: &BlockMetadata, title: &[InlineNode]) -> SafeId {
        // Check if section has an explicit ID in metadata
        if let Some(anchor) = &metadata.id {
            return SafeId::Unmodified(anchor.id.clone());
        }

        // Generate ID from title
        let title_text = converter::inlines_to_string(title);
        SafeId::Modified(
            title_text
                .to_lowercase()
                .chars()
                .filter_map(|c| {
                    if c.is_alphanumeric() {
                        Some(c)
                    } else if c.is_whitespace() || c == '-' || c == '.' {
                        Some('_')
                    } else {
                        None
                    }
                })
                .collect::<String>(),
        )
    }
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
        if !self.metadata.is_default() {
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
                let mut my_value: Option<String> = None;
                let mut my_id = None;
                let mut my_title = None;
                let mut my_level = None;
                let mut my_metadata = None;
                let mut my_location = None;
                let mut my_ref_text = None;
                let mut my_form = None;
                let mut my_target = None;
                let mut my_source = None;
                let mut my_sources = None;
                let mut my_variant = None;
                let mut my_anchors = None;
                let mut my_marker = None;
                let mut my_blocks = None;
                let mut my_items = None;
                let mut my_inlines = None;
                let mut my_content: Option<serde_json::Value> = None;
                let mut my_notation: Option<serde_json::Value> = None;
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
                        "value" => {
                            if my_value.is_some() {
                                return Err(de::Error::duplicate_field("value"));
                            }
                            my_value = Some(map.next_value::<String>()?);
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
                        "source" => {
                            if my_source.is_some() {
                                return Err(de::Error::duplicate_field("source"));
                            }
                            my_source = Some(map.next_value::<Source>()?);
                        }
                        "sources" => {
                            if my_sources.is_some() {
                                return Err(de::Error::duplicate_field("sources"));
                            }
                            my_sources = Some(map.next_value::<Vec<Source>>()?);
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
                        "notation" => {
                            if my_notation.is_some() {
                                return Err(de::Error::duplicate_field("notation"));
                            }
                            my_notation = Some(map.next_value()?);
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
                        let my_blocks = if let Some(blocks) = my_blocks {
                            match blocks {
                                serde_json::Value::Array(blocks) => blocks
                                    .into_iter()
                                    .map(|v| serde_json::from_value(v).map_err(de::Error::custom))
                                    .collect::<Result<Vec<Block>, _>>()?,
                                _ => return Err(de::Error::custom("blocks must be an array")),
                            }
                        } else {
                            // blocks can be empty
                            Vec::new()
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
                        let source = my_source.ok_or_else(|| de::Error::missing_field("source"))?;
                        Ok(Block::Image(Image {
                            title: my_title,
                            source,
                            metadata: my_metadata,
                            location: my_location,
                        }))
                    }
                    ("audio", "block") => {
                        let my_form = my_form.ok_or_else(|| de::Error::missing_field("form"))?;
                        if my_form != "macro" {
                            return Err(de::Error::custom(format!("unexpected form: {my_form}")));
                        }
                        let source = my_source.ok_or_else(|| de::Error::missing_field("source"))?;
                        Ok(Block::Audio(Audio {
                            title: my_title,
                            source,
                            metadata: my_metadata,
                            location: my_location,
                        }))
                    }
                    ("video", "block") => {
                        // Handle both simplified format with "target" and full format with "sources"
                        let sources = if let Some(sources_value) = my_sources {
                            sources_value
                        } else {
                            // Fallback to simplified format with target
                            let my_form =
                                my_form.ok_or_else(|| de::Error::missing_field("form"))?;
                            if my_form != "macro" {
                                return Err(de::Error::custom(format!(
                                    "unexpected form: {my_form}"
                                )));
                            }
                            let target =
                                my_target.ok_or_else(|| de::Error::missing_field("target"))?;
                            let source = Source::from_str(&target).map_err(de::Error::custom)?;
                            vec![source]
                        };
                        Ok(Block::Video(Video {
                            title: my_title,
                            sources,
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
                        metadata: my_metadata,
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
                    ("stem", "block") => {
                        let my_form = my_form.ok_or_else(|| de::Error::missing_field("form"))?;
                        if my_form != "delimited" {
                            return Err(de::Error::custom(format!("unexpected form: {my_form}")));
                        }
                        let my_delimiter =
                            my_delimiter.ok_or_else(|| de::Error::missing_field("delimiter"))?;
                        let serde_json::Value::String(content) =
                            my_content.ok_or_else(|| de::Error::missing_field("content"))?
                        else {
                            return Err(de::Error::custom("content must be a string"));
                        };
                        let notation = match my_notation {
                            Some(serde_json::Value::String(n)) => {
                                StemNotation::from_str(&n).map_err(de::Error::custom)?
                            }
                            _ => StemNotation::Latexmath, // Default
                        };
                        Ok(Block::DelimitedBlock(DelimitedBlock {
                            metadata: my_metadata,
                            inner: DelimitedBlockType::DelimitedStem(StemContent {
                                content,
                                notation,
                            }),
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
                        match my_variant.as_str() {
                            "unordered" => {
                                let my_marker =
                                    my_marker.ok_or_else(|| de::Error::missing_field("marker"))?;
                                Ok(Block::UnorderedList(UnorderedList {
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
                                        _ => {
                                            return Err(de::Error::custom(
                                                "items must be an array",
                                            ));
                                        }
                                    },
                                    location: my_location,
                                }))
                            }
                            "ordered" => {
                                let my_marker =
                                    my_marker.ok_or_else(|| de::Error::missing_field("marker"))?;
                                Ok(Block::OrderedList(OrderedList {
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
                                        _ => {
                                            return Err(de::Error::custom(
                                                "items must be an array",
                                            ));
                                        }
                                    },
                                    location: my_location,
                                }))
                            }
                            "callout" => Ok(Block::CalloutList(CalloutList {
                                title: my_title,
                                metadata: my_metadata,
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
                    ("toc", "block") => Ok(Block::TableOfContents(TableOfContents {
                        metadata: my_metadata,
                        location: my_location,
                    })),
                    // Document attribute is not something that currently the TCK
                    // supports. I've added it because I believe it should be there. Where
                    // in the document an attribute appears has implications on its scope.
                    (name, "attribute") => Ok(Block::DocumentAttribute(DocumentAttribute {
                        name: name.to_string(),
                        value: if let Some(value) = my_value {
                            if value.is_empty() {
                                AttributeValue::None
                            } else if value.eq_ignore_ascii_case("true") {
                                AttributeValue::Bool(true)
                            } else if value.eq_ignore_ascii_case("false") {
                                AttributeValue::Bool(false)
                            } else {
                                AttributeValue::String(value.clone())
                            }
                        } else {
                            AttributeValue::None
                        },
                        location: my_location,
                    })),
                    _ => Err(de::Error::custom(format!(
                        "unexpected name/type combination: {my_name}/{my_type}",
                    ))),
                }
            }
        }

        deserializer.deserialize_map(MyStructVisitor)
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
                            tracing::debug!("ignoring unexpected field in ListItem: {key}");
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
