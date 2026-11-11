use super::ExtensionParseResult;
use crate::{
    Error, Warning, WarningKind,
    grammar::reference_walk::{self, ReferenceVisitor},
};

#[derive(Default)]
struct Uses<'a> {
    xrefs: Vec<(&'a str, crate::Location)>,
    seen: std::collections::HashMap<&'a str, crate::Location>,
    warnings: Vec<Warning>,
}
impl<'a> Uses<'a> {
    fn register(&mut self, id: &'a str, location: &crate::Location) {
        if let Some(first) = self.seen.get(id) {
            if first != location {
                self.warnings.push(Warning::new(
                    WarningKind::DuplicateId {
                        id: id.into(),
                        first: Box::new(crate::SourceLocation::at_location(None, first.clone())),
                    },
                    Some(crate::SourceLocation::at_location(None, location.clone())),
                ));
            }
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
            document
                .rebuild_footnotes()
                .map_err(|error| failure(document, error))?;
            document
                .rebuild_extension_references()
                .map_err(|error| failure(document, error))?;
            if let Some(index) = self.targets.get() {
                index.repair_section_references(document);
            }
            let mut uses = Uses::default();
            if let Some(header) = &document.header {
                reference_walk::header(&mut uses, header).unwrap_or_else(|never| match never {});
            }
            reference_walk::blocks(&mut uses, &mut document.blocks, true)
                .unwrap_or_else(|never| match never {});
            crate::grammar::walk_document_inline_nodes_mut(document, &mut |inline| {
                if let crate::InlineNode::Macro(crate::InlineMacro::CrossReference(reference)) =
                    inline
                    && (reference.target_is_local
                        || reference.source_syntax.target_is_local(reference.target))
                {
                    uses.xrefs
                        .push((reference.target, reference.location.clone()));
                }
            });
            uses.warnings.extend(
                uses.xrefs
                    .into_iter()
                    .filter(|(target, _)| !document.references.contains_key(target))
                    .map(|(target, location)| {
                        Warning::new(
                            WarningKind::UnresolvedReference {
                                target: target.into(),
                            },
                            Some(crate::SourceLocation::at_location(None, location)),
                        )
                    }),
            );
            Ok::<_, Error>(uses.warnings)
        })?;
        self.parsed.replace_reference_warnings(warnings);
        if let Some(index) = self.targets.get_mut() {
            index.reserve_warnings(&self.parsed);
        }
        Ok(())
    }
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

pub(crate) fn repair_macros(document: &mut crate::Document<'_>) -> Result<(), Error> {
    document
        .rebuild_footnotes()
        .map_err(|error| failure(document, error))?;
    document
        .rebuild_references_and_toc()
        .map_err(|error| failure(document, error))
}
