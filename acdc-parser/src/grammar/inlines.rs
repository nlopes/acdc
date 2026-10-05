use std::{borrow::Cow, ops::Range};

use crate::{
    Anchor, AttributeValue, Autolink, BlockMetadata, Bold, Button, CurvedApostrophe,
    CurvedQuotation, ElementAttributes, Footnote, Form, Highlight, ICON_SIZES, Icon, Image,
    IndexTerm, IndexTermKind, IndexTermRelationship, InlineMacro, InlineNode, Italic, Keyboard,
    LineBreak, Link, Mailto, Menu, Monospace, Pass, PassthroughKind, Plain, Raw, Source,
    StandaloneCurvedApostrophe, Stem, StemNotation, Subscript, Substitution, Superscript, Title,
    Url,
    grammar::{
        ParserState, inline_preprocessing,
        inline_preprocessor::{InlinePreprocessorParserState, ProcessedKind, SourceMap},
        inline_processing::{process_inlines, process_inlines_no_autolinks},
    },
    model::{
        strip_quotes,
        substitution::{HEADER, SubstitutionPlan},
    },
};

use super::{
    helpers::{
        BlockParsingMetadata, MacroAttributeContext, PositionWithOffset,
        RESERVED_NAMED_ATTRIBUTE_ID, RESERVED_NAMED_ATTRIBUTE_OPTIONS,
        RESERVED_NAMED_ATTRIBUTE_ROLE, Shorthand, is_valid_bibliography_id, process_attribute_list,
        restore_url_path,
    },
    state::{InlineContext, InlineRules, ParserScope},
};

/// The parts of `xref:target[...]` that decide what the link shows.
struct XrefMacroText<'s> {
    /// The link text, empty for an automatic reference.
    text: Cow<'s, str>,
    /// Where `text` starts within the brackets.
    offset: usize,
    /// Maps unescaped label positions back to the original label.
    source_map: SourceMap,
    /// Whether `text` is the whole bracket content as written.
    as_written: bool,
    /// A `xrefstyle=` for this reference alone, overriding the document's.
    xrefstyle: Option<String>,
    /// A `role=` for the link.
    role: Option<String>,
}

/// Read `xref:target[...]`'s brackets the way Asciidoctor does.
///
/// Brackets holding an `=` are an attribute list: the first positional
/// attribute is the link text, `xrefstyle=` overrides the document's style
/// for this reference, so `xref:fig[xrefstyle=short]` prints `Figure 1`, and
/// `role=` is kept for the link. Any other brackets, and all brackets in
/// compat mode, are the link text as written, commas included.
///
/// Other named attributes are read and set aside rather than shown, so
/// `xref:fig[id=x]` still has automatic text, as in Asciidoctor.
fn xref_macro_text(raw: &str, compat_mode: bool) -> XrefMacroText<'_> {
    if compat_mode || !raw.contains('=') {
        return XrefMacroText {
            text: Cow::Borrowed(raw),
            offset: 0,
            source_map: SourceMap::default(),
            as_written: true,
            xrefstyle: None,
            role: None,
        };
    }
    let mut parsed = XrefMacroText {
        text: Cow::Borrowed(""),
        offset: 0,
        source_map: SourceMap::default(),
        as_written: false,
        xrefstyle: None,
        role: None,
    };
    for (index, attribute) in super::document::scan_attribute_list(raw, &[])
        .into_iter()
        .enumerate()
    {
        match attribute.name.as_deref() {
            Some("xrefstyle") => parsed.xrefstyle = Some(attribute.value),
            // A repeated attribute takes its last value, as in Asciidoctor.
            Some("role") => parsed.role = Some(attribute.value),
            // Positional attributes are numbered by their place in the list,
            // so text after a named attribute is not the link text:
            // `xref:fig[xrefstyle=short,Words]` is still automatic.
            None if index == 0 => {
                let slice = raw
                    .get(attribute.value_start..attribute.value_end)
                    .unwrap_or_default();
                // A quoted value with escaped quotes reads differently from
                // the source slice; only then does the text need its own copy.
                parsed.text = if slice == attribute.value {
                    Cow::Borrowed(slice)
                } else {
                    if let Some(quote @ ('\'' | '"')) = raw
                        .get(..attribute.value_start)
                        .and_then(|prefix| prefix.chars().next_back())
                    {
                        for (offset, _) in slice.match_indices(&format!("\\{quote}")) {
                            parsed.source_map.add_replacement(
                                offset,
                                offset + 2,
                                1,
                                ProcessedKind::Escape,
                            );
                        }
                    }
                    Cow::Owned(attribute.value)
                };
                parsed.offset = attribute.value_start;
            }
            Some(_) | None => {}
        }
    }
    parsed
}

fn process_unescaped_xref_label<'a>(
    state: &mut ParserState<'a>,
    metadata: &BlockParsingMetadata<'_>,
    text: &'a str,
    start: usize,
    source_map: &SourceMap,
) -> Result<Vec<InlineNode<'a>>, crate::Error> {
    // Use the unescaped input for UTF-8 boundaries until all locations are mapped.
    let mut inline_ctx = state.inline_ctx;
    inline_ctx.offset = 0;
    let mut child = ParserState::for_inline_parsing(text, state, inline_ctx);
    let end = start + source_map.map_position(text.len())?;
    // Each recorded quote escape removes one byte from the original label.
    let unescaped_offset = |position| {
        position
            - source_map
                .replacements
                .partition_point(|replacement| replacement.absolute_start < position)
    };
    child.attribute_value_ranges = state
        .attribute_value_ranges
        .iter()
        .filter_map(|range| {
            let range_start = range.start.max(start);
            let range_end = range.end.min(end);
            (range_start < range_end)
                .then(|| unescaped_offset(range_start - start)..unescaped_offset(range_end - start))
        })
        .collect();
    let mut inlines = process_inlines_no_autolinks(&mut child, metadata, 0, text.len(), 0, text)?;
    let mut error = None;
    for inline in &mut inlines {
        super::location_walk::walk_inline_locations_mut(inline, &mut |location| match source_map
            .map_position(location.absolute_start)
            .and_then(|mapped_start| {
                // A single-character inline can extend past the label's last byte.
                source_map
                    .map_end_position(location.absolute_end.min(text.len() - 1))
                    .map(|mapped_end| (mapped_start, mapped_end))
            }) {
            Ok((mapped_start, mapped_end)) => {
                *location = state.create_location(start + mapped_start, start + mapped_end);
            }
            Err(cause) => error = Some(cause),
        });
    }
    error.map_or(Ok(inlines), Err)
}

/// RFC 5321 max local-part length. An email address must have `@` within this
/// many bytes of the start of the local part.
const EMAIL_LOCAL_PART_MAX: usize = 64;

#[derive(Clone, Copy)]
struct IndexTermSegment<'a> {
    text: &'a str,
    start: usize,
}

#[derive(Clone)]
enum IndexTermRelationshipSegments<'a> {
    See(IndexTermSegment<'a>),
    SeeAlso(Vec<IndexTermSegment<'a>>),
}

enum IndexTermMacroItem<'a> {
    Term(IndexTermSegment<'a>),
    Relationship(IndexTermRelationshipSegments<'a>),
}

#[derive(Clone, Copy)]
enum IndexTermForm {
    Flow,
    Concealed,
    NamedFlow,
    NamedConcealed,
}

impl IndexTermForm {
    fn visible(self) -> bool {
        matches!(self, Self::Flow | Self::NamedFlow)
    }
}

#[derive(Clone)]
struct IndexTermParts<'a> {
    terms: Vec<IndexTermSegment<'a>>,
    relationship: Option<IndexTermRelationshipSegments<'a>>,
}

fn trimmed_index_term_segment(text: &str, start: usize) -> IndexTermSegment<'_> {
    let trimmed_start = text.trim_start();
    let leading = text.len() - trimmed_start.len();
    IndexTermSegment {
        text: trimmed_start.trim_end(),
        start: start + leading,
    }
}

fn split_index_term_relationship(
    segment: IndexTermSegment<'_>,
) -> (
    IndexTermSegment<'_>,
    Option<IndexTermRelationshipSegments<'_>>,
) {
    let segment = trimmed_index_term_segment(segment.text, segment.start);
    if let Some((term, target)) = segment.text.split_once(" >> ") {
        return (
            trimmed_index_term_segment(term, segment.start),
            Some(IndexTermRelationshipSegments::See(
                trimmed_index_term_segment(target, segment.start + term.len() + 4),
            )),
        );
    }
    if let Some((term, targets)) = segment.text.split_once(" &> ") {
        let mut start = segment.start + term.len() + 4;
        let targets = targets
            .split(" &> ")
            .map(|target| {
                let segment = trimmed_index_term_segment(target, start);
                start += target.len() + 4;
                segment
            })
            .collect();
        return (
            trimmed_index_term_segment(term, segment.start),
            Some(IndexTermRelationshipSegments::SeeAlso(targets)),
        );
    }
    (segment, None)
}

fn parse_index_term_inlines<'a>(
    state: &mut ParserState<'a>,
    segment: IndexTermSegment<'a>,
    form: Option<IndexTermForm>,
) -> Result<Vec<InlineNode<'a>>, &'static str> {
    if segment.text.is_empty() {
        return Ok(Vec::new());
    }

    // Each label newline becomes one space; equal byte lengths preserve source offsets.
    let text = if segment.text.contains('\n') {
        state.intern_str(&segment.text.replace('\n', " "))
    } else {
        segment.text
    };
    let end = segment.start + segment.text.len();
    let mut rules = state.inline_ctx.rules;
    rules.insert(InlineRules::AUTOLINKS);
    rules.remove(InlineRules::INDEX_TERMS | InlineRules::EOI_HARD_BREAK);
    if let Some(form) = form {
        rules.insert(InlineRules::INDEX_LABEL);
        rules.set(
            InlineRules::NAMED_INDEX_LABEL,
            matches!(
                form,
                IndexTermForm::NamedFlow | IndexTermForm::NamedConcealed
            ),
        );
    }
    let inline_ctx = InlineContext {
        offset: 0,
        substitutions: state.inline_ctx.substitutions,
        rules,
    };
    let mut child = ParserState::for_inline_parsing(text, state, inline_ctx);
    child.attribute_value_ranges = state
        .attribute_value_ranges
        .iter()
        .filter_map(|range| {
            let range_start = range.start.max(segment.start);
            let range_end = range.end.min(end);
            (range_start < range_end)
                .then(|| range_start - segment.start..range_end - segment.start)
        })
        .collect();
    child.empty_attribute_offsets = state
        .empty_attribute_offsets
        .iter()
        .filter(|offset| segment.start <= **offset && **offset <= end)
        .map(|offset| offset - segment.start)
        .collect();
    child.late_attribute_sources = state
        .late_attribute_sources
        .iter()
        .filter(|(range, _)| range.start >= segment.start && range.end <= end)
        .map(|(range, source)| {
            (
                range.start - segment.start..range.end - segment.start,
                *source,
            )
        })
        .collect();

    let parsed = inline_parser::inlines(text, &mut child)
        .map_err(|_| "could not parse index term content")?;

    let mut parsed = parsed;
    for inline in &mut parsed {
        super::location_walk::walk_inline_locations_mut(inline, &mut |location| {
            location.absolute_start += segment.start;
            location.absolute_end += segment.start;
            location.start = state
                .line_map
                .offset_to_position(location.absolute_start, state.input);
            location.end = state
                .line_map
                .offset_to_position(location.absolute_end, state.input);
        });
    }
    Ok(parsed)
}

fn index_term_node(mut term: IndexTerm<'_>, plan: SubstitutionPlan) -> InlineNode<'_> {
    if term.catalog.as_ref().is_some_and(|catalog| {
        catalog.kind == term.kind && catalog.relationship == term.relationship
    }) && !plan.precedes(&Substitution::Macros, &Substitution::Replacements)
    {
        term.catalog = None;
    }
    if term.catalog.is_some() {
        term.catalog_substitutions = Some(plan);
    }
    InlineNode::Macro(InlineMacro::IndexTerm(Box::new(term)))
}

fn expand_escaped_index_terms<'a>(
    nodes: Vec<InlineNode<'a>>,
    state: &mut ParserState<'a>,
) -> Result<Vec<InlineNode<'a>>, &'static str> {
    if !state.inline_ctx.rules.contains(InlineRules::INDEX_TERMS) || !state.input.contains('\\') {
        return Ok(nodes);
    }
    let mut expanded = Vec::with_capacity(nodes.len());
    for node in nodes {
        let InlineNode::PlainText(plain) = &node else {
            expanded.push(node);
            continue;
        };
        let tail = &state.input[plain.location.absolute_start..];
        let escapes = tail.len() - tail.trim_start_matches('\\').len();
        let index_literal = plain.content.starts_with("((")
            || plain.content.starts_with("indexterm:[")
            || plain.content.starts_with("indexterm2:[");
        if escapes == 0 || !index_literal {
            expanded.push(node);
            continue;
        }
        // Escaping the macro leaves its label available to other substitutions.
        let parsed = parse_index_term_inlines(
            state,
            IndexTermSegment {
                text: plain.content,
                start: plain.location.absolute_start + escapes,
            },
            None,
        )?;
        if matches!(parsed.as_slice(), [InlineNode::PlainText(text)] if text.content == plain.content)
        {
            expanded.push(node);
        } else {
            expanded.extend(parsed);
        }
    }
    Ok(expanded)
}

// Recognize a macro once, then use its registration-time parts for both catalog
// and display labels. Later attribute values cannot introduce separators.
fn parse_index_term<'a>(
    state: &mut ParserState<'a>,
    content: IndexTermSegment<'a>,
    form: IndexTermForm,
    location: crate::Location,
) -> Result<InlineNode<'a>, &'static str> {
    let (source, restored_ranges) = registration_source(state, content);
    let mut parts = index_term_parts(state, source, form)?;
    let plan = state.inline_ctx.substitutions;
    let catalog = if plan.precedes(&Substitution::Macros, &Substitution::Attributes)
        || plan.precedes(&Substitution::Macros, &Substitution::Quotes)
        || plan.precedes(&Substitution::Macros, &Substitution::Replacements)
        || state.inline_ctx.rules.intersects(InlineRules::QUOTED_LINK)
        || (state.inline_ctx.rules.contains(InlineRules::BRACKET_LABEL) && source.contains("\\]"))
        || source
            .chars()
            .any(|character| matches!(character, ':' | '<' | '[' | '@'))
    {
        let mut inline_ctx = state.inline_ctx;
        inline_ctx.offset = 0;
        inline_ctx.substitutions = plan.through(&Substitution::Macros);
        inline_ctx.rules.insert(InlineRules::INDEX_CATALOG);
        // Index registration precedes enclosing-label bracket and quote unescaping.
        inline_ctx
            .rules
            .remove(InlineRules::BRACKET_LABEL | InlineRules::QUOTED_LINK);
        let mut child = ParserState::for_inline_parsing(source, state, inline_ctx);
        if restored_ranges.is_empty() {
            child.attribute_value_ranges = state
                .attribute_value_ranges
                .iter()
                .filter_map(|range| {
                    let start = range.start.max(content.start);
                    let end = range.end.min(content.start + content.text.len());
                    (start < end).then(|| start - content.start..end - content.start)
                })
                .collect();
        }
        let catalog_location = child.create_location(0, source.len().saturating_sub(1));
        let term = parse_index_parts(&mut child, parts.clone(), form, catalog_location)?;
        let mut node = InlineNode::Macro(InlineMacro::IndexTerm(Box::new(term)));
        map_registered_inline(&mut node, state, content.start, &restored_ranges);
        let InlineNode::Macro(InlineMacro::IndexTerm(mut term)) = node else {
            return Err("missing index catalog labels");
        };
        term.location = location.clone();
        Some(term)
    } else {
        None
    };
    let map_segment = |segment: &mut IndexTermSegment<'a>| {
        let start = content.start + restored_label_offset(segment.start, &restored_ranges, false);
        let end = content.start
            + restored_label_offset(segment.start + segment.text.len(), &restored_ranges, false);
        *segment = IndexTermSegment {
            text: &state.input[start..end],
            start,
        };
    };
    for segment in &mut parts.terms {
        map_segment(segment);
    }
    match &mut parts.relationship {
        Some(IndexTermRelationshipSegments::See(target)) => map_segment(target),
        Some(IndexTermRelationshipSegments::SeeAlso(targets)) => {
            for target in targets {
                map_segment(target);
            }
        }
        None => {}
    }
    let mut term = parse_index_parts(state, parts, form, location)?;
    term.catalog = catalog;
    Ok(index_term_node(term, plan))
}

fn map_registered_inline(
    node: &mut InlineNode<'_>,
    state: &ParserState<'_>,
    start: usize,
    restored_ranges: &[RestoredRange],
) {
    super::location_walk::walk_inline_locations_mut(node, &mut |location| {
        location.absolute_start =
            start + restored_label_offset(location.absolute_start, restored_ranges, false);
        location.absolute_end =
            start + restored_label_offset(location.absolute_end, restored_ranges, true);
        location.start = state
            .line_map
            .offset_to_position(location.absolute_start, state.input);
        location.end = state
            .line_map
            .offset_to_position(location.absolute_end, state.input);
    });
}

fn parse_footnote_content<'a>(
    state: &mut ParserState<'a>,
    content: IndexTermSegment<'a>,
) -> Result<Vec<InlineNode<'a>>, &'static str> {
    let plan = state.inline_ctx.substitutions;
    let frozen = plan.precedes(&Substitution::Macros, &Substitution::Attributes)
        || plan.precedes(&Substitution::Macros, &Substitution::Quotes)
        || plan.precedes(&Substitution::Macros, &Substitution::Replacements);
    if !frozen {
        let metadata = BlockParsingMetadata {
            substitutions: plan,
            ..BlockParsingMetadata::default()
        };
        return process_inlines(
            state,
            &metadata,
            content.start,
            content.start + content.text.len(),
            state.inline_ctx.offset,
            content.text,
        )
        .map(|(nodes, _)| nodes)
        .map_err(|_| "could not process footnote content");
    }
    let (source, ranges) = registration_source(state, content);
    let mut inline_ctx = state.inline_ctx;
    inline_ctx.offset = 0;
    inline_ctx.substitutions = plan.through(&Substitution::Macros);
    let mut child = ParserState::for_inline_parsing(source, state, inline_ctx);
    let metadata = BlockParsingMetadata {
        substitutions: inline_ctx.substitutions,
        ..BlockParsingMetadata::default()
    };
    let (mut nodes, _) = process_inlines(&mut child, &metadata, 0, source.len(), 0, source)
        .map_err(|_| "could not process footnote content")?;
    for node in &mut nodes {
        map_registered_inline(node, state, content.start, &ranges);
    }
    Ok(nodes)
}

// Freeze text only after passthrough expansion and all source mapping have finished.
pub(crate) fn finalize_registered_inline(node: &mut InlineNode<'_>) {
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

pub(crate) fn finalize_footnote_text(note: &mut Footnote<'_>) {
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
        super::passthrough_processing::convert_plain_to_raw(node, &substitutions);
    }
}

fn catalog_macro_allowed(state: &ParserState<'_>, start: usize) -> bool {
    !state.inline_ctx.rules.contains(InlineRules::INDEX_CATALOG)
        || [
            "image:",
            "icon:",
            "kbd:",
            "btn:",
            "menu:",
            "stem:",
            "latexmath:",
            "asciimath:",
            "pass:",
        ]
        .iter()
        .any(|prefix| state.input[start..].starts_with(prefix))
}

fn catalog_escape_allowed(state: &ParserState<'_>, start: usize) -> bool {
    if !state.inline_ctx.rules.contains(InlineRules::INDEX_CATALOG) {
        return true;
    }
    let tail = state.input[start..].trim_start_matches('\\');
    match tail.as_bytes().first() {
        Some(b'*' | b'_' | b'#' | b'`' | b'^' | b'~') => state
            .inline_ctx
            .substitutions
            .enabled(&Substitution::Quotes),
        Some(b'{') => state
            .inline_ctx
            .substitutions
            .enabled(&Substitution::Attributes),
        _ => true,
    }
}

type RestoredRange = (Range<usize>, Range<usize>);

fn registration_source<'a>(
    state: &ParserState<'a>,
    content: IndexTermSegment<'a>,
) -> (&'a str, Vec<RestoredRange>) {
    let end = content.start + content.text.len();
    let mut source = String::new();
    let mut cursor = content.start;
    let mut restored_ranges = Vec::new();
    for (range, reference) in &state.late_attribute_sources {
        if range.start < cursor || range.end > end {
            continue;
        }
        source.push_str(&state.input[cursor..range.start]);
        let restored_start = source.len();
        source.push_str(reference);
        restored_ranges.push((
            restored_start..source.len(),
            range.start - content.start..range.end - content.start,
        ));
        cursor = range.end;
    }
    if restored_ranges.is_empty() {
        return (content.text, restored_ranges);
    }
    source.push_str(&state.input[cursor..end]);
    (state.intern_str(&source), restored_ranges)
}

fn index_term_parts<'a>(
    state: &mut ParserState<'a>,
    text: &'a str,
    form: IndexTermForm,
) -> Result<IndexTermParts<'a>, &'static str> {
    let content = trimmed_index_term_segment(text, 0);
    let (terms, relationship) = match form {
        IndexTermForm::Flow => {
            let (term, relationship) = split_index_term_relationship(content);
            (vec![term], relationship)
        }
        IndexTermForm::Concealed => {
            let (terms, relationship) = split_index_term_relationship(content);
            let terms = inline_parser::index_term_list(terms.text, state, terms.start)
                .map_err(|_| "could not parse index terms")?;
            (terms, relationship)
        }
        IndexTermForm::NamedFlow | IndexTermForm::NamedConcealed => {
            let items = inline_parser::index_term_macro_list(content.text, state, content.start)
                .map_err(|_| "could not parse index term attributes")?;
            let (terms, relationship) = partition_index_term_macro_items(items);
            let terms = if matches!(form, IndexTermForm::NamedFlow) && relationship.is_none() {
                vec![content]
            } else {
                terms
            };
            (terms, relationship)
        }
    };
    Ok(IndexTermParts {
        terms,
        relationship,
    })
}

fn parse_index_parts<'a>(
    state: &mut ParserState<'a>,
    parts: IndexTermParts<'a>,
    form: IndexTermForm,
    location: crate::Location,
) -> Result<IndexTerm<'a>, &'static str> {
    let mut terms = parts.terms.into_iter();
    let term = parse_index_term_inlines(
        state,
        terms
            .next()
            .unwrap_or(IndexTermSegment { text: "", start: 0 }),
        Some(form),
    )?;
    let kind = if form.visible() {
        IndexTermKind::Flow(term)
    } else {
        IndexTermKind::Concealed {
            term,
            secondary: terms
                .next()
                .map(|segment| parse_index_term_inlines(state, segment, Some(form)))
                .transpose()?,
            tertiary: terms
                .next()
                .map(|segment| parse_index_term_inlines(state, segment, Some(form)))
                .transpose()?,
        }
    };
    Ok(IndexTerm {
        kind,
        relationship: parse_index_term_relationship(state, parts.relationship, form)?,
        catalog: None,
        catalog_substitutions: None,
        location,
    })
}

fn restored_label_offset(offset: usize, ranges: &[RestoredRange], end: bool) -> usize {
    let Some((restored, original)) = ranges.iter().rev().find(|(range, _)| offset >= range.start)
    else {
        return offset;
    };
    if offset >= restored.end {
        original.end + offset - restored.end
    } else if end {
        original.end.saturating_sub(1).max(original.start)
    } else {
        original.start
    }
}

fn parse_index_term_relationship<'a>(
    state: &mut ParserState<'a>,
    relationship: Option<IndexTermRelationshipSegments<'a>>,
    form: IndexTermForm,
) -> Result<Option<IndexTermRelationship<'a>>, &'static str> {
    match relationship {
        None => Ok(None),
        Some(IndexTermRelationshipSegments::See(target)) => Ok(Some(IndexTermRelationship::See {
            target: parse_index_term_inlines(state, target, Some(form))?,
        })),
        Some(IndexTermRelationshipSegments::SeeAlso(targets)) => {
            let targets = targets
                .into_iter()
                .filter(|target| !target.text.is_empty())
                .map(|target| parse_index_term_inlines(state, target, Some(form)))
                .collect::<Result<_, _>>()?;
            Ok(Some(IndexTermRelationship::SeeAlso { targets }))
        }
    }
}

fn partition_index_term_macro_items(
    items: Vec<IndexTermMacroItem<'_>>,
) -> (
    Vec<IndexTermSegment<'_>>,
    Option<IndexTermRelationshipSegments<'_>>,
) {
    let mut terms = Vec::new();
    let mut see = None;
    let mut see_also = None;
    for item in items {
        match item {
            IndexTermMacroItem::Term(term) => terms.push(term),
            IndexTermMacroItem::Relationship(IndexTermRelationshipSegments::See(target)) => {
                see = Some(IndexTermRelationshipSegments::See(target));
            }
            IndexTermMacroItem::Relationship(IndexTermRelationshipSegments::SeeAlso(targets)) => {
                see_also = Some(IndexTermRelationshipSegments::SeeAlso(targets));
            }
        }
    }
    (terms, see.or(see_also))
}

/// Check whether a byte is safe for the `plain_text` quick path — i.e., it
/// cannot start any inline construct. Covers: uppercase A-Z, non-macro-prefix
/// lowercase, digits 0-9, and space (space has an additional runtime check for
/// the hard-wrap pattern ` +`).
///
/// Macro prefix lowercase (a,b,f,h,i,k,l,m,p,s,x) are NOT safe because they
/// can start inline macros.
const fn is_plain_text_safe(b: u8) -> bool {
    matches!(
        b,
        b'A'..=b'Z'
            | b'0'..=b'9'
            | b'c' | b'd' | b'e' | b'g' | b'j'
            | b'n' | b'o' | b'q' | b'r' | b't'
            | b'u' | b'v' | b'w' | b'y' | b'z'
            | b' '
    )
}

fn is_word_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Check whether `@` appears within [`EMAIL_LOCAL_PART_MAX`] bytes from `pos`.
fn has_at_sign_ahead(state: &ParserState, pos: usize) -> bool {
    use crate::grammar::state::AtLookahead;

    if let Some(cache) = state.next_at_sign_cache.get() {
        match cache.first_at {
            Some(at) if at >= pos => return at < pos + EMAIL_LOCAL_PART_MAX,
            None if pos + EMAIL_LOCAL_PART_MAX <= cache.scanned_up_to => return false,
            // Cached `@` is behind us, or the cached range doesn't cover the
            // full lookahead window; fall through and rescan.
            Some(_) | None => {}
        }
    }

    let input = state.input.as_bytes();
    let start = pos.min(input.len());
    let scan_end = (pos + 1024).min(input.len());
    let window = input.get(start..scan_end).unwrap_or(&[]);
    let first_at = window
        .iter()
        .position(|&b| b == b'@')
        .map(|off| start + off);
    state.next_at_sign_cache.set(Some(AtLookahead {
        scanned_up_to: scan_end,
        first_at,
    }));
    first_at.is_some_and(|at| at < pos + EMAIL_LOCAL_PART_MAX)
}

fn byte_came_from_attribute(state: &ParserState<'_>, position: usize) -> bool {
    let range_index = state
        .attribute_value_ranges
        .partition_point(|range| range.end <= position);
    state
        .attribute_value_ranges
        .get(range_index)
        .is_some_and(|range| range.contains(&position))
}

fn structural_token_allowed(
    state: &ParserState<'_>,
    substitution: &Substitution,
    start: usize,
    len: usize,
) -> bool {
    !state
        .inline_ctx
        .substitutions
        .precedes(substitution, &Substitution::Attributes)
        || (!(start..start + len).any(|position| byte_came_from_attribute(state, position))
            // Removing an attribute can join separate characters into a marker
            // that did not exist when the earlier substitution ran.
            && !state.empty_attribute_offsets.iter()
                .any(|offset| start < *offset && *offset < start + len))
}

fn macro_token_allowed(state: &ParserState<'_>, start: usize, len: usize) -> bool {
    structural_token_allowed(state, &Substitution::Macros, start, len)
}

fn index_content_present(state: &ParserState<'_>, start: usize, text: &str) -> bool {
    !text.is_empty()
        || state
            .late_attribute_sources
            .iter()
            .any(|(range, _)| range.start == start && range.is_empty())
}

fn has_inline_line_break_prefix(state: &ParserState<'_>, span_start: usize) -> bool {
    let absolute_pos = span_start + state.inline_ctx.offset;
    let preceded_by_content_or_line_end = absolute_pos > 0
        && state
            .input
            .as_bytes()
            .get(absolute_pos.saturating_sub(1))
            .is_some_and(|&byte| !byte.is_ascii_whitespace() || matches!(byte, b'\n' | b'\r'));
    preceded_by_content_or_line_end
        || state
            .empty_attribute_offsets
            .binary_search(&span_start)
            .is_ok()
}

fn match_constrained_boundary(b: u8) -> bool {
    // Use source punctuation; converter escaping must not change parsing.
    !is_word_char(b) && !matches!(b, b':' | b';' | b'}')
}

/**
Check whether the character before `pos` is a valid constrained opening boundary.

At position 0, falls back to `outer_delimiter` (the byte preceding the current
inline span in the parent context). A word-character outer delimiter means the
boundary is invalid.
*/
fn check_constrained_opening_boundary(
    pos: usize,
    input: &[u8],
    outer_delimiter: Option<u8>,
    marker: u8,
) -> bool {
    if pos == 0 {
        return outer_delimiter.is_none_or(|d| !is_word_char(d));
    }
    match input.get(pos - 1) {
        None => true,
        // A hash after an ampersand belongs to character-reference syntax.
        Some(b'&') if marker == b'#' => false,
        Some(&b) if b.is_ascii() => match_constrained_boundary(b),
        // The preceding byte belongs to a multibyte (non-ASCII) character. It is
        // a valid boundary unless that character is a Unicode word character
        // (letter or number) — matching asciidoctor, where Unicode punctuation
        // such as `“` or `«` opens a constrained span but letters like `é`/`日`
        // do not.
        Some(_) => char_ending_at(input, pos).is_none_or(|c| !c.is_alphanumeric()),
    }
}

/// Decode the UTF-8 character whose final byte is at `end - 1` (i.e. the
/// character immediately preceding byte offset `end`). Returns `None` at the
/// start of input or if the bytes are not valid UTF-8.
fn char_ending_at(input: &[u8], end: usize) -> Option<char> {
    if end == 0 || end > input.len() {
        return None;
    }
    // Walk back over UTF-8 continuation bytes (0b10xx_xxxx) to the lead byte.
    let mut start = end - 1;
    while start > 0
        && input
            .get(start)
            .is_some_and(|&b| b & 0b1100_0000 == 0b1000_0000)
    {
        start -= 1;
    }
    std::str::from_utf8(input.get(start..end)?)
        .ok()?
        .chars()
        .next()
}

/**
Check whether a constrained closing delimiter at `end` is valid.

If `end` is at the end of the input, the outer delimiter must not be a word
character (otherwise the markup would be adjacent to a word character in the
parent context).
*/
fn check_constrained_closing_at_end(
    end: usize,
    input_len: usize,
    outer_delimiter: Option<u8>,
) -> bool {
    end < input_len || outer_delimiter.is_none_or(|d| !is_word_char(d))
}

/// Macro to handle inline processing errors with logging
macro_rules! process_inlines_or_err {
    ($call:expr, $msg:literal) => {
        $call.map_err(|e| {
            tracing::error!(?e, $msg);
            $msg
        })
    };
}

#[derive(Debug)]
struct LinkContent<'a> {
    raw: &'a str,
    protected: Vec<Range<usize>>,
}

impl LinkContent<'_> {
    // Keep nested macro delimiters protected when late attributes change offsets.
    fn registered_protected_ranges(&self, restored: &[RestoredRange]) -> Cow<'_, [Range<usize>]> {
        if restored.is_empty() {
            Cow::Borrowed(self.protected.as_slice())
        } else {
            let reversed: Vec<_> = restored
                .iter()
                .map(|(a, b)| (b.clone(), a.clone()))
                .collect();
            Cow::Owned(
                self.protected
                    .iter()
                    .map(|range| {
                        restored_label_offset(range.start, &reversed, false)
                            ..restored_label_offset(range.end, &reversed, false)
                    })
                    .collect(),
            )
        }
    }
}

#[derive(Default)]
struct ProcessedLinkContent<'a> {
    text: Vec<InlineNode<'a>>,
    attributes: ElementAttributes<'a>,
    subject: Option<&'a str>,
    body: Option<&'a str>,
}

fn process_link_content<'a>(
    state: &mut ParserState<'a>,
    metadata: &BlockParsingMetadata<'_>,
    start: usize,
    end: usize,
    content: &LinkContent<'a>,
    mailto: bool,
) -> Result<ProcessedLinkContent<'a>, crate::Error> {
    // URI arguments freeze when macros run; later attributes may still change
    // the visible label, but must not introduce query values or separators.
    let (raw, restored) = if mailto {
        registration_source(
            state,
            IndexTermSegment {
                text: content.raw,
                start,
            },
        )
    } else {
        (content.raw, Vec::new())
    };
    let protected = content.registered_protected_ranges(&restored);
    let mut text = content.raw;
    let mut offset = 0;
    let mut quote = None;
    let mut result = ProcessedLinkContent::default();
    // Mailto uses commas to introduce subject/body slots. Other links use '=';
    // without that trigger (or in compat mode), quotes and commas are label text.
    if !state.document_attributes.contains_key("compat-mode")
        && raw.contains(if mailto { ',' } else { '=' })
    {
        text = "";
        for (index, attribute) in super::document::scan_attribute_list(raw, &protected)
            .into_iter()
            .enumerate()
        {
            let attribute_quote = raw[..attribute.value_start]
                .chars()
                .next_back()
                .filter(|character| matches!(character, '\'' | '"'));
            if let Some(name) = attribute.name {
                let name = match name.as_str() {
                    "roles" => Cow::Borrowed("role"),
                    "opts" => Cow::Borrowed("options"),
                    _ => Cow::Owned(name),
                };
                // Named presentation attributes still see late substitutions.
                let value = if restored.is_empty() {
                    attribute.value
                } else {
                    let start = restored_label_offset(attribute.value_start, &restored, false);
                    let end = restored_label_offset(attribute.value_end, &restored, false);
                    let value = &content.raw[start..end];
                    attribute_quote.map_or_else(
                        || value.to_string(),
                        |quote| super::document::unescape_attribute_quote(value, quote),
                    )
                };
                result.attributes.set(name, value.into());
            } else if index == 0 {
                quote = attribute_quote;
                offset = restored_label_offset(attribute.value_start, &restored, false);
                let end = restored_label_offset(attribute.value_end, &restored, false);
                text = &content.raw[offset..end];
            } else if mailto && matches!(index, 1 | 2) {
                let value = state.intern_str(&attribute.value.replace("\\]", "]"));
                if index == 1 {
                    result.subject = Some(value);
                } else {
                    result.body = Some(value);
                }
            }
        }
    }
    if let Some(label) = text.strip_suffix('^') {
        text = label;
        result.attributes.insert("window".into(), "_blank".into());
    }
    if text.is_empty() {
        return Ok(result);
    }

    // Keep source slices intact until the label parser consumes each escape.
    // Reset the quote mode for nested links, then restore the enclosing mode on errors too.
    let rules = state.inline_ctx.rules;
    state.inline_ctx.rules.insert(InlineRules::BRACKET_LABEL);
    state
        .inline_ctx
        .rules
        .set(InlineRules::DOUBLE_QUOTED_LINK, quote == Some('"'));
    state
        .inline_ctx
        .rules
        .set(InlineRules::SINGLE_QUOTED_LINK, quote == Some('\''));
    let parsed = process_inlines_no_autolinks(
        state,
        metadata,
        start + offset,
        end,
        state.inline_ctx.offset,
        text,
    );
    state.inline_ctx.rules = rules;
    result.text = parsed?;
    Ok(result)
}

peg::parser! {
    pub(crate) grammar inline_parser(state: &mut ParserState<'input>) for str {
        use std::borrow::Cow;
        use std::str::FromStr;
        use crate::model::{substitute, Substitution};
        use crate::model::substitution::parse_substitution;

        // Injected span endpoints — `span_start`/`span_end` are bound in every
        // action block to the byte range of the sequence leading up to it,
        // replacing the boilerplate `start:position!() ... end:position!()`
        // captures that previously wrapped most rules. See document.rs for the
        // same pair on the document grammar.
        inject span_start(_input, l, _r) -> usize { l }
        inject span_end(_input, _l, r) -> usize { r }

        // Group consecutive ordinary nodes so profiles need no vector per node.
        // Expansions still run at their source position, before later macros.
        pub(crate) rule inlines() -> Vec<InlineNode<'input>>
        = check_attribute_profiles() chunks:(profiled_attribute() / nodes:normal_inline()+ { nodes })+ {? expand_escaped_index_terms(chunks.into_iter().flatten().collect(), state) }
        / nodes:normal_inline()+ {? expand_escaped_index_terms(nodes, state) }

        rule normal_inline() -> InlineNode<'input>
        = non_plain_text() / plain_text()

        pub(crate) rule inlines_no_autolinks() -> Vec<InlineNode<'input>>
        = inlines()

        pub(crate) rule verbatim_inlines() -> Vec<InlineNode<'input>>
        = check_attribute_profiles() chunks:(profiled_attribute() / nodes:verbatim_inline()+ { nodes })+ {? expand_escaped_index_terms(chunks.into_iter().flatten().collect(), state) }
        / nodes:verbatim_inline()+ {? expand_escaped_index_terms(nodes, state) }

        rule verbatim_inline() -> InlineNode<'input>
        = verbatim_index_term() / verbatim_footnote() / verbatim_anchor() / verbatim_link() / quotes_non_plain_text() / verbatim_plain_text()

        rule check_attribute_profiles()
        = {? (!state.attribute_passthroughs.is_empty()).then_some(()).ok_or("no attribute profiles") }

        rule profiled_attribute_match() -> usize
        = "���" index:$(digits()) "���" {?
            index.parse::<usize>().ok().filter(|index| {
                state.attribute_passthroughs.get(*index).is_some_and(|pass| !pass.attribute_fragments.is_empty())
            }).ok_or("not a profiled attribute")
        }

        rule profiled_attribute() -> Vec<InlineNode<'input>>
        = index:profiled_attribute_match() {
            crate::grammar::passthrough_processing::process_attribute_placeholder(index, span_start, span_end, state)
        }

        rule verbatim_anchor() -> InlineNode<'input>
        = check_macros() node:(
            node:escaped_anchor_macro() { node }
            / &("\\"+ "[[") node:escaped_syntax() { node }
            / node:inline_anchor() { node }
        ) { node }

        rule verbatim_footnote() -> InlineNode<'input>
        = check_macros() node:(
            "\\" &footnote_match() content:$("footnote:" id()? "[") {
                InlineNode::PlainText(Plain {
                    content,
                    location: state.create_block_location(span_start, span_end, state.inline_ctx.offset),
                    escaped: false,
                })
            }
            / node:footnote() { node }
        ) { node }

        rule verbatim_link() -> InlineNode<'input>
        = check_macros() node:(
            &("\\"+ verbatim_link_match()) node:escaped_syntax() { node }
            / "\\" content:$(check_autolinks() inline_autolink_match()) {
                InlineNode::PlainText(Plain {
                    content,
                    location: state.create_block_location(span_start, span_end, state.inline_ctx.offset),
                    escaped: false,
                })
            }
            / &['<'] node:cross_reference_shorthand() { node }
            / &['x'] node:cross_reference_macro() { node }
            / &['l'] node:link_macro() { node }
            / &['m'] node:mailto_macro() { node }
            / &['h' | 'f'] node:url_macro() { node }
            / check_autolinks() node:inline_autolink() { node }
        ) { node }

        rule verbatim_link_match()
        = cross_reference_shorthand_match() / cross_reference_macro_match()
        / link_macro_match() / mailto_macro_match() / url_macro_match()
        / check_autolinks() inline_autolink_match()

        rule verbatim_index_term() -> InlineNode<'input>
        = check_index_terms() node:(
            &['\\'] node:escaped_index_prefix() { node }
            / &("\\"+ index_term_match()) node:escaped_syntax() { node }
            / &['('] node:index_term_concealed() { node }
            / &['('] node:index_term_flow() { node }
            / &['i'] node:indexterm_macro() { node }
            / &['i'] node:indexterm2_macro() { node }
        ) { node }

        rule verbatim_plain_text() -> InlineNode<'input>
        = content:$((
            !profiled_attribute_match()
            !(check_index_terms() (
                &("\\"+ index_term_match()) escaped_syntax_match() / index_term_match()
            ))
            !(check_macros() ("\\"? footnote_match()))
            !(check_macros() (
                "\\"? anchor_macro_pattern() / inline_anchor_match()
                / &("\\"+ "[[") escaped_syntax_match()
            ))
            !(check_macros() (
                verbatim_link_match()
                / &("\\"+ verbatim_link_match()) escaped_syntax_match()
                / "\\" check_autolinks() inline_autolink_match()
            ))
            !(check_quotes() (
                escaped_syntax_match() / bold_text_unconstrained_match() / bold_text_constrained_match()
                / italic_text_unconstrained_match() / italic_text_constrained_match()
                / monospace_text_unconstrained_match() / monospace_text_constrained_match()
                / highlight_text_unconstrained_match() / highlight_text_constrained_match()
                / superscript_text_match() / subscript_text_match()
                / curved_quotation_text_match() / curved_apostrophe_text_match() / standalone_curved_apostrophe_match()
            ))
            [_]
        )+) {
            InlineNode::PlainText(Plain {
                content,
                location: state.create_block_location(span_start, span_end, state.inline_ctx.offset),
                escaped: false,
            })
        }

        /// Reduced inline rule set for "quotes" substitution in passthroughs.
        /// Only matches formatting markup + escaped markup + plain text.
        /// Explicit attribute profiles can insert their own enabled constructs.
        pub(crate) rule quotes_only_inlines() -> Vec<InlineNode<'input>>
        = check_attribute_profiles() chunks:(profiled_attribute() / nodes:quotes_inline()+ { nodes })+ { chunks.into_iter().flatten().collect() }
        / quotes_inline()+

        rule quotes_inline() -> InlineNode<'input>
        = quotes_non_plain_text() / quotes_plain_text()

        /// Non-plain-text alternatives for quotes-only mode: formatting markup only.
        /// Keep in sync with the formatting entries in `non_plain_text` above.
        rule quotes_non_plain_text() -> InlineNode<'input>
        = check_quotes() inline:(
            escaped_super_sub:escaped_superscript_subscript() { escaped_super_sub }
            / escaped_syntax:escaped_syntax() { escaped_syntax }
            / bold_text_unconstrained:bold_text_unconstrained() { bold_text_unconstrained }
            / bold_text_constrained:bold_text_constrained() { bold_text_constrained }
            / italic_text_unconstrained:italic_text_unconstrained() { italic_text_unconstrained }
            / italic_text_constrained:italic_text_constrained() { italic_text_constrained }
            / monospace_text_unconstrained:monospace_text_unconstrained() { monospace_text_unconstrained }
            / monospace_text_constrained:monospace_text_constrained() { monospace_text_constrained }
            / highlight_text_unconstrained:highlight_text_unconstrained() { highlight_text_unconstrained }
            / highlight_text_constrained:highlight_text_constrained() { highlight_text_constrained }
            / superscript_text:superscript_text() { superscript_text }
            / subscript_text:subscript_text() { subscript_text }
            / curved_quotation_text:curved_quotation_text() { curved_quotation_text }
            / curved_apostrophe_text:curved_apostrophe_text() { curved_apostrophe_text }
            / standalone_curved_apostrophe:standalone_curved_apostrophe() { standalone_curved_apostrophe }
        ) {
            inline
        }

        /// Plain text for quotes-only mode: reduced negative lookaheads (formatting patterns only).
        /// Keep in sync with the formatting lookaheads in `plain_text` below.
        rule quotes_plain_text() -> InlineNode<'input>
        = start_pos:position!()
        content:$((
            "\\" "^" !([^'^' | ' ' | '\t' | '\n']+ "^")
            / "\\" "~" !([^'~' | ' ' | '\t' | '\n']+ "~")
            // Fast path: characters that can never start any quotes inline construct.
            // Fewer triggers than plain_text since quotes context has no macros/autolinks.
            / [^('\n' | '\r' | '\\' | '[' | '*' | '_' | '`' | '#' | '^' | '~' | '"' | '\'' | '\u{FFFD}')]+
            / (
                !(
                    profiled_attribute_match()
                    / paragraph_break()
                    / ![_]
                    / &['\\'] escaped_syntax_match()
                    / check_quotes() &['*' | '_' | '`' | '#' | '^' | '~' | '"' | '\'' | '['] (
                        bold_text_unconstrained_match() / bold_text_constrained_match() / italic_text_unconstrained_match() / italic_text_constrained_match() / monospace_text_unconstrained_match() / monospace_text_constrained_match() / highlight_text_unconstrained_match() / highlight_text_constrained_match() / superscript_text_match() / subscript_text_match() / curved_quotation_text_match() / curved_apostrophe_text_match() / standalone_curved_apostrophe_match()
                    )
                )
                [_]
            )
        )+)
        end:position!()
        {
            tracing::debug!(?content, "Found quotes-only plain text inline");
            InlineNode::PlainText(Plain {
                content,
                location: state.create_block_location(start_pos, end, state.inline_ctx.offset),
                escaped: false,
            })
        }

        rule non_plain_text() -> InlineNode<'input>
        = inline:(
            // Each alternative is prefixed with a `&[...]` byte-lookahead that
            // rejects most call sites before the real rule is entered. On
            // macro-dense docs `non_plain_text` was the top hotspot: packrat
            // memoisation pays its fair share, but the alternation still had
            // to TRY each of ~30 rules on every candidate position. The
            // byte guards cut that to the subset whose first byte matches.
            // Preserves original ordering to keep PEG first-match semantics.

            // Escaped superscript/subscript must come first - produces RawText to prevent re-parsing
            &['\\'] escaped_super_sub:escaped_superscript_subscript() { escaped_super_sub }
            / &['\\'] check_index_terms() prefix:escaped_index_prefix() { prefix }
            // Escaped syntax must come next - backslash prevents any following syntax from being parsed
            / check_macros() &['\\'] anchor:escaped_anchor_macro() { anchor }
            / &['\\'] bracket:index_label_bracket() { bracket }
            / &['\\'] bracket:escaped_label_character() { bracket }
            / &['\\'] escaped_syntax:escaped_syntax() { escaped_syntax }
            // Index terms: concealed (triple parens) must come before flow (double parens)
            / check_index_terms() &['('] index_term:index_term_concealed() { index_term }
            / check_index_terms() &['('] index_term:index_term_flow() { index_term }
            / check_index_terms() &['i'] indexterm:indexterm_macro() { indexterm }
            / check_index_terms() &['i'] indexterm2:indexterm2_macro() { indexterm2 }
            / check_macros() &['['] invalid_bibliography_anchor:invalid_bibliography_anchor() { invalid_bibliography_anchor }
            / check_macros() &['[' | 'a'] inline_anchor:inline_anchor() { inline_anchor }
            / check_macros() &['<'] cross_reference_shorthand:cross_reference_shorthand() { cross_reference_shorthand }
            / check_macros() &['x'] cross_reference_macro:cross_reference_macro() { cross_reference_macro }
            / check_hardbreaks() &['\n' | '\r'] automatic_line_break:automatic_line_break() { automatic_line_break }
            / check_post_replacements() &[' '] hard_wrap:hard_wrap() { hard_wrap }
            / check_macros() &"footnote:" footnote:footnote() { footnote }
            / check_macros() &['s' | 'a' | 'l'] stem:inline_stem() { stem }
            / check_macros() &['i'] image:inline_image() { image }
            / check_macros() &['i'] icon:inline_icon() { icon }
            / check_macros() &['k'] keyboard:inline_keyboard() { keyboard }
            / check_macros() &['b'] button:inline_button() { button }
            / check_macros() &['m'] menu:inline_menu() { menu }
            // mailto has to come before the url_macro because url_macro calls url() which
            // also matches against mailto:
            / check_macros() &['m'] mailto_macro:mailto_macro() { mailto_macro }
            / check_macros() &['h' | 'f'] url_macro:url_macro() { url_macro }
            / check_macros() &['p'] pass:inline_pass() { pass }
            / check_macros() &['l'] link_macro:link_macro() { link_macro }
            // No byte guard: autolink matches bare URLs (`h`/`f` schemes) AND bare
            // emails (any ASCII alphanumeric), and `<url>` / `<email>`. The
            // upstream `plain_text_quick_safe` fast path already rejects most
            // email candidates before we reach non_plain_text, so this
            // alternative is only tried on positions that actually need it.
            / check_macros() check_autolinks() inline_autolink:inline_autolink() { inline_autolink }
            / check_post_replacements() &[' '] inline_line_break:inline_line_break() { inline_line_break }
            / check_quotes() &['[' | '*'] bold_text_unconstrained:bold_text_unconstrained() { bold_text_unconstrained }
            / check_quotes() &['[' | '*'] bold_text_constrained:bold_text_constrained() { bold_text_constrained }
            / check_quotes() &['[' | '_'] italic_text_unconstrained:italic_text_unconstrained() { italic_text_unconstrained }
            / check_quotes() &['[' | '_'] italic_text_constrained:italic_text_constrained() { italic_text_constrained }
            / check_quotes() &['[' | '`'] monospace_text_unconstrained:monospace_text_unconstrained() { monospace_text_unconstrained }
            / check_quotes() &['[' | '`'] monospace_text_constrained:monospace_text_constrained() { monospace_text_constrained }
            / check_quotes() &['[' | '#'] highlight_text_unconstrained:highlight_text_unconstrained() { highlight_text_unconstrained }
            / check_quotes() &['[' | '#'] highlight_text_constrained:highlight_text_constrained() { highlight_text_constrained }
            / check_quotes() &['[' | '^'] superscript_text:superscript_text() { superscript_text }
            / check_quotes() &['[' | '~'] subscript_text:subscript_text() { subscript_text }
            / check_quotes() &['[' | '"'] curved_quotation_text:curved_quotation_text() { curved_quotation_text }
            / check_quotes() &['[' | '\''] curved_apostrophe_text:curved_apostrophe_text() { curved_apostrophe_text }
            / check_quotes() &['`'] standalone_curved_apostrophe:standalone_curved_apostrophe() { standalone_curved_apostrophe }
            ) {
                inline
            }

        /// Escaped superscript/subscript rule - matches \^content^ or \~content~.
        ///
        /// Produces PlainText with empty substitutions so the content:
        /// 1. Gets HTML escaped by the converter (security)
        /// 2. Doesn't get re-parsed as formatting (no Quotes in substitutions)
        ///
        /// Only matches when there's a complete pattern (content with no spaces
        /// followed by closing marker).
        rule escaped_superscript_subscript() -> InlineNode<'input>
        = check_catalog_escape() check_quotes() "\\" content:escaped_super_sub_pattern() {
            InlineNode::PlainText(Plain {
                content,
                location: state.create_location(span_start + state.inline_ctx.offset, span_end + state.inline_ctx.offset),
                escaped: true,
            })
        }

        /// Match escaped superscript (^content^) or subscript (~content~) pattern.
        rule escaped_super_sub_pattern() -> &'input str
        = "^" inner:$([^'^' | ' ' | '\t' | '\n']+) "^" { state.intern_fmt(format_args!("^{inner}^")) }
        / "~" inner:$([^'~' | ' ' | '\t' | '\n']+) "~" { state.intern_fmt(format_args!("~{inner}~")) }

        // Active bracket-delimited labels consume one closing-bracket escape.
        // Quote escapes apply only inside quoted link labels; the remaining text
        // still needs backend escaping, but no further AsciiDoc replacements.
        rule escaped_label_character() -> InlineNode<'input>
        = content:$(escaped_label_character_match()) {
            let escapes = usize::from(structural_token_allowed(
                state, &Substitution::Macros, span_start, content.len(),
            ));
            InlineNode::RawText(Raw {
                content: &content[escapes..],
                location: state.create_block_location(span_start, span_end, state.inline_ctx.offset),
                subs: vec![Substitution::SpecialChars],
            })
        }

        rule escaped_label_character_match()
        = check_bracket_label() "\\"+ (
            "]"
            / check_link_quote(InlineRules::DOUBLE_QUOTED_LINK) "\""
            / check_link_quote(InlineRules::SINGLE_QUOTED_LINK) "'"
        )

        // A shorthand index has no square-bracket delimiter. Named macros and
        // enclosing labels each consume one closing-bracket escape, in that order.
        // Raw text protects remaining backslashes from converter replacements.
        rule index_label_bracket() -> InlineNode<'input>
        = content:$(index_label_bracket_match()) {
            let rules = state.inline_ctx.rules;
            // A later attribute value did not exist when either macro consumed escapes.
            let escapes = if content.ends_with(']')
                && structural_token_allowed(state, &Substitution::Macros, span_start, content.len())
            {
                usize::from(rules.contains(InlineRules::NAMED_INDEX_LABEL))
                    + usize::from(rules.contains(InlineRules::BRACKET_LABEL))
            } else {
                0
            };
            InlineNode::RawText(Raw {
                content: &content[escapes.min(content.len() - 1)..],
                location: state.create_block_location(span_start, span_end, state.inline_ctx.offset),
                subs: vec![Substitution::SpecialChars],
            })
        }

        // Complete escaped anchors still belong to the ordinary macro escape rule.
        rule index_label_bracket_match()
        = "\\"+ !("[[" (!"]]" [_])* "]]") ['[' | ']']
          {? state.inline_ctx.rules.contains(InlineRules::INDEX_LABEL).then_some(()).ok_or("index label") }

        rule check_link_quote(mode: InlineRules)
        = {? state.inline_ctx.rules.contains(mode).then_some(()).ok_or("quoted link label") }

        rule check_bracket_label()
        = {? state.inline_ctx.rules.contains(InlineRules::BRACKET_LABEL).then_some(()).ok_or("bracket-delimited label") }

        /// Remove the escape before recognized inline syntax, keeping its text literal.
        rule escaped_syntax() -> InlineNode<'input>
        = check_catalog_escape() escapes:$("\\"+) check_macros() content:$(url_macro_match()) {
            // Asciidoctor recognizes a bare URI escape only with one backslash.
            // A longer run leaves the entire URL macro literal, including the run.
            // Late attribute text also stays literal: the original reference's
            // closing brace prevents URI recognition before attributes expand.
            InlineNode::PlainText(Plain {
                content: if escapes.len() == 1 && macro_token_allowed(state, span_start, 1) {
                    content
                } else {
                    &state.input[span_start..span_end]
                },
                location: state.create_block_location(span_start, span_end, state.inline_ctx.offset),
                escaped: false,
            })
        }
        / check_catalog_escape() "\\" content:escaped_content() {
            InlineNode::PlainText(Plain {
                content,
                location: state.create_block_location(span_start, span_end, state.inline_ctx.offset),
                escaped: false,
            })
        }

        // Typography escapes stay intact for the converter's replacement stage.
        rule escaped_content() -> &'input str
        = escapable_macro_pattern()
        // Double backslash followed by escapable pattern: \\<thing> -> <thing>
        / "\\" inner:escapable_pattern() { inner }
        // Single backslash case: \<thing> -> <thing>
        / escapable_pattern()

        // Recognition must use the enabled macro's complete syntax. Otherwise
        // disabled macros, unknown names, and escaped closing brackets lose text.
        rule escapable_macro_pattern() -> &'input str
        = check_macro_escape() content:$(
            check_index_terms() index_term_match()
            / check_macros()
            // The URI pass consumes link: before named macro escapes are handled.
            !("link:" url_macro_match())
            (cross_reference_shorthand_match() / cross_reference_macro_match()
            / footnote_match() / link_macro_match() / mailto_macro_match()
            // These empty forms still recognize an escape in Asciidoctor.
            / "footnote:[]" / "link:[" ("\\]" / !"]" [_])* "]"
            / inline_image_match() / inline_icon_match() / inline_stem_match()
            / inline_keyboard_match() / inline_button_match() / inline_menu_match()
            / inline_pass_match() / inline_anchor_match())
        ) { content }

        /// Non-macro patterns that can be escaped with a backslash.
        rule escapable_pattern() -> &'input str
        = check_quotes() content:(
        // Formatting escapes belong to quote substitution. Code and labels
        // with quotes disabled must retain the complete authored text.
        // Attribute escapes are consumed by the attribute stage, not formatting.
        // Unconstrained formatting: match entire span including content and closing marker
        // \**not bold** -> **not bold**
        "**" inner:$((!"**" [_])*) "**" { state.intern_fmt(format_args!("**{inner}**")) }
        / "__" inner:$((!("__" !['_']) [_])*) "__" { state.intern_fmt(format_args!("__{inner}__")) }
        / "``" inner:$((!"``" [_])*) "``" { state.intern_fmt(format_args!("``{inner}``")) }
        / "##" inner:$((!"##" [_])*) "##" { state.intern_fmt(format_args!("##{inner}##")) }
        // Typography patterns are NOT handled here — they are handled by the
        // converter's strip_backslash_escapes() pipeline. If the parser stripped the
        // backslash, the converter would never see it and would apply the replacement.
        //
        // Superscript: ^content^ where content has no whitespace (must check complete pattern)
        / "^" inner:$([^'^' | ' ' | '\t' | '\n']+) "^" { state.intern_fmt(format_args!("^{inner}^")) }
        // Subscript: ~content~ where content has no whitespace (must check complete pattern)
        / "~" inner:$([^'~' | ' ' | '\t' | '\n']+) "~" { state.intern_fmt(format_args!("~{inner}~")) }
        // Bracket escapes belong to active macro labels, not ordinary text.
        // Constrained formatting markers and other single escapable chars
        // Note: ^ and ~ are NOT included here - they require complete patterns above
        / c:$(['*' | '_' | '#' | '`' | '&']) { c }
        ) { content }

        /// Match escaped syntax without consuming - for use in negative lookaheads.
        rule escaped_syntax_match() -> ()
        = check_catalog_escape() (
            "\\"+ check_macros() url_macro_match()
            / "\\" (escapable_macro_pattern() / "\\"? escapable_pattern_match())
        )

        /// Match escapable patterns without consuming
        rule escapable_pattern_match() -> ()
        = check_quotes() (
        // Unconstrained formatting: match entire span
        "**" (!"**" [_])* "**"
        / "__" (!("__" !['_']) [_])* "__"
        / "``" (!"``" [_])* "``"
        / "##" (!"##" [_])* "##"
        // Typography patterns handled by converter, not here (see escapable_pattern)
        // Superscript/subscript: require complete pattern
        / "^" [^'^' | ' ' | '\t' | '\n']+ "^"
        / "~" [^'~' | ' ' | '\t' | '\n']+ "~"
        // Single escapable chars (excluding ^ and ~ which need complete patterns)
        / ['*' | '_' | '#' | '`' | '&'] {}
        )

        rule footnote() -> InlineNode<'input>
        = footnote_match:footnote_match()
        {?
            let (start, id, content_start, content_str, end) = footnote_match;

            tracing::debug!(?id, content = %content_str, "Found footnote inline");

            // Repeated definitions use the first body without registering its macros again.
            let content = if content_str.is_empty()
                || id.is_some_and(|id| state.footnote_tracker.borrow().contains(id))
            {
                vec![]
            } else if content_str.trim().is_empty() {
                vec![InlineNode::PlainText(Plain {
                    content: content_str,
                    location: state.create_block_location(content_start, end - 1, state.inline_ctx.offset),
                    escaped: false,
                })]
            } else {
                // The note body belongs to the footnote, not the link's quoted attribute.
                let rules = state.inline_ctx.rules;
                state.inline_ctx.rules.remove(InlineRules::QUOTED_LINK);
                state.inline_ctx.rules.insert(InlineRules::BRACKET_LABEL);
                let content = parse_footnote_content(state, IndexTermSegment { text: content_str, start: content_start });
                state.inline_ctx.rules = rules;
                content?
            };

            let plan = state.inline_ctx.substitutions;
            let registration_substitutions = [Substitution::Attributes, Substitution::Quotes, Substitution::Replacements]
                .iter().any(|stage| plan.precedes(&Substitution::Macros, stage)).then_some(plan);
            let mut footnote = Footnote {
                definition_source: (id.is_some() && !content_str.is_empty()).then(|| {
                    registration_source(state, IndexTermSegment { text: content_str, start: content_start }).0
                }),
                registration_substitutions,
                id,
                content,
                number: 0,
                location: state.create_block_location(start, end, state.inline_ctx.offset),
            };
            if !state.footnote_tracker.borrow_mut().push(&mut footnote) {
                let id = id.unwrap_or_default();
                state.add_generic_warning(format!("invalid footnote reference: {id}"));
                return Ok(InlineNode::PlainText(Plain {
                    content: state.intern_fmt(format_args!("[{id}]")),
                    location: footnote.location,
                    escaped: false,
                }));
            }

            Ok(InlineNode::Macro(InlineMacro::Footnote(footnote)))
        }

        rule footnote_match() -> (usize, Option<&'input str>, usize, &'input str, usize)
        = "footnote:"
        id:id()? "[" content_start:position!() content:footnote_content() "]"
        {?
            (id.is_some() || !content.is_empty())
                .then_some((span_start, id, content_start, content, span_end))
                .ok_or("footnote body or reference id")
        }

        // A closing-bracket escape belongs to the body. Preserve its source bytes
        // until label parsing removes it, including for registration-time text.
        rule footnote_content() -> &'input str
        = content:$(footnote_content_part()*) { content }

        rule footnote_content_part()
        = "\\" ['[' | ']']
        / "[" footnote_content() "]"
        / !("\\" ['[' | ']'] / "[" / "]") [_]

        /// Parse content that may contain balanced square brackets (general case)
        /// This is used for nested link brackets and button labels
        rule balanced_bracket_content() -> &'input str
        = content:$(balanced_bracket_content_part()*) { content }

        /// Individual parts of balanced bracket content - either regular text or nested brackets
        rule balanced_bracket_content_part() -> Cow<'input, str>
        = nested_brackets:("[" inner:balanced_bracket_content() "]" { Cow::Owned(format!("[{inner}]")) })
        / regular_text:$([^('[' | ']')]+) { Cow::Borrowed(regular_text) }

        /// Parse content within brackets using escape handling (no bracket balancing).
        /// Used for stem macros where `\]` and `\[` are common (math notation).
        /// - `\\` → literal `\`
        /// - `\[` or `\]` → literal bracket (backslash stripped)
        /// - `\` before other chars → preserved
        /// - `[` → regular text (no nesting)
        /// - `]` → ends content (consumed by caller)
        rule escaped_bracket_content() -> &'input str
        = parts:escaped_bracket_content_part()* {
            match parts.as_slice() {
                [one] => *one,
                _ => state.intern_join(parts.iter(), ""),
            }
        }

        rule escaped_bracket_content_part() -> &'input str
        = "\\\\" { "\\" }
        / "\\" c:$(['[' | ']']) { c }
        / s:$([^(']' | '\\')]+) { s }
        / "\\" { "\\" }

        // Nested macro delimiters belong to their child nodes, not the outer
        // attribute list. Keep their spans relative to this bracket content.
        rule link_macro_content() -> LinkContent<'input>
        = start:position!() parts:link_macro_content_part()* end:position!() {
            LinkContent {
                raw: &state.input[start..end],
                protected: parts.into_iter().flatten().map(|range| range.start - start..range.end - start).collect(),
            }
        }

        rule link_macro_content_part() -> Option<Range<usize>>
        = "\\]" { None }
        / "[" balanced_bracket_content() "]" { Some(span_start..span_end) }
        // '(' starts shorthand; 'i' starts indexterm:/indexterm2:. Both forms
        // own their brackets, which must not terminate the enclosing link.
        / &['(' | 'i'] check_index_terms() index_term_match() { Some(span_start..span_end) }
        / &['<'] cross_reference_shorthand_match() { Some(span_start..span_end) }
        / (!("\\]" / (&['i'] check_index_terms() index_term_match())) [^'[' | ']' | '(' | '<'])+ { None }
        / ['[' | '(' | '<'] { None }

        rule inline_pass() -> InlineNode<'input>
        = check_pass_token() "pass:"
        substitutions:($([^('[' | ']' | ',')]+) ** comma())
        "["
        content:$(("\\]" / [^']'])*)
        "]"
        {
            tracing::debug!(?content, "Found pass inline");
            let location = state.create_block_location(span_start, span_end, state.inline_ctx.offset);
            InlineNode::Macro(InlineMacro::Pass(Pass {
                attribute_fragments: Box::default(),
                text: Some(content),
                substitutions: substitutions.into_iter().filter_map(|s| parse_substitution(s.trim())).collect(),
                location,
                kind: PassthroughKind::Macro,
            }))
        }

        /// Match inline pass without consuming - for use in negative lookaheads.
        rule inline_pass_match()
        = check_pass_token() "pass:" ([^('[' | ']' | ',')]+ ("," [^('[' | ']' | ',')]+)*)? "[" ("\\]" / [^']'])* "]"

        // Whole-value pass wrappers are resolved at definition time. Substitution must
        // not activate a wrapper that remains inside the stored attribute value.
        rule check_pass_token()
        = start:position!() {?
            if let Some((names, _)) = state.input[start..].strip_prefix("pass:").and_then(|tail| tail.split_once('['))
                && (start..start + 6 + names.len()).any(|pos| byte_came_from_attribute(state, pos))
            {
                Err("passthrough introduced by attribute")
            } else {
                Ok(())
            }
        }

        rule index_term_concealed() -> InlineNode<'input>
        = content:index_term_concealed_content() {?
            let location = state.create_block_location(span_start, span_end, state.inline_ctx.offset);
            parse_index_term(state, content, IndexTermForm::Concealed, location)
        }

        rule index_term_flow() -> InlineNode<'input>
        = content:index_term_flow_content() {?
            let location = state.create_block_location(span_start, span_end, state.inline_ctx.offset);
            parse_index_term(state, content, IndexTermForm::Flow, location)
        }

        rule indexterm_macro() -> InlineNode<'input>
        = start:position!() "indexterm:" content:index_term_macro_content() check_index_token(start, 11) {?
            let location = state.create_block_location(span_start, span_end, state.inline_ctx.offset);
            parse_index_term(state, content, IndexTermForm::NamedConcealed, location)
        }

        rule indexterm2_macro() -> InlineNode<'input>
        = start:position!() "indexterm2:" content:index_term_macro_content() check_index_token(start, 12) {?
            let location = state.create_block_location(span_start, span_end, state.inline_ctx.offset);
            parse_index_term(state, content, IndexTermForm::NamedFlow, location)
        }

        pub(crate) rule index_term_macro_list(base: usize) -> Vec<IndexTermMacroItem<'input>>
        = items:(index_term_macro_item(base) ** ",") { items }

        rule index_term_macro_item(base: usize) -> IndexTermMacroItem<'input>
        = whitespace()* "see-also" "=" targets:index_term_see_also_value(base) whitespace()* {
            IndexTermMacroItem::Relationship(IndexTermRelationshipSegments::SeeAlso(targets))
        }
        / whitespace()* "see" "=" target:index_term_relation_value(base) whitespace()* {
            IndexTermMacroItem::Relationship(IndexTermRelationshipSegments::See(target))
        }
        / term:index_term_segment(base) { IndexTermMacroItem::Term(term) }

        rule index_term_relation_value(base: usize) -> IndexTermSegment<'input>
        = "\"" start:position!() content:$([^'"']*) "\"" &(whitespace()* ("," / ![_])) {
            trimmed_index_term_segment(content, base + start)
        }
        / start:position!() content:$([^',']*) {
            trimmed_index_term_segment(content, base + start)
        }

        rule index_term_see_also_value(base: usize) -> Vec<IndexTermSegment<'input>>
        = "\"" targets:(index_term_see_also_target(base) ** ",") "\"" &(whitespace()* ("," / ![_])) { targets }
        / target:index_term_relation_value(base) { vec![target] }

        rule index_term_see_also_target(base: usize) -> IndexTermSegment<'input>
        = whitespace()* start:position!() content:$([^(',' | '"')]*) whitespace()* {
            trimmed_index_term_segment(content, base + start)
        }

        /// Parse comma-separated index term list with support for quoted segments
        /// e.g., "knight, Knight of the Round Table, Lancelot"
        /// or "knight, \"Arthur, King\"" (quoted segment with embedded comma)
        pub(crate) rule index_term_list(base: usize) -> Vec<IndexTermSegment<'input>>
        = terms:(index_term_segment(base) ** ",") {
            terms.into_iter().filter(|segment| !segment.text.is_empty()).collect()
        }

        /// Parse a single index term segment, either quoted or unquoted
        rule index_term_segment(base: usize) -> IndexTermSegment<'input>
        = whitespace()? segment:(index_term_quoted(base) / index_term_unquoted(base)) whitespace()? { segment }

        /// Quotes group commas; the macro delimiter can leave the quote unclosed.
        rule index_term_quoted(base: usize) -> IndexTermSegment<'input>
        = "\"" start:position!() content:$([^'"']*) "\""? &(whitespace()* ("," / ![_])) {
            trimmed_index_term_segment(content, base + start)
        }

        /// Unquoted segment: term without comma
        rule index_term_unquoted(base: usize) -> IndexTermSegment<'input>
        = start:position!() content:$([^',']+) {
            trimmed_index_term_segment(content, base + start)
        }

        rule check_index_token(start: usize, len: usize)
        = {? macro_token_allowed(state, start, len).then_some(()).ok_or("index syntax introduced after macros") }

        rule index_term_shorthand_close() = start:position!() "))" check_index_token(start, 2) !")"
        rule index_term_concealed_close() = start:position!() ")" index_term_shorthand_close() check_index_token(start, 3)

        // Concealed shorthand retains its legacy delimiter precedence.
        rule index_term_concealed_content() -> IndexTermSegment<'input>
        = !escaped_index_inner() open:position!() "(((" check_index_token(open, 3) start:position!()
          content:$((!(index_term_concealed_close() / index_term_shorthand_close()) [_])*)
          index_term_concealed_close() {
            IndexTermSegment { text: content, start }
        }

        // Fallback for literal, unbalanced parentheses in pre-spec labels.
        rule index_term_flow_close() = start:position!() "))" check_index_token(start, 2) !"))"

        rule index_term_flow_content() -> IndexTermSegment<'input>
        = (escaped_index_inner() / !"(((") open:position!() "((" check_index_token(open, 2) start:position!()
          content:index_term_flow_body() {?
            index_content_present(state, start, content)
                .then_some(IndexTermSegment { text: content, start })
                .ok_or("empty index term")
        }

        // Preserve internal pairs before selecting the outer delimiter. If the
        // label is not balanced, retain the existing literal-parenthesis rules.
        rule index_term_flow_body() -> &'input str
        = content:$((index_term_parenthesis_group() / "\\" [_] / [^'(' | ')' | '\\'])*)
          close:position!() "))" check_index_token(close, 2) { content }
        / content:$((!index_term_flow_close() [_])*) index_term_flow_close() { content }

        rule index_term_parenthesis_group()
        = "(" (index_term_parenthesis_group() / "\\" [_] / [^'(' | ')' | '\\'])* ")"

        // Escaping a concealed shorthand leaves its inner visible term active.
        rule escaped_index_prefix() -> InlineNode<'input>
        = escapes:$("\\"+) &index_term_concealed_content() "(" {
            InlineNode::PlainText(Plain {
                content: state.intern_fmt(format_args!("{}(", &escapes[..escapes.len() - 1])),
                location: state.create_block_location(span_start, span_end, state.inline_ctx.offset),
                escaped: false,
            })
        }

        rule escaped_index_inner() -> ()
        = pos:position!() {?
            state.input[..pos].ends_with("\\(").then_some(()).ok_or("unescaped index term")
        }

        rule index_term_macro_content() -> IndexTermSegment<'input>
        = "[" start:position!() content:$(("\\]" / !index_term_macro_close() [_])*) index_term_macro_close() {?
            index_content_present(state, start, content)
                .then_some(IndexTermSegment { text: content, start })
                .ok_or("empty index term")
        }

        rule index_term_macro_close() = start:position!() "]" check_index_token(start, 1)

        /// Match index term patterns without consuming (for negative lookahead in plain_text)
        rule index_term_match() -> ()
        = index_term_concealed_content() {}
        / index_term_flow_content() {}
        / start:position!() "indexterm:" index_term_macro_content() check_index_token(start, 11) {}
        / start:position!() "indexterm2:" index_term_macro_content() check_index_token(start, 12) {}

        rule inline_menu() -> InlineNode<'input>
        = check_experimental() "menu:"
        target:$([^'[']+)
        "["
        items:((item:$([^(']' | '>')]+) { item.trim() }) ** (">" whitespace()?))
        "]"
        {
            tracing::debug!(%target, ?items, "Found menu inline");
            InlineNode::Macro(InlineMacro::Menu(Menu {
                target,
                items,
                location: state.create_block_location(span_start, span_end, state.inline_ctx.offset),
            }))
        }

        /// Match inline menu without consuming - for use in negative lookaheads.
        rule inline_menu_match()
        = check_experimental() "menu:" [^'[']+ "[" ([^']' | '>']+ (">" whitespace()? [^']' | '>']+)*)? "]"

        rule inline_button() -> InlineNode<'input>
        = check_experimental() "btn:[" label:$balanced_bracket_content() "]"
        {
            tracing::debug!(?label, "Found button inline");
            InlineNode::Macro(InlineMacro::Button(Button {
                label: label.trim(),
                location: state.create_block_location(span_start, span_end, state.inline_ctx.offset),
            }))
        }

        /// Match inline button without consuming - for use in negative lookaheads.
        rule inline_button_match()
        = check_experimental() "btn:[" balanced_bracket_content() "]"

        rule inline_keyboard() -> InlineNode<'input>
        = check_experimental() "kbd:["
        keys:((key:$([^(']' | '+' | ',')]+) { key.trim() }) ** (("," / "+") whitespace()?))
        "]"
        {
            tracing::debug!(?keys, "Found keyboard inline");
            InlineNode::Macro(InlineMacro::Keyboard(Keyboard {
                keys,
                location: state.create_block_location(span_start, span_end, state.inline_ctx.offset),
            }))
        }

        /// Match inline keyboard without consuming - for use in negative lookaheads.
        rule inline_keyboard_match()
        = check_experimental() "kbd:[" [^']' | '+' | ',']+ (("," / "+") whitespace()? [^']' | '+' | ',']+)* "]"

        /// Parse URL macros with attribute handling.
        ///
        /// URL macros have the format: `https://example.com[text,attr1=value1,attr2=value2]`
        ///
        /// This is similar to link macros but the URL is directly specified rather than
        /// using the `link:` prefix.
        rule url_macro() -> InlineNode<'input>
        = target:url()
        "["
        content_start:position!() content:link_macro_content() "]"
        {?
            tracing::debug!(?target, "Found url macro");
            let bm = BlockParsingMetadata {
                substitutions: state.inline_ctx.substitutions,
                ..BlockParsingMetadata::default()
            };
            let raw = content.raw;
            let ProcessedLinkContent { text, attributes, .. } = process_link_content(state, &bm, content_start, span_end, &content, false)
                .map_err(|error| {
                    tracing::error!(?error, link_text = raw, "could not process link text");
                    "could not process link text"
                })?;
            let interned_target = state.intern_cow(target);
            let target_source = Source::from_str_borrowed(interned_target).map_err(|_| "failed to parse URL target")?;
            Ok(InlineNode::Macro(InlineMacro::Url(Url {
                text,
                target: target_source,
                attributes,
                location: state.create_block_location(span_start, span_end, state.inline_ctx.offset),
                hide_uri_scheme: state.document_attributes.contains_key("hide-uri-scheme"),
            })))
        }

        /// Match URL macro without consuming - for use in negative lookaheads.
        /// Inlines the url_path character class to avoid action-block processing.
        rule url_macro_match()
        = ("https" / "http" / "ftp" / "irc") "://" ['A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '.' | '_' | '~' | ':' | '/' | '?' | '#' | '@' | '!' | '$' | '&' | '\'' | '(' | ')' | '*' | '+' | ',' | ';' | '=' | '%' | '\\']+ "[" ("\\]" / !"]" [_])* "]"

        /// Parse `mailto:` macros with attribute handling.
        ///
        /// `mailto:` macros accept `[text,subject,body]`, followed by named attributes.
        ///
        /// This is similar to link macros but the `mailto:` is directly specified rather
        /// than using the `link:` prefix.
        rule mailto_macro() -> InlineNode<'input>
        = target:$("mailto:" email_address() ("?" url_path_char()*)?)
        "["
        content_start:position!() content:link_macro_content() "]"
        {?
            tracing::debug!(?target, "Found mailto macro");
            let bm = BlockParsingMetadata {
                substitutions: state.inline_ctx.substitutions,
                ..BlockParsingMetadata::default()
            };
            let raw = content.raw;
            let ProcessedLinkContent { text, attributes, subject, body } = process_link_content(state, &bm, content_start, span_end, &content, true)
                .map_err(|error| {
                    tracing::error!(?error, link_text = raw, "could not process link text");
                    "could not process link text"
                })?;
            let target_source = Source::from_str_borrowed(target).map_err(|_| "failed to parse mailto target")?;
            Ok(InlineNode::Macro(InlineMacro::Mailto(Mailto {
                text,
                subject,
                body,
                target: target_source,
                attributes,
                location: state.create_block_location(span_start, span_end, state.inline_ctx.offset),
            })))
        }

        /// Match mailto macro without consuming - for use in negative lookaheads.
        /// Inlines the url/email_address patterns to avoid action-block processing.
        rule mailto_macro_match()
        = "mailto:" email_address() ("?" url_path_char()*)? "[" ("\\]" / !"]" [_])* "]"

        rule check_autolinks() -> ()
        = {? if state.inline_ctx.rules.contains(InlineRules::AUTOLINKS) { Ok(()) } else { Err("autolinks suppressed") } }

        rule check_macros() -> ()
        = position:position!() {?
            if state.inline_ctx.substitutions.enabled(&Substitution::Macros)
                && structural_token_allowed(state, &Substitution::Macros, position, 1)
                && catalog_macro_allowed(state, position)
            {
                Ok(())
            } else {
                Err("macros disabled")
            }
        }

        rule check_index_terms() -> ()
        = position:position!() {?
            if state.inline_ctx.rules.contains(InlineRules::INDEX_TERMS)
                && state.inline_ctx.substitutions.enabled(&Substitution::Macros)
                && structural_token_allowed(state, &Substitution::Macros, position, 1)
            {
                Ok(())
            } else {
                Err("index terms suppressed")
            }
        }

        rule check_experimental() -> ()
        = {? if state.document_attributes.contains_key("experimental") { Ok(()) } else { Err("experimental UI macros disabled") } }

        rule check_post_replacements() -> ()
        = {? if state.inline_ctx.substitutions.enabled(&Substitution::PostReplacements) { Ok(()) } else { Err("post_replacements disabled") } }

        rule check_replacements_before_post_replacements() -> ()
        = {? if state.inline_ctx.substitutions.precedes(
            &Substitution::Replacements,
            &Substitution::PostReplacements,
        ) { Ok(()) } else { Err("replacements do not precede post_replacements") } }

        rule check_hardbreaks() -> ()
        = {? if state.inline_ctx.rules.contains(InlineRules::HARD_BREAKS) { Ok(()) } else { Err("hard breaks disabled") } }

        rule check_quotes() -> ()
        = {? if state.inline_ctx.substitutions.enabled(&Substitution::Quotes) { Ok(()) } else { Err("quotes disabled") } }

        // A later empty expansion leaves a valid quote body, unlike literal
        // adjacent delimiters. Callers also check for their closing delimiter.
        rule empty_quote_content()
        = position:position!() {?
            (state.inline_ctx.substitutions.precedes(&Substitution::Quotes, &Substitution::Attributes)
                && state.empty_attribute_offsets.binary_search(&position).is_ok())
                .then_some(()).ok_or("quote content was already empty")
        }

        rule check_catalog_escape()
        = position:position!() {? catalog_escape_allowed(state, position).then_some(()).ok_or("escape belongs to a later substitution") }

        // This position follows the backslash. An attribute expanded after
        // macros cannot introduce an escape for that earlier substitution.
        rule check_macro_escape()
        = position:position!() {?
            macro_token_allowed(state, position.saturating_sub(1), 1)
                .then_some(()).ok_or("escape introduced after macros")
        }

        rule check_quote_markers(open: (usize, usize), close: (usize, usize)) -> ()
        = {?
            if structural_token_allowed(state, &Substitution::Quotes, open.0, open.1)
                && structural_token_allowed(state, &Substitution::Quotes, close.0, close.1)
            {
                Ok(())
            } else {
                Err("formatting markers were introduced after quote substitution")
            }
        }

        rule inline_autolink() -> InlineNode<'input>
        = url_info:(
            "<" url:url() ">" { (url, true) }
            / "<" url:email_address() ">" { (Cow::Owned(format!("mailto:{url}")), true) }
            / url:bare_url() { (url, false) }
            / email_at_sign_ahead() url:email_address() { (Cow::Owned(format!("mailto:{url}")), false) }
        )
        {?
            let (url, bracketed) = url_info;
            tracing::debug!(?url, bracketed, "Found autolink inline");
            let interned_url = state.intern_cow(url);
            let url_source = Source::from_str_borrowed(interned_url).map_err(|_| "failed to parse autolink URL")?;
            Ok(InlineNode::Macro(InlineMacro::Autolink(Autolink {
                url: url_source,
                bracketed,
                location: state.create_block_location(span_start, span_end, state.inline_ctx.offset),
                hide_uri_scheme: state.document_attributes.contains_key("hide-uri-scheme"),
            })))
        }

        /// Match inline autolink without consuming - for use in negative lookaheads.
        /// Uses existing sub-rules (url, bare_url, email_address) for correctness;
        /// avoids Source::from_str_borrowed and Autolink struct allocation.
        rule inline_autolink_match()
        = "<" url() ">"
        / "<" email_address() ">"
        / bare_url()
        / email_at_sign_ahead() email_address()

        rule replacement_consumes_eol()
        = check_replacements_before_post_replacements() eol() "--" (" " / eol() / ![_])

        rule line_break_eol()
        = !replacement_consumes_eol() eol()

        /// End of a hard line break: either a newline, or — only in a top-level
        /// block-content parse — end of input. The block-level gate keeps a
        /// nested span ending in ` +` (e.g. `` `code +` ``, a footnote/link)
        /// literal, since `process_inlines` re-parses that inner content and
        /// would otherwise see its trailing ` +` as ending the input.
        rule inline_line_break_end()
        = line_break_eol()
        / ![_] {? if state.inline_ctx.rules.contains(InlineRules::EOI_HARD_BREAK) { Ok(()) } else { Err("hard line break at EOI requires block level") } }

        rule inline_line_break() -> InlineNode<'input>
        = " +" end:position!() inline_line_break_end()
        {?
            // `end` captures position before the trailing eol so the line break's
            // location doesn't include the newline.
            if !has_inline_line_break_prefix(state, span_start) {
                return Err("hard line break requires preceding content or an empty attribute");
            }

            tracing::debug!("Found inline line break");
            Ok(InlineNode::LineBreak(LineBreak {
                location: state.create_block_location(span_start, end, state.inline_ctx.offset),
            }))
        }

        /// Match inline line break without consuming - for use in negative lookaheads.
        rule inline_line_break_match()
        = " +" inline_line_break_end()
        {?
            if has_inline_line_break_prefix(state, span_start) {
                Ok(())
            } else {
                Err("hard line break requires preceding content or an empty attribute")
            }
        }

        rule automatic_line_break() -> InlineNode<'input>
        = line_break_eol()
        {
            InlineNode::LineBreak(LineBreak {
                location: state.create_block_location(span_start, span_end, state.inline_ctx.offset),
            })
        }

        rule hard_wrap() -> InlineNode<'input>
            = " + \\" &eol()
        {
            tracing::debug!("Found hard wrap inline");
            // The trailing `&eol()` is a zero-width lookahead, so `span_end` matches
            // the position right after `\` (before the newline).
            InlineNode::LineBreak(LineBreak {
                location: state.create_block_location(span_start, span_end, state.inline_ctx.offset),
            })
        }

        /// Match hard wrap without consuming - for use in negative lookaheads.
        rule hard_wrap_match()
        = " + \\" &eol()

        rule inline_icon() -> InlineNode<'input>
        = "icon:" source:source() attributes:macro_attributes()
        {
            let (_discrete, metadata, _title_position) = attributes;
            let mut metadata = metadata.clone();
            metadata.move_positional_attributes_to_attributes();
            // For font mode, the first positional (style) can be a size value (1x, 2x,
            // lg, fw) -> stored as "size" attribute;
            //
            // For image mode, the first positional (style) can be alt text.
            if let Some(style) = metadata.style.take() {
                // Strip surrounding quotes if present (quoted positional attributes)
                let style_value = strip_quotes(style).to_owned();
                if ICON_SIZES.contains(&style_value.as_str()) {
                    // Named size= attribute takes precedence over positional size so we
                    // insert rather than set (set overrides).
                    metadata.attributes.insert(
                        "size".into(),
                        AttributeValue::String(Cow::Owned(style_value)),
                    );
                } else {
                    // Other value become alt (fa-{value} in image mode)
                    metadata.attributes.set(
                        "alt".into(),
                        AttributeValue::String(Cow::Owned(style_value)),
                    );
                }
            }
            // Copy roles to attributes so they're accessible in the converter
            if !metadata.roles.is_empty() {
                metadata.attributes.set(
                    "role".into(),
                    AttributeValue::String(metadata.roles.join(" ").into()),
                );
            }
            InlineNode::Macro(InlineMacro::Icon(Icon {
                target: source,
                attributes: metadata.attributes.clone(),
                location: state.create_block_location(span_start, span_end, state.inline_ctx.offset),
            }))
        }

        /// Match inline icon without consuming - for use in negative lookaheads.
        /// Uses source()/macro_attributes() for correctness; avoids the heavy
        /// action-block processing (metadata manipulation, attribute copying).
        rule inline_icon_match()
        = "icon:" source() "[" (!"]" [_])* "]"

        rule inline_stem() -> InlineNode<'input>
        = prefix:$("latexmath" / "asciimath" / "stem") ":[" content:escaped_bracket_content() "]"
        {
            let notation = match prefix {
                "latexmath" => StemNotation::Latexmath,
                "asciimath" => StemNotation::Asciimath,
                _ => {
                    // stem:[] — resolve from :stem: document attribute
                    match state.document_attributes.text("stem") {
                        Some(s) => StemNotation::from_str(s).unwrap_or(StemNotation::Asciimath),
                        _ => StemNotation::Asciimath,
                    }
                }
            };

            InlineNode::Macro(InlineMacro::Stem(Stem {
                content,
                notation,
                location: state.create_block_location(span_start, span_end, state.inline_ctx.offset),
            }))
        }

        /// Match inline stem without consuming - for use in negative lookaheads.
        /// Replicates escaped_bracket_content matching pattern (handles \] escapes).
        rule inline_stem_match()
        = ("latexmath" / "asciimath" / "stem") ":[" ("\\\\" / "\\" ['[' | ']'] / [^(']' | '\\')]+ / "\\")* "]"

        rule inline_image() -> InlineNode<'input>
        = "image:" source:media_source() attributes:image_macro_attributes()
        {?
            let (_discrete, metadata, title_position) = attributes;
            let mut metadata = metadata.clone();
            let mut title = Title::default();
            if let Some(style) = metadata.style.take() {
                // For inline images, the first positional attribute is the alt text (title)
                title = Title::new(vec![InlineNode::PlainText(Plain {
                    content: style,
                    location: state.create_block_location(span_start, span_end, state.inline_ctx.offset),
                    escaped: false,
                })]);
            }
            let [width, height] = metadata.take_positional_attributes::<2>();
            if let Some(height) = height {
                metadata.attributes.insert("height".into(), AttributeValue::String(Cow::Borrowed(height.value)));
            }
            if let Some(width) = width {
                metadata.attributes.insert("width".into(), AttributeValue::String(Cow::Borrowed(width.value)));
            }
            metadata.move_positional_attributes_to_attributes();
            // For inline images, if there's no first positional (no alt text in title field),
            // check if there's a named title attribute. Only then should we use it to populate
            // the title field for rendering purposes, but we keep it in attributes for the
            // HTML title attribute (hover text).
            if title.is_empty()
                && metadata.attributes.get("title").is_some()
                && let Some((title_start, title_end)) = title_position
            {
                // Get the title content directly from the input to avoid lifetime issues
                // with local borrows from metadata.attributes
                let content: &'input str = &state.input[title_start..title_end];
                let bm = BlockParsingMetadata {
                    substitutions: state.inline_ctx.substitutions,
                    ..BlockParsingMetadata::default()
                };
                // Use the captured position from the named_attribute rule
                let title_start_pos = PositionWithOffset {
                    offset: title_start,
                    position: state.line_map.offset_to_position(title_start, state.input),
                };
                let (title_inlines, _) = process_inlines_or_err!(
                    process_inlines(state, &bm, title_start_pos.offset, title_end, state.inline_ctx.offset, content),
                    "could not process title in inline image macro"
                )?;
                title = Title::new(title_inlines);
            }
            // Note: We do NOT remove the title attribute - it's needed for the HTML title attribute

            Ok(InlineNode::Macro(InlineMacro::Image(Box::new(Image {
                title,
                source,
                metadata: metadata.clone(),
                location: state.create_block_location(span_start, span_end, state.inline_ctx.offset),

            }))))
        }

        /// Match inline image without consuming - for use in negative lookaheads.
        rule inline_image_match()
        = "image:" media_source() "[" (!"]" [_])* "]"

        /// Parse a link target and its optional label or named attributes.
        rule link_macro() -> InlineNode<'input>
        = "link:" target:link_macro_source() fragment:path_fragment()? "["
        content_start:position!() content:link_macro_content() "]"
        {?
            tracing::debug!(?target, ?content, "Found link macro inline");
            let bm = BlockParsingMetadata {
                substitutions: state.inline_ctx.substitutions,
                ..BlockParsingMetadata::default()
            };
            let target = match fragment {
                Some(f) => {
                    let combined = state.intern_fmt(format_args!("{target}{f}"));
                    Source::from_str_borrowed(combined).unwrap_or(target)
                }
                None => target,
            };
            let raw = content.raw;
            let ProcessedLinkContent { text, attributes, .. } = process_link_content(state, &bm, content_start, span_end, &content, false)
                .map_err(|error| {
                    tracing::error!(?error, link_text = raw, "could not process link text");
                    "could not process link text"
                })?;
            Ok(InlineNode::Macro(InlineMacro::Link(Link {
                text,
                target,
                attributes,
                location: state.create_block_location(span_start, span_end, state.inline_ctx.offset),
                hide_uri_scheme: state.document_attributes.contains_key("hide-uri-scheme"),
            })))
        }

        /// Match link macro without consuming - for use in negative lookaheads.
        rule link_macro_match()
        = "link:" link_macro_source() path_fragment()? "[" ("\\]" / !"]" [_])* "]"

        // Asciidoctor's URI pass removes this escape before the named link pass.
        rule link_macro_source() -> Source<'input>
        = ("\\" &("http://" / "https://" / "ftp://" / "irc://"))? target:source() { target }

        /// Parse cross-reference shorthand syntax: <<id>> or <<id,custom text>>
        rule cross_reference_shorthand() -> InlineNode<'input>
        = shorthand:cross_reference_shorthand_pattern()
        {?
            let (target, raw_text) = shorthand;
            let target_str: &'input str = target;
            let bm = BlockParsingMetadata {
                substitutions: state.inline_ctx.substitutions,
                ..BlockParsingMetadata::default()
            };
            // Shorthand-specific pre-trim: asciidoctor treats whitespace-only
            // custom text in `<<id, >>` as "no custom text" and falls back to
            // the target's section title. We drop to `vec![]` here so the
            // converter hits its fallback branch. This diverges from the
            // `xref:id[ ]` macro form, where `process_inlines_no_autolinks`
            // preserves the whitespace literally — matching asciidoctor's
            // asymmetry between the two syntaxes.
            let text = if let Some((content_start, t)) = raw_text {
                let trimmed = t.trim();
                if trimmed.is_empty() {
                    vec![]
                } else {
                    let content_pos = PositionWithOffset {
                        offset: content_start,
                        position: state.line_map.offset_to_position(content_start, state.input),
                    };
                    process_inlines_no_autolinks(state, &bm, content_pos.offset, span_end, state.inline_ctx.offset, trimmed)
                        .map_err(|e| {
                            tracing::error!(?e, xref_text = trimmed, "could not process xref text");
                            "could not process xref text"
                        })?
                }
            } else {
                vec![]
            };
            tracing::debug!(?target_str, ?text, "Found cross-reference shorthand");
            let location = state.create_block_location(span_start, span_end, state.inline_ctx.offset);
            let mut xref = crate::CrossReference::new(target_str, location).with_text(text);
            xref.resolve_natural_target = !state.document_attributes.contains_key("compat-mode");
            if xref.resolve_natural_target {
                xref.source_syntax = crate::model::XrefSourceSyntax::Shorthand;
            }
            xref.target_is_local = xref.source_syntax.target_is_local(xref.target);
            xref.xrefstyle = crate::XrefStyle::from_attribute(
                state
                    .document_attributes
                    .get("xrefstyle")
                    .map(|value| value.text().unwrap_or_default())
                    .map(crate::strip_quotes),
            );
            if xref.text.is_empty() {
                xref.caption_label_snapshot_id = Some(state.capture_xref_caption_labels());
            }
            Ok(InlineNode::Macro(InlineMacro::CrossReference(xref)))
        }

        /// Pattern for cross-reference shorthand: <<id>> or <<reference text,custom text>>
        rule cross_reference_shorthand_pattern() -> (&'input str, Option<(usize, &'input str)>)
        = "<<" target:$((!("," / ">>") [_])+) content:("," content_start:position!() text:$((!">>" [_])+) { (content_start, text) })? ">>"
        {?
            if target
                .chars()
                .next()
                .is_some_and(|character| character.is_alphanumeric() || matches!(character, '_' | '#' | '/' | '.' | ':' | '{'))
            {
                Ok((target, content))
            } else {
                Err("invalid first character in cross-reference shorthand")
            }
        }

        /// Parse cross-reference macro syntax: xref:id[text] or xref:file.adoc#anchor[text]
        rule cross_reference_macro() -> InlineNode<'input>
        = "xref:" target_str:xref_target() "[" content_start:position!() raw_text:cross_reference_macro_text() "]"
        {?
            let bm = BlockParsingMetadata {
                substitutions: state.inline_ctx.substitutions,
                ..BlockParsingMetadata::default()
            };
            let macro_text = xref_macro_text(
                raw_text,
                state.document_attributes.contains_key("compat-mode"),
            );
            let text = if macro_text.text.is_empty() {
                vec![]
            } else {
                let rules = state.inline_ctx.rules;
                state.inline_ctx.rules.insert(InlineRules::BRACKET_LABEL);
                state.inline_ctx.rules.remove(InlineRules::QUOTED_LINK);
                let parsed = match macro_text.text {
                    Cow::Borrowed(text) => {
                        // Brackets read as written keep the span they always had.
                        let start = content_start + macro_text.offset;
                        let end = if macro_text.as_written { span_end } else { start + text.len() };
                        process_inlines_no_autolinks(state, &bm, start, end, state.inline_ctx.offset, text)
                    }
                    Cow::Owned(text) => {
                        let start = content_start + macro_text.offset + state.inline_ctx.offset;
                        let text = state.intern_str(&text);
                        process_unescaped_xref_label(state, &bm, text, start, &macro_text.source_map)
                    }
                };
                state.inline_ctx.rules = rules;
                parsed
                    .map_err(|e| {
                        tracing::error!(?e, xref_text = raw_text, "could not process xref text");
                        "could not process xref text"
                    })?
            };
            tracing::debug!(?target_str, ?text, "Found cross-reference macro");
            let location = state.create_block_location(span_start, span_end, state.inline_ctx.offset);
            let mut xref = crate::CrossReference::new(target_str, location).with_text(text);
            if !state.document_attributes.contains_key("compat-mode") {
                xref.source_syntax = crate::model::XrefSourceSyntax::Macro;
            }
            xref.target_is_local = xref.source_syntax.target_is_local(xref.target);
            // A per-reference `xrefstyle=` wins over the document's, including
            // an unrecognised one, which Asciidoctor treats as `basic`.
            xref.xrefstyle = crate::XrefStyle::from_attribute(
                macro_text
                    .xrefstyle
                    .as_deref()
                    .or_else(|| state.document_attributes.get("xrefstyle").map(|value| value.text().unwrap_or_default()))
                    .map(crate::strip_quotes),
            );
            xref.role = macro_text.role.as_deref().map(|role| state.intern_str(role));
            if xref.text.is_empty() {
                xref.caption_label_snapshot_id = Some(state.capture_xref_caption_labels());
            }
            Ok(InlineNode::Macro(InlineMacro::CrossReference(xref)))
        }

        /// Parse explicit xref text without balancing arbitrary brackets.
        ///
        /// Escaped closing brackets and supported nested macros belong to the label.
        /// Other closing brackets end the xref.
        rule cross_reference_macro_text() -> &'input str
        = text:$(cross_reference_macro_text_part()*) { text }

        rule cross_reference_macro_text_part()
        = "\\]"
        / protected_cross_reference_text_macro()
        / !"]" [_]

        rule protected_cross_reference_text_macro()
        = &['p'] inline_pass_match()
        / check_macros() (
            &['[' | 'a'] inline_anchor_match()
            / &['a' | 's'] inline_stem_match()
            / &['b'] inline_button_match()
            / &['f'] footnote_match() {}
            / &['f' | 'h'] url_macro_match()
            / &['i'] (
                (check_index_terms() index_term_match())
                / inline_image_match()
                / inline_icon_match()
                / url_macro_match()
            )
            / &['k'] inline_keyboard_match()
            / &['l'] (inline_stem_match() / link_macro_match())
            / &['m'] (inline_menu_match() / mailto_macro_match())
        )

        /// Match cross-reference shorthand syntax without consuming.
        rule cross_reference_shorthand_match() -> ()
        = cross_reference_shorthand_pattern() {}

        /// Match cross-reference macro syntax without consuming: xref:id[text] or xref:file.adoc#anchor[text]
        rule cross_reference_macro_match()
        = "xref:" xref_target() "[" cross_reference_macro_text() "]"

        rule bold_text_unconstrained() -> InlineNode<'input>
            = attrs:inline_attributes()? start:position!() "**" content_start:position!() content:$(empty_quote_content() &"**" / (!(eol() / ![_] / "**") [_])+) close:position!() "**" check_quote_markers((start, 2), (close, 2)) end:position!()
        {?
            let role = attrs.as_ref().and_then(|(roles, _id)| {
                if roles.is_empty() {
                    None
                } else {
                    Some(state.intern_join(roles.iter(), " "))
                }
            });
            let id = attrs.as_ref().and_then(|(_roles, id)| *id);

            let bm = BlockParsingMetadata {
                substitutions: state.inline_ctx.substitutions,
                ..BlockParsingMetadata::default()
            };
            tracing::debug!(?start, ?content_start, ?end, offset = ?state.inline_ctx.offset, ?content, ?role, "Found unconstrained bold text inline");
            let (content, _) = process_inlines_or_err!(
                process_inlines(state, &bm, content_start, end - 2, state.inline_ctx.offset, content),
                "could not process unconstrained bold text content"
            )?;
            Ok(InlineNode::BoldText(Bold {
                content,
                role,
                id,
                form: Form::Unconstrained,
                location: state.create_block_location(start, end, state.inline_ctx.offset),
            }))
        }

        /// Match unconstrained bold without consuming - for use in negative lookaheads.
        rule bold_text_unconstrained_match()
        = inline_attributes()? open:position!() "**" (empty_quote_content() &"**" / (!(eol() / ![_] / "**") [_])+) close:position!() "**" check_quote_markers((open, 2), (close, 2))

        /// A different non-word character or end of input closes constrained formatting.
        /// Formatting marks are punctuation too; an underscore remains a word
        /// character. A repeated marker stays inside its delimiter run. Consuming
        /// the boundary supports both `&` and `!` lookaheads.
        rule constrained_boundary_follow(marker: char)
        = !['a'..='z' | 'A'..='Z' | '0'..='9' | '_'] c:['\0'..='\x7f'] {?
            if c == marker { Err("same formatting marker") } else { Ok(()) }
        }
        / non_word_non_ascii_char()
        / ![_]

        /// A single non-ASCII character that is not a Unicode word character
        /// (letter or number).
        rule non_word_non_ascii_char()
        = c:$([_]) {?
            match c.chars().next() {
                Some(ch) if !ch.is_ascii() && !ch.is_alphanumeric() => Ok(()),
                _ => Err("not a non-word non-ASCII character"),
            }
        }

        rule bold_text_constrained() -> InlineNode<'input>
        = attrs:inline_attributes()?
        start:position!()
        content_start:position()
        "*"
        content:$(
            empty_quote_content() &"*"
            / [^(' ' | '\t' | '\n')] [^'*']* ("*" !constrained_boundary_follow('*') [^'*']*)*
        )
        close:position!() "*" check_quote_markers((start, 1), (close, 1))
        end:position!() &constrained_boundary_follow('*')
        {?
            let role = attrs.as_ref().and_then(|(roles, _id)| {
                if roles.is_empty() {
                    None
                } else {
                    Some(state.intern_join(roles.iter(), " "))
                }
            });
            let id = attrs.as_ref().and_then(|(_roles, id)| *id);

            // Check if we're at start of input OR preceded by word boundary character
            let absolute_pos = start + state.inline_ctx.offset;
            if !check_constrained_opening_boundary(absolute_pos, state.input.as_bytes(), state.outer_constrained_delimiter, b'*') {
                tracing::debug!(absolute_pos, prev_byte = ?state.input.as_bytes().get(absolute_pos.saturating_sub(1)), "Invalid word boundary for constrained bold");
                return Err("invalid word boundary for constrained bold");
            }

            // Check closing boundary: if at end of input, validate outer delimiter
            if !check_constrained_closing_at_end(end, state.input.len(), state.outer_constrained_delimiter) {
                return Err("invalid closing boundary for constrained bold");
            }

            let bm = BlockParsingMetadata {
                substitutions: state.inline_ctx.substitutions,
                ..BlockParsingMetadata::default()
            };
            tracing::debug!(offset = ?state.inline_ctx.offset, ?content, ?role, "Found constrained bold text inline");
            let adjusted_content_start = PositionWithOffset {
                offset: content_start.offset + 1,
                position: content_start.position,
            };
            let saved_delimiter = state.outer_constrained_delimiter;
            state.outer_constrained_delimiter = Some(b'*');
            let result = process_inlines_or_err!(
                process_inlines(state, &bm, adjusted_content_start.offset, end - 1, state.inline_ctx.offset, content),
                "could not process constrained bold text content"
            );
            state.outer_constrained_delimiter = saved_delimiter;
            let (content, _) = result?;

            Ok(InlineNode::BoldText(Bold {
                content,
                role,
                id,
                form: Form::Constrained,
                location: state.create_block_location(start, end, state.inline_ctx.offset),
            }))
        }

        rule bold_text_constrained_match() -> ()
        = boundary_pos:position!()
        inline_attributes()?
        open:position!() "*"
        (
            empty_quote_content() &"*"
            / [^(' ' | '\t' | '\n')] [^'*']* ("*" !constrained_boundary_follow('*') [^'*']*)*
        )
        close:position!() "*" check_quote_markers((open, 1), (close, 1))
        closing_pos:position!()
        constrained_boundary_follow('*')
        {?
            let valid_opening = check_constrained_opening_boundary(boundary_pos, state.input.as_bytes(), state.outer_constrained_delimiter, b'*');
            let valid_closing = check_constrained_closing_at_end(closing_pos, state.input.len(), state.outer_constrained_delimiter);

            if valid_opening && valid_closing { Ok(()) } else { Err("invalid word boundary") }
        }

        rule italic_text_constrained() -> InlineNode<'input>
        = attrs:inline_attributes()?
        start:position!()
        content_start:position()
        "_"
        content:$(
            empty_quote_content() &"_"
            / [^(' ' | '\t' | '\n')] [^'_']* ("_" !constrained_boundary_follow('_') [^'_']*)*
        )
        close:position!() "_" check_quote_markers((start, 1), (close, 1))
        end:position!() &constrained_boundary_follow('_')
        {?
            let role = attrs.as_ref().and_then(|(roles, _id)| {
                if roles.is_empty() {
                    None
                } else {
                    Some(state.intern_join(roles.iter(), " "))
                }
            });
            let id = attrs.as_ref().and_then(|(_roles, id)| *id);

            // Check if we're at start of input OR preceded by word boundary character
            let absolute_pos = start + state.inline_ctx.offset;
            if !check_constrained_opening_boundary(absolute_pos, state.input.as_bytes(), state.outer_constrained_delimiter, b'_') {
                return Err("invalid word boundary for constrained italic");
            }

            // Check closing boundary: if at end of input, validate outer delimiter
            if !check_constrained_closing_at_end(end, state.input.len(), state.outer_constrained_delimiter) {
                return Err("invalid closing boundary for constrained italic");
            }

            let bm = BlockParsingMetadata {
                substitutions: state.inline_ctx.substitutions,
                ..BlockParsingMetadata::default()
            };
            tracing::debug!(offset = ?state.inline_ctx.offset, ?content, ?role, "Found constrained italic text inline");
            let adjusted_content_start = PositionWithOffset {
                offset: content_start.offset + 1,
                position: content_start.position,
            };
            let saved_delimiter = state.outer_constrained_delimiter;
            state.outer_constrained_delimiter = Some(b'_');
            let result = process_inlines_or_err!(
                process_inlines(state, &bm, adjusted_content_start.offset, end - 1, state.inline_ctx.offset, content),
                "could not process constrained italic text content"
            );
            state.outer_constrained_delimiter = saved_delimiter;
            let (content, _) = result?;
            Ok(InlineNode::ItalicText(Italic {
                content,
                role,
                id,
                form: Form::Constrained,
                location: state.create_block_location(start, end, state.inline_ctx.offset),
            }))
        }

        rule italic_text_constrained_match() -> ()
        = boundary_pos:position!()
        inline_attributes()?
        open:position!() "_"
        (
            empty_quote_content() &"_"
            / [^(' ' | '\t' | '\n')] [^'_']* ("_" !constrained_boundary_follow('_') [^'_']*)*
        )
        close:position!() "_" check_quote_markers((open, 1), (close, 1))
        closing_pos:position!()
        constrained_boundary_follow('_')
        {?
            let valid_opening = check_constrained_opening_boundary(boundary_pos, state.input.as_bytes(), state.outer_constrained_delimiter, b'_');
            let valid_closing = check_constrained_closing_at_end(closing_pos, state.input.len(), state.outer_constrained_delimiter);

            if valid_opening && valid_closing { Ok(()) } else { Err("invalid word boundary") }
        }

        rule italic_text_unconstrained() -> InlineNode<'input>
            = attrs:inline_attributes()? start:position!() "__" content_start:position!() content:$(empty_quote_content() &"__" / (!(eol() / ![_] / "__") [_])+) close:position!() "__" check_quote_markers((start, 2), (close, 2)) end:position!()
        {?
            let role = attrs.as_ref().and_then(|(roles, _id)| {
                if roles.is_empty() {
                    None
                } else {
                    Some(state.intern_join(roles.iter(), " "))
                }
            });
            let id = attrs.as_ref().and_then(|(_roles, id)| *id);

            let bm = BlockParsingMetadata {
                substitutions: state.inline_ctx.substitutions,
                ..BlockParsingMetadata::default()
            };
            tracing::debug!(?start, ?content_start, ?end, offset = ?state.inline_ctx.offset, ?content, ?role, "Found unconstrained italic text inline");
            let (content, _) = process_inlines_or_err!(
                process_inlines(state, &bm, content_start, end - 2, state.inline_ctx.offset, content),
                "could not process unconstrained italic text content"
            )?;
            Ok(InlineNode::ItalicText(Italic {
                content,
                role,
                id,
                form: Form::Unconstrained,
                location: state.create_block_location(start, end, state.inline_ctx.offset),
            }))
        }

        /// Match unconstrained italic without consuming - for use in negative lookaheads.
        rule italic_text_unconstrained_match()
        = inline_attributes()? open:position!() "__" (empty_quote_content() &"__" / (!(eol() / ![_] / "__") [_])+) close:position!() "__" check_quote_markers((open, 2), (close, 2))

        rule monospace_text_unconstrained() -> InlineNode<'input>
            = attrs:inline_attributes()? start:position!() "``" content_start:position!() content:$(empty_quote_content() &"``" / (!(eol() / ![_] / "``") [_])+) close:position!() "``" check_quote_markers((start, 2), (close, 2)) end:position!()
        {?
            let role = attrs.as_ref().and_then(|(roles, _id)| {
                if roles.is_empty() {
                    None
                } else {
                    Some(state.intern_join(roles.iter(), " "))
                }
            });
            let id = attrs.as_ref().and_then(|(_roles, id)| *id);

            let bm = BlockParsingMetadata {
                substitutions: state.inline_ctx.substitutions,
                ..BlockParsingMetadata::default()
            };
            tracing::debug!(?start, ?content_start, ?end, offset = ?state.inline_ctx.offset, ?content, ?role, "Found unconstrained monospace text inline");
            let (content, _) = process_inlines_or_err!(
                process_inlines(state, &bm, content_start, end - 2, state.inline_ctx.offset, content),
                "could not process unconstrained monospace text content"
            )?;
            Ok(InlineNode::MonospaceText(Monospace {
                content,
                role,
                id,
                form: Form::Unconstrained,
                location: state.create_block_location(start, end, state.inline_ctx.offset),
            }))
        }

        /// Match unconstrained monospace without consuming - for use in negative lookaheads.
        rule monospace_text_unconstrained_match()
        = inline_attributes()? open:position!() "``" (empty_quote_content() &"``" / (!(eol() / ![_] / "``") [_])+) close:position!() "``" check_quote_markers((open, 2), (close, 2))

        // Reserve backticks beside quotes for curved quotation syntax.
        rule monospace_boundary_follow()
        = !['"' | '\''] constrained_boundary_follow('`')

        rule constrained_monospace_content() -> &'input str
        = content:$(
            empty_quote_content() &"`"
            / [^(' ' | '\t' | '\n')] [^'`']* ("`" !monospace_boundary_follow() [^'`']*)*
        )
        {?
            // Constrained code cannot end with whitespace. A later attribute
            // expansion can leave valid content empty.
            if content.as_bytes().last().is_some_and(u8::is_ascii_whitespace) {
                Err("constrained monospace must not end with whitespace")
            } else {
                Ok(content)
            }
        }

        rule monospace_text_constrained() -> InlineNode<'input>
        = attrs:inline_attributes()?
        start:position!()
        content_start:position()
        "`"
        content:constrained_monospace_content()
        close:position!() "`" check_quote_markers((start, 1), (close, 1))
        end:position!()
        &monospace_boundary_follow()
        {?
            let role = attrs.as_ref().and_then(|(roles, _id)| {
                if roles.is_empty() {
                    None
                } else {
                    Some(state.intern_join(roles.iter(), " "))
                }
            });
            let id = attrs.as_ref().and_then(|(_roles, id)| *id);

            // Check if we're at start of input OR preceded by word boundary character
            let absolute_pos = start + state.inline_ctx.offset;
            if !check_constrained_opening_boundary(absolute_pos, state.input.as_bytes(), state.outer_constrained_delimiter, b'`') {
                return Err("monospace must be at word boundary");
            }

            // Check closing boundary: if at end of input, validate outer delimiter
            if !check_constrained_closing_at_end(end, state.input.len(), state.outer_constrained_delimiter) {
                return Err("invalid closing boundary for constrained monospace");
            }

            let bm = BlockParsingMetadata {
                substitutions: state.inline_ctx.substitutions,
                ..BlockParsingMetadata::default()
            };
            tracing::debug!(?start, ?content_start, ?end, offset = ?state.inline_ctx.offset, ?content, ?role, "Found constrained monospace text inline");
            let adjusted_content_start = PositionWithOffset {
                offset: content_start.offset + 1,
                position: content_start.position,
            };
            let saved_delimiter = state.outer_constrained_delimiter;
            state.outer_constrained_delimiter = Some(b'`');
            let result = process_inlines_or_err!(
                process_inlines(state, &bm, adjusted_content_start.offset, end - 1, state.inline_ctx.offset, content),
                "could not process constrained monospace text content"
            );
            state.outer_constrained_delimiter = saved_delimiter;
            let (content, _) = result?;
            Ok(InlineNode::MonospaceText(Monospace {
                content,
                role,
                id,
                form: Form::Constrained,
                location: state.create_block_location(start, end, state.inline_ctx.offset),
            }))
        }

        rule monospace_text_constrained_match() -> ()
        = boundary_pos:position!()
        inline_attributes()?
        open:position!() "`"
        constrained_monospace_content()
        close:position!() "`" check_quote_markers((open, 1), (close, 1))
        closing_pos:position!()
        monospace_boundary_follow()
        {?
            let valid_opening = check_constrained_opening_boundary(boundary_pos, state.input.as_bytes(), state.outer_constrained_delimiter, b'`');
            let valid_closing = check_constrained_closing_at_end(closing_pos, state.input.len(), state.outer_constrained_delimiter);

            if valid_opening && valid_closing { Ok(()) } else { Err("monospace must be at word boundary") }
        }

        rule highlight_text_unconstrained() -> InlineNode<'input>
            = attrs:inline_attributes()? start:position!() "##" content_start:position!() content:$(empty_quote_content() &"##" / (!(eol() / ![_] / "##") [_])+) close:position!() "##" check_quote_markers((start, 2), (close, 2)) end:position!()
        {?
            let role = attrs.as_ref().and_then(|(roles, _id)| {
                if roles.is_empty() {
                    None
                } else {
                    Some(state.intern_join(roles.iter(), " "))
                }
            });
            let id = attrs.as_ref().and_then(|(_roles, id)| *id);

            let bm = BlockParsingMetadata {
                substitutions: state.inline_ctx.substitutions,
                ..BlockParsingMetadata::default()
            };
            tracing::debug!(?start, ?content_start, ?end, offset = ?state.inline_ctx.offset, ?content, ?role, "Found unconstrained highlight text inline");
            let (content, _) = process_inlines_or_err!(
                process_inlines(state, &bm, content_start, end - 2, state.inline_ctx.offset, content),
                "could not process unconstrained highlight text content"
            )?;
            Ok(InlineNode::HighlightText(Highlight {
                content,
                role,
                id,
                form: Form::Unconstrained,
                location: state.create_block_location(start, end, state.inline_ctx.offset),
            }))
        }

        /// Match unconstrained highlight without consuming - for use in negative lookaheads.
        rule highlight_text_unconstrained_match()
        = inline_attributes()? open:position!() "##" (empty_quote_content() &"##" / (!(eol() / ![_] / "##") [_])+) close:position!() "##" check_quote_markers((open, 2), (close, 2))

        rule highlight_text_constrained() -> InlineNode<'input>
        = attrs:inline_attributes()?
        start:position!()
        content_start:position()
        "#"
        content:$(
            empty_quote_content() &"#"
            / [^(' ' | '\t' | '\n')] [^'#']* ("#" !constrained_boundary_follow('#') [^'#']*)*
        )
        close:position!() "#" check_quote_markers((start, 1), (close, 1))
        end:position!()
        &constrained_boundary_follow('#')
        {?
            let role = attrs.as_ref().and_then(|(roles, _id)| {
                if roles.is_empty() {
                    None
                } else {
                    Some(state.intern_join(roles.iter(), " "))
                }
            });
            let id = attrs.as_ref().and_then(|(_roles, id)| *id);

            // Check if we're at start of input OR preceded by word boundary character
            let absolute_pos = start + state.inline_ctx.offset;
            if !check_constrained_opening_boundary(absolute_pos, state.input.as_bytes(), state.outer_constrained_delimiter, b'#') {
                tracing::debug!(absolute_pos, prev_byte = ?state.input.as_bytes().get(absolute_pos.saturating_sub(1)), "Invalid word boundary for constrained highlight");
                return Err("invalid word boundary for constrained highlight");
            }

            // Check closing boundary: if at end of input, validate outer delimiter
            if !check_constrained_closing_at_end(end, state.input.len(), state.outer_constrained_delimiter) {
                return Err("invalid closing boundary for constrained highlight");
            }

            let bm = BlockParsingMetadata {
                substitutions: state.inline_ctx.substitutions,
                ..BlockParsingMetadata::default()
            };
            tracing::debug!(?start, ?content_start, ?end, offset = ?state.inline_ctx.offset, ?content, ?role, "Found constrained highlight text inline");
            let adjusted_content_start = PositionWithOffset {
                offset: content_start.offset + 1,
                position: content_start.position,
            };
            let saved_delimiter = state.outer_constrained_delimiter;
            state.outer_constrained_delimiter = Some(b'#');
            let result = process_inlines_or_err!(
                process_inlines(state, &bm, adjusted_content_start.offset, end - 1, state.inline_ctx.offset, content),
                "could not process constrained highlight text content"
            );
            state.outer_constrained_delimiter = saved_delimiter;
            let (content, _) = result?;
            Ok(InlineNode::HighlightText(Highlight {
                content,
                role,
                id,
                form: Form::Constrained,
                location: state.create_block_location(start, end, state.inline_ctx.offset),
            }))
        }

        rule highlight_text_constrained_match() -> ()
        = boundary_pos:position!()
        inline_attributes()?
        open:position!() "#"
        (
            empty_quote_content() &"#"
            / [^(' ' | '\t' | '\n')] [^'#']* ("#" !constrained_boundary_follow('#') [^'#']*)*
        )
        close:position!() "#" check_quote_markers((open, 1), (close, 1))
        closing_pos:position!()
        constrained_boundary_follow('#')
        {?
            let valid_opening = check_constrained_opening_boundary(boundary_pos, state.input.as_bytes(), state.outer_constrained_delimiter, b'#');
            let valid_closing = check_constrained_closing_at_end(closing_pos, state.input.len(), state.outer_constrained_delimiter);

            if valid_opening && valid_closing { Ok(()) } else { Err("invalid word boundary") }
        }

        /// Parse superscript text (^text^)
        rule superscript_text() -> InlineNode<'input>
            = attrs:inline_attributes()? start:position!() "^" content_start:position!() content:$(empty_quote_content() &"^" / [^('^' | ' ' | '\t' | '\n')]+) close:position!() "^" check_quote_markers((start, 1), (close, 1)) end:position!()
        {?
            let role = attrs.as_ref().and_then(|(roles, _id)| {
                if roles.is_empty() {
                    None
                } else {
                    Some(state.intern_join(roles.iter(), " "))
                }
            });
            let id = attrs.as_ref().and_then(|(_roles, id)| *id);

            let bm = BlockParsingMetadata {
                substitutions: state.inline_ctx.substitutions,
                ..BlockParsingMetadata::default()
            };
            tracing::debug!(?start, ?content_start, ?end, offset = ?state.inline_ctx.offset, ?content, ?role, "Found superscript text inline");
            let (content, _) = process_inlines_or_err!(
                process_inlines(state, &bm, content_start, end - 1, state.inline_ctx.offset, content),
                "could not process superscript text content"
            )?;
            Ok(InlineNode::SuperscriptText(Superscript {
                content,
                role,
                id,
                form: Form::Unconstrained,
                location: state.create_block_location(start, end, state.inline_ctx.offset),
            }))
        }

        /// Match superscript text without consuming - for use in negative lookaheads.
        rule superscript_text_match()
        = inline_attributes()? open:position!() "^" (empty_quote_content() &"^" / [^('^' | ' ' | '\t' | '\n')]+) close:position!() "^" check_quote_markers((open, 1), (close, 1))

        /// Parse subscript text (~text~)
        rule subscript_text() -> InlineNode<'input>
            = attrs:inline_attributes()? start:position!() "~" content_start:position!() content:$(empty_quote_content() &"~" / [^('~' | ' ' | '\t' | '\n')]+) close:position!() "~" check_quote_markers((start, 1), (close, 1)) end:position!()
        {?
            let role = attrs.as_ref().and_then(|(roles, _id)| {
                if roles.is_empty() {
                    None
                } else {
                    Some(state.intern_join(roles.iter(), " "))
                }
            });
            let id = attrs.as_ref().and_then(|(_roles, id)| *id);

            let bm = BlockParsingMetadata {
                substitutions: state.inline_ctx.substitutions,
                ..BlockParsingMetadata::default()
            };
            tracing::debug!(?start, ?content_start, ?end, offset = ?state.inline_ctx.offset, ?content, ?role, "Found subscript text inline");
            let (content, _) = process_inlines_or_err!(
                process_inlines(state, &bm, content_start, end - 1, state.inline_ctx.offset, content),
                "could not process subscript text content"
            )?;
            Ok(InlineNode::SubscriptText(Subscript {
                content,
                role,
                id,
                form: Form::Unconstrained,
                location: state.create_block_location(start, end, state.inline_ctx.offset),
            }))
        }

        /// Match subscript text without consuming - for use in negative lookaheads.
        rule subscript_text_match()
        = inline_attributes()? open:position!() "~" (empty_quote_content() &"~" / [^('~' | ' ' | '\t' | '\n')]+) close:position!() "~" check_quote_markers((open, 1), (close, 1))

        /// Parse curved quotation text (`"text"`)
        rule curved_quotation_text() -> InlineNode<'input>
            = attrs:inline_attributes()? start:position!() "\"`" content_start:position!() content:$(empty_quote_content() &"`\"" / (!("`\"") [_])+) close:position!() "`\"" check_quote_markers((start, 2), (close, 2)) end:position!()
        {?
            let role = attrs.as_ref().and_then(|(roles, _id)| {
                if roles.is_empty() {
                    None
                } else {
                    Some(state.intern_join(roles.iter(), " "))
                }
            });
            let id = attrs.as_ref().and_then(|(_roles, id)| *id);

            let bm = BlockParsingMetadata {
                substitutions: state.inline_ctx.substitutions,
                ..BlockParsingMetadata::default()
            };
            tracing::debug!(?start, ?content_start, ?end, offset = ?state.inline_ctx.offset, ?content, ?role, "Found curved quotation text inline");
            let (content, _) = process_inlines_or_err!(
                process_inlines(state, &bm, content_start, end - 2, state.inline_ctx.offset, content),
                "could not process curved quotation text content"
            )?;
            Ok(InlineNode::CurvedQuotationText(CurvedQuotation {
                content,
                role,
                id,
                form: Form::Unconstrained,
                location: state.create_block_location(start, end, state.inline_ctx.offset),
            }))
        }

        /// Match curved quotation text without consuming - for use in negative lookaheads.
        rule curved_quotation_text_match()
        = inline_attributes()? open:position!() "\"`" (empty_quote_content() &"`\"" / (!("`\"") [_])+) close:position!() "`\"" check_quote_markers((open, 2), (close, 2))

        /// Parse curved apostrophe text (`'text'`)
        rule curved_apostrophe_text() -> InlineNode<'input>
            = attrs:inline_attributes()? start:position!() "'`" content_start:position!() content:$(empty_quote_content() &"`'" / (!("`'") [_])+) close:position!() "`'" check_quote_markers((start, 2), (close, 2)) end:position!()
        {?
            let role = attrs.as_ref().and_then(|(roles, _id)| {
                if roles.is_empty() {
                    None
                } else {
                    Some(state.intern_join(roles.iter(), " "))
                }
            });
            let id = attrs.as_ref().and_then(|(_roles, id)| *id);

            let bm = BlockParsingMetadata {
                substitutions: state.inline_ctx.substitutions,
                ..BlockParsingMetadata::default()
            };
            tracing::debug!(?start, ?content_start, ?end, offset = ?state.inline_ctx.offset, ?content, ?role, "Found curved apostrophe text inline");
            let (content, _) = process_inlines_or_err!(
                process_inlines(state, &bm, content_start, end - 2, state.inline_ctx.offset, content),
                "could not process curved apostrophe text content"
            )?;
            Ok(InlineNode::CurvedApostropheText(CurvedApostrophe {
                content,
                role,
                id,
                form: Form::Unconstrained,
                location: state.create_block_location(start, end, state.inline_ctx.offset),
            }))
        }

        /// Match curved apostrophe text without consuming - for use in negative lookaheads.
        rule curved_apostrophe_text_match()
        = inline_attributes()? open:position!() "'`" (empty_quote_content() &"`'" / (!("`'") [_])+) close:position!() "`'" check_quote_markers((open, 2), (close, 2))

        /// Match standalone curved apostrophe without consuming - for use in negative lookaheads.
        rule standalone_curved_apostrophe_match()
        = start:position!() "`'" check_quote_markers((start, 2), (start, 2))

        /// Parse standalone curved apostrophe (`')
        rule standalone_curved_apostrophe() -> InlineNode<'input>
            = start:position!() "`'" check_quote_markers((start, 2), (start, 2))
        {?
            tracing::debug!(start = span_start, end = span_end, offset = ?state.inline_ctx.offset, "Found standalone curved apostrophe inline");
            Ok(InlineNode::StandaloneCurvedApostrophe(StandaloneCurvedApostrophe {
                location: state.create_block_location(span_start, span_end, state.inline_ctx.offset),
            }))
        }

        rule warn_anchor_id_with_whitespace() -> ()
        = &(
            id:$([^'\'' | ',' | ']' | '.' | '#']+)
            {?
                if id.chars().any(char::is_whitespace) {
                    let location = state.create_block_location(span_start, span_end, state.inline_ctx.offset);
                    state.add_generic_warning_at(
                        format!("anchor id '{id}' contains whitespace which is not allowed, treating as literal text"),
                        location,
                    );
                }
                // Always fail so the lookahead doesn't match - we just want the side
                // effect
                Err::<(), &'static str>("")
            }
        )

        // Consume only one escape and only for complete syntax. Invalid or disabled
        // anchor macros must retain their backslash instead of using the generic escape.
        rule escaped_anchor_macro() -> InlineNode<'input>
        = "\\" content:$(anchor_macro_pattern()) {
            InlineNode::PlainText(Plain {
                content,
                location: state.create_block_location(span_start, span_end, state.inline_ctx.offset),
                escaped: false,
            })
        }

        rule anchor_macro() -> InlineNode<'input>
        = parts:anchor_macro_pattern() {
            let (id, text) = parts;
            let reftext = (!text.is_empty()).then(|| {
                if text.contains("\\]") {
                    state.intern_str(&text.replace("\\]", "]"))
                } else {
                    text
                }
            });
            InlineNode::InlineAnchor(
                Anchor::new(id, state.create_block_location(span_start, span_end, state.inline_ctx.offset))
                    .with_xreflabel(reftext),
            )
        }

        // Share recognition with lookaheads so plain text and xref labels stop at
        // exactly the syntax that creates an anchor. Labels are text, not attributes.
        rule anchor_macro_pattern() -> (&'input str, &'input str)
        = start:position!() "anchor:" id:anchor_macro_id() "["
          open_end:position!() check_anchor_macro_token(start, open_end - start)
          text:$(("\\]" / !anchor_macro_close() !eol() [_])*) anchor_macro_close()
        { (id, text) }

        // Keep combining marks and other non-ASCII ID characters without category
        // tables. This is broader than Asciidoctor's word class; ASCII syntax,
        // whitespace, controls, and the first character remain constrained.
        rule anchor_macro_id() -> &'input str
        = id:$((['_' | ':'] / c:[_] {? c.is_alphabetic().then_some(()).ok_or("anchor ID start") })
          (['a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-' | ':' | '.']
          / c:['\u{80}'..='\u{10FFFF}'] {?
              (!c.is_whitespace() && !c.is_control())
                  .then_some(()).ok_or("anchor ID character")
          })*) { id }

        rule check_anchor_macro_token(start: usize, len: usize)
        = {? macro_token_allowed(state, start, len).then_some(()).ok_or("anchor syntax introduced after macros") }

        rule anchor_macro_close()
        = start:position!() "]" check_anchor_macro_token(start, 1)

        rule inline_anchor() -> InlineNode<'input>
        = anchor_macro()
        / start:position!() double_open_square_bracket()
        // Whitespace is excluded - IDs must not contain spaces
        warn_anchor_id_with_whitespace()?
        id:$([^'\'' | ',' | ']' | '[' | ' ' | '\t' | '\n' | '\r']+)
        reftext:(
            comma() reftext:anchor_reftext(start) {
                Some(reftext)
            } /
            {
                None
            }
        )
        double_close_square_bracket()
        {
            let substituted_id = state.intern_cow(substitute(id, HEADER, &state.document_attributes));
            let substituted_reftext = reftext.map(|rt| state.intern_cow(substitute(rt, HEADER, &state.document_attributes)));
            InlineNode::InlineAnchor(Anchor {
                id: substituted_id,
                xreflabel: substituted_reftext,
                location: state.create_block_location(span_start, span_end, state.inline_ctx.offset),
                bibliography_label: None,
                bibliography: false,
            })
        }

        rule inline_anchor_match() -> ()
        = anchor_macro_pattern() {}
        / start:position!() double_open_square_bracket() [^'\'' | ',' | ']' | '[' | ' ' | '\t' | '\n' | '\r']+ (comma() anchor_reftext(start))? double_close_square_bracket()

        rule bibliography_anchor_start(start: usize)
        = {?
            start.checked_sub(1)
                .and_then(|offset| state.input.get(offset..start))
                .filter(|previous| *previous == "[")
                .map(|_| ())
                .ok_or("not a bibliography anchor")
        }

        rule anchor_reftext(start: usize) -> &'input str
        // Reserve the last three closing brackets for the bibliography delimiter.
        = bibliography_anchor_start(start)
          label:$((!("]]]" !"]") [^'\n' | '\r'])+) { label }
        / !bibliography_anchor_start(start) label:$([^']']+) { label }

        rule invalid_bibliography_anchor() -> InlineNode<'input>
        = syntax:$("[[[" [^']' | '\n']* "]]]") {?
            let body = &syntax[3..syntax.len() - 3];
            let id = body.split_once(',').map_or(body, |(id, _)| id);
            if is_valid_bibliography_id(id) {
                Err("valid bibliography anchor")
            } else {
                Ok(InlineNode::PlainText(Plain {
                    content: syntax,
                    location: state.create_block_location(span_start, span_end, state.inline_ctx.offset),
                    escaped: false,
                }))
            }
        }

        /// Rust-native guard: returns Ok for characters that cannot start any inline
        /// construct. Uses the static `PLAIN_TEXT_SAFE` lookup table plus context
        /// checks for email (`@` within 64 bytes) and hard wrap (space followed by `+`).
        rule plain_text_quick_safe() -> ()
        = pos:position!() {?
            let b = state.input.as_bytes().get(pos).copied().unwrap_or(0);
            if !is_plain_text_safe(b) {
                return Err("needs full check");
            }
            if b == b' ' {
                if state.input.as_bytes().get(pos + 1).copied() == Some(b'+') {
                    return Err("potential hard wrap");
                }
                return Ok(());
            }
            if b.is_ascii_alphanumeric() && has_at_sign_ahead(state, pos) {
                return Err("potential email");
            }
            Ok(())
        }

        rule plain_text() -> InlineNode<'input>
        = start_pos:position!()
        content:$((
            // Escape sequences for superscript/subscript markers - only when NOT followed by
            // a complete pattern (those are handled by escaped_superscript_subscript rule)
            "\\" "^" !([^'^' | ' ' | '\t' | '\n']+ "^")
            / "\\" "~" !([^'~' | ' ' | '\t' | '\n']+ "~")
            // Fast path: characters that can never start any inline construct.
            / ['\t' | ',' | ';' | '.' | '?' | '!' | ':' | '/' | '>' | ')' | ']' | '}' | '|' | '@' | '&' | '=' | '{' | '-' | '\u{00A0}'..='\u{FFFC}' | '\u{FFFE}'..='\u{10FFFF}']+
            // Quick path: Rust-native lookup table check for safe characters (uppercase,
            // non-macro lowercase, digits, space). Skips the full 7-branch PEG lookahead.
            / plain_text_quick_safe() [_]
            // Slow path: potential construct trigger character. Use character-class guards to
            // skip groups of rules whose starting character doesn't match.
            / (
                !(
                    // Attribute expansion can create an internal empty line. Let
                    // plain text absorb it when post replacements cannot handle it.
                    check_post_replacements() paragraph_break()
                    / profiled_attribute_match()
                    / check_hardbreaks() line_break_eol()
                    / ![_]
                    / &['\\'] (check_macros() "\\" anchor_macro_pattern() / index_label_bracket_match() / escaped_label_character_match() / escaped_syntax_match())
                    / check_post_replacements() &[' '] (hard_wrap_match() / inline_line_break_match())
                    // Macro guard: [ ( < for delimiters, then first letters of each macro:
                    // a=anchor/asciimath, b=btn, f=footnote/ftp, h=http(s), i=image/icon/indexterm/irc,
                    // k=kbd, l=link/latexmath, m=menu/mailto, p=pass, s=stem, x=xref
                    / (check_macros() &['[' | '(' | '<' | 'a' | 'b' | 'f' | 'h' | 'i' | 'k' | 'l' | 'm' | 'p' | 's' | 'x'] (inline_anchor_match() / (check_index_terms() index_term_match()) / cross_reference_shorthand_match() / cross_reference_macro_match() / footnote_match() / inline_image_match() / inline_icon_match() / inline_stem_match() / inline_keyboard_match() / inline_button_match() / inline_menu_match() / mailto_macro_match() / url_macro_match() / inline_pass_match() / link_macro_match()))
                    / (check_macros() check_autolinks() inline_autolink_match())
                    / (check_quotes() &['*' | '_' | '`' | '#' | '^' | '~' | '"' | '\'' | '['] (bold_text_unconstrained_match() / bold_text_constrained_match() / italic_text_unconstrained_match() / italic_text_constrained_match() / monospace_text_unconstrained_match() / monospace_text_constrained_match() / highlight_text_unconstrained_match() / highlight_text_constrained_match() / superscript_text_match() / subscript_text_match() / curved_quotation_text_match() / curved_apostrophe_text_match() / standalone_curved_apostrophe_match()))
                ) [_]
            )
        )+)
        end:position!()
        {
            tracing::trace!(?content, "Found plain text inline");
            // Note: Backslash escape stripping (e.g., \^ -> ^) is handled by the converter,
            // not here, so that verbatim contexts (like monospace) preserve backslashes.
            InlineNode::PlainText(Plain {
                content,
                location: state.create_block_location(start_pos, end, state.inline_ctx.offset),
                escaped: false,
            })
        }

        /// Parse optional attribute list for inline elements
        /// Returns (roles, id) extracted from attributes like [.role1.role2] or [#id.role]
        /// This is a simplified version of block attributes, used for inline formatting
        /// In inline context, % is treated as a literal character, not an option separator
        /// Stops parsing shorthands at invalid characters (comma, space, etc.)
        rule inline_attributes() -> (Vec<&'input str>, Option<&'input str>)
        = open_square_bracket() shorthands:inline_shorthand()+ [^']']* close_square_bracket()
        {
            let mut roles: Vec<&'input str> = Vec::new();
            let mut id: Option<&'input str> = None;

            for s in shorthands {
                match s {
                    Shorthand::Role(r) => roles.push(state.intern_cow(r)),
                    Shorthand::Id(i) => {
                        // If multiple IDs are specified, last one wins
                        id = Some(state.intern_cow(i));
                    }
                }
            }

            (roles, id)
        }

        /// Parse inline attribute shorthand: .role, #id, %role, or bare role
        /// In inline context, % is not an option separator - it's a literal character
        /// Leading % is treated as part of the role name
        /// Bare roles (no prefix) are supported for asciidoctor compatibility
        rule inline_shorthand() -> Shorthand<'input>
        = "#" id:inline_id() { Shorthand::Id(id.into()) }
        / "." role:inline_role() { Shorthand::Role(role.into()) }
        / "%" role:inline_role() { Shorthand::Role(Cow::Owned(format!("%{role}"))) }
        / role:bare_inline_role() { Shorthand::Role(role.into()) }

        /// Bare role pattern for inline contexts (no prefix) - matches CSS-like identifiers
        /// Starts with letter, followed by letters, numbers, or hyphens
        /// Used for syntax like [line-through]#text# (asciidoctor compatibility)
        rule bare_inline_role() -> &'input str = $(['a'..='z' | 'A'..='Z'] ['a'..='z' | 'A'..='Z' | '0'..='9' | '-']*)

        /// Role pattern for inline contexts - allows % as literal character
        rule inline_role() -> &'input str = $([^(',' | ']' | '#' | '.')]+)

        /// ID pattern for inline contexts - allows % as literal character
        rule inline_id() -> &'input str = $(id_start_char() inline_id_subsequent_char()*)
        rule inline_id_subsequent_char() = ['A'..='Z' | 'a'..='z' | '0'..='9' | '_' | '-' | '%']

        /// Macro attribute parsing - simpler than block attributes.
        ///
        /// Does NOT support shorthand syntax (.role, #id, %option).
        /// Shorthands are only valid in block-level attributes, not inside macro brackets.
        ///
        /// Asciidoctor behavior:
        /// - `image::photo.jpg[.role]` -> alt=".role" (literal text, NOT a role)
        /// - `image::photo.jpg[Diablo 4 picture of Lilith.]` -> alt="Diablo 4 picture of Lilith."
        pub(crate) rule macro_attributes() -> (bool, BlockMetadata<'input>, Option<(usize, usize)>)
            = open_square_bracket()
              attrs:(att:macro_attribute() comma()? { att })*
              close_square_bracket()
        {
            let mut metadata = BlockMetadata::default();
            let title_position = process_attribute_list(
                attrs,
                &mut metadata,
                state,
                span_start,
                span_end,
                MacroAttributeContext::General,
            );
            // macro_attributes never sets discrete flag (that's block-level only)
            (false, metadata, title_position)
        }

        rule image_macro_attributes() -> (bool, BlockMetadata<'input>, Option<(usize, usize)>)
            = open_square_bracket()
              attrs:(att:image_macro_attribute() comma()? { att })*
              close_square_bracket()
        {
            let mut metadata = BlockMetadata::default();
            let title_position = process_attribute_list(
                attrs,
                &mut metadata,
                state,
                span_start,
                span_end,
                MacroAttributeContext::Image,
            );
            (false, metadata, title_position)
        }

        /// Positional value in macro attributes - allows . # % as literal characters
        /// This is the key difference from block attributes.
        rule macro_positional_value() -> Option<Cow<'input, str>>
            = quoted:inner_attribute_value() {
                let trimmed = strip_quotes(quoted);
                if trimmed.is_empty() { None } else { Some(trimmed.into()) }
            }
            / s:$([^('"' | ',' | ']' | '=')]+) {
                let trimmed = s.trim();
                if trimmed.is_empty() { None } else { Some(trimmed.into()) }
            }

        /// Named attribute or additional positional in macro context
        rule macro_attribute() -> Option<(Cow<'input, str>, AttributeValue<'input>, Option<(usize, usize)>)>
            = whitespace()* att:named_attribute() { att }
            / val:macro_positional_value() {
                val.map(|v| (v, AttributeValue::None, None))
            }

        rule image_macro_attribute() -> Option<(Cow<'input, str>, AttributeValue<'input>, Option<(usize, usize)>)>
            = whitespace()* "link" "=" start:position!() value:image_link_attribute_value() end:position!() {
                let substituted = substitute(value, &[Substitution::Attributes], &state.document_attributes);
                Some((Cow::Borrowed("link"), AttributeValue::String(substituted), Some((start, end))))
            }
            / macro_attribute()

        rule image_link_attribute_value() -> &'input str
            = value:named_attribute_value() { value }
            / &("," / "]") { "" }

        rule open_square_bracket() = "["
        rule close_square_bracket() = "]"
        rule double_open_square_bracket() = "[["
        rule double_close_square_bracket() = "]]"
        rule comma() = ","
        rule period() = "."
        rule empty_style() = ""
        rule role() -> &'input str = $([^(',' | ']' | '#' | '.' | '%')]+)

        rule attribute_name() -> &'input str = $((['A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_'])+)

        rule attribute() -> Option<(Cow<'input, str>, AttributeValue<'input>, Option<(usize, usize)>)>
            = whitespace()* att:named_attribute() { att }
              / whitespace()* start:position!() att:positional_attribute_value() end:position!() {
                  let substituted = substitute(att, &[Substitution::Attributes], &state.document_attributes);
                  Some((substituted, AttributeValue::None, Some((start, end))))
              }

        // Add a simple ID rule
        rule id() -> &'input str
            = id:$((['A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_'])+) { id }

        rule id_start_char() = ['A'..='Z' | 'a'..='z' | '_']

        rule block_style_id() -> &'input str = $(id_start_char() block_style_id_subsequent_char()*)

        rule block_style_id_subsequent_char() =
            ['A'..='Z' | 'a'..='z' | '0'..='9' | '_' | '-']

        // TODO(nlopes): this should instead return an enum
        rule named_attribute() -> Option<(Cow<'input, str>, AttributeValue<'input>, Option<(usize, usize)>)>
            = "id" "=" start:position!() id:id() end:position!()
                { Some((Cow::Borrowed(RESERVED_NAMED_ATTRIBUTE_ID), id.into(), Some((start, end)))) }
              / ("role" / "roles") "=" value:named_attribute_value()
                { Some((Cow::Borrowed(RESERVED_NAMED_ATTRIBUTE_ROLE), value.into(), None)) }
              / ("options" / "opts") "=" value:named_attribute_value()
                { Some((Cow::Borrowed(RESERVED_NAMED_ATTRIBUTE_OPTIONS), value.into(), None)) }
              / name:attribute_name() "=" start:position!() value:named_attribute_value() end:position!()
                {
                    let substituted_value = substitute(value, &[Substitution::Attributes], &state.document_attributes);
                    Some((Cow::Borrowed(name), AttributeValue::String(substituted_value), Some((start, end))))
                }

        rule named_attribute_value() -> &'input str
        = &("\"" / "'") inner:inner_attribute_value()
        {
            // Strip surrounding quotes from quoted values
            let trimmed = strip_quotes(inner);
            tracing::debug!(%inner, %trimmed, "Found named attribute value (inner)");
            trimmed
        }
        / s:$([^(',' | '"' | '\'' | ']')]+)
        {
            tracing::debug!(%s, "Found named attribute value");
            s
        }

        rule positional_attribute_value() -> &'input str
        = quoted:inner_attribute_value() {
            let trimmed = strip_quotes(quoted);
            tracing::debug!(%quoted, %trimmed, "Found quoted positional attribute value");
            trimmed
        }
        / s:$([^('"' | ',' | ']' | '#' | '.' | '%')] [^(',' | ']' | '#' | '.' | '%' | '=')]*)
        {
            let trimmed = s.trim();
            tracing::debug!(%s, %trimmed, "Found unquoted positional attribute value");
            trimmed
        }

        rule inner_attribute_value() -> &'input str
        = s:$("\"" [^'"']* "\"") { s }
        / s:$("'" [^'\'']* "'") { s }

        /// URL rule matches both web URLs (proto://) and mailto: URLs
        pub rule url() -> Cow<'input, str> =
        proto:$("https" / "http" / "ftp" / "irc") "://" path:url_path() { Cow::Owned(format!("{proto}://{path}")) }
        / "mailto:" email:email_address() { Cow::Owned(format!("mailto:{email}")) }

        rule media_url() -> Cow<'input, str> =
        proto:$("https" / "http" / "ftp" / "irc") "://" path:media_url_path() { Cow::Owned(format!("{proto}://{path}")) }
        / "mailto:" email:email_address() { Cow::Owned(format!("mailto:{email}")) }

        /// Email address pattern (RFC 822 simplified)
        ///
        /// Local part: alphanumeric plus . _ % + -
        /// Domain: alphanumeric plus . - (must contain TLD, must end with alphanumeric)
        ///
        /// - Domain must contain at least one dot (e.g., `foo@bar` is not valid,
        ///   `foo@bar.com` is)
        ///
        /// - Domain must end with alphanumeric (prevents capturing trailing punctuation
        ///   like `user@example.com.` - the dot stays outside the email for sentence
        ///   endings)

        /// Fast Rust-native guard: check if '@' appears within the next 64 bytes
        /// (RFC 5321 max local-part length). Prevents the expensive `email_address()`
        /// greedy scan at positions where no email can possibly start.
        rule email_at_sign_ahead() -> ()
        = pos:position!() {?
            if has_at_sign_ahead(state, pos) {
                Ok(())
            } else {
                Err("no @ sign ahead")
            }
        }

        rule email_address() -> Cow<'input, str>
        = local:$(
            // Quoted local part: "Jane Doe"@example.com
            // Quotes allow spaces and special chars in the local part (RFC 5321).
            "\"" [^'"']+ "\""
            // Unquoted local part (no spaces allowed)
            / ['a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '_' | '%' | '+' | '-']+
        )
        "@"
        // Format: alphanumeric+ (separator alphanumeric+)*
        // This ensures domain ends with alphanumeric (not . or -) and has proper structure.
        // e.g., `example.com.` -> matches `example.com`, trailing dot stays outside
        domain:$(
            ['a'..='z' | 'A'..='Z' | '0'..='9']+
            (['.' | '-'] ['a'..='z' | 'A'..='Z' | '0'..='9']+)*
        )
        {?
            // Require TLD - domain must contain at least one dot. This prevents `foo@bar`
            // from becoming a mailto link.
            if !domain.contains('.') {
                return Err("email domain must have TLD (contain a dot)");
            }

            Ok(Cow::Owned(format!("{local}@{domain}")))
        }

        /// URL target content following `://`.
        /// Supports query parameters, fragments, and percent escapes while excluding
        /// brackets that delimit the macro attributes.
        rule url_path() -> Cow<'input, str> = path:$(url_path_char()+)
        {?
            // Inline text was already processed; attribute values must not introduce passthroughs.
            let inline_state = InlinePreprocessorParserState::new(
                path,
                state.line_map.clone(),
                state.input,
                state.arena,
                matches!(state.scope, ParserScope::Document),
                true,
            );
            let processed = inline_preprocessing::run(path, &state.document_attributes, &inline_state)
            .map_err(|e| {
                tracing::error!(?e, "could not preprocess url path");
                "could not preprocess url path"
            })?;
            for warning in inline_state.drain_warnings() {
                state.add_inline_preprocessor_warning(warning);
            }
            Ok(Cow::Owned(restore_url_path(processed)))
        }

        /// URL target content for an inline media macro.
        /// Spaces must be internal to the target.
        rule media_url_path() -> Cow<'input, str> = path:$(url_path_char() (url_path_char() / internal_url_path_spaces())*)
        {?
            let inline_state = InlinePreprocessorParserState::new(
                path,
                state.line_map.clone(),
                state.input,
                state.arena,
                matches!(state.scope, ParserScope::Document),
                true,
            );
            let processed = inline_preprocessing::run(path, &state.document_attributes, &inline_state)
            .map_err(|e| {
                tracing::error!(?e, "could not preprocess media URL path");
                "could not preprocess media URL path"
            })?;
            for warning in inline_state.drain_warnings() {
                state.add_inline_preprocessor_warning(warning);
            }
            Ok(Cow::Owned(restore_url_path(processed)))
        }

        rule url_path_char() = passthrough_placeholder() / ['A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '.' | '_' | '~' | ':' | '/' | '?' | '#' | '@' | '!' | '$' | '&' | '\'' | '(' | ')' | '*' | '+' | ',' | ';' | '=' | '%' | '\\' ]
        rule internal_url_path_spaces() = [' ']+ &url_path_char()

        /// URL for bare autolinks — avoids capturing trailing sentence punctuation
        /// (., ;, !, etc.) by only consuming punctuation when more URL chars follow.
        rule bare_url() -> Cow<'input, str> =
        proto:$("https" / "http" / "ftp" / "irc") "://" path:bare_url_path()
        { Cow::Owned(format!("{proto}://{path}")) }

        /// URL path for bare autolinks. Like url_path() but:
        /// - Trailing punctuation (. , ; ! ? : ' *) only consumed when followed by more URL chars.
        /// - `)` only consumed as part of a balanced `(...)` group, preventing capture of
        ///   sentence-level parens like `(see http://example.com)`.
        rule bare_url_path() -> Cow<'input, str> = path:$(
            bare_url_safe_char()
            ( bare_url_safe_char()
            / bare_url_paren_group()
            / "("
            / bare_url_trailing_char() &bare_url_char()
            )*
        )
        {?
            let inline_state = InlinePreprocessorParserState::new(
                path,
                state.line_map.clone(),
                state.input,
                state.arena,
                matches!(state.scope, ParserScope::Document),
                true,
            );
            let processed = inline_preprocessing::run(path, &state.document_attributes, &inline_state)
                .map_err(|e| {
                    tracing::error!(?e, "could not preprocess bare url path");
                    "could not preprocess bare url path"
                })?;
            for warning in inline_state.drain_warnings() {
                state.add_inline_preprocessor_warning(warning);
            }
            Ok(Cow::Owned(restore_url_path(processed)))
        }

        /// Balanced parenthesized group in a URL path.
        /// Handles nested parens: `http://example.com/wiki/Foo_(bar_(baz))`
        /// Only `)` consumed via this rule — unbalanced `)` is never captured.
        rule bare_url_paren_group()
        = "(" (bare_url_safe_char() / bare_url_trailing_char() / bare_url_paren_group() / "(")* ")"

        /// URL chars that are safe to end a bare URL — won't be confused with sentence punctuation.
        /// Excludes `(` and `)` which are handled separately via `bare_url_paren_group`.
        rule bare_url_safe_char() = passthrough_placeholder() / ['A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '~'
            | '/' | '#' | '@' | '$' | '&'
            | '+' | '=' | '%' | '\\']

        /// URL chars that are valid mid-URL but should not end a bare URL.
        /// Excludes `)` which is only consumed via balanced `bare_url_paren_group`.
        rule bare_url_trailing_char() = ['.' | ',' | ';' | '!' | '?' | ':' | '\'' | '*']

        /// Any valid URL path char (for lookahead in trailing char rule).
        /// Includes `(` because it can start a paren group.
        /// Excludes `)` so that trailing chars before `)` aren't greedily consumed
        /// (e.g., `http://example.com.)` keeps both `.` and `)` outside).
        rule bare_url_char() = bare_url_safe_char() / bare_url_trailing_char() / "("

        // Xref targets are IDs or source paths, not media URLs. Keep their
        // punctuation intact until source syntax selects a local or external link.
        rule xref_target() -> &'input str
            = target:$((passthrough_placeholder() / path_char() / [':' | '#']) (!['[' | ' ' | '\t' | '\r' | '\n'] [_])*)
        { target }

        /// Fragment identifier for a `link:` macro.
        rule path_fragment() -> Cow<'input, str>
            = "#" fragment:$(['a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-']+)
        {
            Cow::Owned(format!("#{fragment}"))
        }

        /// Filesystem path accepted by inline macros.
        ///
        /// ASCII input uses a conservative filename set. Non-ASCII Unicode characters
        /// are accepted unchanged, and `{`/`}` permit `AsciiDoc` attribute substitution.
        pub rule path() -> Cow<'input, str> = path:$(path_char()+)
        {?
            let inline_state = InlinePreprocessorParserState::new_all_enabled(
                path,
                state.line_map.clone(),
                state.input,
                state.arena,
            );
            let processed = inline_preprocessing::run(path, &state.document_attributes, &inline_state)
            .map_err(|e| {
                tracing::error!(?e, "could not preprocess path");
                "could not preprocess path"
            })?;
            for warning in inline_state.drain_warnings() {
                state.add_inline_preprocessor_warning(warning);
            }
            Ok(processed.text)
        }

        /// Filesystem path for an inline media target.
        /// Existing percent escapes and internal spaces are preserved.
        rule media_path() -> Cow<'input, str> = path:$(media_path_char() (media_path_char() / internal_media_path_spaces())*)
        {?
            let inline_state = InlinePreprocessorParserState::new_all_enabled(
                path,
                state.line_map.clone(),
                state.input,
                state.arena,
            );
            let processed = inline_preprocessing::run(path, &state.document_attributes, &inline_state)
            .map_err(|e| {
                tracing::error!(?e, "could not preprocess media path");
                "could not preprocess media path"
            })?;
            for warning in inline_state.drain_warnings() {
                state.add_inline_preprocessor_warning(warning);
            }
            Ok(processed.text)
        }

        rule path_char() = ['A'..='Z' | 'a'..='z' | '0'..='9' | '{' | '}' | '_' | '-' | '.' | '/' | '\\' | '\u{80}'..='\u{10FFFF}' ]
        rule media_path_char() = path_char() / "%"
        rule internal_media_path_spaces() = [' ']+ &media_path_char()

        pub rule source() -> Source<'input>
            = source:
        (
            p:passthrough_placeholder() {
                Source::Name(p)
            }
            / u:url() {?
                let interned = state.intern_cow(u);
                Source::from_str_borrowed(interned).map_err(|_| "failed to parse URL")
            }
            / p:path() {?
                let interned = state.intern_cow(p);
                Source::from_str_borrowed(interned).map_err(|_| "failed to parse path")
            }
        )
        { source }

        rule media_source() -> Source<'input>
            = source:
        (
            p:passthrough_placeholder() {
                Source::Name(p)
            }
            / u:media_url() {?
                let interned = state.intern_cow(u);
                Source::from_str_borrowed(interned).map_err(|_| "failed to parse media URL")
            }
            / p:media_path() {?
                let interned = state.intern_cow(p);
                Source::from_str_borrowed(interned).map_err(|_| "failed to parse media path")
            }
        )
        { source }

        rule passthrough_placeholder() -> &'input str
            = placeholder:$(
                "\u{FFFD}\u{FFFD}\u{FFFD}"
                digits()
                "\u{FFFD}\u{FFFD}\u{FFFD}"
            ) {
                placeholder
            }

        rule digits() = ['0'..='9']+

        // Verse stanzas share one inline body; an empty line is not a block boundary.
        rule paragraph_break()
        = eol()*<2,> {?
            if state.inline_ctx.rules.contains(InlineRules::PRESERVE_BLANK_LINES) {
                Err("verse preserves empty lines")
            } else {
                Ok(())
            }
        }

        rule whitespace() = quiet!{ " " / "\t" }
        rule eol() = quiet!{ "\n" }

        rule position() -> PositionWithOffset = offset:position!() {
            PositionWithOffset {
                offset,
                position: state.line_map.offset_to_position(offset, state.input)
            }
        }
    }
}
