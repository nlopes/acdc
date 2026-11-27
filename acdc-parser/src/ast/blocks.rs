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

use crate::{
    Block, DelimitedComment, Document, Example, Listing, Literal, Open, Paragraph, Passthrough,
    Quote, Section, Sidebar, StemBlock, Table, Verse, model::Delimited,
};

use super::{AttributesRef, ContentRef, ElementIdRef, ImageRef, LocationRef, TableRef, TokensRef};

macro_rules! delimited_ref {
    ($name:ident<$lifetime:lifetime>, $body:ty, $doc:literal) => {
        #[doc = $doc]
        #[derive(Clone, Copy, Debug)]
        pub struct $name<$lifetime> {
            node: &$lifetime Delimited<$lifetime, $body>,
            pub(in crate::ast) body: &$lifetime $body,
        }

        impl<$lifetime> $name<$lifetime> {
            pub(super) fn new(node: &$lifetime Delimited<$lifetime, $body>) -> Self {
                Self { node, body: &node.body }
            }

            /// The source span of this node.
            #[must_use]
            pub fn location(self) -> LocationRef<$lifetime> {
                LocationRef::new(&self.node.location)
            }
        }
    };
}

node_ref!(
    ExtensionRef<'doc>,
    crate::extensions::ExtensionBlock<'doc>,
    "A pending extension block in a working session."
);
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
    DiscreteHeadingRef<'doc>,
    crate::DiscreteHeading<'doc>,
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
    crate::model::StemContent<'doc>,
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
    Extension(ExtensionRef<'doc>),
    TableOfContents(TableOfContentsRef<'doc>),
    Admonition(AdmonitionRef<'doc>),
    DiscreteHeading(DiscreteHeadingRef<'doc>),
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
    DelimitedExample(ExampleRef<'doc>),
    DelimitedListing(ListingRef<'doc>),
    DelimitedLiteral(LiteralRef<'doc>),
    DelimitedOpen(OpenRef<'doc>),
    DelimitedSidebar(SidebarRef<'doc>),
    DelimitedTable(TableRef<'doc>),
    DelimitedPassthrough(PassthroughRef<'doc>),
    DelimitedQuote(QuoteRef<'doc>),
    DelimitedVerse(VerseRef<'doc>),
    DelimitedStem(StemBlockRef<'doc>),
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
            BlockKind::Extension(node) => node.location(),
            BlockKind::TableOfContents(node) => node.location(),
            BlockKind::Admonition(node) => node.location(),
            BlockKind::DiscreteHeading(node) => node.location(),
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
            BlockKind::DelimitedExample(node) => node.location(),
            BlockKind::DelimitedListing(node) => node.location(),
            BlockKind::DelimitedLiteral(node) => node.location(),
            BlockKind::DelimitedOpen(node) => node.location(),
            BlockKind::DelimitedSidebar(node) => node.location(),
            BlockKind::DelimitedTable(node) => node.location(),
            BlockKind::DelimitedPassthrough(node) => node.location(),
            BlockKind::DelimitedQuote(node) => node.location(),
            BlockKind::DelimitedVerse(node) => node.location(),
            BlockKind::DelimitedStem(node) => node.location(),
        }
    }
}

impl<'doc> DocumentRef<'doc> {
    /// Whether the parser retained an authored conversion-time inline macro.
    #[must_use]
    pub fn has_inline_macros(self) -> bool {
        self.node.has_inline_macros
    }

    /// Whether the document contains deferred display requests.
    #[must_use]
    pub fn has_deferred_displays(self) -> bool {
        self.node.generated_displays.is_active()
    }
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
            Block::DiscreteHeading(node) => {
                BlockKind::DiscreteHeading(DiscreteHeadingRef::new(node))
            }
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
            Block::Extension(node) => BlockKind::Extension(ExtensionRef::new(node)),
            Block::Section(node) => BlockKind::Section(SectionRef::from(node)),
            Block::Paragraph(node) => BlockKind::Paragraph(ParagraphRef::from(node)),
            Block::Image(node) => BlockKind::Image(ImageRef::new(node)),
            Block::Audio(node) => BlockKind::Audio(AudioRef::new(node)),
            Block::Video(node) => BlockKind::Video(VideoRef::new(node)),
            Block::Comment(node) => BlockKind::Comment(CommentRef::new(node)),
            Block::DelimitedComment(node) => {
                BlockKind::DelimitedComment(DelimitedCommentRef::new(&node.0))
            }
            Block::DelimitedExample(node) => BlockKind::DelimitedExample(ExampleRef::new(&node.0)),
            Block::DelimitedListing(node) => BlockKind::DelimitedListing(ListingRef::new(&node.0)),
            Block::DelimitedLiteral(node) => BlockKind::DelimitedLiteral(LiteralRef::new(&node.0)),
            Block::DelimitedOpen(node) => BlockKind::DelimitedOpen(OpenRef::new(&node.0)),
            Block::DelimitedSidebar(node) => BlockKind::DelimitedSidebar(SidebarRef::new(&node.0)),
            Block::DelimitedTable(node) => BlockKind::DelimitedTable(TableRef::new(&node.0)),
            Block::DelimitedPassthrough(node) => {
                BlockKind::DelimitedPassthrough(PassthroughRef::new(&node.0))
            }
            Block::DelimitedQuote(node) => BlockKind::DelimitedQuote(QuoteRef::new(&node.0)),
            Block::DelimitedVerse(node) => BlockKind::DelimitedVerse(VerseRef::new(&node.0)),
            Block::DelimitedStem(node) => BlockKind::DelimitedStem(StemBlockRef::new(&node.0)),
        };
        Self { kind }
    }
}

macro_rules! delimited_ref_from {
    ($($kind:ident => $read:ident),+ $(,)?) => { $(
        impl<'doc> From<&'doc $kind<'doc>> for $read<'doc> {
            fn from(block: &'doc $kind<'doc>) -> Self {
                Self::new(&block.0)
            }
        }
    )+ };
}

delimited_ref_from! {
    DelimitedComment => DelimitedCommentRef,
    Example => ExampleRef,
    Listing => ListingRef,
    Literal => LiteralRef,
    Open => OpenRef,
    Passthrough => PassthroughRef,
    Quote => QuoteRef,
    Sidebar => SidebarRef,
    StemBlock => StemBlockRef,
    Table => TableRef,
    Verse => VerseRef,
}

/// Copy the complete document for independent edits, including its registered
/// catalogs, source locations, and resolved captions and table presentation.
/// Copies can still borrow source text and cannot outlive the parse result.
///
/// ```compile_fail,E0597
/// use acdc_parser::{Options, parse, edit::Document};
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
            untitled_header_location: document.untitled_header_location.clone(),
            has_inline_macros: document.has_inline_macros,
            generated_displays: document.generated_displays.clone(),
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
            BlockKind::DiscreteHeading(node) => Self::DiscreteHeading(node.node.clone()),
            BlockKind::DocumentAttribute(node) => Self::DocumentAttribute(node.node.clone()),
            BlockKind::ThematicBreak(node) => Self::ThematicBreak(node.node.clone()),
            BlockKind::PageBreak(node) => Self::PageBreak(node.node.clone()),
            BlockKind::UnorderedList(node) => Self::UnorderedList(node.node.clone()),
            BlockKind::OrderedList(node) => Self::OrderedList(node.node.clone()),
            BlockKind::CalloutList(node) => Self::CalloutList(node.node.clone()),
            BlockKind::DescriptionList(node) => Self::DescriptionList(node.node.clone()),
            BlockKind::Extension(node) => Self::Extension(Box::new(node.node.clone())),
            BlockKind::Section(node) => Self::Section(node.node.clone()),
            BlockKind::Paragraph(node) => Self::Paragraph(node.node.clone()),
            BlockKind::Image(node) => Self::Image(node.node.clone()),
            BlockKind::Audio(node) => Self::Audio(node.node.clone()),
            BlockKind::Video(node) => Self::Video(node.node.clone()),
            BlockKind::Comment(node) => Self::Comment(node.node.clone()),
            BlockKind::DelimitedComment(node) => {
                Self::DelimitedComment(DelimitedComment(node.node.clone()))
            }
            BlockKind::DelimitedExample(node) => Self::DelimitedExample(Example(node.node.clone())),
            BlockKind::DelimitedListing(node) => Self::DelimitedListing(Listing(node.node.clone())),
            BlockKind::DelimitedLiteral(node) => Self::DelimitedLiteral(Literal(node.node.clone())),
            BlockKind::DelimitedOpen(node) => Self::DelimitedOpen(Open(node.node.clone())),
            BlockKind::DelimitedSidebar(node) => Self::DelimitedSidebar(Sidebar(node.node.clone())),
            BlockKind::DelimitedPassthrough(node) => {
                Self::DelimitedPassthrough(Passthrough(node.node.clone()))
            }
            BlockKind::DelimitedQuote(node) => Self::DelimitedQuote(Quote(node.node.clone())),
            BlockKind::DelimitedVerse(node) => Self::DelimitedVerse(Verse(node.node.clone())),
            BlockKind::DelimitedStem(node) => Self::DelimitedStem(StemBlock(node.node.clone())),
            BlockKind::DelimitedTable(node) => Self::DelimitedTable(Table(node.block.clone())),
        }
    }
}
