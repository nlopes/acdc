//! Finalization over the accepted document tree.

use std::collections::{HashMap, HashSet};

use crate::{
    Block, Document, DocumentAttributes, Error, Header, Location, Position, Warning, WarningKind,
    grammar::{ParserState, helpers::PositionWithOffset},
    model::{caption, section},
};

use super::{
    doctype::is_book_doctype,
    header::ParsedHeader,
    references::{
        CrossReferenceUse, ReferenceCatalog, collect_header_references, collect_references,
        finalize_inline_semantics, normalize_bibliography_lists, register_document_references,
        resolve_source_target, resolve_xref_target,
    },
};

pub(super) fn finish<'a>(
    state: &mut ParserState<'a>,
    start: PositionWithOffset,
    end: usize,
    parsed_header: ParsedHeader<'a>,
    attributes: DocumentAttributes<'a>,
    mut blocks: Vec<Block<'a>>,
) -> Result<Document<'a>, Error> {
    let ParsedHeader {
        mut header,
        untitled_location: untitled_header_location,
    } = parsed_header;
    warn_section_levels(state, header.as_ref(), &blocks);
    normalize_bibliography_lists(state, &mut blocks);
    crate::grammar::reference_text::resolve_tree(state, &mut header, &mut blocks)?;
    super::table::finalize_footnotes(&mut blocks);
    caption::renumber_captions(&mut blocks);

    let header_has_anchor = header
        .as_ref()
        .is_some_and(|header| header.metadata.id.is_some() || !header.metadata.anchors.is_empty());
    let mut references = ReferenceCatalog {
        entries: HashMap::with_capacity(state.toc_entries.len() + usize::from(header_has_anchor)),
        next_suffix: HashMap::new(),
        natural_targets: HashMap::new(),
    };
    let mut xrefs = Vec::new();
    if let Some(header) = &header {
        collect_header_references(state, header, &attributes, &mut references, &mut xrefs);
    }
    collect_references(state, &mut blocks, &mut references, &mut xrefs);
    section::number_parsed_sections(
        &mut blocks,
        &mut state.toc_entries,
        &mut references.entries,
        is_book_doctype(&attributes),
    );

    let mut document = Document {
        header,
        untitled_header_location,
        location: document_location(state, start, end),
        attributes,
        blocks,
        footnotes: state.footnote_tracker.borrow().footnotes.clone(),
        toc_entries: state.toc_entries.clone(),
        references: references.entries,
    };
    register_document_references(
        state,
        &mut document,
        &xrefs,
        &mut references.natural_targets,
    )?;
    let reference_ids = document.references.keys().copied().collect::<HashSet<_>>();
    validate_references(
        state,
        &mut document,
        xrefs,
        &reference_ids,
        &references.natural_targets,
    );
    finalize_inline_semantics(
        state,
        &mut document,
        &reference_ids,
        &references.natural_targets,
    )?;
    Ok(document)
}

fn document_location(state: &ParserState<'_>, start: PositionWithOffset, end: usize) -> Location {
    let mut end = state.source_snap(end, crate::grammar::utf8_utils::RoundDirection::Forward);
    if end != 0 {
        end = state.source_step(end, crate::grammar::utf8_utils::RoundDirection::Backward);
    }
    // Whitespace-only input can put the start beyond the last content byte.
    let absolute_end = end.max(start.offset);
    let (start_position, end_position) =
        if state.source_len() == 0 || (start.offset == 0 && absolute_end == 0) {
            (Position::new(1, 0), Position::new(1, 0))
        } else {
            (start.position, state.source_position(absolute_end))
        };
    Location {
        absolute_start: start.offset,
        absolute_end,
        start: start_position,
        end: end_position,
    }
}

fn warn_section_levels(state: &ParserState<'_>, header: Option<&Header<'_>>, blocks: &[Block<'_>]) {
    if state.toc_entries.is_empty() {
        return;
    }
    let mut anchored = header.is_some_and(|header| !header.title.is_empty());
    let mut seen_section = false;
    for block in blocks {
        if let Block::Section(section) = block {
            // Without a title or preamble, the first section establishes the base level.
            if !anchored {
                break;
            }
            if section.level > 1 {
                state.add_warning(Warning::new(
                    WarningKind::SectionLevelOutOfSequence {
                        expected: 1,
                        got: section.level,
                    },
                    Some(state.create_error_source_location(section.location.clone())),
                ));
            }
            seen_section = true;
        } else if !seen_section && !matches!(block, Block::Comment(_)) {
            anchored = true;
        }
    }
}

fn validate_references<'a>(
    state: &ParserState<'a>,
    document: &mut Document<'a>,
    xrefs: Vec<CrossReferenceUse<'a>>,
    reference_ids: &HashSet<&'a str>,
    natural_targets: &HashMap<&'a str, &'a str>,
) {
    for xref in xrefs {
        let source_target = resolve_source_target(state, xref.target, xref.source_syntax);
        let target = source_target.unwrap_or_else(|| {
            resolve_xref_target(
                xref.target,
                xref.resolve_natural_target,
                reference_ids,
                natural_targets,
            )
        });
        let local = source_target.is_some() || xref.source_syntax.target_is_local(target);
        if local
            && xref.automatic
            && let Some(reference) = document.references.get_mut(target)
            && reference.is_bibliography()
        {
            reference.automatic_citation = true;
        }
        if local && !reference_ids.contains(target) {
            state.add_warning(Warning::new(
                WarningKind::UnresolvedReference {
                    target: target.to_owned(),
                },
                Some(state.create_error_source_location(xref.location)),
            ));
        }
    }
}
