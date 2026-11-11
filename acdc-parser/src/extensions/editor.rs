use crate::{Block, BlockMetadata, Caption, Error, SourceLocation, Title};

use super::{
    BlockBuilder, DeferredBlock, DocumentQuery, ExtensionParseResult, TargetId,
    traversal::BlockPath,
};

/// Scoped access to queries, builders, and checked structural edits.
///
/// Every edit repairs catalogs before the next query. Surviving targets keep
/// their handles and section/caption numbers; removed handles never become live
/// again, even if a new block uses the same output ID. Footnotes are compacted.
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

    /// Pending requests whose placeholders are still in the document.
    #[must_use]
    pub fn deferred_blocks(&self) -> &[DeferredBlock] {
        self.result.deferred_blocks()
    }

    /// Assign or return an output ID without changing block identity or numbering.
    ///
    /// # Errors
    /// Rejects foreign, removed, or unreferenceable targets.
    pub fn ensure_id(&mut self, target: &TargetId) -> Result<&str, Error> {
        let location = self.location(target);
        self.result
            .ensure_id(target)
            .map_err(|error| Error::Extension(Box::new(location), error.to_string()))
    }

    /// Insert ordinary content immediately before a target in its own container.
    ///
    /// The new block uses the target's source location and receives a new handle.
    ///
    /// # Errors
    /// Rejects foreign/removed targets and empty or duplicate requested IDs.
    pub fn insert_before(
        &mut self,
        target: &TargetId,
        content: BlockBuilder,
    ) -> Result<TargetId, Error> {
        let path = self.path(target)?;
        self.validate_id(target, &content, false)?;
        self.prepare_edit();
        let location = self.location(target);
        self.result.parsed.with_document_mut(|owner, document| {
            let block = content.build(
                &owner.arena,
                location.location.clone(),
                BlockMetadata::default(),
                Title::default(),
            );
            let (blocks, position) = path.container_mut(&mut document.blocks).ok_or_else(|| {
                Error::Extension(Box::new(location.clone()), "target is absent".into())
            })?;
            blocks.insert(position, block);
            Ok::<(), Error>(())
        })?;
        let new_target = self
            .result
            .targets
            .get_mut()
            .and_then(|index| index.insert_before(target))
            .ok_or_else(|| self.error(target, "target is absent"))?;
        self.result.repair_after_edit()?;
        Ok(new_target)
    }

    /// Replace a pending placeholder, retaining its handle, source, title, and metadata.
    /// A builder-supplied ID overrides the placeholder's ID.
    ///
    /// The request is consumed. The replacement invokes no parsing or callbacks.
    ///
    /// # Errors
    /// Rejects targets that are not pending placeholders and invalid requested IDs.
    pub fn replace_placeholder(
        &mut self,
        target: &TargetId,
        content: BlockBuilder,
    ) -> Result<(), Error> {
        let path = self.path(target)?;
        if !self
            .result
            .deferred_blocks()
            .iter()
            .any(|request| request.target() == target)
        {
            return Err(self.error(target, "target is not a pending placeholder"));
        }
        self.validate_id(target, &content, true)?;
        self.prepare_edit();
        let caption_kind = self
            .query()
            .get(target)
            .and_then(|view| view.storage().metadata())
            .and_then(|metadata| content.caption_kind(metadata.style));
        let numbered_kind = caption_kind.filter(|kind| {
            self.query().get(target).is_some_and(|view| {
                let Block::Extension(extension) = view.storage() else {
                    return false;
                };
                !extension.title.is_empty()
                    && matches!(
                        Caption::resolve(
                            &extension.metadata,
                            &extension.invocation.document_attributes,
                            *kind
                        ),
                        Caption::Numbered { .. }
                    )
            })
        });
        let caption_number =
            numbered_kind.and_then(|kind| self.result.targets.get_mut()?.next_caption(kind));
        let location = self.location(target);
        self.result.parsed.with_document_mut(|owner, document| {
            let block = path.get_mut(&mut document.blocks).ok_or_else(|| {
                Error::Extension(Box::new(location.clone()), "target is absent".into())
            })?;
            let Block::Extension(extension) = block else {
                return Err(Error::Extension(
                    Box::new(location.clone()),
                    "target is not a placeholder".into(),
                ));
            };
            let mut metadata = std::mem::take(&mut extension.metadata);
            if let Some(kind) = caption_kind {
                metadata.caption = Some(Caption::resolve(
                    &metadata,
                    &extension.invocation.document_attributes,
                    kind,
                ));
                if let Some(Caption::Numbered { number, .. }) = &mut metadata.caption {
                    *number = caption_number;
                }
            }
            let title = std::mem::take(&mut extension.title);
            *block = content.build(&owner.arena, extension.location.clone(), metadata, title);
            Ok(())
        })?;
        self.result
            .session
            .deferred
            .retain(|request| request.target() != target);
        self.result.repair_after_edit()
    }

    /// Remove a section and its entire subtree, including pending placeholders.
    ///
    /// The host applies no empty-section policy; the processor decides what to remove.
    ///
    /// # Errors
    /// Rejects foreign/removed targets and targets that are not sections.
    pub fn remove_section(&mut self, target: &TargetId) -> Result<(), Error> {
        let path = self.path(target)?;
        if !matches!(
            path.get(&self.result.parsed.document_storage().blocks),
            Some(Block::Section(_))
        ) {
            return Err(self.error(target, "target is not a section"));
        }
        self.remove(target, &path)
    }

    /// Remove a pending placeholder and consume its request.
    ///
    /// # Errors
    /// Rejects targets that are not pending placeholders.
    pub fn discard_placeholder(&mut self, target: &TargetId) -> Result<(), Error> {
        let path = self.path(target)?;
        if !self
            .result
            .deferred_blocks()
            .iter()
            .any(|request| request.target() == target)
        {
            return Err(self.error(target, "target is not a pending placeholder"));
        }
        self.remove(target, &path)
    }

    fn remove(&mut self, target: &TargetId, path: &BlockPath) -> Result<(), Error> {
        self.prepare_edit();
        let location = self.location(target);
        self.result.parsed.with_document_mut(|_, document| {
            let (blocks, position) = path.container_mut(&mut document.blocks).ok_or_else(|| {
                Error::Extension(Box::new(location.clone()), "target is absent".into())
            })?;
            blocks.remove(position);
            Ok::<(), Error>(())
        })?;
        if let Some(index) = self.result.targets.get_mut() {
            index.remove(target);
            self.result
                .session
                .deferred
                .retain(|request| index.entry(request.target()).is_some());
        }
        self.result.repair_after_edit()
    }

    fn validate_id(
        &self,
        target: &TargetId,
        content: &BlockBuilder,
        replacing: bool,
    ) -> Result<(), Error> {
        if let Some(id) = &content.id {
            if id.is_empty() {
                return Err(self.error(target, "output ID must not be empty"));
            }
            let current = self.query().get(target).and_then(|view| view.output_id());
            if self
                .result
                .parsed
                .document_storage()
                .references
                .contains_key(id.as_str())
                && !(replacing && current == Some(id.as_str()))
            {
                return Err(self.error(target, &format!("output ID is already in use: {id}")));
            }
        }
        Ok(())
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

    fn prepare_edit(&mut self) {
        if let Some(index) = self.result.targets.get_mut() {
            self.result
                .parsed
                .with_document_mut(|_, document| index.prepare_edit(document));
        }
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
