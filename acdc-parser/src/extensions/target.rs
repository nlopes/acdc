use std::{
    fmt,
    hash::{Hash, Hasher},
    rc::Rc,
};

use crate::{Anchor, Block, ReferenceTarget, grammar::reference_walk};

use super::ExtensionParseResult;

#[derive(Clone, Debug, Default)]
pub(super) struct DocumentIdentity(Rc<()>);

impl PartialEq for DocumentIdentity {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for DocumentIdentity {}

impl Hash for DocumentIdentity {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Rc::as_ptr(&self.0).hash(state);
    }
}

/// An opaque block identity belonging to one parsed document.
///
/// Assigning an output ID does not change this handle. Handles from separate
/// parses are distinct, even when the inputs and output IDs are identical.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct TargetId {
    pub(super) document: DocumentIdentity,
    pub(super) slot: usize,
}

impl fmt::Debug for TargetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TargetId").finish_non_exhaustive()
    }
}

/// A target cannot be used for the requested operation.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum TargetError {
    #[error("target does not belong to this document or is no longer present")]
    UnknownTarget,
    #[error("this block cannot carry an output ID")]
    NotReferenceable,
}

impl ExtensionParseResult {
    /// Return the target's output ID, assigning one when it has none.
    ///
    /// Existing authored and generated IDs are preserved. New IDs avoid all
    /// registered targets and unresolved reference names, so a previously broken
    /// reference cannot accidentally bind to this block. IDs are deterministic
    /// for identical conversions, independent of request order.
    /// The reference catalog is updated without rerunning extension callbacks.
    ///
    /// # Errors
    /// Rejects foreign or removed targets and blocks that cannot carry an ID.
    pub fn ensure_id(&mut self, target: &TargetId) -> Result<&str, TargetError> {
        let entry = self
            .target_index()
            .entry(target)
            .ok_or(TargetError::UnknownTarget)?;
        let block = entry
            .path
            .get(&self.parsed.document_storage().blocks)
            .ok_or(TargetError::UnknownTarget)?;
        if super::query::output_id(block).is_none() {
            if block.metadata().is_none() && !matches!(block, Block::ThematicBreak(_)) {
                return Err(TargetError::NotReferenceable);
            }
            let path = entry.path.clone();
            let is_section = matches!(block, Block::Section(_));
            if is_section && let Some(index) = self.targets.get_mut() {
                self.parsed
                    .with_document_mut(|_, document| index.prepare_edit(document));
            }
            let section_reference = self.target_index().section_reference(target).cloned();
            let base = format!("_acdc_{}", target.slot + 1);
            let mut id = base.clone();
            let mut suffix = 2;
            while self
                .parsed
                .document_storage()
                .references
                .contains_key(id.as_str())
                || self.target_index().reserves_id(&id)
            {
                id = format!("{base}_{suffix}");
                suffix += 1;
            }
            self.parsed.with_document_mut(|owner, document| {
                let id: &str = owner.arena.alloc_str(&id);
                let block = path
                    .get_mut(&mut document.blocks)
                    .ok_or(TargetError::UnknownTarget)?;
                let anchor = Anchor::new(id, block.location().clone());
                if let Block::ThematicBreak(block) = block {
                    block.anchors.push(anchor);
                } else {
                    block
                        .metadata_mut()
                        .ok_or(TargetError::NotReferenceable)?
                        .id = Some(anchor);
                }
                if let Block::Section(section) = block {
                    section.id = Some(id);
                    if document.references.get("").is_some_and(|reference| {
                        reference.section_category().is_some()
                            && reference.location.start == section.location.start
                            && reference.location.absolute_start == section.location.absolute_start
                    }) {
                        document.references.remove("");
                    }
                    for entry in &mut document.toc_entries {
                        if entry.id.is_empty()
                            && entry.level == section.level
                            && entry.kind == section.kind
                            && entry.title == section.title
                            && entry.location.start.file == section.location.start.file
                            && section.location.absolute_start <= entry.location.absolute_start
                            && section.location.absolute_end >= entry.location.absolute_end
                        {
                            entry.id = id;
                        }
                    }
                }
                let mut reference = ReferenceTarget::for_anchor(
                    &Anchor::new(id, block.location().clone()),
                    block
                        .metadata()
                        .and_then(reference_walk::metadata_xreflabel),
                    block.title().cloned(),
                    block
                        .metadata()
                        .and_then(|metadata| metadata.caption.clone()),
                );
                reference.section = section_reference;
                document.references.insert(id, reference);
                Ok(())
            })?;
        }
        self.query()
            .get(target)
            .and_then(|view| view.output_id())
            .ok_or(TargetError::UnknownTarget)
    }
}
