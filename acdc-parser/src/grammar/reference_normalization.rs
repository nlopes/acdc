//! Normalize accepted reference uses before resolution and share their targets with catalog copies.

use std::collections::{HashMap, hash_map::Entry};

use crate::{
    Document, Error, InlineMacro, InlineNode, Options,
    extensions::{ReferenceNormalizer, Registry},
    model::XrefSourceSyntax,
};

use super::{ParserState, location_walk::try_walk_owned_inline_nodes_mut};

struct Normalization<'a> {
    processors: Vec<Box<ReferenceNormalizer>>,
    force_local: bool,
    // Diagnostics, the tree, and copied titles can all refer to the same occurrence.
    rewritten: HashMap<(usize, usize, &'a str), &'a str>,
}

impl<'a> Normalization<'a> {
    fn new(options: &Options<'_>) -> Self {
        Self {
            processors: options
                .extensions
                .iter()
                .flat_map(Registry::reference_normalizers)
                .collect(),
            force_local: options.force_local_file_xrefs(),
            rewritten: HashMap::new(),
        }
    }

    fn is_active(&self) -> bool {
        self.force_local || !self.processors.is_empty()
    }

    fn normalize(
        &mut self,
        state: &ParserState<'a>,
        node: &mut InlineNode<'a>,
    ) -> Result<(), Error> {
        let InlineNode::Macro(InlineMacro::CrossReference(xref)) = node else {
            return Ok(());
        };
        let target = &mut xref.target;
        let location = &xref.location;
        if !self.processors.is_empty() && !excluded(target) {
            let key = (location.absolute_start, location.absolute_end, *target);
            *target = match self.rewritten.entry(key) {
                Entry::Occupied(entry) => *entry.get(),
                Entry::Vacant(entry) => {
                    let source = state.create_error_source_location(location.clone());
                    let replacement = rewrite(&mut self.processors, target, &source)
                        .map_err(|message| Error::Extension(Box::new(source), message))?;
                    *entry.insert(
                        replacement
                            .as_deref()
                            .map_or(*target, |value| state.intern_str(value)),
                    )
                }
            };
        }
        if self.force_local
            && let Some((path, fragment)) = target.split_once('#')
            && !fragment.is_empty()
            && !excluded(path)
        {
            *target = fragment;
            xref.source_syntax = XrefSourceSyntax::LocalFragment;
        }
        xref.target_is_local = xref.source_syntax.target_is_local(xref.target);
        Ok(())
    }
}

fn rewrite(
    processors: &mut [Box<ReferenceNormalizer>],
    target: &str,
    source: &crate::SourceLocation,
) -> Result<Option<String>, String> {
    let mut replacement = None;
    for processor in processors {
        let current = replacement.as_deref().unwrap_or(target);
        if excluded(current) {
            break;
        }
        if let Some(next) = processor(current, source)? {
            replacement = Some(next);
        }
    }
    Ok(replacement)
}

/// Preserve network paths and multi-letter URI schemes. A single letter can be a Windows drive.
fn excluded(target: &str) -> bool {
    target.starts_with("//") || crate::preprocessor::uri_scheme(target).is_some()
}

pub(super) fn normalize_document<'a>(
    state: &ParserState<'a>,
    document: &mut Document<'a>,
) -> Result<(), Error> {
    let mut normalization = Normalization::new(&state.options);
    if normalization.is_active() {
        super::try_walk_document_inline_nodes_mut(document, &mut |node| {
            normalization.normalize(state, node)
        })?;
    }
    Ok(())
}

pub(crate) fn normalize_fragment<'a>(
    state: &ParserState<'a>,
    inlines: &mut [InlineNode<'a>],
) -> Result<(), Error> {
    let mut normalization = Normalization::new(&state.options);
    if normalization.is_active() {
        for node in inlines {
            try_walk_owned_inline_nodes_mut(node, &mut |node| {
                normalization.normalize(state, node)
            })?;
        }
    }
    Ok(())
}
