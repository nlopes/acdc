//! Reference registration and document finalization.

use crate::{
    Anchor, Block, BlockMetadata, ContentAttribute, DelimitedBlockType, Document, Footnote, Header,
    InlineMacro, InlineMetadata, InlineNode, Location, Plain, Reference, Section, Title, TocEntry,
    Warning, WarningKind,
    grammar::{
        ParserState,
        document::sections::warn_for_nested_special_section,
        helpers::{BlockParsingMetadata, is_valid_bibliography_id},
        location_walk::walk_document_inline_nodes_mut,
        marked_text::MarkedText,
    },
    model::{
        Caption, SectionKind, SectionLevel, Substitution, section, substitution::SubstitutionPlan,
    },
};
use std::collections::{HashMap, HashSet};

fn displayed_reference_label<'a>(
    label: Option<&ContentAttribute<'a>>,
) -> Option<ContentAttribute<'a>> {
    label
        .filter(|content| {
            !content.iter().all(|node| {
                if let InlineNode::PlainText(text) = node {
                    text.content.trim().is_empty()
                } else if let InlineNode::RawText(raw) = node {
                    raw.content.trim().is_empty()
                } else {
                    false
                }
            })
        })
        .cloned()
}

/// Insert an anchor into the cross-reference catalog with optional reference text.
///
/// The first registration of an id wins, matching asciidoctor: when two elements
/// claim the same id, `<<id>>` uses the reference text of the one that claimed it
/// first.
pub(super) fn insert_reference<'a>(
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
    let mut xreflabel = displayed_reference_label(label);
    if !anchor.is_bibliography()
        && let Some(label) = natural_label.filter(|label| !label.trim().is_empty())
    {
        refs.natural_targets.entry(label).or_insert(anchor.id);
    }
    if anchor.is_bibliography()
        && let Some(label) = xreflabel.as_mut()
    {
        label.inlines_mut().insert(
            0,
            InlineNode::PlainText(Plain {
                content: "[",
                location: anchor.location.clone(),
                escaped: false,
            }),
        );
        label.inlines_mut().push(InlineNode::PlainText(Plain {
            content: "]",
            location: anchor.location.clone(),
            escaped: false,
        }));
    }
    refs.entries.insert(
        anchor.id,
        Reference {
            xreflabel,
            title,
            location: anchor.location.clone(),
            caption,
            section: None,
            bibliography: anchor.is_bibliography(),
            automatic_citation: false,
        },
    );
}

pub(super) struct ReferenceCatalog<'a> {
    pub(super) entries: HashMap<&'a str, Reference<'a>>,
    pub(super) next_suffix: HashMap<&'a str, usize>,
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
        let base = Section::generate_id(state.arena, &section.metadata, &section.title)
            .as_arena_str(state.arena);
        if base.is_empty() || !self.entries.contains_key(base) {
            return base;
        }
        let next = self.next_suffix.entry(base).or_insert(2);
        loop {
            let candidate = format!("{base}_{next}");
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
        Reference {
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
) {
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
        finalize_footnote_text(footnote);
    }
    walk_document_inline_nodes_mut(document, &mut |inline| {
        finalize_registered_inline(inline);
        let InlineNode::Macro(InlineMacro::CrossReference(xref)) = inline else {
            return;
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
            return;
        };
        if let Some(kind) = caption_kinds.get(xref.target) {
            xref.caption_label = state.xref_caption_label(snapshot, *kind);
        }
        xref.section_signifiers = state.xref_signifiers(snapshot);
        xref.refresh_signifier(section_names.get(xref.target).copied());
    });
}

pub(super) fn metadata_xreflabel<'a, 'b>(
    metadata: &'b BlockMetadata<'a>,
) -> Option<&'b ContentAttribute<'a>> {
    metadata.reftext.as_ref().or_else(|| {
        metadata
            .anchors
            .last()
            .and_then(|anchor| anchor.xreflabel.as_ref())
    })
}

pub(super) fn metadata_natural_label<'a>(
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

pub(super) fn register_section_header<'a>(
    state: &mut ParserState<'a>,
    block_metadata: &BlockParsingMetadata<'a>,
    title: Title<'a>,
    natural_title: &'a str,
    level: SectionLevel,
    location: Location,
    direct_parent_section_kind: Option<SectionKind>,
) -> (Title<'a>, section::SectionNumbering, &'a str) {
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
    warn_for_nested_special_section(state, direct_parent_section_kind, location);

    let numbering = section::SectionNumbering::from_attributes(&state.document_attributes);
    (title, numbering, reference_text)
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

pub(super) fn register_document_top<'a>(
    state: &mut ParserState<'a>,
    document: &mut Document<'a>,
    xrefs: &[CrossReferenceUse<'a>],
) -> Result<(), crate::Error> {
    if !xrefs
        .iter()
        .any(|xref| resolve_source_target(state, xref.target, xref.source_syntax) == Some(""))
    {
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
    let label = if let Some(label) = document.attributes.text("reftext") {
        let source = crate::grammar::reference_text::capture_inline(
            state,
            state.intern_str(label),
            location.absolute_start,
            false,
        );
        let content = crate::grammar::reference_text::resolve(state, source)?;
        displayed_reference_label(Some(&content))
    } else {
        document
            .header
            .as_ref()
            .and_then(|header| displayed_reference_label(metadata_xreflabel(&header.metadata)))
    };
    document.references.insert(
        "",
        Reference {
            xreflabel: label,
            title,
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

/// Catalog a formatted span's ID and recurse into its inline content.
fn collect_formatted_references<'a, T>(
    state: &mut ParserState<'a>,
    text: &T,
    refs: &mut ReferenceCatalog<'a>,
    xrefs: &mut Vec<CrossReferenceUse<'a>>,
) where
    T: MarkedText<'a, Content = Vec<InlineNode<'a>>>,
{
    if let Some(id) = text.id() {
        insert_untitled_reference(state, refs, id, text.location());
    }
    collect_inline_references(state, text.content(), refs, xrefs);
}

fn collect_link_references<'a>(
    state: &mut ParserState<'a>,
    metadata: &InlineMetadata<'a>,
    location: &Location,
    text: &[InlineNode<'a>],
    refs: &mut ReferenceCatalog<'a>,
    xrefs: &mut Vec<CrossReferenceUse<'a>>,
) {
    if let Some(id) = &metadata.id {
        insert_untitled_reference(state, refs, crate::strip_quotes(id.id), location);
    }
    collect_inline_references(state, text, refs, xrefs);
}

pub(super) fn header_reference_title<'a>(header: &Header<'a>) -> Title<'a> {
    let mut inlines = header.title.clone().into_inlines();
    if let Some(subtitle) = &header.subtitle {
        inlines.push(InlineNode::PlainText(Plain {
            content: ": ",
            location: header.location.clone(),
            escaped: false,
        }));
        inlines.extend(subtitle.clone().into_inlines());
    }
    Title::new(inlines)
}

pub(super) fn collect_metadata_references<'a>(
    state: &mut ParserState<'a>,
    metadata: &BlockMetadata<'a>,
    refs: &mut ReferenceCatalog<'a>,
    xrefs: &mut Vec<CrossReferenceUse<'a>>,
) {
    if let Some(label) = metadata_xreflabel(metadata) {
        collect_inline_references(state, label, refs, xrefs);
    }
    if let Some(attribution) = &metadata.attribution {
        collect_inline_references(state, attribution, refs, xrefs);
    }
    if let Some(citetitle) = &metadata.citetitle {
        collect_inline_references(state, citetitle, refs, xrefs);
    }
}

fn register_section_reference<'a>(
    state: &mut ParserState<'a>,
    section: &mut Section<'a>,
    refs: &mut ReferenceCatalog<'a>,
) {
    let id = refs.section_id(state, section);
    section.id = Some(id);
    let reference_text = section.reference_text.take();
    let xreflabel = metadata_xreflabel(&section.metadata);
    let mut location = section.location.clone();
    if let Some(last) = section.title.last() {
        location.absolute_end = last.location().absolute_end;
        location.end = last.location().end.clone();
    }
    if !refs.duplicate(state, id, &location) {
        if let Some(text) = reference_text.filter(|text| !text.is_empty()) {
            refs.natural_targets.entry(text).or_insert(id);
        }
        refs.entries.insert(
            id,
            Reference {
                xreflabel: displayed_reference_label(xreflabel),
                title: Some(section.title.clone()),
                location,
                caption: None,
                section: None,
                bibliography: false,
                automatic_citation: false,
            },
        );
    }
}

/// Walk the final document tree to (1) populate the cross-reference catalog `refs` with
/// every anchor (block IDs, inline `[[id]]` anchors, formatted span IDs, and link IDs) and (2)
/// collect every `<<id>>` / `xref:id[]` use for unresolved-reference checking and
/// bibliography citation metadata. Titles and quote credits are part of this walk because
/// converters render their inline content too. A target with no title is still registered
/// (reference text `None`), so an `<<id>>` to it resolves to the literal `[id]` rather than
/// being treated as unresolved. Generated section IDs are assigned in source order,
/// including sections in nested documents.
pub(super) fn collect_references<'a>(
    state: &mut ParserState<'a>,
    blocks: &mut [Block<'a>],
    refs: &mut ReferenceCatalog<'a>,
    xrefs: &mut Vec<CrossReferenceUse<'a>>,
) {
    for block in blocks {
        if let Block::Section(section) = block {
            register_section_reference(state, section, refs);
        }
        if !matches!(block, Block::Section(_))
            && let Some(anchor) = block.anchor()
        {
            let caption = block
                .metadata()
                .and_then(|metadata| metadata.caption.clone());
            let label = block
                .metadata()
                .and_then(metadata_xreflabel)
                .or(anchor.xreflabel.as_ref());
            let natural = block
                .metadata()
                .and_then(|metadata| metadata_natural_label(state, metadata))
                .or_else(|| crate::grammar::reference_text::text(state, anchor.reftext_source));
            insert_reference(
                state,
                refs,
                anchor,
                label,
                natural,
                block.title().cloned(),
                caption,
            );
        }
        if let Some(title) = block.title() {
            collect_inline_references(state, title, refs, xrefs);
        }
        if let Some(metadata) = block.metadata() {
            collect_metadata_references(state, metadata, refs, xrefs);
        }

        match block {
            Block::Section(s) => {
                collect_references(state, &mut s.content, refs, xrefs);
            }
            Block::Paragraph(p) => collect_inline_references(state, &p.content, refs, xrefs),
            // A simple admonition's content is a synthetic paragraph that shares
            // the admonition's anchor; the admonition registered it first, so its
            // reference text stands.
            Block::Admonition(a) => collect_references(state, &mut a.blocks, refs, xrefs),
            Block::UnorderedList(l) => {
                for item in &mut l.items {
                    collect_inline_references(state, &item.principal, refs, xrefs);
                    collect_references(state, &mut item.blocks, refs, xrefs);
                }
            }
            Block::OrderedList(l) => {
                for item in &mut l.items {
                    collect_inline_references(state, &item.principal, refs, xrefs);
                    collect_references(state, &mut item.blocks, refs, xrefs);
                }
            }
            Block::CalloutList(l) => {
                for item in &mut l.items {
                    collect_inline_references(state, &item.principal, refs, xrefs);
                    collect_references(state, &mut item.blocks, refs, xrefs);
                }
            }
            Block::DescriptionList(l) => {
                for item in &mut l.items {
                    for anchor in &item.anchors {
                        insert_reference(
                            state,
                            refs,
                            anchor,
                            anchor.xreflabel.as_ref(),
                            crate::grammar::reference_text::text(state, anchor.reftext_source),
                            None,
                            None,
                        );
                    }
                    collect_inline_references(state, &item.term, refs, xrefs);
                    collect_inline_references(state, &item.principal_text, refs, xrefs);
                    collect_references(state, &mut item.description, refs, xrefs);
                }
            }
            Block::DelimitedBlock(d) => {
                collect_delimited_references(state, &mut d.inner, refs, xrefs);
            }
            Block::DiscreteHeader(_)
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
                    normalize_bibliography_lists(state, &mut item.description);
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
            | Block::DiscreteHeader(_)
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
                .get(after_comma..)
                .map_or(0, |tail| tail.len() - tail.trim_start().len());
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

/// Walk the content of a delimited block for anchors and cross-references.
fn collect_delimited_references<'a>(
    state: &mut ParserState<'a>,
    inner: &mut DelimitedBlockType<'a>,
    refs: &mut ReferenceCatalog<'a>,
    xrefs: &mut Vec<CrossReferenceUse<'a>>,
) {
    match inner {
        DelimitedBlockType::DelimitedExample(blocks)
        | DelimitedBlockType::DelimitedOpen(blocks)
        | DelimitedBlockType::DelimitedSidebar(blocks)
        | DelimitedBlockType::DelimitedQuote(blocks) => {
            collect_references(state, blocks, refs, xrefs);
        }
        DelimitedBlockType::DelimitedListing(inlines)
        | DelimitedBlockType::DelimitedLiteral(inlines)
        | DelimitedBlockType::DelimitedPass(inlines)
        | DelimitedBlockType::DelimitedVerse(inlines)
        | DelimitedBlockType::DelimitedComment(inlines) => {
            collect_inline_references(state, inlines, refs, xrefs);
        }
        DelimitedBlockType::DelimitedTable(table) => {
            for row in table
                .header
                .iter_mut()
                .chain(table.rows.iter_mut())
                .chain(table.footer.iter_mut())
            {
                for column in &mut row.columns {
                    collect_references(state, &mut column.content, refs, xrefs);
                }
            }
        }
        DelimitedBlockType::DelimitedStem(_) => {}
    }
}

/// Walk inline content for target IDs and cross-references, including nested inline content.
pub(super) fn collect_inline_references<'a>(
    state: &mut ParserState<'a>,
    inlines: &[InlineNode<'a>],
    refs: &mut ReferenceCatalog<'a>,
    xrefs: &mut Vec<CrossReferenceUse<'a>>,
) {
    for inline in inlines {
        match inline {
            InlineNode::InlineAnchor(anchor) => {
                insert_reference(
                    state,
                    refs,
                    anchor,
                    anchor.xreflabel.as_ref(),
                    crate::grammar::reference_text::text(state, anchor.reftext_source),
                    None,
                    None,
                );
                if let Some(label) = anchor.bibliography_label() {
                    collect_inline_references(state, label, refs, xrefs);
                }
            }
            InlineNode::Macro(InlineMacro::CrossReference(xref)) => {
                xrefs.push(CrossReferenceUse {
                    target: xref.target,
                    location: xref.location.clone(),
                    automatic: xref.text.is_empty(),
                    resolve_natural_target: xref.resolve_natural_target,
                    source_syntax: xref.source_syntax,
                });
                collect_inline_references(state, &xref.text, refs, xrefs);
            }
            InlineNode::Macro(InlineMacro::Footnote(footnote)) => {
                collect_inline_references(state, &footnote.content, refs, xrefs);
            }
            InlineNode::Macro(InlineMacro::IndexTerm(term)) if term.is_visible() => {
                // Only the displayed label owns document targets; the catalog
                // copy and concealed labels must not register them again.
                collect_inline_references(state, term.term(), refs, xrefs);
            }
            InlineNode::Macro(InlineMacro::Link(link)) => collect_link_references(
                state,
                &link.metadata,
                &link.location,
                &link.text,
                refs,
                xrefs,
            ),
            InlineNode::Macro(InlineMacro::Url(url)) => {
                collect_link_references(
                    state,
                    &url.metadata,
                    &url.location,
                    &url.text,
                    refs,
                    xrefs,
                );
            }
            InlineNode::Macro(InlineMacro::Mailto(mailto)) => collect_link_references(
                state,
                &mailto.metadata,
                &mailto.location,
                &mailto.text,
                refs,
                xrefs,
            ),
            InlineNode::BoldText(t) => collect_formatted_references(state, t, refs, xrefs),
            InlineNode::ItalicText(t) => collect_formatted_references(state, t, refs, xrefs),
            InlineNode::MonospaceText(t) => collect_formatted_references(state, t, refs, xrefs),
            InlineNode::HighlightText(t) => collect_formatted_references(state, t, refs, xrefs),
            InlineNode::SubscriptText(t) => collect_formatted_references(state, t, refs, xrefs),
            InlineNode::SuperscriptText(t) => collect_formatted_references(state, t, refs, xrefs),
            InlineNode::CurvedQuotationText(t) => {
                collect_formatted_references(state, t, refs, xrefs);
            }
            InlineNode::CurvedApostropheText(t) => {
                collect_formatted_references(state, t, refs, xrefs);
            }
            InlineNode::PlainText(_)
            | InlineNode::RawText(_)
            | InlineNode::VerbatimText(_)
            | InlineNode::StandaloneCurvedApostrophe(_)
            | InlineNode::LineBreak(_)
            | InlineNode::Macro(_)
            | InlineNode::CalloutRef(_) => {}
        }
    }
}

// Freeze text only after passthrough expansion and all source mapping have finished.
fn finalize_registered_inline(node: &mut InlineNode<'_>) {
    if let InlineNode::Macro(InlineMacro::IndexTerm(term)) = node {
        if let Some(plan) = term.catalog_substitutions.take()
            && let Some(catalog) = &mut term.catalog
        {
            catalog.for_each_label_mut(|nodes| freeze_registered_text(nodes, plan));
        }
    } else if let InlineNode::Macro(InlineMacro::Footnote(footnote)) = node {
        finalize_footnote_text(footnote);
    }
}

fn finalize_footnote_text(note: &mut Footnote<'_>) {
    if let Some(plan) = note.registration_substitutions.take() {
        freeze_registered_text(&mut note.content, plan);
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
