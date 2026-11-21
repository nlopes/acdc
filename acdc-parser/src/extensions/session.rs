use std::{cell::OnceCell, fmt};

use crate::{Block, Error, ParseResult, Warning, WarningKind, grammar::ParserState};

use super::{
    BlockBuilder, BlockMacroOutput, BlockMacroProcessor, BlockStyleOutput, DeferredBlock,
    DocumentQuery, Invocation, PlaceholderId, RawBlockInput, RawBlockProcessor, TargetId,
    TreeProcessor, query::TargetIndex, target::DocumentIdentity,
};

pub(crate) struct Session {
    processors: Vec<Box<dyn BlockMacroProcessor>>,
    styles: Box<[Box<dyn RawBlockProcessor<BlockStyleOutput>>]>,
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
    pub(super) mode: crate::CompatibilityMode,
    pub(super) parsed: ParseResult,
    pub(super) session: Session,
    // Keep unqueried extension results small.
    pub(super) targets: OnceCell<Box<TargetIndex>>,
}

impl ExtensionParseResult {
    pub(super) fn new(
        parsed: ParseResult,
        session: Session,
        mode: crate::CompatibilityMode,
    ) -> Self {
        Self {
            mode,
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
    pub fn finish(mut self) -> Result<ParseResult, Error> {
        if let Some(request) = self.session.deferred.first() {
            return Err(Error::Extension(
                Box::new(request.invocation.location.clone()),
                "unresolved deferred block".into(),
            ));
        }
        self.bind_displays()?;
        Ok(self.parsed)
    }

    /// Repair structural catalogs and diagnostics without invoking processors.
    /// Successful edits already normalize before returning. Repeating this
    /// operation preserves their observable result and assigned numbers.
    ///
    /// # Errors
    /// Returns a located error if retained semantic content cannot be repaired.
    pub fn normalize(&mut self) -> Result<(), Error> {
        self.target_index();
        if let Some(index) = self.targets.get_mut() {
            self.parsed
                .with_document_mut(|_, document| index.prepare_edit(document));
        }
        self.repair_after_edit()
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
        styles: Box<[Box<dyn RawBlockProcessor<BlockStyleOutput>>]>,
        tree_processors: Vec<Box<dyn TreeProcessor>>,
    ) -> Self {
        Self {
            processors,
            styles,
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

            let builder = match &mut extension.invocation {
                Invocation::Style {
                    context,
                    body,
                    body_location,
                    location: origin,
                    parse_children,
                } => {
                    let fail = |message| Error::Extension(Box::new(origin.clone()), message);
                    let processor = self.styles.get_mut(extension.processor).ok_or_else(|| {
                        fail("style processor is absent from the document session".into())
                    })?;
                    let mut warnings = Vec::new();
                    let raw = RawBlockInput {
                        context: *context,
                        body,
                        body_location,
                        location: origin,
                        document_attributes: state.document_attributes.as_ref().into(),
                        metadata: &extension.metadata,
                        title: extension.title.source(),
                        warnings: &mut warnings,
                    };
                    let output = processor.process(raw).map_err(fail)?;
                    for message in warnings {
                        state.add_warning(Warning::new(
                            WarningKind::Other(message.into()),
                            Some(origin.clone()),
                        ));
                    }
                    extension.metadata.style = None;
                    match output {
                        BlockStyleOutput::ParseChildren => {
                            *parse_children = true;
                            return Ok(true);
                        }
                        BlockStyleOutput::None => return Ok(false),
                        BlockStyleOutput::Block(builder) => builder,
                    }
                }
                Invocation::Macro(call) => {
                    let fail = |message| Error::Extension(Box::new(call.location.clone()), message);
                    let output = self
                        .processors
                        .get_mut(extension.processor)
                        .ok_or_else(|| {
                            fail("processor is absent from the document session".into())
                        })?
                        .process(call, context)
                        .map_err(fail)?;
                    match output {
                        BlockMacroOutput::Paragraph(text) => BlockBuilder::paragraph(text),
                        BlockMacroOutput::Block(builder) => builder,
                        BlockMacroOutput::None => return Ok(false),
                        BlockMacroOutput::Deferred => {
                            let placeholder = PlaceholderId(self.deferred.len());
                            extension.placeholder = Some(placeholder);
                            self.deferred.push(DeferredBlock {
                                placeholder,
                                invocation: call.to_owned_invocation(),
                                target: TargetId {
                                    document: self.identity().clone(),
                                    slot: placeholder.0,
                                },
                            });
                            return Ok(true);
                        }
                    }
                }
            };
            *block = builder
                .build_at_invocation(extension, state)
                .map_err(|error| {
                    if matches!(error, Error::Extension(..)) {
                        return error;
                    }
                    let message =
                        if let Error::Parse(_, message) | Error::PegParse(_, message) = error {
                            message
                        } else {
                            error.to_string()
                        };
                    Error::Extension(Box::new(extension.invocation.location().clone()), message)
                })?;
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
