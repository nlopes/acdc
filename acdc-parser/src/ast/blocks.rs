mod bodies;
mod boundaries;
mod document;
mod lists;

pub use document::{AuthorRef, DocumentAttributesRef, FootnoteDefinitionRef, HeaderRef};
pub use lists::{
    CalloutListItemRef, DescriptionListDefinitionRef, DescriptionListItemRef,
    DescriptionListTermRef, ListItemRef,
};

use std::borrow::Cow;

use crate::{Block, DelimitedBlockType, Document, Paragraph, Section};

use super::{AttributesRef, ContentRef, ElementIdRef, ImageRef, LocationRef, TableRef, TokensRef};

macro_rules! delimited_ref {
    ($name:ident<$lifetime:lifetime>, $body:ty, $doc:literal) => {
        #[doc = $doc]
        #[derive(Clone, Copy, Debug)]
        pub struct $name<$lifetime> {
            node: &$lifetime crate::DelimitedBlock<$lifetime>,
            // A thin reference keeps this handle the same size as TableRef.
            pub(in crate::ast) body: &$lifetime $body,
        }

        impl<$lifetime> $name<$lifetime> {
            pub(super) fn new(
                node: &$lifetime crate::DelimitedBlock<$lifetime>,
                body: &$lifetime $body,
            ) -> Self {
                Self { node, body }
            }

            /// The source span of this node.
            #[must_use]
            pub fn location(self) -> LocationRef<$lifetime> {
                LocationRef::new(&self.node.location)
            }
        }
    };
}

node_ref!(DocumentRef<'doc>, Document<'doc>, "A document.");
node_ref!(SectionRef<'doc>, Section<'doc>, "A section.");
node_ref!(ParagraphRef<'doc>, Paragraph<'doc>, "A paragraph.");
node_ref!(
    TableOfContentsRef<'doc>,
    crate::TableOfContents<'doc>,
    "A table-of-contents block."
);
node_ref!(
    AdmonitionRef<'doc>,
    crate::Admonition<'doc>,
    "An admonition."
);
node_ref!(
    DiscreteHeaderRef<'doc>,
    crate::DiscreteHeader<'doc>,
    "A discrete heading."
);
node_ref!(
    DocumentAttributeRef<'doc>,
    crate::DocumentAttribute<'doc>,
    "A document-attribute assignment."
);
node_ref!(
    ThematicBreakRef<'doc>,
    crate::ThematicBreak<'doc>,
    "A thematic break."
);
node_ref!(PageBreakRef<'doc>, crate::PageBreak<'doc>, "A page break.");
node_ref!(
    UnorderedListRef<'doc>,
    crate::UnorderedList<'doc>,
    "An unordered list."
);
node_ref!(
    OrderedListRef<'doc>,
    crate::OrderedList<'doc>,
    "An ordered list."
);
node_ref!(
    CalloutListRef<'doc>,
    crate::CalloutList<'doc>,
    "A callout list."
);
node_ref!(
    DescriptionListRef<'doc>,
    crate::DescriptionList<'doc>,
    "A description list."
);
node_ref!(AudioRef<'doc>, crate::Audio<'doc>, "An audio block.");
node_ref!(VideoRef<'doc>, crate::Video<'doc>, "A video block.");
node_ref!(
    CommentRef<'doc>,
    crate::Comment<'doc>,
    "A line or paragraph comment."
);
delimited_ref!(
    DelimitedCommentRef<'doc>,
    Vec<crate::InlineNode<'doc>>,
    "A delimited comment."
);
delimited_ref!(ExampleRef<'doc>, Vec<Block<'doc>>, "An example block.");
delimited_ref!(
    ListingRef<'doc>,
    Vec<crate::InlineNode<'doc>>,
    "A listing block."
);
delimited_ref!(
    LiteralRef<'doc>,
    Vec<crate::InlineNode<'doc>>,
    "A literal block."
);
delimited_ref!(OpenRef<'doc>, Vec<Block<'doc>>, "An open block.");
delimited_ref!(SidebarRef<'doc>, Vec<Block<'doc>>, "A sidebar.");
delimited_ref!(
    PassthroughRef<'doc>,
    Vec<crate::InlineNode<'doc>>,
    "A passthrough block."
);
delimited_ref!(QuoteRef<'doc>, Vec<Block<'doc>>, "A quote block.");
delimited_ref!(
    VerseRef<'doc>,
    Vec<crate::InlineNode<'doc>>,
    "A verse block."
);
delimited_ref!(
    StemBlockRef<'doc>,
    crate::StemContent<'doc>,
    "A mathematical block."
);

/// A block with a semantic kind and source location.
#[derive(Clone, Copy, Debug)]
pub struct BlockRef<'doc> {
    kind: BlockKind<'doc>,
}

/// The semantic kind of a block, independent of its backing representation.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum BlockKind<'doc> {
    TableOfContents(TableOfContentsRef<'doc>),
    Admonition(AdmonitionRef<'doc>),
    DiscreteHeader(DiscreteHeaderRef<'doc>),
    DocumentAttribute(DocumentAttributeRef<'doc>),
    ThematicBreak(ThematicBreakRef<'doc>),
    PageBreak(PageBreakRef<'doc>),
    UnorderedList(UnorderedListRef<'doc>),
    OrderedList(OrderedListRef<'doc>),
    CalloutList(CalloutListRef<'doc>),
    DescriptionList(DescriptionListRef<'doc>),
    Section(SectionRef<'doc>),
    Paragraph(ParagraphRef<'doc>),
    Image(ImageRef<'doc>),
    Audio(AudioRef<'doc>),
    Video(VideoRef<'doc>),
    Comment(CommentRef<'doc>),
    DelimitedComment(DelimitedCommentRef<'doc>),
    Example(ExampleRef<'doc>),
    Listing(ListingRef<'doc>),
    Literal(LiteralRef<'doc>),
    Open(OpenRef<'doc>),
    Sidebar(SidebarRef<'doc>),
    Table(TableRef<'doc>),
    Passthrough(PassthroughRef<'doc>),
    Quote(QuoteRef<'doc>),
    Verse(VerseRef<'doc>),
    Stem(StemBlockRef<'doc>),
}

impl<'doc> BlockRef<'doc> {
    /// Match the semantic kind while retaining a concrete typed handle.
    #[must_use]
    pub fn kind(self) -> BlockKind<'doc> {
        self.kind
    }

    /// The source span of this block.
    #[must_use]
    pub fn location(self) -> LocationRef<'doc> {
        match self.kind {
            BlockKind::TableOfContents(node) => node.location(),
            BlockKind::Admonition(node) => node.location(),
            BlockKind::DiscreteHeader(node) => node.location(),
            BlockKind::DocumentAttribute(node) => node.location(),
            BlockKind::ThematicBreak(node) => node.location(),
            BlockKind::PageBreak(node) => node.location(),
            BlockKind::UnorderedList(node) => node.location(),
            BlockKind::OrderedList(node) => node.location(),
            BlockKind::CalloutList(node) => node.location(),
            BlockKind::DescriptionList(node) => node.location(),
            BlockKind::Section(node) => node.location(),
            BlockKind::Paragraph(node) => node.location(),
            BlockKind::Image(node) => node.location(),
            BlockKind::Audio(node) => node.location(),
            BlockKind::Video(node) => node.location(),
            BlockKind::Comment(node) => node.location(),
            BlockKind::DelimitedComment(node) => node.location(),
            BlockKind::Example(node) => node.location(),
            BlockKind::Listing(node) => node.location(),
            BlockKind::Literal(node) => node.location(),
            BlockKind::Open(node) => node.location(),
            BlockKind::Sidebar(node) => node.location(),
            BlockKind::Table(node) => node.location(),
            BlockKind::Passthrough(node) => node.location(),
            BlockKind::Quote(node) => node.location(),
            BlockKind::Verse(node) => node.location(),
            BlockKind::Stem(node) => node.location(),
        }
    }
}

impl<'doc> DocumentRef<'doc> {
    /// The highest stored caption ordinal for this category, or zero if none is assigned.
    ///
    /// Scans the block tree, including nested lists and table cells, without
    /// renumbering blocks or consulting reference catalogs.
    #[must_use]
    pub fn highest_caption_number(self, kind: crate::CaptionKind) -> u32 {
        self.node.highest_caption_number(kind)
    }

    /// Read direct blocks in document order, excluding derived catalogs.
    ///
    /// The iterator cannot outlive the owning parse result:
    ///
    /// ```compile_fail,E0597
    /// use acdc_parser::{Options, parse};
    ///
    /// let blocks = {
    ///     let parsed = parse("A paragraph.", &Options::default()).unwrap();
    ///     parsed.document().blocks()
    /// };
    /// println!("{}", blocks.count());
    /// ```
    pub fn blocks(self) -> impl Iterator<Item = BlockRef<'doc>> + Send + Sync + 'doc {
        self.node.blocks.iter().map(BlockRef::from)
    }

    /// Read the document title; a supplied empty title remains present.
    #[must_use]
    pub fn title(self) -> Option<ContentRef<'doc>> {
        self.node
            .header
            .as_ref()
            .and_then(|header| header.title.content())
    }
}

impl<'doc> SectionRef<'doc> {
    /// Read this element's attributes.
    #[must_use]
    pub fn attributes(self) -> AttributesRef<'doc> {
        AttributesRef::section(self.node)
    }

    /// Role tokens in their retained order.
    #[must_use]
    pub fn roles(self) -> TokensRef<'doc> {
        self.attributes().roles()
    }

    /// Option tokens in their retained order.
    #[must_use]
    pub fn options(self) -> TokensRef<'doc> {
        self.attributes().options()
    }

    /// The retained block style, including an explicitly empty style.
    #[must_use]
    pub fn style(self) -> Option<&'doc str> {
        self.attributes().style()
    }

    /// The explicit or parser-assigned section ID. Caller-built sections without
    /// an explicit ID have no assigned value; this read does not generate one.
    #[must_use]
    pub fn id(self) -> Option<ElementIdRef<'doc>> {
        self.attributes().id()
    }

    /// Return the explicit or assigned ID, or derive one from the title.
    ///
    /// Caller-built sections use a fallback without collision checks. This does
    /// not assign an ID.
    #[must_use]
    pub fn resolve_id(self) -> Cow<'doc, str> {
        self.node.id()
    }

    /// Read direct blocks and subsections in document order.
    pub fn blocks(self) -> impl Iterator<Item = BlockRef<'doc>> + Send + Sync + 'doc {
        self.node.content.iter().map(BlockRef::from)
    }

    /// Read the section title; a supplied empty title remains present.
    #[must_use]
    pub fn title(self) -> Option<ContentRef<'doc>> {
        self.node.title.content()
    }

    /// The section's nesting level.
    #[must_use]
    pub fn level(self) -> u8 {
        self.node.level
    }

    /// The section's category, including special-section styles.
    #[must_use]
    pub fn kind(self) -> crate::SectionKind {
        self.node.kind
    }

    /// The assigned number without a signifier or presentation punctuation.
    /// This read does not renumber sections after an edit.
    #[must_use]
    pub fn number(self) -> Option<&'doc str> {
        self.node.number()
    }
}

impl<'doc> ParagraphRef<'doc> {
    /// Read this element's attributes.
    #[must_use]
    pub fn attributes(self) -> AttributesRef<'doc> {
        AttributesRef::paragraph(self.node)
    }

    /// Role tokens in their retained order.
    #[must_use]
    pub fn roles(self) -> TokensRef<'doc> {
        self.attributes().roles()
    }

    /// Option tokens in their retained order.
    #[must_use]
    pub fn options(self) -> TokensRef<'doc> {
        self.attributes().options()
    }

    /// The retained block style, including an explicitly empty style.
    #[must_use]
    pub fn style(self) -> Option<&'doc str> {
        self.attributes().style()
    }

    /// The primary ID, if present.
    #[must_use]
    pub fn id(self) -> Option<ElementIdRef<'doc>> {
        self.attributes().id()
    }

    /// Read the paragraph body in inline order.
    #[must_use]
    pub fn content(self) -> ContentRef<'doc> {
        ContentRef::new(&self.node.content)
    }

    /// Body text after document preprocessing, before inline substitutions.
    ///
    /// Absent when no source text was retained, including constructed paragraphs.
    #[must_use]
    pub fn source_text(self) -> Option<&'doc str> {
        self.node.source_text()
    }

    /// Read the paragraph title; a supplied empty title remains present.
    #[must_use]
    pub fn title(self) -> Option<ContentRef<'doc>> {
        self.node.title.content()
    }
}

impl<'doc> DocumentRef<'doc> {
    pub(crate) fn as_document(self) -> &'doc Document<'doc> {
        self.node
    }
}

impl serde::Serialize for DocumentRef<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.as_document().serialize(serializer)
    }
}

impl<'doc> From<&'doc Document<'doc>> for DocumentRef<'doc> {
    fn from(document: &'doc Document<'doc>) -> Self {
        Self::new(document)
    }
}

impl<'doc> From<&'doc Section<'doc>> for SectionRef<'doc> {
    fn from(section: &'doc Section<'doc>) -> Self {
        Self::new(section)
    }
}

impl<'doc> From<&'doc Paragraph<'doc>> for ParagraphRef<'doc> {
    fn from(paragraph: &'doc Paragraph<'doc>) -> Self {
        Self::new(paragraph)
    }
}

impl<'doc> From<&'doc Block<'doc>> for BlockRef<'doc> {
    fn from(block: &'doc Block<'doc>) -> Self {
        let kind = match block {
            Block::TableOfContents(node) => {
                BlockKind::TableOfContents(TableOfContentsRef::new(node))
            }
            Block::Admonition(node) => BlockKind::Admonition(AdmonitionRef::new(node)),
            Block::DiscreteHeader(node) => BlockKind::DiscreteHeader(DiscreteHeaderRef::new(node)),
            Block::DocumentAttribute(node) => {
                BlockKind::DocumentAttribute(DocumentAttributeRef::new(node))
            }
            Block::ThematicBreak(node) => BlockKind::ThematicBreak(ThematicBreakRef::new(node)),
            Block::PageBreak(node) => BlockKind::PageBreak(PageBreakRef::new(node)),
            Block::UnorderedList(node) => BlockKind::UnorderedList(UnorderedListRef::new(node)),
            Block::OrderedList(node) => BlockKind::OrderedList(OrderedListRef::new(node)),
            Block::CalloutList(node) => BlockKind::CalloutList(CalloutListRef::new(node)),
            Block::DescriptionList(node) => {
                BlockKind::DescriptionList(DescriptionListRef::new(node))
            }
            Block::Section(node) => BlockKind::Section(SectionRef::from(node)),
            Block::Paragraph(node) => BlockKind::Paragraph(ParagraphRef::from(node)),
            Block::Image(node) => BlockKind::Image(ImageRef::new(node)),
            Block::Audio(node) => BlockKind::Audio(AudioRef::new(node)),
            Block::Video(node) => BlockKind::Video(VideoRef::new(node)),
            Block::Comment(node) => BlockKind::Comment(CommentRef::new(node)),
            Block::DelimitedBlock(node) => return Self::from(node),
        };
        Self { kind }
    }
}

impl<'doc> From<&'doc crate::DelimitedBlock<'doc>> for BlockRef<'doc> {
    fn from(block: &'doc crate::DelimitedBlock<'doc>) -> Self {
        let kind = match &block.inner {
            DelimitedBlockType::DelimitedComment(body) => {
                BlockKind::DelimitedComment(DelimitedCommentRef::new(block, body))
            }
            DelimitedBlockType::DelimitedExample(body) => {
                BlockKind::Example(ExampleRef::new(block, body))
            }
            DelimitedBlockType::DelimitedListing(body) => {
                BlockKind::Listing(ListingRef::new(block, body))
            }
            DelimitedBlockType::DelimitedLiteral(body) => {
                BlockKind::Literal(LiteralRef::new(block, body))
            }
            DelimitedBlockType::DelimitedOpen(body) => BlockKind::Open(OpenRef::new(block, body)),
            DelimitedBlockType::DelimitedSidebar(body) => {
                BlockKind::Sidebar(SidebarRef::new(block, body))
            }
            DelimitedBlockType::DelimitedTable(table) => {
                BlockKind::Table(TableRef::new(block, table))
            }
            DelimitedBlockType::DelimitedPass(body) => {
                BlockKind::Passthrough(PassthroughRef::new(block, body))
            }
            DelimitedBlockType::DelimitedQuote(body) => {
                BlockKind::Quote(QuoteRef::new(block, body))
            }
            DelimitedBlockType::DelimitedVerse(body) => {
                BlockKind::Verse(VerseRef::new(block, body))
            }
            DelimitedBlockType::DelimitedStem(body) => {
                BlockKind::Stem(StemBlockRef::new(block, body))
            }
        };
        Self { kind }
    }
}

/// Copy the complete document for independent edits, including its registered
/// catalogs, source locations, and resolved captions and table presentation.
/// Copies can still borrow source text and cannot outlive the parse result.
///
/// ```compile_fail,E0597
/// use acdc_parser::{Document, Options, parse};
///
/// let copy = {
///     let parsed = parse("Text.", &Options::default()).unwrap();
///     Document::from(parsed.document())
/// };
/// println!("{copy:?}");
/// ```
impl<'doc> From<DocumentRef<'doc>> for Document<'doc> {
    fn from(document: DocumentRef<'doc>) -> Self {
        let document = document.node;
        Self {
            header: document.header.as_ref().map(|header| crate::Header {
                metadata: header.metadata.clone(),
                title: header.title.clone(),
                subtitle: header.subtitle.clone(),
                authors: header
                    .authors
                    .iter()
                    .map(|author| crate::Author {
                        first_name: author.first_name,
                        middle_name: author.middle_name,
                        last_name: author.last_name,
                        initials: author.initials,
                        email: author.email,
                    })
                    .collect(),
                location: header.location.clone(),
            }),
            attributes: document.attributes.clone(),
            blocks: document.blocks.clone(),
            footnotes: document.footnotes.clone(),
            toc_entries: document.toc_entries.clone(),
            references: document.references.clone(),
            location: document.location.clone(),
        }
    }
}

/// Copy a block and its descendants for independent edits, preserving retained
/// source and resolved values. The copy can still borrow source text.
impl<'doc> From<BlockRef<'doc>> for Block<'doc> {
    fn from(block: BlockRef<'doc>) -> Self {
        match block.kind() {
            BlockKind::TableOfContents(node) => Self::TableOfContents(node.node.clone()),
            BlockKind::Admonition(node) => Self::Admonition(node.node.clone()),
            BlockKind::DiscreteHeader(node) => Self::DiscreteHeader(node.node.clone()),
            BlockKind::DocumentAttribute(node) => Self::DocumentAttribute(node.node.clone()),
            BlockKind::ThematicBreak(node) => Self::ThematicBreak(node.node.clone()),
            BlockKind::PageBreak(node) => Self::PageBreak(node.node.clone()),
            BlockKind::UnorderedList(node) => Self::UnorderedList(node.node.clone()),
            BlockKind::OrderedList(node) => Self::OrderedList(node.node.clone()),
            BlockKind::CalloutList(node) => Self::CalloutList(node.node.clone()),
            BlockKind::DescriptionList(node) => Self::DescriptionList(node.node.clone()),
            BlockKind::Section(node) => Self::Section(node.node.clone()),
            BlockKind::Paragraph(node) => Self::Paragraph(node.node.clone()),
            BlockKind::Image(node) => Self::Image(node.node.clone()),
            BlockKind::Audio(node) => Self::Audio(node.node.clone()),
            BlockKind::Video(node) => Self::Video(node.node.clone()),
            BlockKind::Comment(node) => Self::Comment(node.node.clone()),
            BlockKind::DelimitedComment(node) => Self::DelimitedBlock(node.node.clone()),
            BlockKind::Example(node) => Self::DelimitedBlock(node.node.clone()),
            BlockKind::Listing(node) => Self::DelimitedBlock(node.node.clone()),
            BlockKind::Literal(node) => Self::DelimitedBlock(node.node.clone()),
            BlockKind::Open(node) => Self::DelimitedBlock(node.node.clone()),
            BlockKind::Sidebar(node) => Self::DelimitedBlock(node.node.clone()),
            BlockKind::Passthrough(node) => Self::DelimitedBlock(node.node.clone()),
            BlockKind::Quote(node) => Self::DelimitedBlock(node.node.clone()),
            BlockKind::Verse(node) => Self::DelimitedBlock(node.node.clone()),
            BlockKind::Stem(node) => Self::DelimitedBlock(node.node.clone()),
            BlockKind::Table(node) => Self::DelimitedBlock(node.block.clone()),
        }
    }
}
