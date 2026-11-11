use std::collections::{HashMap, HashSet};

use crate::{
    Block, CaptionKind, DelimitedBlockType, Document, InlineMacro, InlineNode, ParseResult,
    SourceLocation, WarningKind, model::SectionReference,
};

use super::{
    Session, TargetId,
    target::DocumentIdentity,
    traversal::{BlockPath, Children, visit_children},
};

/// The semantic context used to select blocks, independent of their Rust representation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum BlockContext {
    Section,
    Paragraph,
    Example,
    Open,
    Sidebar,
    Quote,
    Verse,
    Listing,
    Literal,
    Passthrough,
    Stem,
    Table,
    Image,
    Audio,
    Video,
    Admonition,
    UnorderedList,
    OrderedList,
    DescriptionList,
    CalloutList,
    DiscreteHeading,
    ThematicBreak,
    PageBreak,
    TableOfContents,
    Comment,
    DocumentAttribute,
    Extension,
}

impl BlockContext {
    fn of(block: &Block<'_>) -> Self {
        match block {
            Block::Section(_) => Self::Section,
            Block::Paragraph(block) => Self::styled(block.metadata.style, Self::Paragraph),
            Block::DelimitedBlock(block) => match &block.inner {
                DelimitedBlockType::DelimitedExample(_) => Self::Example,
                DelimitedBlockType::DelimitedOpen(_) => {
                    Self::styled(block.metadata.style, Self::Open)
                }
                DelimitedBlockType::DelimitedSidebar(_) => Self::Sidebar,
                DelimitedBlockType::DelimitedQuote(_) => Self::Quote,
                DelimitedBlockType::DelimitedVerse(_) => Self::Verse,
                DelimitedBlockType::DelimitedListing(_) => match block.metadata.style {
                    Some("literal") => Self::Literal,
                    _ => Self::Listing,
                },
                DelimitedBlockType::DelimitedLiteral(_) => match block.metadata.style {
                    Some("listing" | "source") => Self::Listing,
                    _ => Self::Literal,
                },
                DelimitedBlockType::DelimitedPass(_) => Self::Passthrough,
                DelimitedBlockType::DelimitedStem(_) => Self::Stem,
                DelimitedBlockType::DelimitedTable(_) => Self::Table,
                DelimitedBlockType::DelimitedComment(_) => Self::Comment,
            },
            Block::Image(_) => Self::Image,
            Block::Audio(_) => Self::Audio,
            Block::Video(_) => Self::Video,
            Block::Admonition(_) => Self::Admonition,
            Block::UnorderedList(_) => Self::UnorderedList,
            Block::OrderedList(_) => Self::OrderedList,
            Block::DescriptionList(_) => Self::DescriptionList,
            Block::CalloutList(_) => Self::CalloutList,
            Block::DiscreteHeading(_) => Self::DiscreteHeading,
            Block::ThematicBreak(_) => Self::ThematicBreak,
            Block::PageBreak(_) => Self::PageBreak,
            Block::TableOfContents(_) => Self::TableOfContents,
            Block::Comment(_) => Self::Comment,
            Block::DocumentAttribute(_) => Self::DocumentAttribute,
            Block::Extension(_) => Self::Extension,
        }
    }

    fn styled(style: Option<&str>, default: Self) -> Self {
        match style {
            Some("example") => Self::Example,
            Some("listing" | "source") => Self::Listing,
            Some("literal") => Self::Literal,
            Some("quote") => Self::Quote,
            Some("verse") => Self::Verse,
            Some("sidebar") => Self::Sidebar,
            Some("pass") => Self::Passthrough,
            Some("stem" | "latexmath" | "asciimath") => Self::Stem,
            _ => default,
        }
    }
}

#[derive(Debug)]
pub(super) struct Entry {
    pub(super) target: TargetId,
    pub(super) path: BlockPath,
    section: Option<TargetId>,
}

#[derive(Debug)]
pub(super) struct TargetIndex {
    document: DocumentIdentity,
    entries: Vec<Entry>,
    slots: Vec<Option<usize>>,
    unresolved_targets: HashSet<String>,
    caption_high_water: Option<[u32; 4]>,
    section_references: HashMap<usize, SectionReference>,
}

impl TargetIndex {
    pub(super) fn prepare_edit(&mut self, document: &mut Document<'_>) {
        if self.caption_high_water.is_some() {
            return;
        }
        let is_book = document.attributes.text("doctype") == Some("book");
        let references =
            crate::model::section::extension_reference_numbers(&mut document.blocks, is_book);
        let sections = self
            .entries
            .iter()
            .filter(|entry| matches!(entry.path.get(&document.blocks), Some(Block::Section(_))));
        self.section_references.extend(
            sections
                .zip(references)
                .map(|(entry, reference)| (entry.target.slot, reference)),
        );
        self.caption_high_water.get_or_insert_with(|| {
            [
                CaptionKind::Example,
                CaptionKind::Figure,
                CaptionKind::Listing,
                CaptionKind::Table,
            ]
            .map(|kind| document.highest_caption_number(kind))
        });
    }

    pub(super) fn section_reference(&self, target: &TargetId) -> Option<&SectionReference> {
        self.entry(target)?;
        self.section_references.get(&target.slot)
    }

    pub(super) fn repair_section_references(&self, document: &mut Document<'_>) {
        for entry in &self.entries {
            let Some(Block::Section(section)) = entry.path.get(&document.blocks) else {
                continue;
            };
            if let Some(reference) = document.references.get_mut(section.id().as_ref())
                && reference.occurrence == section.occurrence
            {
                reference.section = self.section_reference(&entry.target).cloned();
            }
        }
        let names: HashMap<_, _> = document
            .references
            .iter()
            .filter_map(|(id, reference)| Some((*id, reference.section.as_ref()?.name)))
            .collect();
        crate::grammar::walk_document_inline_nodes_mut(document, &mut |inline| {
            if let InlineNode::Macro(InlineMacro::CrossReference(xref)) = inline {
                xref.refresh_signifier(names.get(xref.target).copied());
            }
        });
    }

    pub(super) fn reserve_warnings(&mut self, parsed: &ParseResult) {
        self.unresolved_targets
            .extend(parsed.warnings().iter().filter_map(|warning| {
                if let WarningKind::UnresolvedReference { target } = &warning.kind {
                    Some(target.clone())
                } else {
                    None
                }
            }));
    }
    pub(super) fn new(parsed: &ParseResult, session: &Session) -> Self {
        let unresolved_targets = parsed
            .warnings()
            .iter()
            .filter_map(|warning| {
                if let WarningKind::UnresolvedReference { target } = &warning.kind {
                    Some(target.clone())
                } else {
                    None
                }
            })
            .collect();
        let mut index = Self {
            document: session.identity().clone(),
            entries: Vec::new(),
            slots: vec![None; session.deferred.len()],
            unresolved_targets,
            caption_high_water: None,
            section_references: HashMap::new(),
        };
        for (position, block) in parsed.document_storage().blocks.iter().enumerate() {
            index.visit(block, &BlockPath::root(position), None, session);
        }
        index
    }

    pub(super) fn reserves_id(&self, id: &str) -> bool {
        self.unresolved_targets.contains(id)
    }

    fn visit(
        &mut self,
        block: &Block<'_>,
        path: &BlockPath,
        section: Option<&TargetId>,
        session: &Session,
    ) {
        let deferred = if let Block::Extension(block) = block {
            block
                .placeholder()
                .and_then(|placeholder| session.deferred.get(placeholder.0))
        } else {
            None
        };
        let target = if let Some(request) = deferred {
            let target = request.target().clone();
            if let Some(slot) = self.slots.get_mut(target.slot) {
                *slot = Some(self.entries.len());
            }
            target
        } else {
            let target = TargetId {
                document: self.document.clone(),
                slot: self.slots.len(),
            };
            self.slots.push(Some(self.entries.len()));
            target
        };
        self.entries.push(Entry {
            target: target.clone(),
            path: path.clone(),
            section: section.cloned(),
        });
        let section = if matches!(block, Block::Section(_)) {
            Some(&target)
        } else {
            section
        };
        visit_children(block, |children, blocks| {
            let section = if matches!(children, Children::Cell { .. }) {
                None
            } else {
                section
            };
            for (position, child) in blocks.iter().enumerate() {
                self.visit(child, &path.child(children, position), section, session);
            }
        });
    }

    pub(super) fn entry(&self, target: &TargetId) -> Option<&Entry> {
        if target.document != self.document {
            return None;
        }
        self.entries
            .get(self.slots.get(target.slot).copied().flatten()?)
    }
}

/// Read-only access to blocks in depth-first document order, parents before children.
///
/// Traversal includes list attachments, compound blocks, and `AsciiDoc` table-cell documents.
/// List items and table cells are containers, rather than separate block targets.
/// The document header is outside this block query.
pub struct DocumentQuery<'doc> {
    pub(super) parsed: &'doc ParseResult,
    pub(super) index: &'doc TargetIndex,
}

impl<'doc> DocumentQuery<'doc> {
    /// Visit every block in document order.
    pub fn blocks(&self) -> impl Iterator<Item = BlockView<'doc>> + '_ {
        self.index
            .entries
            .iter()
            .filter_map(|entry| self.view(entry))
    }

    /// Select blocks by semantic context, retaining document order.
    pub fn select(&self, context: BlockContext) -> impl Iterator<Item = BlockView<'doc>> + '_ {
        self.blocks().filter(move |view| view.context() == context)
    }

    /// Resolve a document-local handle. Foreign or removed targets return `None`.
    #[must_use]
    pub fn get(&self, target: &TargetId) -> Option<BlockView<'doc>> {
        self.view(self.index.entry(target)?)
    }

    fn view(&self, entry: &'doc Entry) -> Option<BlockView<'doc>> {
        Some(BlockView {
            parsed: self.parsed,
            entry,
            block: entry.path.get(&self.parsed.document_storage().blocks)?,
        })
    }
}

/// A selected block, its identity, and its source and containing-section context.
pub struct BlockView<'doc> {
    parsed: &'doc ParseResult,
    entry: &'doc Entry,
    block: &'doc Block<'doc>,
}

impl<'doc> BlockView<'doc> {
    #[must_use]
    pub const fn target(&self) -> &'doc TargetId {
        &self.entry.target
    }

    /// The nearest enclosing section within the same document scope.
    /// A section excludes itself, and `AsciiDoc` cells start a new document scope.
    #[must_use]
    pub fn containing_section(&self) -> Option<&'doc TargetId> {
        self.entry.section.as_ref()
    }

    #[must_use]
    pub fn context(&self) -> BlockContext {
        BlockContext::of(self.block)
    }

    #[must_use]
    pub fn block(&self) -> crate::ast::BlockRef<'doc> {
        crate::ast::BlockRef::from(self.block)
    }

    #[cfg(test)]
    pub(super) const fn storage(&self) -> &'doc Block<'doc> {
        self.block
    }

    /// The title after its source-position substitutions, before backend rendering.
    #[must_use]
    pub fn title(&self) -> Option<crate::ast::ContentRef<'doc>> {
        self.block().title()
    }

    #[must_use]
    pub fn attributes(&self) -> Option<crate::ast::AttributesRef<'doc>> {
        self.block().attributes()
    }

    /// Caption settings and ordinal resolved by the host for this block.
    #[must_use]
    pub fn caption(&self) -> Option<crate::ast::CaptionRef<'doc>> {
        self.block
            .metadata()?
            .caption
            .as_ref()
            .map(crate::ast::CaptionRef::from)
    }

    #[must_use]
    pub fn source_location(&self) -> SourceLocation {
        self.parsed.source_location(self.block.location())
    }

    /// The authored or host-assigned output ID, if present and non-empty.
    #[must_use]
    pub fn output_id(&self) -> Option<&'doc str> {
        output_id(self.block)
    }
}

pub(super) fn output_id<'doc>(block: &'doc Block<'_>) -> Option<&'doc str> {
    let id = if let Block::Section(section) = block {
        crate::Section::explicit_id(&section.metadata).or(section.id)
    } else {
        block.anchor().map(|anchor| anchor.id)
    };
    id.filter(|id| !id.is_empty())
}
