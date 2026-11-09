//! Reference registration and document finalization.

use crate::{
    Anchor, Block, BlockMetadata, ContentAttribute, DelimitedBlockType, Document,
    DocumentAttributes, Header, InlineMacro, InlineNode, Location, ReferenceTarget, Section, Title,
    TocEntry, Warning, WarningKind,
    grammar::{
        ParserState,
        document::sections::warn_for_nested_special_section,
        helpers::{BlockParsingMetadata, is_valid_bibliography_id},
        location_walk::try_walk_document_inline_nodes_mut,
        reference_walk::{
            self, ReferenceVisitor, header_title as header_reference_title, metadata_xreflabel,
        },
    },
    model::{
        Caption, SectionKind, SectionLevel, Substitution, displayed_reference_label, section,
        substitution::SubstitutionPlan,
    },
};
use std::{
    collections::{HashMap, HashSet},
    mem::take,
};

/// Insert an anchor into the cross-reference catalog with optional reference text.
///
/// The first registration of an id wins, matching asciidoctor: when two elements
/// claim the same id, `<<id>>` uses the reference text of the one that claimed it
/// first.
fn insert_reference<'a>(
    state: &mut ParserState<'a>,
    refs: &mut ReferenceCatalog<'a>,
    anchor: &Anchor<'a>,
    label: Option<&ContentAttribute<'a>>,
    natural_label: Option<&'a str>,
    title: Option<Title<'a>>,
    caption: Option<Caption<'a>>,
) {
    if refs.duplicate(state, anchor.id, &anchor.location) {
        return;
    }
    if !anchor.is_bibliography()
        && let Some(label) = natural_label.filter(|label| !label.trim().is_empty())
    {
        refs.natural_targets.entry(label).or_insert(anchor.id);
    }
    refs.entries.insert(
        anchor.id,
        ReferenceTarget::for_anchor(anchor, label, title, caption),
    );
}

pub(super) struct ReferenceCatalog<'a> {
    pub(super) entries: HashMap<&'a str, ReferenceTarget<'a>>,
    pub(super) next_suffix: HashMap<(&'a str, Option<char>), usize>,
    pub(super) natural_targets: HashMap<&'a str, &'a str>,
}

impl<'a> ReferenceCatalog<'a> {
    fn duplicate(&self, state: &mut ParserState<'a>, id: &str, location: &Location) -> bool {
        let Some(first) = self.entries.get(id) else {
            return false;
        };
        // Synthetic children can share their parent's source anchor.
        if first.location != *location {
            state.add_warning(Warning::new(
                WarningKind::DuplicateId {
                    id: id.to_owned(),
                    first: Box::new(state.create_error_source_location(first.location.clone())),
                },
                Some(state.create_error_source_location(location.clone())),
            ));
        }
        true
    }

    fn section_id(&mut self, state: &ParserState<'a>, section: &Section<'a>) -> &'a str {
        if let Some(id) = Section::explicit_id(&section.metadata) {
            return id;
        }
        let base = section.id_settings.generate(
            state.arena,
            &section.metadata,
            section.title.as_inlines(),
        );
        if !self.entries.contains_key(base) {
            return base;
        }
        let separator = section.id_settings.separator;
        let next = self.next_suffix.entry((base, separator)).or_insert(2);
        loop {
            let candidate = if let Some(separator) = separator {
                format!("{base}{separator}{next}")
            } else {
                format!("{base}{next}")
            };
            *next += 1;
            if !self.entries.contains_key(candidate.as_str()) {
                return state.intern_str(&candidate);
            }
        }
    }
}

pub(super) struct CrossReferenceUse<'a> {
    pub(super) target: &'a str,
    pub(super) location: Location,
    pub(super) automatic: bool,
    pub(super) resolve_natural_target: bool,
    pub(super) source_syntax: crate::model::XrefSourceSyntax,
}

fn insert_untitled_reference<'a>(
    state: &mut ParserState<'a>,
    refs: &mut ReferenceCatalog<'a>,
    id: &'a str,
    location: &Location,
) {
    if refs.duplicate(state, id, location) {
        return;
    }
    refs.entries.insert(
        id,
        ReferenceTarget {
            occurrence: None,
            document_label: None,
            target: id,
            xreflabel: None,
            title: None,
            location: location.clone(),
            caption: None,
            section: None,
            bibliography: false,
            automatic_citation: false,
        },
    );
}

pub(super) fn finalize_inline_semantics<'a>(
    state: &ParserState<'a>,
    document: &mut Document<'a>,
    reference_ids: &HashSet<&'a str>,
    natural_targets: &HashMap<&'a str, &'a str>,
) -> Result<(), crate::Error> {
    let document_targets = document
        .references
        .iter()
        .filter_map(|(target, reference)| reference.document_label.is_some().then_some(*target))
        .collect::<HashSet<_>>();
    let caption_kinds = document
        .references
        .iter()
        .filter_map(|(target, reference)| {
            let Caption::Numbered { kind, .. } = reference.caption.as_ref()? else {
                return None;
            };
            Some((*target, *kind))
        })
        .collect::<HashMap<_, _>>();
    let section_names = document
        .references
        .iter()
        .filter_map(|(target, reference)| Some((*target, reference.section.as_ref()?.name)))
        .collect::<HashMap<_, _>>();

    for footnote in &mut document.footnotes {
        finalize_footnote_text(
            &mut footnote.registration_substitutions,
            &mut footnote.content,
        );
    }
    try_walk_document_inline_nodes_mut(document, &mut |inline| {
        finalize_registered_inline(state, inline);
        let InlineNode::Macro(InlineMacro::CrossReference(xref)) = inline else {
            return Ok(());
        };
        if let Some(target) = resolve_source_target(state, xref.target, xref.source_syntax) {
            xref.target = target;
            xref.target_is_local = true;
        } else {
            let target = resolve_xref_target(
                xref.target,
                xref.resolve_natural_target,
                reference_ids,
                natural_targets,
            );
            if target != xref.target {
                xref.target_is_local = true;
            }
            xref.target = target;
        }
        let Some(snapshot) = xref.caption_label_snapshot_id.take() else {
            return Ok(());
        };
        if let Some(kind) = caption_kinds.get(xref.target) {
            xref.caption_label = state.xref_caption_label(snapshot, *kind);
        }
        if xref.target_is_local
            && document_targets.contains(xref.target)
            && let Some(source) = state.xref_document_reftext(snapshot)
        {
            xref.document_reftext = Some(crate::grammar::reference_text::resolve_shared(
                state, source,
            )?);
        }
        xref.section_signifiers = state.xref_signifiers(snapshot);
        xref.refresh_signifier(section_names.get(xref.target).copied());
        Ok(())
    })
}

fn metadata_natural_label<'a>(
    state: &ParserState<'a>,
    metadata: &BlockMetadata<'a>,
) -> Option<&'a str> {
    let source = metadata.reftext_source.or_else(|| {
        metadata
            .anchors
            .last()
            .and_then(|anchor| anchor.reftext_source)
    });
    crate::grammar::reference_text::text(state, source)
}

pub(super) struct ParsedSectionHeader<'a> {
    pub(super) heading_location: Location,
    pub(super) title: Title<'a>,
    pub(super) numbering: section::SectionNumbering,
    pub(super) reference_text: &'a str,
    pub(super) id_settings: section::SectionIdSettings<'a>,
}

pub(super) fn register_section_header<'a>(
    state: &mut ParserState<'a>,
    block_metadata: &BlockParsingMetadata<'a>,
    title: Title<'a>,
    natural_title: &'a str,
    level: SectionLevel,
    location: Location,
    direct_parent_section_kind: Option<SectionKind>,
) -> ParsedSectionHeader<'a> {
    let reference_text = metadata_natural_label(state, &block_metadata.metadata)
        .filter(|label| !label.trim().is_empty())
        .unwrap_or(natural_title);

    let kind = SectionKind::from_style(block_metadata.metadata.style);
    state.toc_entries.push(TocEntry::for_section(
        "",
        title.clone(),
        level,
        None,
        kind,
        location.clone(),
    ));
    warn_for_nested_special_section(state, direct_parent_section_kind, location.clone());

    let numbering = section::SectionNumbering::from_attributes(&state.document_attributes);
    let id_settings =
        section::SectionIdSettings::from_attributes(state.arena, &state.document_attributes);
    ParsedSectionHeader {
        heading_location: location,
        title,
        numbering,
        reference_text,
        id_settings,
    }
}

// Resolve against files that preprocessing already included. Cross-references never
// load their targets, and xref paths are compared as written, as in Asciidoctor.
#[allow(
    clippy::case_sensitive_file_extension_comparisons,
    reason = "Asciidoctor recognizes only the lowercase .adoc suffix in xref macros"
)]
pub(super) fn resolve_source_target<'a>(
    state: &ParserState<'_>,
    target: &'a str,
    syntax: crate::model::XrefSourceSyntax,
) -> Option<&'a str> {
    use crate::model::XrefSourceSyntax;

    if matches!(
        syntax,
        XrefSourceSyntax::Literal | XrefSourceSyntax::LocalFragment
    ) {
        return None;
    }
    let (path, fragment) = match target.split_once('#') {
        Some(parts) => parts,
        None if matches!(syntax, XrefSourceSyntax::Macro) && target.ends_with(".adoc") => {
            (target, "")
        }
        None => return None,
    };
    if path.is_empty() {
        return Some(fragment);
    }
    let stem = match syntax {
        XrefSourceSyntax::Shorthand => [".adoc", ".asciidoc", ".asc", ".ad", ".txt"]
            .iter()
            .find_map(|extension| path.strip_suffix(extension))
            .unwrap_or(path),
        XrefSourceSyntax::Macro => path
            .strip_suffix(".adoc")
            .or_else(|| (!path.rsplit('/').next().unwrap_or(path).contains('.')).then_some(path))?,
        XrefSourceSyntax::Literal | XrefSourceSyntax::LocalFragment => return None,
    };
    (state.document_attributes.text("docname") == Some(stem) || state.included_files.contains(stem))
        .then_some(fragment)
}

pub(super) fn register_document_references<'a>(
    state: &mut ParserState<'a>,
    document: &mut Document<'a>,
    xrefs: &[CrossReferenceUse<'a>],
    natural_targets: &mut HashMap<&'a str, &'a str>,
) -> Result<(), crate::Error> {
    let needs_top = xrefs
        .iter()
        .any(|xref| resolve_source_target(state, xref.target, xref.source_syntax) == Some(""));
    let header_id = document
        .header
        .as_ref()
        .and_then(|header| header.metadata.heading_anchor().map(|anchor| anchor.id));
    if !needs_top && header_id.is_none() {
        return Ok(());
    }
    let title = document
        .header
        .as_ref()
        .map(header_reference_title)
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
    let has_attribute = document.attributes.assignment("reftext").is_some()
        || document.attributes.contains_key("reftext");
    let label = if has_attribute {
        let label = document.attributes.text("reftext").unwrap_or("");
        let source = state.header_reftext_source.unwrap_or_else(|| {
            crate::grammar::reference_text::capture_unknown(state, state.intern_str(label))
        });
        let content = crate::grammar::reference_text::resolve(state, source)?;
        displayed_reference_label(Some(&content))
    } else {
        document
            .header
            .as_ref()
            .and_then(|header| displayed_reference_label(metadata_xreflabel(&header.metadata)))
    };
    let origin = if has_attribute {
        crate::model::DocumentReferenceLabel::Attribute
    } else {
        crate::model::DocumentReferenceLabel::Header
    };
    if let Some(id) = header_id {
        if let Some(reference) = document.references.get_mut(id) {
            reference.xreflabel.clone_from(&label);
            reference.document_label = Some(origin);
        }
        if has_attribute
            && let Some(label) = document
                .attributes
                .text("reftext")
                .filter(|label| !label.trim().is_empty())
        {
            natural_targets.insert(state.intern_str(label), id);
        }
    }
    if !needs_top {
        return Ok(());
    }
    document.references.insert(
        "",
        ReferenceTarget {
            occurrence: None,
            document_label: Some(origin),
            target: "",
            xreflabel: label,
            title: title.map(|title| ContentAttribute::new(title.into_inlines())),
            location,
            caption: None,
            section: None,
            bibliography: false,
            automatic_citation: false,
        },
    );
    Ok(())
}

/// Finalize a cross-reference target after IDs and reference-text aliases are known.
///
/// Exact IDs are kept. Reference-text lookup applies only when enabled at the
/// reference's source position. Missing aliases leave the target unchanged.
pub(super) fn resolve_xref_target<'a>(
    target: &'a str,
    resolve_natural_target: bool,
    reference_ids: &HashSet<&'a str>,
    natural_targets: &HashMap<&'a str, &'a str>,
) -> &'a str {
    if !resolve_natural_target || target.contains('#') || reference_ids.contains(target) {
        return target;
    }
    let resembles_reference_text = target.contains(' ') || target.chars().any(char::is_uppercase);
    if resembles_reference_text {
        natural_targets.get(target).copied().unwrap_or(target)
    } else {
        target
    }
}

fn register_section_reference<'a>(
    state: &mut ParserState<'a>,
    section: &mut Section<'a>,
    refs: &mut ReferenceCatalog<'a>,
) {
    let id = refs.section_id(state, section);
    section.id = Some(id);
    section.id_generated = Section::explicit_id(&section.metadata).is_none();
    let reference_text = section.reference_text.take();
    let xreflabel = metadata_xreflabel(&section.metadata);
    let mut location = section.location.clone();
    if let Some(last) = section.title.as_inlines().last() {
        location.absolute_end = last.location().absolute_end;
        location.end = last.location().end.clone();
    }
    if !refs.duplicate(state, id, &location) {
        if let Some(text) = reference_text.filter(|text| !text.is_empty()) {
            refs.natural_targets.entry(text).or_insert(id);
        }
        refs.entries.insert(
            id,
            ReferenceTarget {
                occurrence: None,
                document_label: None,
                target: id,
                xreflabel: displayed_reference_label(xreflabel),
                title: Some(ContentAttribute::new(section.title.clone().into_inlines())),
                location,
                caption: None,
                section: None,
                bibliography: false,
                automatic_citation: false,
            },
        );
    }
}

struct ParsingReferences<'s, 'a> {
    state: &'s mut ParserState<'a>,
    references: &'s mut ReferenceCatalog<'a>,
    uses: &'s mut Vec<CrossReferenceUse<'a>>,
}

impl<'a> ReferenceVisitor<'a> for ParsingReferences<'_, 'a> {
    type Error = std::convert::Infallible;

    fn section(&mut self, section: &mut Section<'a>, _record_toc: bool) -> Result<(), Self::Error> {
        register_section_reference(self.state, section, self.references);
        Ok(())
    }

    fn anchor(
        &mut self,
        anchor: &Anchor<'a>,
        label: Option<&ContentAttribute<'a>>,
        metadata: Option<&BlockMetadata<'a>>,
        title: Option<Title<'a>>,
        caption: Option<Caption<'a>>,
    ) -> Result<(), Self::Error> {
        let natural = metadata
            .and_then(|metadata| metadata_natural_label(self.state, metadata))
            .or_else(|| crate::grammar::reference_text::text(self.state, anchor.reftext_source));
        insert_reference(
            self.state,
            self.references,
            anchor,
            label,
            natural,
            title,
            caption,
        );
        Ok(())
    }

    fn untitled(&mut self, id: &'a str, location: &Location) -> Result<(), Self::Error> {
        insert_untitled_reference(self.state, self.references, id, location);
        Ok(())
    }

    fn cross_reference(
        &mut self,
        reference: &crate::CrossReference<'a>,
    ) -> Result<(), Self::Error> {
        self.uses.push(CrossReferenceUse {
            target: reference.target,
            location: reference.location.clone(),
            automatic: reference.text.is_empty(),
            resolve_natural_target: reference.resolve_natural_target,
            source_syntax: reference.source_syntax,
        });
        Ok(())
    }
}

pub(super) fn collect_header_references<'a>(
    state: &mut ParserState<'a>,
    header: &Header<'a>,
    attributes: &DocumentAttributes<'a>,
    references: &mut ReferenceCatalog<'a>,
    uses: &mut Vec<CrossReferenceUse<'a>>,
) {
    if let Some(anchor) = header.metadata.heading_anchor() {
        let natural =
            if attributes.assignment("reftext").is_some() || attributes.contains_key("reftext") {
                None
            } else {
                metadata_natural_label(state, &header.metadata)
            };
        insert_reference(
            state,
            references,
            anchor,
            metadata_xreflabel(&header.metadata),
            natural,
            Some(header_reference_title(header)),
            None,
        );
    }
    reference_walk::header(
        &mut ParsingReferences {
            state,
            references,
            uses,
        },
        header,
    )
    .unwrap_or_else(|never| match never {});
}

pub(super) fn collect_references<'a>(
    state: &mut ParserState<'a>,
    blocks: &mut [Block<'a>],
    references: &mut ReferenceCatalog<'a>,
    uses: &mut Vec<CrossReferenceUse<'a>>,
) {
    reference_walk::blocks(
        &mut ParsingReferences {
            state,
            references,
            uses,
        },
        blocks,
        true,
    )
    .unwrap_or_else(|never| match never {});
}

pub(super) fn normalize_bibliography_lists<'a>(state: &ParserState<'a>, blocks: &mut [Block<'a>]) {
    for block in blocks {
        match block {
            Block::Section(section) => {
                if section.kind == SectionKind::Bibliography {
                    for child in &mut section.content {
                        if let Block::UnorderedList(list) = child
                            && list.metadata.style.is_none()
                        {
                            list.metadata.style = Some("bibliography");
                        }
                    }
                }
                normalize_bibliography_lists(state, &mut section.content);
            }
            Block::UnorderedList(list) => {
                if list.metadata.style == Some("bibliography") {
                    for item in &mut list.items {
                        promote_bibliography_anchor(state, &mut item.principal);
                    }
                }
                for item in &mut list.items {
                    normalize_bibliography_lists(state, &mut item.blocks);
                }
            }
            Block::OrderedList(list) => {
                for item in &mut list.items {
                    normalize_bibliography_lists(state, &mut item.blocks);
                }
            }
            Block::CalloutList(list) => {
                for item in &mut list.items {
                    normalize_bibliography_lists(state, &mut item.blocks);
                }
            }
            Block::DescriptionList(list) => {
                for item in &mut list.items {
                    if let Some(definition) = &mut item.definition {
                        normalize_bibliography_lists(state, &mut definition.blocks);
                    }
                }
            }
            Block::Admonition(admonition) => {
                normalize_bibliography_lists(state, &mut admonition.blocks);
            }
            Block::DelimitedBlock(block) => match &mut block.inner {
                DelimitedBlockType::DelimitedExample(blocks)
                | DelimitedBlockType::DelimitedOpen(blocks)
                | DelimitedBlockType::DelimitedSidebar(blocks)
                | DelimitedBlockType::DelimitedQuote(blocks) => {
                    normalize_bibliography_lists(state, blocks);
                }
                DelimitedBlockType::DelimitedTable(table) => {
                    for row in table
                        .header
                        .iter_mut()
                        .chain(table.rows.iter_mut())
                        .chain(table.footer.iter_mut())
                    {
                        for column in &mut row.columns {
                            normalize_bibliography_lists(state, &mut column.content);
                        }
                    }
                }
                DelimitedBlockType::DelimitedListing(_)
                | DelimitedBlockType::DelimitedLiteral(_)
                | DelimitedBlockType::DelimitedPass(_)
                | DelimitedBlockType::DelimitedVerse(_)
                | DelimitedBlockType::DelimitedComment(_)
                | DelimitedBlockType::DelimitedStem(_) => {}
            },
            Block::Paragraph(_)
            | Block::DiscreteHeading(_)
            | Block::ThematicBreak(_)
            | Block::PageBreak(_)
            | Block::Image(_)
            | Block::Audio(_)
            | Block::Video(_)
            | Block::TableOfContents(_)
            | Block::DocumentAttribute(_)
            | Block::Comment(_) => {}
        }
    }
}

fn promote_bibliography_anchor<'a>(state: &ParserState<'a>, principal: &mut Vec<InlineNode<'a>>) {
    let [
        InlineNode::PlainText(open),
        InlineNode::InlineAnchor(anchor),
        InlineNode::PlainText(close),
        ..,
    ] = principal.as_mut_slice()
    else {
        return;
    };
    if open.content != "["
        || !close.content.starts_with(']')
        || !is_valid_bibliography_id(anchor.id)
        || open.location.absolute_start + 1 != anchor.location.absolute_start
        || anchor.location.absolute_end + 1 != close.location.absolute_start
    {
        return;
    }

    anchor.bibliography = true;
    if let Some(label) = &mut anchor.bibliography_label {
        anchor.xreflabel = None;
        anchor.reftext_source = label.source.take().map(|text| {
            let after_comma = anchor.location.absolute_start + anchor.id.len() + 3;
            let whitespace = state
                .input
                .get(after_comma..anchor.location.absolute_end.saturating_sub(1))
                .map_or(0, |authored| {
                    authored.len()
                        - crate::grammar::inline_processing::trim_bibliography_label(authored).len()
                });
            crate::grammar::reference_text::capture_inline(
                state,
                text,
                after_comma + whitespace,
                false,
            )
        });
    }
    anchor.location =
        state.create_location(open.location.absolute_start, close.location.absolute_start);
    let remove_close = if close.content == "]" {
        true
    } else {
        close.content = &close.content[1..];
        close.location = state.create_location(
            close.location.absolute_start + 1,
            close.location.absolute_end,
        );
        false
    };

    principal.remove(0);
    if remove_close {
        principal.remove(1);
    }
}

fn finalize_registered_inline<'a>(state: &ParserState<'a>, node: &mut InlineNode<'a>) {
    if let InlineNode::Macro(InlineMacro::IndexTerm(term)) = node {
        crate::grammar::passthrough_processing::prepare_index_catalog(term, state);
        if let Some(catalog) = &mut term.catalog {
            catalog.for_each_label_mut(|nodes| {
                *nodes = crate::grammar::passthrough_processing::resolve_registered_text(
                    take(nodes),
                    state,
                );
            });
        }
    } else if let InlineNode::Macro(InlineMacro::Footnote(footnote)) = node
        && let Some(content) = &mut footnote.content
    {
        finalize_footnote_text(&mut footnote.registration_substitutions, content);
    }
}

pub(super) fn finalize_footnote_text(
    plan: &mut Option<SubstitutionPlan>,
    content: &mut [InlineNode<'_>],
) {
    if let Some(plan) = plan.take() {
        freeze_registered_text(content, plan);
    }
}

fn freeze_registered_text(nodes: &mut [InlineNode<'_>], plan: SubstitutionPlan) {
    let mut substitutions = vec![Substitution::SpecialChars];
    if plan.precedes(&Substitution::Replacements, &Substitution::Macros) {
        substitutions.push(Substitution::Replacements);
    }
    for node in nodes {
        crate::grammar::passthrough_processing::convert_plain_to_raw(node, &substitutions);
    }
}
