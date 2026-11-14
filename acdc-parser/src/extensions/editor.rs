use super::{
    BlockBuilder, DeferredBlock, DocumentQuery, ExtensionParseResult, TargetId,
    traversal::BlockPath,
};
use crate::{Block, Error, SourceLocation};
mod transaction;
use transaction::Edit;

/// Scoped queries and checked edits. Successful edits repair catalogs before the
/// next observation. Validation and repair failures preserve the tree, handles,
/// assigned numbers, and diagnostics. Surviving targets retain their identity.
pub struct DocumentEditor<'doc> {
    result: &'doc mut ExtensionParseResult,
}

impl<'doc> DocumentEditor<'doc> {
    pub(super) const fn new(result: &'doc mut ExtensionParseResult) -> Self {
        Self { result }
    }
    #[must_use]
    pub fn query(&self) -> DocumentQuery<'_> {
        self.result.query()
    }
    #[cfg(test)]
    pub(super) fn document_storage(&self) -> &crate::Document<'_> {
        self.result.parsed.document_storage()
    }
    /// Pending requests whose placeholders remain in the document.
    #[must_use]
    pub fn deferred_blocks(&self) -> &[DeferredBlock] {
        self.result.deferred_blocks()
    }

    /// Return or assign an output ID without changing identity or numbering.
    ///
    /// # Errors
    /// Rejects foreign, removed, or unreferenceable targets.
    pub fn ensure_id(&mut self, target: &TargetId) -> Result<&str, Error> {
        let location = self.location(target);
        self.result
            .ensure_id(target)
            .map_err(|error| Error::Extension(Box::new(location), error.to_string()))
    }
    /// Insert content before an ordinary block or section in its own container,
    /// with a new handle.
    /// The target supplies its location and source-position attribute context.
    ///
    /// # Errors
    /// Rejects foreign/removed targets, comments, attribute events, invalid
    /// metadata, and duplicate IDs.
    pub fn insert_before(
        &mut self,
        target: &TargetId,
        content: BlockBuilder,
    ) -> Result<TargetId, Error> {
        self.apply(target, &Edit::Insert(content))
    }
    /// Replace an ordinary block, retaining its handle, location, title, and
    /// metadata unless overridden. Descendant handles and requests are removed.
    ///
    /// # Errors
    /// Rejects foreign/removed targets, sections, attribute events, and invalid content.
    pub fn replace_block(&mut self, target: &TargetId, content: BlockBuilder) -> Result<(), Error> {
        self.apply(target, &Edit::Replace(content)).map(|_| ())
    }
    /// Replace a pending placeholder through the shared construction and repair path.
    ///
    /// # Errors
    /// Rejects nonpending, foreign, or removed targets and invalid content.
    pub fn replace_placeholder(
        &mut self,
        target: &TargetId,
        content: BlockBuilder,
    ) -> Result<(), Error> {
        self.pending(target)?;
        self.replace_block(target, content)
    }
    /// Replace retained paragraph or listing/literal source. Inline substitutions
    /// use the captured context and `subs=`. No block grammar or callbacks run.
    ///
    /// # Errors
    /// Rejects unsupported, foreign, or removed targets and invalid inline content.
    pub fn replace_source(
        &mut self,
        target: &TargetId,
        source: impl Into<String>,
    ) -> Result<(), Error> {
        let source = source.into();
        let view = self
            .query()
            .get(target)
            .ok_or_else(|| self.error(target, "target is absent"))?;
        let mut builder = if matches!(view.storage(), Block::Paragraph(_)) {
            BlockBuilder::paragraph(source)
        } else if let Block::DelimitedBlock(block) = view.storage() {
            if matches!(block.inner, crate::DelimitedBlockType::DelimitedListing(_)) {
                BlockBuilder::listing(source)
            } else if matches!(block.inner, crate::DelimitedBlockType::DelimitedLiteral(_)) {
                BlockBuilder::literal(source)
            } else {
                return Err(self.error(target, "target has no replaceable inline source body"));
            }
        } else {
            return Err(self.error(target, "target has no replaceable inline source body"));
        };
        builder.source_body = true;
        self.replace_block(target, builder)
    }
    /// Remove a section and its subtree without selecting an empty-section policy.
    ///
    /// # Errors
    /// Rejects foreign/removed targets and targets that are not sections.
    pub fn remove_section(&mut self, target: &TargetId) -> Result<(), Error> {
        if !self
            .query()
            .get(target)
            .is_some_and(|view| matches!(view.storage(), Block::Section(_)))
        {
            return Err(self.error(target, "target is not a section"));
        }
        self.apply(target, &Edit::Remove).map(|_| ())
    }
    /// Remove a pending placeholder and consume its request.
    ///
    /// # Errors
    /// Rejects nonpending, foreign, or removed targets.
    pub fn discard_placeholder(&mut self, target: &TargetId) -> Result<(), Error> {
        self.pending(target)?;
        self.apply(target, &Edit::Remove).map(|_| ())
    }
    fn pending(&self, target: &TargetId) -> Result<(), Error> {
        if self
            .deferred_blocks()
            .iter()
            .any(|request| request.target() == target)
        {
            Ok(())
        } else {
            Err(self.error(target, "target is not a pending placeholder"))
        }
    }
    fn path(&self, target: &TargetId) -> Result<BlockPath, Error> {
        self.result
            .target_index()
            .entry(target)
            .map(|entry| entry.path.clone())
            .ok_or_else(|| {
                self.error(
                    target,
                    "target does not belong to this document or is no longer present",
                )
            })
    }
    fn location(&self, target: &TargetId) -> SourceLocation {
        self.query().get(target).map_or_else(
            || {
                self.result
                    .parsed
                    .source_location(&self.result.parsed.document_storage().location)
            },
            |view| view.source_location(),
        )
    }
    fn error(&self, target: &TargetId, message: &str) -> Error {
        Error::Extension(Box::new(self.location(target)), message.to_owned())
    }
}
