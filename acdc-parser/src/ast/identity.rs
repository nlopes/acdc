use super::{
    AdmonitionRef, AnchorRef, AttributesRef, AudioRef, BlockKind, BlockRef, CalloutListRef,
    ContentRef, DelimitedCommentRef, DescriptionListRef, DiscreteHeaderRef, DocumentRef,
    ElementIdRef, ExampleRef, HeaderRef, ImageRef, InlineKind, InlineRef, ListingRef, LiteralRef,
    OpenRef, OrderedListRef, PageBreakRef, ParagraphRef, PassthroughRef, QuoteRef, SectionRef,
    SidebarRef, StemBlockRef, TableOfContentsRef, TableRef, ThematicBreakRef, UnorderedListRef,
    VerseRef, VideoRef,
};

impl<'doc> BlockRef<'doc> {
    /// The retained title, including a present empty title. Owners without titles
    /// return `None`; captions and inline tooltips are separate.
    #[must_use]
    pub fn title(self) -> Option<ContentRef<'doc>> {
        self.element_attributes().and_then(AttributesRef::title)
    }

    /// The primary ID, including a parser-assigned section ID.
    ///
    /// Sections select the dedicated ID or last anchor; other blocks select the
    /// dedicated ID or first anchor, matching reference registration. Reads do
    /// not generate an ID for a caller-built section without an assigned one.
    ///
    /// ```
    /// use acdc_parser::{Options, parse, ast::IdOrigin};
    ///
    /// let parsed = parse("== Introduction\n", &Options::default())?;
    /// let block = parsed.document_ref().blocks().next().ok_or("section")?;
    /// let id = block.id().ok_or("assigned ID")?;
    /// assert_eq!(id.as_str(), "_introduction");
    /// assert_eq!(id.origin(), IdOrigin::Generated);
    /// assert!(block.primary_anchor().is_none());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[must_use]
    pub fn id(self) -> Option<ElementIdRef<'doc>> {
        self.element_attributes().and_then(AttributesRef::id)
    }

    /// The retained anchor selected as primary. A generated section ID has no
    /// authored anchor; read it through [`Self::id`].
    #[must_use]
    pub fn primary_anchor(self) -> Option<AnchorRef<'doc>> {
        self.element_attributes()
            .and_then(AttributesRef::primary_anchor)
    }

    /// The dedicated ID anchor followed by all retained anchors in stored order.
    /// Duplicates remain visible. These are not necessarily registered targets;
    /// generated section IDs and anchors inside inline content are separate.
    pub fn anchors(self) -> impl Iterator<Item = AnchorRef<'doc>> + Send + Sync + 'doc {
        self.element_attributes()
            .into_iter()
            .flat_map(AttributesRef::anchors)
    }

    fn element_attributes(self) -> Option<AttributesRef<'doc>> {
        match self.kind() {
            BlockKind::Section(node) => Some(node.attributes()),
            BlockKind::Paragraph(node) => Some(node.attributes()),
            BlockKind::Admonition(node) => Some(node.attributes()),
            BlockKind::TableOfContents(node) => Some(node.attributes()),
            BlockKind::DiscreteHeader(node) => Some(node.attributes()),
            BlockKind::ThematicBreak(node) => Some(node.attributes()),
            BlockKind::PageBreak(node) => Some(node.attributes()),
            BlockKind::UnorderedList(node) => Some(node.attributes()),
            BlockKind::OrderedList(node) => Some(node.attributes()),
            BlockKind::DescriptionList(node) => Some(node.attributes()),
            BlockKind::CalloutList(node) => Some(node.attributes()),
            BlockKind::Image(node) => Some(node.attributes()),
            BlockKind::Audio(node) => Some(node.attributes()),
            BlockKind::Video(node) => Some(node.attributes()),
            BlockKind::DelimitedComment(node) => Some(node.attributes()),
            BlockKind::Example(node) => Some(node.attributes()),
            BlockKind::Listing(node) => Some(node.attributes()),
            BlockKind::Literal(node) => Some(node.attributes()),
            BlockKind::Open(node) => Some(node.attributes()),
            BlockKind::Sidebar(node) => Some(node.attributes()),
            BlockKind::Table(node) => Some(node.attributes()),
            BlockKind::Passthrough(node) => Some(node.attributes()),
            BlockKind::Quote(node) => Some(node.attributes()),
            BlockKind::Verse(node) => Some(node.attributes()),
            BlockKind::Stem(node) => Some(node.attributes()),
            BlockKind::DocumentAttribute(_) | BlockKind::Comment(_) => None,
        }
    }
}

macro_rules! primary_anchor_reads {
    ($($name:ident),+ $(,)?) => { $(
        impl<'doc> $name<'doc> {
            /// The retained primary anchor; generated IDs have no authored anchor.
            /// See [`BlockRef::id`] for selection order.
            #[must_use]
            pub fn primary_anchor(self) -> Option<AnchorRef<'doc>> {
                self.attributes().primary_anchor()
            }
        }
    )+ };
}

primary_anchor_reads!(
    HeaderRef,
    SectionRef,
    ParagraphRef,
    AdmonitionRef,
    TableOfContentsRef,
    DiscreteHeaderRef,
    ThematicBreakRef,
    PageBreakRef,
    UnorderedListRef,
    OrderedListRef,
    DescriptionListRef,
    CalloutListRef,
    ImageRef,
    AudioRef,
    VideoRef,
    DelimitedCommentRef,
    ExampleRef,
    ListingRef,
    LiteralRef,
    OpenRef,
    SidebarRef,
    TableRef,
    PassthroughRef,
    QuoteRef,
    VerseRef,
    StemBlockRef,
);

macro_rules! anchor_reads {
    ($($name:ident),+ $(,)?) => { $(
        impl<'doc> $name<'doc> {
            /// All retained anchors in [`BlockRef::anchors`] order, without registration.
            pub fn anchors(self) -> impl Iterator<Item = AnchorRef<'doc>> + Send + Sync + 'doc {
                self.attributes().anchors()
            }
        }
    )+ };
}

anchor_reads!(
    HeaderRef,
    SectionRef,
    ParagraphRef,
    AdmonitionRef,
    UnorderedListRef,
    OrderedListRef,
    DescriptionListRef,
    CalloutListRef,
    ImageRef,
    DelimitedCommentRef,
    ExampleRef,
    ListingRef,
    LiteralRef,
    OpenRef,
    SidebarRef,
    TableRef,
    PassthroughRef,
    QuoteRef,
    VerseRef,
    StemBlockRef,
);

impl<'doc> DocumentRef<'doc> {
    /// The header's primary ID. The whole-document reference target `""` is separate.
    #[must_use]
    pub fn id(self) -> Option<ElementIdRef<'doc>> {
        self.header().and_then(HeaderRef::id)
    }

    /// The header's selected anchor, when present.
    #[must_use]
    pub fn primary_anchor(self) -> Option<AnchorRef<'doc>> {
        self.header().and_then(HeaderRef::primary_anchor)
    }

    /// Retained header anchors; this does not walk the document body.
    pub fn anchors(self) -> impl Iterator<Item = AnchorRef<'doc>> + Send + Sync + 'doc {
        self.header().into_iter().flat_map(HeaderRef::anchors)
    }
}

impl<'doc> InlineRef<'doc> {
    /// The retained element ID, including inline anchors. A footnote key or xref
    /// target is not this occurrence's ID. Retention does not imply registration
    /// in the document's reference catalog.
    #[must_use]
    pub fn id(self) -> Option<ElementIdRef<'doc>> {
        match self.kind() {
            InlineKind::Bold(node) => node.id(),
            InlineKind::Italic(node) => node.id(),
            InlineKind::Monospace(node) => node.id(),
            InlineKind::Highlight(node) => node.id(),
            InlineKind::Subscript(node) => node.id(),
            InlineKind::Superscript(node) => node.id(),
            InlineKind::CurvedQuotation(node) => node.id(),
            InlineKind::CurvedApostrophe(node) => node.id(),
            InlineKind::Image(node) => node.id(),
            InlineKind::Icon(node) => node.id(),
            InlineKind::Link(node) => node.id(),
            InlineKind::Url(node) => node.id(),
            InlineKind::Mailto(node) => node.id(),
            InlineKind::CrossReference(node) => node.id(),
            InlineKind::IndexTerm(node) => node.id(),
            InlineKind::Anchor(node) => Some(node.element_id()),
            InlineKind::Plain(_)
            | InlineKind::Raw(_)
            | InlineKind::Verbatim(_)
            | InlineKind::StandaloneCurvedApostrophe(_)
            | InlineKind::LineBreak(_)
            | InlineKind::Callout(_)
            | InlineKind::Footnote(_)
            | InlineKind::Keyboard(_)
            | InlineKind::Button(_)
            | InlineKind::Menu(_)
            | InlineKind::Autolink(_)
            | InlineKind::Pass(_)
            | InlineKind::Stem(_) => None,
        }
    }
}

impl<'doc> AnchorRef<'doc> {
    /// The same ID as [`Self::id`], with its recorded location and explicit origin.
    #[must_use]
    pub fn element_id(self) -> ElementIdRef<'doc> {
        ElementIdRef::from(self.node)
    }
}
