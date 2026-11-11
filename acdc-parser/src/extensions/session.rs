use std::{cell::OnceCell, fmt};

use crate::{
    Block, DelimitedBlockType, Document, Error, InlineNode, Paragraph, ParseResult, Plain,
    grammar::ParserState,
};

use super::{
    BlockMacroOutput, BlockMacroProcessor, DeferredBlock, DocumentQuery, PlaceholderId, TargetId,
    TreeProcessor, query::TargetIndex, target::DocumentIdentity,
};

pub(crate) struct Session {
    processors: Vec<Box<dyn BlockMacroProcessor>>,
    pub(super) tree_processors: Vec<Box<dyn TreeProcessor>>,
    pub(super) deferred: Vec<DeferredBlock>,
    pub(super) processed_blocks: bool,
    identity: OnceCell<DocumentIdentity>,
}

impl fmt::Debug for Session {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Session")
            .field("deferred", &self.deferred)
            .finish_non_exhaustive()
    }
}

/// A parsed document and the processors and requests that belong to it.
#[derive(Debug)]
#[must_use]
pub struct ExtensionParseResult {
    pub(super) parsed: ParseResult,
    pub(super) session: Session,
    // Keep unqueried extension results small.
    pub(super) targets: OnceCell<Box<TargetIndex>>,
}

impl ExtensionParseResult {
    pub(super) fn new(parsed: ParseResult, session: Session) -> Self {
        Self {
            parsed,
            session,
            targets: OnceCell::new(),
        }
    }

    /// Recoverable problems from parsing and subsequent edits.
    #[must_use]
    pub fn warnings(&self) -> &[crate::Warning] {
        self.parsed.warnings()
    }

    /// Finish this session for conversion.
    ///
    /// # Errors
    /// Returns a located error if a deferred request remains unresolved.
    pub fn finish(self) -> Result<ParseResult, Error> {
        if let Some(request) = self.session.deferred.first() {
            return Err(Error::Extension(
                Box::new(request.invocation.location.clone()),
                "unresolved deferred block".into(),
            ));
        }
        Ok(self.parsed)
    }

    #[must_use]
    pub fn deferred_blocks(&self) -> &[DeferredBlock] {
        &self.session.deferred
    }

    /// Query blocks and their document-local identities.
    ///
    /// The host builds the query index on first use. Parsing without queries
    /// does not allocate an index.
    #[must_use]
    pub fn query(&self) -> DocumentQuery<'_> {
        DocumentQuery {
            parsed: &self.parsed,
            index: self.target_index(),
        }
    }

    pub(super) fn target_index(&self) -> &TargetIndex {
        self.targets
            .get_or_init(|| Box::new(TargetIndex::new(&self.parsed, &self.session)))
    }
}

impl Session {
    pub(super) fn new(
        processors: Vec<Box<dyn BlockMacroProcessor>>,
        tree_processors: Vec<Box<dyn TreeProcessor>>,
    ) -> Self {
        Self {
            processors,
            tree_processors,
            deferred: Vec::new(),
            processed_blocks: false,
            identity: OnceCell::new(),
        }
    }

    pub(super) fn identity(&self) -> &DocumentIdentity {
        self.identity.get_or_init(DocumentIdentity::default)
    }

    pub(crate) fn process<'a>(
        &mut self,
        document: &mut Document<'a>,
        state: &ParserState<'a>,
    ) -> Result<bool, Error> {
        self.blocks(&mut document.blocks, state)?;
        Ok(self.processed_blocks)
    }

    fn blocks<'a>(
        &mut self,
        blocks: &mut Vec<Block<'a>>,
        state: &ParserState<'a>,
    ) -> Result<(), Error> {
        let mut result = Ok(());
        blocks.retain_mut(|block| {
            if result.is_err() {
                return true;
            }
            match self.block(block, state) {
                Ok(keep) => keep,
                Err(error) => {
                    result = Err(error);
                    true
                }
            }
        });
        result
    }

    fn block<'a>(&mut self, block: &mut Block<'a>, state: &ParserState<'a>) -> Result<bool, Error> {
        match block {
            Block::Extension(extension) => {
                self.processed_blocks = true;
                let output = self
                    .processors
                    .get_mut(extension.processor)
                    .ok_or_else(|| {
                        Error::Extension(
                            Box::new(extension.invocation.location.clone()),
                            "processor is absent from the document session".into(),
                        )
                    })?
                    .process(&extension.invocation)
                    .map_err(|message| {
                        Error::Extension(Box::new(extension.invocation.location.clone()), message)
                    })?;
                match output {
                    BlockMacroOutput::Paragraph(text) => {
                        let mut paragraph = Paragraph::new(
                            vec![InlineNode::PlainText(Plain {
                                content: state.intern_str(&text),
                                location: extension.location.clone(),
                                escaped: true,
                            })],
                            extension.location.clone(),
                        );
                        paragraph.metadata = std::mem::take(&mut extension.metadata);
                        paragraph.title = std::mem::take(&mut extension.title);
                        if let Some(kind) = crate::CaptionKind::for_style(paragraph.metadata.style)
                        {
                            paragraph.metadata.caption = Some(crate::Caption::resolve(
                                &paragraph.metadata,
                                &extension.invocation.document_attributes,
                                kind,
                            ));
                        }
                        *block = Block::Paragraph(paragraph);
                    }
                    BlockMacroOutput::None => return Ok(false),
                    BlockMacroOutput::Deferred => {
                        let placeholder = PlaceholderId(self.deferred.len());
                        extension.placeholder = Some(placeholder);
                        self.deferred.push(DeferredBlock {
                            placeholder,
                            invocation: extension.invocation.to_owned_invocation(),
                            target: TargetId {
                                document: self.identity().clone(),
                                slot: placeholder.0,
                            },
                        });
                    }
                }
            }
            Block::Section(section) => self.blocks(&mut section.content, state)?,
            Block::Admonition(admonition) => self.blocks(&mut admonition.blocks, state)?,
            Block::UnorderedList(list) => {
                for item in &mut list.items {
                    self.blocks(&mut item.blocks, state)?;
                }
            }
            Block::OrderedList(list) => {
                for item in &mut list.items {
                    self.blocks(&mut item.blocks, state)?;
                }
            }
            Block::CalloutList(list) => {
                for item in &mut list.items {
                    self.blocks(&mut item.blocks, state)?;
                }
            }
            Block::DescriptionList(list) => {
                for item in &mut list.items {
                    if let Some(definition) = &mut item.definition {
                        self.blocks(&mut definition.blocks, state)?;
                    }
                }
            }
            Block::DelimitedBlock(delimited) => self.delimited(&mut delimited.inner, state)?,
            Block::TableOfContents(_)
            | Block::DiscreteHeading(_)
            | Block::DocumentAttribute(_)
            | Block::ThematicBreak(_)
            | Block::PageBreak(_)
            | Block::Paragraph(_)
            | Block::Image(_)
            | Block::Audio(_)
            | Block::Video(_)
            | Block::Comment(_) => {}
        }
        Ok(true)
    }
    fn delimited<'a>(
        &mut self,
        inner: &mut DelimitedBlockType<'a>,
        state: &ParserState<'a>,
    ) -> Result<(), Error> {
        match inner {
            DelimitedBlockType::DelimitedExample(blocks)
            | DelimitedBlockType::DelimitedOpen(blocks)
            | DelimitedBlockType::DelimitedSidebar(blocks)
            | DelimitedBlockType::DelimitedQuote(blocks) => self.blocks(blocks, state)?,
            DelimitedBlockType::DelimitedTable(table) => {
                for row in table
                    .header
                    .iter_mut()
                    .chain(&mut table.rows)
                    .chain(&mut table.footer)
                {
                    for column in &mut row.columns {
                        self.blocks(&mut column.content, state)?;
                    }
                }
            }
            DelimitedBlockType::DelimitedComment(_)
            | DelimitedBlockType::DelimitedListing(_)
            | DelimitedBlockType::DelimitedLiteral(_)
            | DelimitedBlockType::DelimitedPass(_)
            | DelimitedBlockType::DelimitedVerse(_)
            | DelimitedBlockType::DelimitedStem(_) => {}
        }
        Ok(())
    }
}
