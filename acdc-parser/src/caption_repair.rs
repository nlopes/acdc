use std::{collections::HashMap, convert::Infallible};

use crate::{
    Anchor, BlockMetadata, Caption, CaptionKind, CaptionRepairError, ContentAttribute, Document,
    Header, Location, Section, Title,
    ast::{CaptionRef, DocumentRef, StructuralRef},
    grammar::reference_walk::{self, ReferenceVisitor},
};

#[derive(Default)]
struct Captions<'a> {
    entries: HashMap<&'a str, Option<Caption<'a>>>,
}

impl<'a> ReferenceVisitor<'a> for Captions<'a> {
    type Error = Infallible;

    fn header(&mut self, header: &Header<'a>) -> Result<(), Self::Error> {
        if let Some(anchor) = header.metadata.heading_anchor() {
            self.entries.entry(anchor.id).or_insert(None);
        }
        Ok(())
    }

    fn section(&mut self, section: &mut Section<'a>, _record_toc: bool) -> Result<(), Self::Error> {
        if let Some(id) = Section::explicit_id(&section.metadata).or(section.id) {
            self.entries.entry(id).or_insert(None);
        }
        Ok(())
    }

    fn anchor(
        &mut self,
        anchor: &Anchor<'a>,
        _label: Option<&ContentAttribute<'a>>,
        _metadata: Option<&BlockMetadata<'a>>,
        _title: Option<Title<'a>>,
        caption: Option<Caption<'a>>,
    ) -> Result<(), Self::Error> {
        self.entries.entry(anchor.id).or_insert(caption);
        Ok(())
    }

    fn untitled(&mut self, id: &'a str, _location: &Location) -> Result<(), Self::Error> {
        self.entries.entry(id).or_insert(None);
        Ok(())
    }
}

fn validate_context(document: &Document<'_>) -> Result<(), CaptionRepairError> {
    let read = DocumentRef::from(document);
    for node in read.descendants() {
        let StructuralRef::Block(block) = node else {
            continue;
        };
        let expected = CaptionKind::for_block(block);
        let stored = block
            .attributes()
            .and_then(|attributes| CaptionRef::resolve(attributes, read.attributes(), None));
        let missing = expected.is_some() && stored.is_none();
        let incompatible = if let Some(CaptionRef::Numbered { kind, .. }) = stored {
            expected != Some(kind)
        } else {
            false
        };
        if missing || incompatible {
            return Err(CaptionRepairError::MissingContext {
                location: Box::new(Location::from(block.location())),
            });
        }
    }
    Ok(())
}

impl Document<'_> {
    /// Renumber prepared captions and refresh caption snapshots in existing references.
    /// Custom, empty and disabled prefixes keep their declaration-time values.
    ///
    /// Only caption fields change. A removed target loses its caption snapshot;
    /// reference membership, titles, TOC, footnotes and index registrations stay
    /// unchanged. After structural edits, run [`Self::rebuild_footnotes`] and then
    /// [`Self::rebuild_references_and_toc`] before this operation when those catalogs
    /// need repair. Aliases receive their canonical target's caption.
    ///
    /// # Errors
    /// Returns an error without changing the document if a caption-capable block
    /// lacks a prepared prefix or its numbered category no longer fits its form.
    /// Supply a resolved caption or reparse source to recover declaration context.
    pub fn rebuild_captions(&mut self) -> Result<(), CaptionRepairError> {
        validate_context(self)?;
        self.renumber_captions();
        let mut captions = Captions::default();
        if let Some(header) = &self.header {
            reference_walk::header(&mut captions, header).unwrap_or_else(|never| match never {});
        }
        reference_walk::blocks(&mut captions, &mut self.blocks, true)
            .unwrap_or_else(|never| match never {});
        for reference in self.references.values_mut() {
            reference.caption = captions.entries.get(reference.target).cloned().flatten();
        }
        Ok(())
    }
}
