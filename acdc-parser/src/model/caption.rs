//! Caption behavior resolved from block and document attributes.
//!
//! A caption is a property of a parsed block, so the parser resolves it while it still holds
//! the document attributes in effect at the block's source position, then assigns the ordinals
//! in one post-order pass over the finished tree. Converters read the result and never re-derive
//! the precedence chain.

use std::{borrow::Cow, num::NonZeroU32};

use super::{AttributeValue, Block, BlockMetadata, DelimitedBlockType, DocumentAttributes, Table};

/// The caption a titled block takes.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Caption<'a> {
    /// A type-specific caption and its document-wide ordinal.
    ///
    /// `kind` names the counter the ordinal came from. `number` is `Some` only for a block that
    /// had a title when ordinals were assigned; such a block consumed one ordinal from that
    /// counter. It is `None` for a block that had no title, which consumed nothing — a consumer
    /// that adds a title later either supplies its own ordinal or calls
    /// [`Document::renumber_captions`](super::Document::renumber_captions).
    Numbered {
        /// The counter this caption draws its ordinal from.
        kind: CaptionKind,
        /// The caption label, e.g. `Example`. Empty when the attribute is set with no value.
        label: Cow<'a, str>,
        /// The document-wide ordinal, or `None` when the block had no title. Kept narrow
        /// because it sits on every block's metadata.
        number: Option<NonZeroU32>,
    },
    /// An explicit prefix, which takes no ordinal.
    Custom(Cow<'a, str>),
    /// Caption-capable, but no prefix applies.
    Unnumbered,
}

/// A block category with its own caption attribute and counter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum CaptionKind {
    /// An example block or paragraph controlled by `example-caption`.
    Example,
    /// A block image controlled by `figure-caption`.
    Figure,
    /// A listing or source block controlled by `listing-caption`.
    Listing,
    /// A table controlled by `table-caption`.
    Table,
}

impl CaptionKind {
    const fn attribute_name(self) -> &'static str {
        match self {
            Self::Example => "example-caption",
            Self::Figure => "figure-caption",
            Self::Listing => "listing-caption",
            Self::Table => "table-caption",
        }
    }

    /// The caption category for a parsed block, or `None` when the block takes no caption.
    ///
    /// Classification follows the block's effective context rather than its AST variant alone,
    /// matching asciidoctor: a delimiter that carries a caption context of its own keeps it, so
    /// `[listing]` on `====` is still an example. A style only changes the context of the
    /// verbatim delimiters and of the open block, which carry none.
    pub(crate) fn for_block(block: &Block<'_>) -> Option<Self> {
        match block {
            Block::DelimitedBlock(delimited) => {
                Self::for_delimited(&delimited.inner, delimited.metadata.style)
            }
            Block::Paragraph(paragraph) => Self::for_style(paragraph.metadata.style),
            Block::Image(_) => Some(Self::Figure),
            Block::Section(_)
            | Block::Admonition(_)
            | Block::DiscreteHeader(_)
            | Block::PageBreak(_)
            | Block::ThematicBreak(_)
            | Block::TableOfContents(_)
            | Block::UnorderedList(_)
            | Block::OrderedList(_)
            | Block::CalloutList(_)
            | Block::DescriptionList(_)
            | Block::Audio(_)
            | Block::Video(_)
            | Block::DocumentAttribute(_)
            | Block::Comment(_) => None,
        }
    }

    /// The caption category a `[style]` names on a paragraph, or on a delimiter that carries
    /// no context of its own.
    ///
    /// Parsed blocks already carry their caption in [`BlockMetadata::caption`]; this is for a
    /// consumer classifying a block it built itself.
    #[must_use]
    pub fn for_style(style: Option<&str>) -> Option<Self> {
        match style {
            Some("example") => Some(Self::Example),
            Some("listing" | "source") => Some(Self::Listing),
            _ => None,
        }
    }

    /// The caption category of a delimited block, from its delimiter and style.
    ///
    /// Parsed blocks already carry their caption in [`BlockMetadata::caption`]; this is for a
    /// consumer classifying a block it built itself.
    #[must_use]
    pub fn for_delimited(inner: &DelimitedBlockType<'_>, style: Option<&str>) -> Option<Self> {
        match inner {
            // These delimiters carry their own caption context, whatever the style says.
            DelimitedBlockType::DelimitedExample(_) => Some(Self::Example),
            DelimitedBlockType::DelimitedTable(_) => Some(Self::Table),
            // A verbatim block is a listing unless the style says otherwise: `[literal]` on
            // `----` takes no caption, and `[listing]`/`[source]` on `....` takes a listing one.
            DelimitedBlockType::DelimitedListing(_) => {
                Self::for_verbatim(style, Some(Self::Listing))
            }
            DelimitedBlockType::DelimitedLiteral(_) => Self::for_verbatim(style, None),
            // An open block has no caption context, so the style supplies one.
            DelimitedBlockType::DelimitedOpen(_) => Self::for_style(style),
            // Sidebar, quote, verse, passthrough, comment and stem take no caption.
            DelimitedBlockType::DelimitedSidebar(_)
            | DelimitedBlockType::DelimitedQuote(_)
            | DelimitedBlockType::DelimitedVerse(_)
            | DelimitedBlockType::DelimitedPass(_)
            | DelimitedBlockType::DelimitedComment(_)
            | DelimitedBlockType::DelimitedStem(_) => None,
        }
    }

    /// The context of a verbatim block, where only `literal`, `listing` and `source` override
    /// the delimiter's own `default`.
    fn for_verbatim(style: Option<&str>, default: Option<Self>) -> Option<Self> {
        match style {
            Some("listing" | "source") => Some(Self::Listing),
            Some("literal") => None,
            _ => default,
        }
    }
}

/// Where a caption prefix comes from.
///
/// The inner `None` is an attribute set with no value, whose label is empty.
enum CaptionSource<'value, 'a> {
    /// An explicit prefix, from the block's `caption=` or the document's `caption`.
    Custom(Option<&'value Cow<'a, str>>),
    /// A type-specific label, from `<kind>-caption`.
    Numbered(Option<&'value Cow<'a, str>>),
    /// No caption applies.
    None,
}

/// Collapsible examples take an empty custom caption. Other blocks use the block's own
/// `caption=`, then the document-wide `caption`, then `<kind>-caption`. Any value found in the
/// first two positions is used verbatim and takes no ordinal, matching asciidoctor — including
/// an empty one.
fn caption_source<'value, 'a>(
    metadata: &'value BlockMetadata<'a>,
    attributes: &'value DocumentAttributes<'a>,
    kind: CaptionKind,
) -> CaptionSource<'value, 'a> {
    if kind == CaptionKind::Example && metadata.options.contains(&"collapsible") {
        return CaptionSource::Custom(None);
    }

    // Only a value counts: `caption=` gives an empty custom caption, but the bare marker in
    // `[listing,caption]` is a stray positional that asciidoctor ignores, and it reaches here
    // as `AttributeValue::None`.
    match metadata.attributes.get("caption") {
        Some(AttributeValue::String(value)) => return CaptionSource::Custom(Some(value)),
        Some(AttributeValue::Bool(_) | AttributeValue::None) | None => {}
    }
    match attributes.get("caption") {
        Some(AttributeValue::String(value)) => return CaptionSource::Custom(Some(value)),
        Some(AttributeValue::Bool(true)) => return CaptionSource::Custom(None),
        Some(AttributeValue::Bool(false) | AttributeValue::None) | None => {}
    }
    match attributes.get(kind.attribute_name()) {
        Some(AttributeValue::String(value)) => CaptionSource::Numbered(Some(value)),
        Some(AttributeValue::Bool(true)) => CaptionSource::Numbered(None),
        Some(AttributeValue::Bool(false) | AttributeValue::None) | None => CaptionSource::None,
    }
}

/// A label is stored verbatim. A document attribute's value is literal text — asciidoctor
/// renders `:example-caption: 'Sample'` as `'Sample' 1.` — and the block-attribute parser has
/// already removed the syntactic quotes around an element value.
fn borrowed_label<'a>(value: Option<&Cow<'a, str>>) -> Cow<'a, str> {
    value.cloned().unwrap_or(Cow::Borrowed(""))
}

fn owned_label(value: Option<&Cow<'_, str>>) -> Cow<'static, str> {
    value.map_or(Cow::Borrowed(""), |value| Cow::Owned(value.to_string()))
}

impl<'a> Caption<'a> {
    /// Resolve caption behavior from the block's own attributes and the document attributes in
    /// effect at the block's source position. Sets no ordinal.
    #[must_use]
    pub fn resolve(
        metadata: &BlockMetadata<'a>,
        attributes: &DocumentAttributes<'a>,
        kind: CaptionKind,
    ) -> Self {
        match caption_source(metadata, attributes, kind) {
            CaptionSource::Custom(value) => Self::Custom(borrowed_label(value)),
            CaptionSource::Numbered(value) => Self::Numbered {
                kind,
                label: borrowed_label(value),
                number: None,
            },
            CaptionSource::None => Self::Unnumbered,
        }
    }
}

impl Caption<'static> {
    /// [`Caption::resolve`] for metadata and attributes whose lifetimes are unrelated, as a
    /// converter's are: its stored attributes are independent of the borrowed document. Owns its
    /// strings, and sets no ordinal.
    #[must_use]
    pub fn resolve_owned(
        metadata: &BlockMetadata<'_>,
        attributes: &DocumentAttributes<'_>,
        kind: CaptionKind,
    ) -> Self {
        match caption_source(metadata, attributes, kind) {
            CaptionSource::Custom(value) => Self::Custom(owned_label(value)),
            CaptionSource::Numbered(value) => Self::Numbered {
                kind,
                label: owned_label(value),
                number: None,
            },
            CaptionSource::None => Self::Unnumbered,
        }
    }
}

/// One counter per [`CaptionKind`].
#[derive(Debug, Default)]
struct CaptionCounters {
    example: u32,
    figure: u32,
    listing: u32,
    table: u32,
}

impl CaptionCounters {
    fn next(&mut self, kind: CaptionKind) -> Option<NonZeroU32> {
        let counter = match kind {
            CaptionKind::Example => &mut self.example,
            CaptionKind::Figure => &mut self.figure,
            CaptionKind::Listing => &mut self.listing,
            CaptionKind::Table => &mut self.table,
        };
        *counter = counter.saturating_add(1);
        NonZeroU32::new(*counter)
    }
}

/// Assign document-wide caption ordinals in the order asciidoctor assigns them: a block's
/// content is numbered before the block itself.
///
/// Every automatic ordinal is written on every run — `Some(n)` for a titled block and `None`
/// otherwise — so the pass is idempotent and clears ordinals left stale by a mutation that
/// removed a title or a block.
pub(crate) fn renumber_captions(blocks: &mut [Block<'_>]) {
    let mut counters = CaptionCounters::default();
    renumber_blocks(blocks, &mut counters);
}

/// The highest ordinal assigned to a caption of `kind` anywhere in `blocks`, or 0 when none
/// carries one. A converter numbering a block the parser could not — a caller-built one, or a
/// parsed one that gained its title afterwards — starts past this so it cannot collide.
pub(crate) fn highest_caption_number(blocks: &[Block<'_>], kind: CaptionKind) -> u32 {
    let mut highest = 0;
    visit_captions(blocks, &mut |caption| {
        if let Caption::Numbered {
            kind: caption_kind,
            number: Some(number),
            ..
        } = caption
            && *caption_kind == kind
        {
            highest = highest.max(number.get());
        }
    });
    highest
}

fn visit_captions(blocks: &[Block<'_>], visit: &mut impl FnMut(&Caption<'_>)) {
    for block in blocks {
        if let Some(caption) = block
            .metadata()
            .and_then(|metadata| metadata.caption.as_ref())
        {
            visit(caption);
        }
        match block {
            Block::Section(section) => visit_captions(&section.content, visit),
            Block::Admonition(admonition) => visit_captions(&admonition.blocks, visit),
            Block::UnorderedList(list) => {
                for item in &list.items {
                    visit_captions(&item.blocks, visit);
                }
            }
            Block::OrderedList(list) => {
                for item in &list.items {
                    visit_captions(&item.blocks, visit);
                }
            }
            Block::CalloutList(list) => {
                for item in &list.items {
                    visit_captions(&item.blocks, visit);
                }
            }
            Block::DescriptionList(list) => {
                for item in &list.items {
                    visit_captions(&item.description, visit);
                }
            }
            Block::DelimitedBlock(delimited) => match &delimited.inner {
                DelimitedBlockType::DelimitedExample(blocks)
                | DelimitedBlockType::DelimitedOpen(blocks)
                | DelimitedBlockType::DelimitedSidebar(blocks)
                | DelimitedBlockType::DelimitedQuote(blocks) => visit_captions(blocks, visit),
                DelimitedBlockType::DelimitedTable(table) => {
                    for row in table
                        .header
                        .iter()
                        .chain(table.rows.iter())
                        .chain(table.footer.iter())
                    {
                        for column in &row.columns {
                            visit_captions(&column.content, visit);
                        }
                    }
                }
                DelimitedBlockType::DelimitedListing(_)
                | DelimitedBlockType::DelimitedLiteral(_)
                | DelimitedBlockType::DelimitedVerse(_)
                | DelimitedBlockType::DelimitedPass(_)
                | DelimitedBlockType::DelimitedComment(_)
                | DelimitedBlockType::DelimitedStem(_) => {}
            },
            Block::Paragraph(_)
            | Block::DiscreteHeader(_)
            | Block::PageBreak(_)
            | Block::ThematicBreak(_)
            | Block::TableOfContents(_)
            | Block::Image(_)
            | Block::Audio(_)
            | Block::Video(_)
            | Block::DocumentAttribute(_)
            | Block::Comment(_) => {}
        }
    }
}

fn renumber_blocks(blocks: &mut [Block<'_>], counters: &mut CaptionCounters) {
    for block in blocks {
        renumber_block(block, counters);
    }
}

fn renumber_block(block: &mut Block<'_>, counters: &mut CaptionCounters) {
    renumber_children(block, counters);

    let titled = block.title().is_some();
    if let Some(metadata) = block.metadata_mut()
        && let Some(Caption::Numbered { kind, number, .. }) = &mut metadata.caption
    {
        *number = titled.then(|| counters.next(*kind)).flatten();
    }
}

/// Number a block's content before the block itself. The container coverage mirrors
/// `collect_references`, but the table walk follows source order (header, rows, footer) rather
/// than that function's header/footer/rows order.
fn renumber_children(block: &mut Block<'_>, counters: &mut CaptionCounters) {
    match block {
        Block::Section(section) => renumber_blocks(&mut section.content, counters),
        Block::Admonition(admonition) => renumber_blocks(&mut admonition.blocks, counters),
        Block::UnorderedList(list) => {
            for item in &mut list.items {
                renumber_blocks(&mut item.blocks, counters);
            }
        }
        Block::OrderedList(list) => {
            for item in &mut list.items {
                renumber_blocks(&mut item.blocks, counters);
            }
        }
        Block::CalloutList(list) => {
            for item in &mut list.items {
                renumber_blocks(&mut item.blocks, counters);
            }
        }
        Block::DescriptionList(list) => {
            for item in &mut list.items {
                renumber_blocks(&mut item.description, counters);
            }
        }
        Block::DelimitedBlock(delimited) => match &mut delimited.inner {
            DelimitedBlockType::DelimitedExample(blocks)
            | DelimitedBlockType::DelimitedOpen(blocks)
            | DelimitedBlockType::DelimitedSidebar(blocks)
            | DelimitedBlockType::DelimitedQuote(blocks) => renumber_blocks(blocks, counters),
            DelimitedBlockType::DelimitedTable(table) => renumber_table(table, counters),
            DelimitedBlockType::DelimitedListing(_)
            | DelimitedBlockType::DelimitedLiteral(_)
            | DelimitedBlockType::DelimitedVerse(_)
            | DelimitedBlockType::DelimitedPass(_)
            | DelimitedBlockType::DelimitedComment(_)
            | DelimitedBlockType::DelimitedStem(_) => {}
        },
        Block::Paragraph(_)
        | Block::DiscreteHeader(_)
        | Block::PageBreak(_)
        | Block::ThematicBreak(_)
        | Block::TableOfContents(_)
        | Block::Image(_)
        | Block::Audio(_)
        | Block::Video(_)
        | Block::DocumentAttribute(_)
        | Block::Comment(_) => {}
    }
}

fn renumber_table(table: &mut Table<'_>, counters: &mut CaptionCounters) {
    for row in table
        .header
        .iter_mut()
        .chain(table.rows.iter_mut())
        .chain(table.footer.iter_mut())
    {
        for column in &mut row.columns {
            renumber_blocks(&mut column.content, counters);
        }
    }
}
