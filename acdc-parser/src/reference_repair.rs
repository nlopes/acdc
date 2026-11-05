use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use crate::{
    Anchor, Block, BlockMetadata, Caption, ContentAttribute, CrossReference, Document, Header,
    Location, Reference, ReferenceRepairError, Section, Title, TocEntry,
    grammar::reference_walk::{self, ReferenceVisitor},
    model::{DocumentTopLabel, SectionOccurrence, displayed_reference_label},
};

struct RebuiltReferences<'a> {
    entries: HashMap<&'a str, Reference<'a>>,
    toc: Vec<TocEntry<'a>>,
    occurrences: Occurrences,
    automatic: Vec<&'a str>,
    needs_document_top: bool,
}

impl<'a> ReferenceVisitor<'a> for RebuiltReferences<'a> {
    type Error = ReferenceRepairError;

    fn header(&mut self, header: &Header<'a>) -> Result<(), Self::Error> {
        if let Some(anchor) = header.metadata.heading_anchor() {
            self.anchor(
                anchor,
                reference_walk::metadata_xreflabel(&header.metadata),
                Some(&header.metadata),
                Some(reference_walk::header_title(header)),
                None,
            )?;
        }
        Ok(())
    }

    fn section(&mut self, section: &mut Section<'a>, record_toc: bool) -> Result<(), Self::Error> {
        let id = Section::explicit_id(&section.metadata)
            .or(section.id)
            .ok_or_else(|| ReferenceRepairError::UnassignedSectionId {
                location: Box::new(section.location.clone()),
            })?;
        let occurrence = self.occurrences.key(section);
        let label = reference_walk::metadata_xreflabel(&section.metadata);
        self.entries.entry(id).or_insert_with(|| {
            let mut location = section.location.clone();
            location.end.clone_from(&section.heading_location.end);
            location.absolute_end = section
                .heading_location
                .absolute_end
                .max(location.absolute_start);
            let mut reference = Reference::new(id, location);
            reference.occurrence = Some(occurrence.clone());
            reference.title = Some(ContentAttribute::new(section.title.clone().into_inlines()));
            reference.xreflabel = displayed_reference_label(label);
            reference
        });
        if record_toc {
            let mut entry = TocEntry::for_section(
                id,
                section.title.clone(),
                section.level,
                label.cloned(),
                section.kind,
                section.heading_location.clone(),
            );
            entry.occurrence = Some(occurrence);
            self.toc.push(entry);
        }
        Ok(())
    }

    fn anchor(
        &mut self,
        anchor: &Anchor<'a>,
        label: Option<&ContentAttribute<'a>>,
        _metadata: Option<&BlockMetadata<'a>>,
        title: Option<Title<'a>>,
        caption: Option<Caption<'a>>,
    ) -> Result<(), Self::Error> {
        self.entries
            .entry(anchor.id)
            .or_insert_with(|| Reference::for_anchor(anchor, label, title, caption));
        Ok(())
    }

    fn untitled(&mut self, id: &'a str, location: &Location) -> Result<(), Self::Error> {
        self.entries
            .entry(id)
            .or_insert_with(|| Reference::new(id, location.clone()));
        Ok(())
    }

    fn cross_reference(&mut self, reference: &CrossReference<'a>) -> Result<(), Self::Error> {
        if reference.text.is_empty() {
            self.automatic.push(reference.target);
        }
        self.needs_document_top |= reference.target.is_empty() && reference.target_is_local;
        Ok(())
    }
}

#[derive(Default)]
struct Occurrences {
    namespace: Option<Arc<()>>,
    seen: HashSet<SectionOccurrence>,
    next: usize,
}

impl Occurrences {
    fn key(&mut self, section: &Section<'_>) -> SectionOccurrence {
        if let Some(existing) = &section.occurrence
            && self.seen.insert(existing.clone())
        {
            return existing.clone();
        }
        let namespace = self.namespace.get_or_insert_with(|| Arc::new(())).clone();
        let occurrence = SectionOccurrence::new(namespace, self.next);
        self.next += 1;
        occurrence
    }
}

struct AssignSections {
    occurrences: Occurrences,
}

impl<'a> ReferenceVisitor<'a> for AssignSections {
    type Error = std::convert::Infallible;

    fn section(&mut self, section: &mut Section<'a>, _record_toc: bool) -> Result<(), Self::Error> {
        section.occurrence = Some(self.occurrences.key(section));
        if let Some(id) = Section::explicit_id(&section.metadata) {
            section.id = Some(id);
            section.id_generated = false;
        }
        Ok(())
    }
}

fn document_top<'a>(document: &Document<'a>) -> Result<Reference<'a>, ReferenceRepairError> {
    let previous = document.references.get("");
    let origin = previous.and_then(|reference| reference.document_top_label);
    if document.attributes.text("reftext").is_some() && origin != Some(DocumentTopLabel::Attribute)
    {
        return Err(ReferenceRepairError::MissingDocumentReftext {
            location: Box::new(document.location.clone()),
        });
    }
    let title = document
        .header
        .as_ref()
        .map(reference_walk::header_title)
        .or_else(|| {
            document.blocks.iter().find_map(|block| {
                if let Block::Section(section) = block {
                    Some(section.title.clone())
                } else {
                    None
                }
            })
        });
    let location = document.header.as_ref().map_or_else(
        || document.location.clone(),
        |header| header.location.clone(),
    );
    let mut reference = Reference::new("", location);
    reference.title = title.map(|title| ContentAttribute::new(title.into_inlines()));
    reference.document_top_label = Some(origin.unwrap_or(DocumentTopLabel::Header));
    reference.xreflabel = if origin == Some(DocumentTopLabel::Attribute) {
        previous.and_then(|reference| reference.xreflabel.clone())
    } else {
        document.header.as_ref().and_then(|header| {
            displayed_reference_label(reference_walk::metadata_xreflabel(&header.metadata))
        })
    };
    Ok(reference)
}

impl Document<'_> {
    /// Rebuild reference and TOC membership from the current prepared tree and
    /// reassign section numbers. The first occurrence of an ID owns its target.
    ///
    /// Assigned generated IDs are retained. New sections need an explicit ID;
    /// this operation does not generate source text, parse labels, or retarget
    /// cross-reference uses. Existing aliases follow surviving canonical targets.
    /// Document `reftext` retains its prepared declaration snapshot. Caption,
    /// footnote, index and author registrations remain unchanged.
    ///
    /// # Errors
    /// Returns an error without changing the document if a section has no assigned
    /// or explicit ID, or a document-top label lacks prepared attribute context.
    pub fn rebuild_references_and_toc(&mut self) -> Result<(), ReferenceRepairError> {
        let mut rebuilt = RebuiltReferences {
            entries: HashMap::new(),
            toc: Vec::new(),
            occurrences: Occurrences::default(),
            automatic: Vec::new(),
            needs_document_top: false,
        };
        if let Some(header) = &self.header {
            reference_walk::header(&mut rebuilt, header)?;
        }
        reference_walk::blocks(&mut rebuilt, &mut self.blocks, true)?;
        if rebuilt.needs_document_top {
            rebuilt.entries.insert("", document_top(self)?);
        }
        for target in &rebuilt.automatic {
            let canonical = rebuilt
                .entries
                .get(target)
                .map(|reference| reference.target)
                .or_else(|| {
                    self.references
                        .get(target)
                        .filter(|reference| reference.target != *target)
                        .map(|reference| reference.target)
                });
            if let Some(reference) = canonical.and_then(|target| rebuilt.entries.get_mut(target))
                && reference.bibliography
            {
                reference.automatic_citation = true;
            }
        }
        for (&alias, reference) in &self.references {
            if alias != reference.target
                && !rebuilt.entries.contains_key(alias)
                && let Some(canonical) = rebuilt.entries.get(reference.target).cloned()
            {
                rebuilt.entries.insert(alias, canonical);
            }
        }
        reference_walk::blocks(
            &mut AssignSections {
                occurrences: Occurrences {
                    namespace: rebuilt.occurrences.namespace.clone(),
                    ..Occurrences::default()
                },
            },
            &mut self.blocks,
            true,
        )
        .unwrap_or_else(|never| match never {});
        self.references = rebuilt.entries;
        self.toc_entries = rebuilt.toc;
        self.renumber_sections();
        Ok(())
    }
}
