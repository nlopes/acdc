use std::{cell::OnceCell, fmt};

use crate::{Block, Error, InlineNode, Paragraph, ParseResult, Plain, grammar::ParserState};

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

    fn block<'a>(
        &mut self,
        block: &mut Block<'a>,
        state: &ParserState<'a>,
        context: &mut super::MacroExecutionContext,
    ) -> Result<bool, Error> {
        if let Block::Extension(extension) = block {
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
                .process(&extension.invocation, context)
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
                    if let Some(kind) = crate::CaptionKind::for_style(paragraph.metadata.style) {
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
        Ok(true)
    }
}

impl crate::grammar::SourceHandler for Session {
    fn construct<'a>(
        &mut self,
        block: &mut Block<'a>,
        state: &mut ParserState<'a>,
    ) -> Result<(bool, crate::grammar::SourceAssignments<'a>), Error> {
        let mut context = super::MacroExecutionContext::default();
        let keep = self.block(block, state, &mut context)?;
        Ok((keep, context.assignments))
    }
}
