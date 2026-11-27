//! The data models for the `AsciiDoc` document.
use std::{collections::HashMap, fmt::Display, str::FromStr, string::ToString};

use bumpalo::Bump;
use serde::{
    Serialize,
    ser::{SerializeMap, Serializer},
};

mod admonition;
mod anchor;
pub(crate) use anchor::{
    BibliographyLabel, DocumentReferenceLabel, SectionOccurrence, SectionReference,
    displayed_reference_label,
};
mod attributes;
mod attribution;
mod boundaries;
pub(crate) mod caption;
pub(crate) mod content_attribute;
mod delimited;
mod inlines;
mod lists;
mod location;
mod media;
mod metadata;
pub(crate) mod section;
pub(crate) mod substitution;
mod tables;
mod title;
mod token_assignment;

pub use admonition::{Admonition, AdmonitionForm, AdmonitionVariant};
pub use anchor::{Anchor, ReferenceTarget, TocEntry, UNNUMBERED_SECTION_STYLES};
pub(crate) use attributes::RawAttributes;
pub(crate) use attributes::{
    AttributeInlineFragment, AttributeInlineSource, AttributeTextFragment, DocumentAttributeStatus,
    canonical_attribute_name, default_document_attribute_values,
};
pub use attributes::{
    AttributeName, AttributeValue, DocumentAttributeAssignment, DocumentAttributeValue,
    DocumentAttributes, ElementAttributes, MAX_SECTION_LEVELS, MAX_TOC_LEVELS,
};
pub use attribution::{Attribution, CiteTitle};
pub use caption::{Caption, CaptionKind};
pub use content_attribute::ContentAttribute;
pub(crate) use delimited::{Delimited, compound_block, inline_body_block, verbatim_block};
pub use delimited::{
    DelimitedComment, Example, Listing, Literal, Open, Passthrough, Quote, Sidebar, StemBlock,
    Table, Verse,
};
pub use inlines::*;
pub(crate) use lists::ListLevel;
pub use lists::{
    CalloutList, CalloutListItem, DescriptionList, DescriptionListDefinition, DescriptionListItem,
    DescriptionListTerm, ListItem, ListItemCheckedStatus, OrderedList, UnorderedList,
};
pub use location::*;
pub use media::{Audio, Image, InlineImage, Source, SourceUrl, Video};
pub(crate) use metadata::{
    AttributePosition, BlockPrefixEvents, MacroAttributes, PositionalAttribute,
};
pub use metadata::{BlockMetadata, ElementId, InlineMetadata, ReservedAttribute, Role};
pub use section::*;
pub use substitution::*;
pub(crate) use tables::TableContent;
pub use tables::{
    ColumnFormat, ColumnStyle, ColumnWidth, HorizontalAlignment, TableCell, TableFrame, TableGrid,
    TablePresentation, TableRow, TableStripes, VerticalAlignment,
};
pub use title::{Subtitle, Title};
pub use token_assignment::TokenAssignment;

/// An editable `AsciiDoc` document tree.
///
/// Edits do not reparse content or rebuild reference, TOC, or footnote catalogs.
/// [`Self::renumber_sections`] and [`Self::renumber_captions`] update only their
/// documented numbering state. Use [`Self::rebuild_references_and_toc`] or
/// [`Self::rebuild_captions`] to refresh the corresponding prepared catalogs.
/// Use [`Self::rebuild_footnotes`] for prepared note registrations and
/// [`Self::rebuild_author_attributes`] for author-derived header assignments.
/// When composing repairs after structural edits, rebuild footnotes first,
/// references and TOC second, and captions last. Each repair is atomic on error;
/// the sequence is not a transaction.
/// Reparse source to recompute declaration-order attribute snapshots.
#[derive(Default, Debug, PartialEq)]
#[non_exhaustive]
pub struct Document<'a> {
    pub(crate) has_inline_macros: bool,
    pub(crate) generated_displays: crate::extensions::DisplayState<'a>,
    pub(crate) header: Option<Header<'a>>,
    /// Span of the leading attribute entries and comments of a document
    /// without a title. A titled header records its span in [`Header`].
    /// Not serialized.
    pub(crate) untitled_header_location: Option<Location>,
    /// Effective end-of-header values. Later accepted changes are ordered
    /// [`Block::DocumentAttribute`] nodes and do not update this snapshot.
    pub(crate) attributes: DocumentAttributes<'a>,
    pub(crate) blocks: Vec<Block<'a>>,
    pub(crate) footnotes: Vec<FootnoteDefinition<'a>>,
    pub(crate) toc_entries: Vec<TocEntry<'a>>,
    /// Cross-reference targets keyed by id, for O(1) `<<id>>` resolution.
    /// Covers sections, blocks, explicit inline anchors, and IDs on formatted
    /// inline spans; collected during parsing. `toc_entries` remains the ordered
    /// list used to render the table of contents. An empty key supplies the
    /// document title or label when a reference addresses the document top.
    /// Like `toc_entries` and
    /// `footnotes`, this is not serialized.
    pub(crate) references: HashMap<&'a str, ReferenceTarget<'a>>,
    pub(crate) location: Location,
}

impl<'a> Document<'a> {
    /// Replace the header without updating author-derived document attributes.
    pub fn set_header(&mut self, header: Option<Header<'a>>) {
        self.header = header;
    }

    /// Remove and return the header without updating derived attributes.
    #[must_use]
    pub fn take_header(&mut self) -> Option<Header<'a>> {
        self.header.take()
    }

    /// Replace the end-of-header snapshot without reinterpreting stored content.
    pub fn set_attributes(&mut self, attributes: DocumentAttributes<'a>) {
        self.attributes = attributes;
    }

    /// Remove the end-of-header snapshot, leaving empty document attributes.
    #[must_use]
    pub fn take_attributes(&mut self) -> DocumentAttributes<'a> {
        std::mem::take(&mut self.attributes)
    }

    /// Replace the block tree without rebuilding registrations or numbering.
    pub fn set_blocks(&mut self, blocks: impl IntoIterator<Item = Block<'a>>) {
        self.blocks = blocks.into_iter().collect();
    }

    /// Append a block without rebuilding registrations or numbering.
    pub fn push_block(&mut self, block: Block<'a>) {
        self.blocks.push(block);
    }

    /// Remove the block tree, retaining the source lifetime of each owned block.
    pub fn take_blocks(
        &mut self,
    ) -> impl DoubleEndedIterator<Item = Block<'a>> + ExactSizeIterator + use<'a> {
        std::mem::take(&mut self.blocks).into_iter()
    }

    /// Replace prepared footnote registrations without changing tree occurrences.
    pub fn set_footnotes(&mut self, footnotes: impl IntoIterator<Item = FootnoteDefinition<'a>>) {
        self.footnotes = footnotes.into_iter().collect();
    }

    /// Remove footnote registrations without changing tree occurrences.
    pub fn take_footnotes(
        &mut self,
    ) -> impl DoubleEndedIterator<Item = FootnoteDefinition<'a>> + ExactSizeIterator + use<'a> {
        std::mem::take(&mut self.footnotes).into_iter()
    }

    /// Replace prepared TOC entries without changing the section tree.
    pub fn set_toc_entries(&mut self, entries: impl IntoIterator<Item = TocEntry<'a>>) {
        self.toc_entries = entries.into_iter().collect();
    }

    /// Remove TOC entries without changing the section tree.
    pub fn take_toc_entries(
        &mut self,
    ) -> impl DoubleEndedIterator<Item = TocEntry<'a>> + ExactSizeIterator + use<'a> {
        std::mem::take(&mut self.toc_entries).into_iter()
    }

    /// Replace prepared reference snapshots without registering tree targets.
    ///
    /// If a key occurs more than once, the last value wins. Stored title presence
    /// and source identity are preserved; catalog coherence is not checked.
    pub fn set_references(
        &mut self,
        references: impl IntoIterator<Item = (&'a str, ReferenceTarget<'a>)>,
    ) {
        self.references = references.into_iter().collect();
    }

    /// Remove reference snapshots without changing target elements.
    pub fn take_references(
        &mut self,
    ) -> impl ExactSizeIterator<Item = (&'a str, ReferenceTarget<'a>)> + use<'a> {
        std::mem::take(&mut self.references).into_iter()
    }

    /// Replace the document span without relocating its children or catalogs.
    pub fn set_location(&mut self, location: Location) {
        self.location = location;
    }

    /// Reassign section numbers from the current section tree.
    ///
    /// Parsing does this automatically. Call it after changing the section tree or a
    /// section's numbering policy. Existing cross-reference targets and
    /// table-of-contents entries receive the new numbers, but this method does not
    /// add, remove, or reorder those entries.
    ///
    /// Automatic cross-references select the signifier for the target's current
    /// category from their original source-position attributes. Explicit
    /// [`CrossReference::set_signifier`] overrides are preserved.
    pub fn renumber_sections(&mut self) {
        let is_book = matches!(
            self.attributes.get("doctype"),
            Some(value) if value.as_str() == Some("book")
        );
        section::renumber_sections(
            &mut self.blocks,
            &mut self.toc_entries,
            &mut self.references,
            is_book,
        );
        let section_names = self
            .references
            .iter()
            .filter_map(|(target, reference)| Some((*target, reference.section.as_ref()?.name)))
            .collect::<HashMap<_, _>>();
        crate::grammar::walk_document_inline_nodes_mut(self, &mut |inline| {
            if let InlineNode::CrossReference(xref) = inline {
                xref.refresh_signifier(section_names.get(xref.target).copied());
            }
        });
    }

    /// Reassign every automatic caption ordinal, numbering a block's content before the block
    /// itself as asciidoctor does.
    ///
    /// Parsing does this already. Call it after reordering or removing blocks, changing titles,
    /// or inserting a block whose `metadata.caption` has been set. A caller-built block whose
    /// `metadata.caption` is `None` stays unnumbered: nothing knows the document attributes at
    /// a source position it never had, so a converter's own fallback owns it.
    /// Use [`Self::rebuild_captions`] to refresh reference caption snapshots too.
    pub fn renumber_captions(&mut self) {
        caption::renumber_captions(&mut self.blocks);
    }

    /// The highest caption ordinal assigned to any block of `kind`, or 0 when none carries one.
    ///
    /// A converter that renders a title the parser never numbered — one on a caller-built
    /// block, or added after parsing — should start its own counter here, so its numbers
    /// cannot collide with the parser's.
    #[must_use]
    pub fn highest_caption_number(&self, kind: CaptionKind) -> u32 {
        caption::highest_caption_number(&self.blocks, kind)
    }
}

/// A `Header` represents the header of a document with a title.
///
/// A document without a title can still have header attribute entries; read
/// their span with [`crate::ast::DocumentRef::header_location`].
///
/// The header contains the title, subtitle, authors, and optional metadata
/// (such as ID and roles) that can be applied to the document title.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct Header<'a> {
    #[serde(skip_serializing_if = "BlockMetadata::is_empty_for_serialization")]
    pub(crate) metadata: BlockMetadata<'a>,
    #[serde(skip_serializing_if = "Title::is_empty")]
    pub(crate) title: Title<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) subtitle: Option<Subtitle<'a>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) authors: Vec<Author<'a>>,
    pub(crate) location: Location,
}

/// An `Author` represents the author of a document.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct Author<'a> {
    #[serde(rename = "firstname")]
    pub(crate) first_name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none", rename = "middlename")]
    pub(crate) middle_name: Option<&'a str>,
    #[serde(rename = "lastname")]
    pub(crate) last_name: &'a str,
    pub(crate) initials: &'a str,
    #[serde(skip_serializing_if = "Option::is_none", rename = "address")]
    pub(crate) email: Option<&'a str>,
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

    /// Replace metadata without shadowing dedicated fields.
    ///
    /// # Errors
    /// Rejects scalar attributes for dedicated metadata or title fields.
    pub fn with_metadata(
        mut self,
        metadata: BlockMetadata<'a>,
    ) -> Result<Self, crate::ReservedAttribute> {
        self.set_metadata(metadata)?;
        Ok(self)
    }

    /// Set the subtitle.
    #[must_use]
    pub fn with_subtitle(mut self, subtitle: Subtitle<'a>) -> Self {
        self.set_subtitle(Some(subtitle));
        self
    }

    /// Set the authors.
    #[must_use]
    pub fn with_authors(mut self, authors: Vec<Author<'a>>) -> Self {
        self.set_authors(authors);
        self
    }
    /// Replace metadata without shadowing the header title or common metadata.
    ///
    /// # Errors
    /// Rejects scalar attributes for dedicated fields, leaving the header unchanged.
    pub fn set_metadata(
        &mut self,
        metadata: BlockMetadata<'a>,
    ) -> Result<(), crate::ReservedAttribute> {
        metadata.validate_attributes(&["title"])?;
        self.metadata = metadata;
        Ok(())
    }

    /// Copy metadata to prepare a checked replacement, retaining its source lifetime.
    #[must_use]
    pub fn clone_metadata(&self) -> BlockMetadata<'a> {
        self.metadata.clone()
    }

    /// Remove metadata without changing title, subtitle, or authors.
    #[must_use]
    pub fn take_metadata(&mut self) -> BlockMetadata<'a> {
        std::mem::take(&mut self.metadata)
    }

    /// Replace the title, including an explicitly empty value.
    pub fn set_title(&mut self, title: Title<'a>) {
        self.title = title;
    }

    /// Remove the title, leaving it absent.
    #[must_use]
    pub fn take_title(&mut self) -> Title<'a> {
        std::mem::take(&mut self.title)
    }

    /// Replace the optional subtitle. `Some` retains explicit empty content.
    pub fn set_subtitle(&mut self, subtitle: Option<Subtitle<'a>>) {
        self.subtitle = subtitle;
    }

    /// Remove and return the optional subtitle.
    #[must_use]
    pub fn take_subtitle(&mut self) -> Option<Subtitle<'a>> {
        self.subtitle.take()
    }

    /// Replace authors without updating author-derived document attributes.
    pub fn set_authors(&mut self, authors: impl IntoIterator<Item = Author<'a>>) {
        self.authors = authors.into_iter().collect();
    }

    /// Remove authors, retaining each author's source lifetime.
    pub fn take_authors(
        &mut self,
    ) -> impl DoubleEndedIterator<Item = Author<'a>> + ExactSizeIterator + use<'a> {
        std::mem::take(&mut self.authors).into_iter()
    }

    /// Replace the header span without relocating its content or metadata.
    pub fn set_location(&mut self, location: Location) {
        self.location = location;
    }
}

impl<'a> Author<'a> {
    /// Replace the prepared first name without normalization.
    pub fn set_first_name(&mut self, first_name: &'a str) {
        self.first_name = first_name;
    }

    /// Replace or remove the prepared middle name without normalization.
    pub fn set_middle_name(&mut self, middle_name: Option<&'a str>) {
        self.middle_name = middle_name;
    }

    /// Replace the prepared last name without normalization.
    pub fn set_last_name(&mut self, last_name: &'a str) {
        self.last_name = last_name;
    }

    /// Replace prepared initials without recomputing them from the name.
    pub fn set_initials(&mut self, initials: &'a str) {
        self.initials = initials;
    }

    /// Replace or remove the email address without updating document attributes.
    pub fn set_email(&mut self, email: Option<&'a str>) {
        self.email = email;
    }

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
        self.set_email(Some(email));
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

/// The syntactic form a [`Comment`] originated from.
///
/// A `[comment]`-styled `--` block is represented as a
/// [`DelimitedComment`] (distinguished from a `////` block
/// by its retained `comment` style), so it is not covered here.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum CommentKind {
    /// A `//` single-line comment.
    #[default]
    Line,
    /// A `[comment]`-styled paragraph.
    Paragraph,
}

/// A comment in a document that produces no output.
///
/// Either a `//` single-line comment or a `[comment]`-styled paragraph; the
/// [`ast::CommentRef::kind`](crate::ast::CommentRef::kind) distinguishes them.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Comment<'a> {
    pub(crate) kind: CommentKind,
    pub(crate) content: &'a str,
    pub(crate) location: Location,
    // Styled paragraph comments must retain preceding source events without metadata.
    pub(crate) prefix_events: Option<Box<BlockPrefixEvents<'a>>>,
}

/// A `Block` represents a block in a document.
///
/// A block is a structural element in a document that can contain other blocks.
/// Variants match [`crate::ast::BlockKind`] one to one.
// The variant sizes differ widely by design: delimited blocks and `Image` are large
// value types while `PageBreak`/`Comment` are tiny. Boxing the large variants is a
// separate refactor; the disparity is structural, not a per-node-allocation concern.
#[allow(clippy::large_enum_variant)]
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Block<'a> {
    // TODO(nlopes): we shouldn't have an admonition type here, instead it should be
    // picked up from the style attribute from the block metadata.
    //
    // The main one that would need changing is the Paragraph and the Delimited Example
    // blocks, where we currently use this but don't need to.
    Admonition(Admonition<'a>),
    Audio(Audio<'a>),
    CalloutList(CalloutList<'a>),
    Comment(Comment<'a>),
    DelimitedComment(DelimitedComment<'a>),
    DelimitedExample(Example<'a>),
    DelimitedListing(Listing<'a>),
    DelimitedLiteral(Literal<'a>),
    DelimitedOpen(Open<'a>),
    DelimitedPassthrough(Passthrough<'a>),
    DelimitedQuote(Quote<'a>),
    DelimitedSidebar(Sidebar<'a>),
    DelimitedStem(StemBlock<'a>),
    DelimitedTable(Table<'a>),
    DelimitedVerse(Verse<'a>),
    DescriptionList(DescriptionList<'a>),
    DiscreteHeading(DiscreteHeading<'a>),
    DocumentAttribute(DocumentAttribute<'a>),
    Extension(Box<crate::extensions::ExtensionBlock<'a>>),
    Image(Image<'a>),
    OrderedList(OrderedList<'a>),
    PageBreak(PageBreak<'a>),
    Paragraph(Paragraph<'a>),
    Section(Section<'a>),
    TableOfContents(TableOfContents<'a>),
    ThematicBreak(ThematicBreak<'a>),
    UnorderedList(UnorderedList<'a>),
    Video(Video<'a>),
}

impl<'a> Block<'a> {
    /// The source location of this block.
    #[must_use]
    pub fn location(&self) -> &Location {
        match self {
            Block::Extension(e) => &e.location,
            Block::Section(s) => &s.location,
            Block::Paragraph(p) => &p.location,
            Block::UnorderedList(l) => &l.location,
            Block::OrderedList(l) => &l.location,
            Block::DescriptionList(l) => &l.location,
            Block::CalloutList(l) => &l.location,
            crate::model::compound_block!(d) => &d.location,
            crate::model::inline_body_block!(d) => &d.location,
            Block::DelimitedStem(crate::model::StemBlock(d)) => &d.location,
            Block::DelimitedTable(crate::model::Table(d)) => &d.location,
            Block::Admonition(a) => &a.location,
            Block::TableOfContents(t) => &t.location,
            Block::DiscreteHeading(h) => &h.location,
            Block::DocumentAttribute(a) => &a.location,
            Block::ThematicBreak(tb) => &tb.location,
            Block::PageBreak(pb) => &pb.location,
            Block::Image(i) => &i.location,
            Block::Audio(a) => &a.location,
            Block::Video(v) => &v.location,
            Block::Comment(c) => &c.location,
        }
    }

    /// Copy this block's metadata without copying its title or content.
    ///
    /// Use [`crate::ast::BlockRef::attributes`] to read attributes without a copy.
    ///
    /// ```compile_fail,E0624
    /// use acdc_parser::{Location, edit::{Block, Paragraph}};
    /// let block = Block::Paragraph(Paragraph::new(Vec::new(), Location::default()));
    /// block.metadata();
    /// ```
    #[must_use]
    pub fn clone_metadata(&self) -> Option<BlockMetadata<'a>> {
        self.metadata().cloned()
    }

    /// This block's metadata, for the blocks that carry any.
    #[must_use]
    pub(crate) fn metadata(&self) -> Option<&BlockMetadata<'a>> {
        match self {
            Block::Extension(e) => Some(&e.metadata),
            Block::Section(block) => Some(&block.metadata),
            crate::model::compound_block!(block) => Some(&block.metadata),
            crate::model::inline_body_block!(block) => Some(&block.metadata),
            Block::DelimitedStem(crate::model::StemBlock(block)) => Some(&block.metadata),
            Block::DelimitedTable(crate::model::Table(block)) => Some(&block.metadata),
            Block::Admonition(block) => Some(&block.metadata),
            Block::DiscreteHeading(block) => Some(&block.metadata),
            Block::PageBreak(block) => Some(&block.metadata),
            Block::Paragraph(block) => Some(&block.metadata),
            Block::Image(block) => Some(&block.metadata),
            Block::Audio(block) => Some(&block.metadata),
            Block::Video(block) => Some(&block.metadata),
            Block::UnorderedList(block) => Some(&block.metadata),
            Block::OrderedList(block) => Some(&block.metadata),
            Block::CalloutList(block) => Some(&block.metadata),
            Block::DescriptionList(block) => Some(&block.metadata),
            Block::TableOfContents(block) => Some(&block.metadata),
            Block::ThematicBreak(_) | Block::DocumentAttribute(_) | Block::Comment(_) => None,
        }
    }

    /// This block's metadata, for the blocks that carry any.
    pub(crate) fn metadata_mut(&mut self) -> Option<&mut BlockMetadata<'a>> {
        match self {
            Block::Extension(e) => Some(&mut e.metadata),
            Block::Section(block) => Some(&mut block.metadata),
            crate::model::compound_block!(block) => Some(&mut block.metadata),
            crate::model::verbatim_block!(block) => Some(&mut block.metadata),
            Block::DelimitedComment(crate::model::DelimitedComment(block)) => {
                Some(&mut block.metadata)
            }
            Block::DelimitedStem(crate::model::StemBlock(block)) => Some(&mut block.metadata),
            Block::DelimitedTable(crate::model::Table(block)) => Some(&mut block.metadata),
            Block::Admonition(block) => Some(&mut block.metadata),
            Block::DiscreteHeading(block) => Some(&mut block.metadata),
            Block::PageBreak(block) => Some(&mut block.metadata),
            Block::Paragraph(block) => Some(&mut block.metadata),
            Block::Image(block) => Some(&mut block.metadata),
            Block::Audio(block) => Some(&mut block.metadata),
            Block::Video(block) => Some(&mut block.metadata),
            Block::UnorderedList(block) => Some(&mut block.metadata),
            Block::OrderedList(block) => Some(&mut block.metadata),
            Block::CalloutList(block) => Some(&mut block.metadata),
            Block::DescriptionList(block) => Some(&mut block.metadata),
            Block::TableOfContents(block) => Some(&mut block.metadata),
            Block::ThematicBreak(_) | Block::DocumentAttribute(_) | Block::Comment(_) => None,
        }
    }

    /// The authored anchor selected as this block's cross-reference target.
    pub(crate) fn anchor(&self) -> Option<&Anchor<'a>> {
        match self {
            Block::Extension(e) => e.metadata.id_anchor(),
            Block::Section(b) => b.metadata.id_anchor(),
            crate::model::compound_block!(b) => b.metadata.id_anchor(),
            crate::model::inline_body_block!(b) => b.metadata.id_anchor(),
            Block::DelimitedStem(crate::model::StemBlock(b)) => b.metadata.id_anchor(),
            Block::DelimitedTable(crate::model::Table(b)) => b.metadata.id_anchor(),
            Block::Admonition(b) => b.metadata.id_anchor(),
            Block::DiscreteHeading(b) => b.metadata.id_anchor(),
            Block::PageBreak(b) => b.metadata.id_anchor(),
            Block::Paragraph(b) => b.metadata.id_anchor(),
            Block::Image(b) => b.metadata.id_anchor(),
            Block::Audio(b) => b.metadata.id_anchor(),
            Block::Video(b) => b.metadata.id_anchor(),
            Block::UnorderedList(b) => b.metadata.id_anchor(),
            Block::OrderedList(b) => b.metadata.id_anchor(),
            Block::CalloutList(b) => b.metadata.id_anchor(),
            Block::DescriptionList(b) => b.metadata.id_anchor(),
            Block::TableOfContents(b) => b.metadata.id_anchor(),
            Block::ThematicBreak(b) => b.primary_anchor(),
            Block::DocumentAttribute(_) | Block::Comment(_) => None,
        }
    }

    /// This block's title, if one was supplied (used as the cross-reference
    /// text when an `<<id>>` to this block has no explicit label).
    pub(crate) fn title(&self) -> Option<&Title<'a>> {
        let title = match self {
            Block::Extension(e) => &e.title,
            Block::Section(b) => &b.title,
            crate::model::compound_block!(b) => &b.title,
            crate::model::inline_body_block!(b) => &b.title,
            Block::DelimitedStem(crate::model::StemBlock(b)) => &b.title,
            Block::DelimitedTable(crate::model::Table(b)) => &b.title,
            Block::Admonition(b) => &b.title,
            Block::DiscreteHeading(b) => &b.title,
            Block::PageBreak(b) => &b.title,
            Block::Paragraph(b) => &b.title,
            Block::Image(b) => &b.title,
            Block::Audio(b) => &b.title,
            Block::Video(b) => &b.title,
            Block::UnorderedList(b) => &b.title,
            Block::OrderedList(b) => &b.title,
            Block::CalloutList(b) => &b.title,
            Block::DescriptionList(b) => &b.title,
            Block::ThematicBreak(b) => &b.title,
            Block::TableOfContents(_) | Block::DocumentAttribute(_) | Block::Comment(_) => {
                return None;
            }
        };
        title.is_present().then_some(title)
    }
}

impl Block<'_> {
    /// Mutable access to this block's own location (the post-parse source remap
    /// pass rewrites it). Counterpart to [`Self::location`].
    pub(crate) fn location_mut(&mut self) -> &mut Location {
        match self {
            Block::Extension(e) => &mut e.location,
            Block::Section(s) => &mut s.location,
            Block::Paragraph(p) => &mut p.location,
            Block::UnorderedList(l) => &mut l.location,
            Block::OrderedList(l) => &mut l.location,
            Block::DescriptionList(l) => &mut l.location,
            Block::CalloutList(l) => &mut l.location,
            crate::model::compound_block!(d) => &mut d.location,
            crate::model::inline_body_block!(d) => &mut d.location,
            Block::DelimitedStem(crate::model::StemBlock(d)) => &mut d.location,
            Block::DelimitedTable(crate::model::Table(d)) => &mut d.location,
            Block::Admonition(a) => &mut a.location,
            Block::TableOfContents(t) => &mut t.location,
            Block::DiscreteHeading(h) => &mut h.location,
            Block::DocumentAttribute(a) => &mut a.location,
            Block::ThematicBreak(tb) => &mut tb.location,
            Block::PageBreak(pb) => &mut pb.location,
            Block::Image(i) => &mut i.location,
            Block::Audio(a) => &mut a.location,
            Block::Video(v) => &mut v.location,
            Block::Comment(c) => &mut c.location,
        }
    }
}

/// An accepted document-attribute assignment in the document body.
#[derive(Clone, PartialEq)]
#[non_exhaustive]
pub struct DocumentAttribute<'a> {
    pub(crate) name: AttributeName<'a>,
    pub(crate) location: Location,
    assignment: DocumentAttributeAssignment<'a>,
    accepted: bool,
}

impl<'a> DocumentAttribute<'a> {
    pub(crate) fn accepted(
        name: AttributeName<'a>,
        applied: DocumentAttributeAssignment<'a>,
        location: Location,
    ) -> Self {
        Self {
            name,
            location,
            assignment: applied,
            accepted: true,
        }
    }

    pub(crate) fn rejected(name: AttributeName<'a>, location: Location) -> Self {
        Self {
            name,
            location,
            assignment: DocumentAttributeAssignment::Unset,
            accepted: false,
        }
    }

    pub(crate) const fn is_accepted(&self) -> bool {
        self.accepted
    }

    /// Borrow the accepted set or unset assignment.
    #[must_use]
    pub const fn assignment(&self) -> &DocumentAttributeAssignment<'a> {
        &self.assignment
    }
}

impl std::fmt::Debug for DocumentAttribute<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DocumentAttribute")
            .field("name", &self.name)
            .field("assignment", &self.assignment)
            .field("location", &self.location)
            .field("accepted", &self.accepted)
            .finish()
    }
}

impl Serialize for DocumentAttribute<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(None)?;
        state.serialize_entry("name", &self.name)?;
        state.serialize_entry("type", "attribute")?;
        state.serialize_entry("value", &self.assignment.serialized_value())?;
        state.serialize_entry("location", &self.location)?;
        state.end()
    }
}

/// A heading outside the section hierarchy.
///
/// Discrete headings are useful for making headings inside of other blocks, like a
/// sidebar.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct DiscreteHeading<'a> {
    pub(crate) metadata: BlockMetadata<'a>,
    pub(crate) title: Title<'a>,
    pub(crate) level: u8,
    pub(crate) location: Location,
}

/// A `ThematicBreak` represents a thematic break in a document.
#[derive(Clone, Default, Debug, PartialEq)]
#[non_exhaustive]
pub struct ThematicBreak<'a> {
    pub(crate) anchors: Vec<Anchor<'a>>,
    pub(crate) title: Title<'a>,
    pub(crate) location: Location,
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
        if self.title.is_present() {
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
    pub(crate) title: Title<'a>,
    pub(crate) metadata: BlockMetadata<'a>,
    pub(crate) location: Location,
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
        if self.title.is_present() {
            state.serialize_entry("title", &self.title)?;
        }
        if !self.metadata.is_empty_for_serialization() {
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
        // Omit the default `Line` kind so plain `//` comments serialize unchanged.
        if self.kind != CommentKind::Line {
            state.serialize_entry("variant", &self.kind)?;
        }
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
    pub(crate) metadata: BlockMetadata<'a>,
    pub(crate) location: Location,
}

impl Serialize for TableOfContents<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(None)?;
        state.serialize_entry("name", "toc")?;
        state.serialize_entry("type", "block")?;
        if !self.metadata.is_empty_for_serialization() {
            state.serialize_entry("metadata", &self.metadata)?;
        }
        state.serialize_entry("location", &self.location)?;
        state.end()
    }
}

/// A `Paragraph` represents a paragraph in a document.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Paragraph<'a> {
    pub(crate) metadata: BlockMetadata<'a>,
    pub(crate) title: Title<'a>,
    pub(crate) content: Vec<InlineNode<'a>>,
    pub(crate) location: Location,
    pub(crate) source_text: Option<&'a str>,
}

impl PartialEq for Paragraph<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.metadata == other.metadata
            && self.title == other.title
            && self.content == other.content
            && self.location == other.location
    }
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
            source_text: None,
        }
    }

    /// The original body after document preprocessing, before inline substitutions.
    ///
    /// Semantic edits retain this source snapshot. It is absent for constructed
    /// paragraphs and is not part of ASG serialization or semantic equality.
    #[must_use]
    pub fn source_text(&self) -> Option<&'a str> {
        self.source_text
    }

    /// Replace metadata without shadowing dedicated fields.
    ///
    /// # Errors
    /// Rejects scalar attributes for dedicated metadata or title fields.
    pub fn with_metadata(
        mut self,
        metadata: BlockMetadata<'a>,
    ) -> Result<Self, crate::ReservedAttribute> {
        self.set_metadata(metadata)?;
        Ok(self)
    }

    /// Set the title.
    #[must_use]
    pub fn with_title(mut self, title: Title<'a>) -> Self {
        self.set_title(title);
        self
    }
    /// Replace metadata without shadowing the title or common metadata fields.
    ///
    /// # Errors
    /// Rejects scalar attributes for dedicated fields, leaving the paragraph unchanged.
    pub fn set_metadata(
        &mut self,
        metadata: BlockMetadata<'a>,
    ) -> Result<(), crate::ReservedAttribute> {
        metadata.validate_attributes(&["title"])?;
        self.metadata = metadata;
        Ok(())
    }

    /// Copy metadata to prepare a checked replacement, retaining its source lifetime.
    #[must_use]
    pub fn clone_metadata(&self) -> BlockMetadata<'a> {
        self.metadata.clone()
    }

    /// Remove metadata without changing content or its source snapshot.
    #[must_use]
    pub fn take_metadata(&mut self) -> BlockMetadata<'a> {
        std::mem::take(&mut self.metadata)
    }

    /// Replace the title, including an explicitly empty value.
    pub fn set_title(&mut self, title: Title<'a>) {
        self.title = title;
    }

    /// Remove the title, leaving it absent.
    #[must_use]
    pub fn take_title(&mut self) -> Title<'a> {
        std::mem::take(&mut self.title)
    }

    /// Replace prepared content, retaining the original source snapshot.
    ///
    /// Footnote and reference registrations are not rebuilt.
    pub fn set_content(&mut self, content: impl IntoIterator<Item = InlineNode<'a>>) {
        self.content = content.into_iter().collect();
    }

    /// Append a prepared inline without rebuilding registrations or source text.
    pub fn push_inline(&mut self, inline: InlineNode<'a>) {
        self.content.push(inline);
    }

    /// Remove content, retaining each inline's source lifetime and the source snapshot.
    pub fn take_content(
        &mut self,
    ) -> impl DoubleEndedIterator<Item = InlineNode<'a>> + ExactSizeIterator + use<'a> {
        std::mem::take(&mut self.content).into_iter()
    }

    /// Replace the paragraph span without relocating content or its source snapshot.
    pub fn set_location(&mut self, location: Location) {
        self.location = location;
    }
}

/// Notation type for mathematical expressions.
/// Parsing accepts `latexmath`, `latex`, and `tex` for LaTeX, or `asciimath`.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum StemNotation {
    Latexmath,
    Asciimath,
}

impl StemNotation {
    pub(crate) fn resolve(value: Option<&str>) -> Self {
        match value {
            Some("latexmath" | "latex" | "tex") => Self::Latexmath,
            _ => Self::Asciimath,
        }
    }
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
            "latexmath" | "latex" | "tex" => Ok(Self::Latexmath),
            "asciimath" => Ok(Self::Asciimath),
            _ => Err(format!("unknown stem notation: {s}")),
        }
    }
}

/// The formula and notation stored in a [`StemBlock`].
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct StemContent<'a> {
    pub(crate) content: &'a str,
    pub(crate) notation: StemNotation,
}

impl<'a> StemContent<'a> {
    /// Create a new stem content with the given content and notation.
    #[must_use]
    pub(crate) fn new(content: &'a str, notation: StemNotation) -> Self {
        Self { content, notation }
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
        } else if !self.attributes.serialization_is_empty() {
            state.serialize_entry("attributes", &self.attributes)?;
        }
        if !self.blocks.is_empty() {
            state.serialize_entry("blocks", &self.blocks)?;
        }
        state.serialize_entry("location", &self.location)?;
        state.end()
    }
}

impl Serialize for DiscreteHeading<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(None)?;
        state.serialize_entry("name", "heading")?;
        state.serialize_entry("type", "block")?;
        if self.title.is_present() {
            state.serialize_entry("title", &self.title)?;
        }
        state.serialize_entry("level", &self.level)?;
        if !self.metadata.is_empty_for_serialization() {
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
        if self.title.is_present() {
            state.serialize_entry("title", &self.title)?;
        }
        state.serialize_entry("inlines", &self.content)?;
        if !self.metadata.is_empty_for_serialization() {
            state.serialize_entry("metadata", &self.metadata)?;
        }
        state.serialize_entry("location", &self.location)?;
        state.end()
    }
}
