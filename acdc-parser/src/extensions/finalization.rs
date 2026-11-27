use super::ExtensionParseResult;
use crate::{
    Error, Warning, WarningKind,
    grammar::reference_walk::{self, ReferenceVisitor},
};

#[derive(Default)]
struct Uses<'a> {
    seen: std::collections::HashMap<&'a str, crate::Location>,
    warnings: Vec<Warning>,
}
impl<'a> Uses<'a> {
    fn register(&mut self, id: &'a str, location: &crate::Location) {
        if let Some(first) = self.seen.get(id) {
            self.warnings.push(Warning::new(
                WarningKind::DuplicateId {
                    id: id.into(),
                    first: Box::new(crate::SourceLocation::at_location(None, first.clone())),
                },
                Some(crate::SourceLocation::at_location(None, location.clone())),
            ));
        } else {
            self.seen.insert(id, location.clone());
        }
    }
}
impl<'a> ReferenceVisitor<'a> for Uses<'a> {
    type Error = std::convert::Infallible;
    fn header(&mut self, header: &crate::Header<'a>) -> Result<(), Self::Error> {
        if let Some(anchor) = header.metadata.heading_anchor() {
            self.register(anchor.id, &anchor.location);
        }
        Ok(())
    }
    fn section(&mut self, section: &mut crate::Section<'a>, _: bool) -> Result<(), Self::Error> {
        if let Some(id) = section.id {
            self.register(id, &section.heading_location);
        }
        Ok(())
    }
    fn anchor(
        &mut self,
        anchor: &crate::Anchor<'a>,
        _: Option<&crate::ContentAttribute<'a>>,
        _: Option<&crate::BlockMetadata<'a>>,
        _: Option<crate::Title<'a>>,
        _: Option<crate::Caption<'a>>,
    ) -> Result<(), Self::Error> {
        self.register(anchor.id, &anchor.location);
        Ok(())
    }
    fn untitled(&mut self, id: &'a str, location: &crate::Location) -> Result<(), Self::Error> {
        self.register(id, location);
        Ok(())
    }
}

impl ExtensionParseResult {
    pub(super) fn repair_after_edit(&mut self) -> Result<(), Error> {
        let warnings = self.parsed.with_document_mut(|_, document| {
            repair_document(document, self.targets.get().map(Box::as_ref))
        })?;
        self.parsed.replace_reference_warnings(warnings);
        if let Some(index) = self.targets.get_mut() {
            index.reserve_warnings(&self.parsed);
        }
        Ok(())
    }
}

pub(super) fn repair_document(
    document: &mut crate::Document<'_>,
    index: Option<&super::query::TargetIndex>,
) -> Result<Vec<Warning>, Error> {
    let mut repaired = document
        .with_repaired_footnotes()
        .map_err(|error| failure(document, error))?;
    let candidate = repaired.as_mut().unwrap_or(document);
    candidate
        .rebuild_extension_references()
        .map_err(|error| failure(candidate, error))?;
    if let Some(repaired) = repaired {
        document.commit_repair(repaired);
    }

    if let Some(index) = index {
        index.repair_section_references(document);
    }
    let mut uses = Uses::default();
    if let Some(header) = &document.header {
        reference_walk::header(&mut uses, header).unwrap_or_else(|never| match never {});
    }
    reference_walk::blocks(&mut uses, &mut document.blocks, true)
        .unwrap_or_else(|never| match never {});
    let ids: std::collections::HashSet<_> = document.references.keys().copied().collect();
    crate::grammar::walk_document_inline_nodes_mut(document, &mut |inline| {
        if let crate::InlineNode::CrossReference(reference) = inline
            && (reference.target_is_local
                || reference.source_syntax.target_is_local(reference.target))
            && !ids.contains(reference.target)
        {
            uses.warnings.push(Warning::new(
                WarningKind::UnresolvedReference {
                    target: reference.target.into(),
                },
                Some(crate::SourceLocation::at_location(
                    None,
                    reference.location.clone(),
                )),
            ));
        }
    });
    Ok::<_, Error>(uses.warnings)
}

fn failure(document: &crate::Document<'_>, error: impl std::fmt::Display) -> Error {
    Error::Extension(
        Box::new(crate::SourceLocation::at_location(
            None,
            document.location.clone(),
        )),
        error.to_string(),
    )
}
