use acdc_parser::{
    Document, Location,
    ast::{
        BlockKind, DescriptionListItemRef, DocumentRef, IdOrigin, InlineRef, SectionRef,
        StructuralRef,
    },
};

/// A retained ID occurrence, independent of reference-catalog registration.
pub(crate) struct IdOccurrence<'doc> {
    pub(crate) id: &'doc str,
    pub(crate) origin: IdOrigin,
    pub(crate) location: Location,
}

/// Visit every retained ID, including duplicate declarations.
pub(crate) fn occurrences<'doc>(
    doc: &'doc Document<'doc>,
) -> impl Iterator<Item = IdOccurrence<'doc>> {
    let doc = DocumentRef::from(doc);
    let header = doc.anchors().map(|anchor| (anchor.element_id(), None));
    let blocks = doc
        .descendants()
        .filter_map(|node| {
            if let StructuralRef::Block(block) = node {
                Some(block)
            } else {
                None
            }
        })
        .flat_map(|block| {
            let generated = if let BlockKind::Section(section) = block.kind() {
                section
                    .id()
                    .filter(|id| id.origin() == IdOrigin::Generated)
                    .map(|id| (id, Some(heading_location(section))))
            } else {
                None
            };
            generated
                .into_iter()
                .chain(block.anchors().map(|anchor| (anchor.element_id(), None)))
        });
    let terms = doc
        .descendants()
        .filter_map(|node| {
            if let StructuralRef::DescriptionListItem(item) = node {
                Some(item)
            } else {
                None
            }
        })
        .flat_map(DescriptionListItemRef::anchors)
        .map(|anchor| (anchor.element_id(), None));
    let inlines = doc
        .content_occurrences()
        .flat_map(|field| field.content().inlines())
        .filter_map(InlineRef::id)
        .map(|id| (id, None));
    header
        .chain(blocks)
        .chain(terms)
        .chain(inlines)
        .map(|(id, location)| IdOccurrence {
            id: id.as_str(),
            origin: id.origin(),
            location: location.unwrap_or_else(|| id.location().into()),
        })
}

/// Generated IDs select the heading, without treating its body as an ID spelling.
pub(super) fn heading_location(section: SectionRef<'_>) -> Location {
    let mut location: Location = section.location().into();
    if let Some(last) = section.title().and_then(|title| title.inlines().last()) {
        let end: Location = last.location().into();
        location.absolute_end = end.absolute_end;
        location.end = end.end;
    }
    location
}
