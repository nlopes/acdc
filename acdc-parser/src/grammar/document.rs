// The `peg` macro adds 5 hidden parameters to every rule function, so even
// rules with just 3 explicit params exceed clippy's 7-argument threshold.
#![allow(clippy::too_many_arguments)]

use std::{
    borrow::Cow,
    collections::{HashMap, HashSet, VecDeque},
    rc::Rc,
};

use crate::{
    Admonition, AdmonitionVariant, Anchor, AttributeValue, Attribution, Audio, Author, Block,
    BlockMetadata, CalloutList, CalloutListItem, CalloutRef, CiteTitle, Comment, CommentKind,
    DelimitedBlock, DelimitedBlockType, DescriptionList, DescriptionListItem, DiscreteHeader,
    Document, DocumentAttribute, DocumentAttributes, ElementAttributes, Error, Header, Image,
    InlineMacro, InlineNode, ListItem, ListItemCheckedStatus, Location, OrderedList, PageBreak,
    Paragraph, Plain, Raw, Reference, Section, Source, SourceLocation, StemContent, StemNotation,
    Subtitle, Table, TableOfContents, TablePresentation, TableRow, ThematicBreak, Title, TocEntry,
    UnorderedList, Verbatim, Video,
    blocks::table::MAX_TABLE_COLUMNS,
    grammar::{
        ParserState,
        attributes::AttributeEntry,
        author::derive_author_attrs,
        doctype::{is_book_doctype, is_manpage_doctype},
        inline_preprocessing,
        inline_preprocessor::InlinePreprocessorParserState,
        inline_processing::{adjust_and_log_parse_error, process_inlines},
        location_walk::walk_document_inline_nodes_mut,
        manpage::{NameSectionAttributes, derive_manpage_header_attrs, derive_name_section_attrs},
        marked_text::MarkedText,
        revision::{IgnoredRevisionFields, RevisionInfo, process_revision_info},
        table::parse_table_cell,
    },
    model::{
        Caption, CaptionKind, LeveloffsetRange, ListLevel, Locateable, PositionalAttribute,
        SectionKind, SectionLevel, Substitution, caption, section, strip_quotes, substitute,
        substitution::{HEADER, SubstitutionPlan},
    },
};

#[cfg(feature = "pre-spec-subs")]
use crate::model::substitution::parse_subs_attribute;

use super::helpers::{
    AttributeOrAnchorLine, BlockMetadataLine, BlockParsingMetadata, MacroAttributeContext,
    PositionWithOffset, RESERVED_NAMED_ATTRIBUTE_ID, RESERVED_NAMED_ATTRIBUTE_OPTIONS,
    RESERVED_NAMED_ATTRIBUTE_ROLE, RESERVED_NAMED_ATTRIBUTE_SUBS, is_valid_bibliography_id,
    parse_comma_separated_values, strip_url_backslash_escapes, title_looks_like_description_list,
};
use super::setext;

struct ManpageNameSection<'input> {
    title: &'input str,
    attributes: NameSectionAttributes,
    metadata_attributes: Vec<AttributeEntry<'input>>,
}

fn prepare_manpage_name_attributes<'input>(
    state: &mut ParserState<'input>,
    section: Option<ManpageNameSection<'input>>,
) {
    if !is_manpage_doctype(&state.document_attributes) {
        return;
    }

    if let (Some(name), Some(purpose)) = (
        state.document_attributes.get_string("manname"),
        state.document_attributes.get_string("manpurpose"),
    ) {
        let name = state.intern_str(&name);
        let purpose = state.intern_str(&purpose);
        set_manpage_name_attributes(state, name, Some(purpose), Some("Name"));
        return;
    }

    if let Some(section) = section {
        let mut attributes = DocumentAttributes::clone(&state.document_attributes);
        for AttributeEntry { key, value, .. } in section.metadata_attributes {
            if !state.options.is_document_attribute_locked(key, false) {
                let value = state.resolve_document_attribute_value(value, &attributes);
                attributes.set(key.into(), value);
            }
        }

        let name = substitute(&section.attributes.name, HEADER, &attributes);
        let name = name.split(',').next().unwrap_or_default().trim();
        let name = state.intern_str(name);
        let purpose = substitute(&section.attributes.purpose, HEADER, &attributes);
        let purpose = state.intern_str(&purpose);
        let title = substitute(section.title, HEADER, &attributes);
        let title = state.intern_str(&title);
        set_manpage_name_attributes(state, name, Some(purpose), Some(title));
        return;
    }

    let fallback = state
        .document_attributes
        .get_string("docname")
        .unwrap_or(Cow::Borrowed("command"));
    let fallback = state.intern_str(&fallback);
    set_manpage_name_attributes(state, fallback, None, None);
}

fn set_manpage_name_attributes<'input>(
    state: &mut ParserState<'input>,
    name: &'input str,
    purpose: Option<&'input str>,
    title: Option<&'input str>,
) {
    let attributes = Rc::make_mut(&mut state.document_attributes);
    attributes.set("manname".into(), AttributeValue::String(name.into()));
    if let Some(purpose) = purpose {
        attributes.set("manpurpose".into(), AttributeValue::String(purpose.into()));
    }
    if let Some(title) = title {
        attributes.insert("manname-title".into(), AttributeValue::String(title.into()));
    }
    if attributes.get_string("backend").as_deref() == Some("manpage") {
        attributes.set("docname".into(), AttributeValue::String(name.into()));
    }
}

/// Resolve the caption a block takes, from the document attributes in effect once the block —
/// content included — has been parsed. That is when asciidoctor assigns a caption, so an
/// attribute line inside an example changes that example's own caption. The ordinal comes
/// later, from `caption::renumber_captions` over the finished tree.
fn assign_block_caption<'input>(state: &ParserState<'input>, block: &mut Block<'input>) {
    let Some(kind) = CaptionKind::for_block(block) else {
        return;
    };
    if let Some(metadata) = block.metadata_mut() {
        let caption = Caption::resolve(metadata, &state.document_attributes, kind);
        metadata.caption = Some(caption);
    }
}

fn merge_attribute_metadata<'input>(
    metadata: &mut BlockMetadata<'input>,
    attribute_metadata: BlockMetadata<'input>,
) {
    if attribute_metadata.id.is_some() {
        metadata.id = attribute_metadata.id;
    }
    if attribute_metadata.style.is_some() {
        metadata.style = attribute_metadata.style;
    }
    metadata.roles.extend(attribute_metadata.roles);
    metadata.options.extend(attribute_metadata.options);
    for (name, value) in attribute_metadata.attributes.iter() {
        metadata.attributes.set(name.clone(), value.clone());
    }
    metadata.overlay_positional_attributes(&attribute_metadata.positional_attributes);
    #[cfg(feature = "pre-spec-subs")]
    if attribute_metadata.substitutions.is_some() {
        metadata.substitutions = attribute_metadata.substitutions;
    }
    if attribute_metadata.attribution.is_some() {
        metadata.attribution = attribute_metadata.attribution;
        metadata.attribution_substitutions = attribute_metadata.attribution_substitutions;
    } else if attribute_metadata.attribution_substitutions {
        metadata.attribution_substitutions = true;
    }
    if attribute_metadata.citetitle.is_some() {
        metadata.citetitle = attribute_metadata.citetitle;
        metadata.citetitle_substitutions = attribute_metadata.citetitle_substitutions;
    } else if attribute_metadata.citetitle_substitutions {
        metadata.citetitle_substitutions = true;
    }
}

fn finish_block_parsing_metadata<'input>(
    state: &mut ParserState<'input>,
    mut metadata: BlockMetadata<'input>,
    title: Title<'input>,
    parent_section_level: Option<SectionLevel>,
    discrete: bool,
    offset: usize,
) -> Result<BlockParsingMetadata<'input>, Error> {
    #[cfg(feature = "pre-spec-subs")]
    let substitutions = metadata
        .substitutions
        .as_ref()
        .map_or_else(SubstitutionPlan::default, SubstitutionPlan::for_block_spec);
    #[cfg(not(feature = "pre-spec-subs"))]
    let substitutions = SubstitutionPlan::default();
    let hardbreaks = substitutions.enabled(&Substitution::PostReplacements)
        && (state.hardbreaks || metadata.options.contains(&"hardbreaks"));
    extract_source_attributes(state, &mut metadata);
    extract_quote_attributes(&mut metadata);
    apply_quote_attribute_substitutions(state, &mut metadata, offset, substitutions)?;
    Ok(BlockParsingMetadata {
        metadata,
        title,
        parent_section_level,
        substitutions,
        hardbreaks,
        discrete,
    })
}

/// Helper to check delimiter matching and return error if mismatched
fn check_delimiters(
    open: &str,
    close: &str,
    block_type: &str,
    detail: SourceLocation,
) -> Result<(), Error> {
    if open == close {
        Ok(())
    } else {
        Err(Error::mismatched_delimiters(detail, block_type))
    }
}

/// Resolve a delimited block's closing delimiter into its `close_delimiter_location`.
///
/// When the block was closed (`p.close` is `Some((close_start, close_delim))`),
/// validate the delimiter pairing and return the close location. When it ran to
/// end of input unclosed (`p.close` is `None`), emit an
/// [`UnterminatedDelimitedBlock`](crate::WarningKind::UnterminatedDelimitedBlock)
/// warning anchored at the opening delimiter and return `None`, so the block is
/// still produced — matching asciidoctor's recovery (it warns and closes the
/// block at EOF).
fn resolve_delimited_close<'input>(
    state: &mut ParserState<'input>,
    p: &DelimitedParams<'input>,
) -> Result<Option<Location>, Error> {
    if let Some((close_start, close_delim)) = p.close {
        check_delimiters(
            p.open_delim,
            close_delim,
            p.kind.name(),
            state.create_error_source_location(
                state.create_block_location(p.start, p.end, p.offset),
            ),
        )?;
        Ok(Some(state.create_block_location(
            close_start,
            p.end,
            p.offset,
        )))
    } else {
        let open_delimiter_location = state.create_location(
            p.open_start + p.offset,
            p.open_start + p.offset + p.open_delim.len().saturating_sub(1),
        );
        state.add_warning(crate::Warning::new(
            crate::WarningKind::UnterminatedDelimitedBlock {
                kind: p.kind.name(),
                delimiter: p.open_delim.to_string(),
            },
            Some(state.create_error_source_location(open_delimiter_location)),
        ));
        Ok(None)
    }
}

/// Which delimited block an opening delimiter introduces. Drives
/// [`build_delimited_block`]'s dispatch and the `kind` carried by an
/// `UnterminatedDelimitedBlock` warning. The Markdown ```` ``` ```` fence maps to
/// [`DelimitedKind::Listing`] (it differs only by carrying a language).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DelimitedKind {
    Example,
    Comment,
    Listing,
    Literal,
    Open,
    Sidebar,
    Pass,
    Quote,
}

impl DelimitedKind {
    /// Block name used in the `unterminated <name> block` warning and the
    /// mismatched-delimiter error.
    fn name(self) -> &'static str {
        match self {
            DelimitedKind::Example => "example",
            DelimitedKind::Comment => "comment",
            DelimitedKind::Listing => "listing",
            DelimitedKind::Literal => "literal",
            DelimitedKind::Open => "open",
            DelimitedKind::Sidebar => "sidebar",
            DelimitedKind::Pass => "pass",
            DelimitedKind::Quote => "quote",
        }
    }
}

/// Parse a delimited block's inner text as nested blocks. Empty content yields
/// no blocks; a parse error is logged (with positions remapped to the original
/// source) and recovered as an empty block list, matching the per-rule behaviour
/// the delimited-block rules previously inlined.
fn parse_block_content<'input>(
    state: &mut ParserState<'input>,
    block_metadata: &BlockParsingMetadata<'input>,
    content: &'input str,
    content_start: usize,
    offset: usize,
    error_context: &str,
) -> Result<Vec<Block<'input>>, Error> {
    if content.trim().is_empty() {
        Ok(Vec::new())
    } else {
        document_parser::blocks(
            content,
            state,
            content_start + offset,
            block_metadata.parent_section_level,
            None,
        )
        .unwrap_or_else(|e| {
            adjust_and_log_parse_error(&e, content, content_start + offset, state, error_context);
            Ok(Vec::new())
        })
    }
}

/// A matched delimited-block open/content/optional-close, ready for construction.
/// Bundling these (rather than passing ~11 arguments) mirrors `TableParseParams`.
/// `close` is `None` when the block ran to end of input unclosed.
struct DelimitedParams<'input> {
    kind: DelimitedKind,
    /// The opening delimiter as it appeared in source (e.g. `"===="`).
    open_delim: &'input str,
    /// Language captured after a Markdown ```` ``` ```` fence, if any.
    lang: Option<&'input str>,
    content: &'input str,
    open_start: usize,
    start: usize,
    content_start: usize,
    content_end: usize,
    /// End offset of the whole block (`span_end`).
    end: usize,
    offset: usize,
    close: Option<(usize, &'input str)>,
}

/// Build a delimited block from a matched [`DelimitedParams`]. This is the single
/// construction site for every non-table delimited block: the grammar's generic
/// `delimited_block` rule matches the open/content/close skeleton once and
/// delegates here, mirroring how table rules delegate to `parse_table_block_impl`.
/// An absent `close` means the block ran to end of input; `resolve_delimited_close`
/// emits the unterminated warning and the block is still produced (closed at EOF),
/// matching asciidoctor. Per-kind construction lives in the `*_inner` helpers.
fn build_delimited_block<'input>(
    state: &mut ParserState<'input>,
    block_metadata: &BlockParsingMetadata<'input>,
    p: &DelimitedParams<'input>,
) -> Result<Block<'input>, Error> {
    let close_delimiter_location = resolve_delimited_close(state, p)?;
    let location = state.create_block_location(p.start, p.end, p.offset);
    let open_delimiter_location = state.create_location(
        p.open_start + p.offset,
        p.open_start + p.offset + p.open_delim.len().saturating_sub(1),
    );
    let mut metadata = block_metadata.metadata.clone();

    let inner = match p.kind {
        // An example block can become an admonition (a different `Block` variant),
        // so it constructs and returns the whole block itself.
        DelimitedKind::Example => {
            return build_example_block(
                state,
                block_metadata,
                metadata,
                p,
                location,
                open_delimiter_location,
                close_delimiter_location,
            );
        }
        DelimitedKind::Comment => comment_inner(state, &mut metadata, p),
        DelimitedKind::Listing | DelimitedKind::Literal => {
            verbatim_inner(state, block_metadata, &mut metadata, p)
        }
        DelimitedKind::Open => open_inner(state, block_metadata, &mut metadata, p)?,
        DelimitedKind::Sidebar => sidebar_inner(state, block_metadata, &mut metadata, p)?,
        DelimitedKind::Pass => pass_inner(state, &mut metadata, p),
        DelimitedKind::Quote => quote_inner(state, block_metadata, &mut metadata, p)?,
    };

    Ok(assemble_delimited(
        metadata,
        p.open_delim,
        inner,
        block_metadata.title.clone(),
        location,
        open_delimiter_location,
        close_delimiter_location,
    ))
}

/// Assemble the common `Block::DelimitedBlock` shell shared by every kind.
fn assemble_delimited<'input>(
    metadata: BlockMetadata<'input>,
    open_delim: &'input str,
    inner: DelimitedBlockType<'input>,
    title: Title<'input>,
    location: Location,
    open_delimiter_location: Location,
    close_delimiter_location: Option<Location>,
) -> Block<'input> {
    Block::DelimitedBlock(DelimitedBlock {
        metadata,
        delimiter: open_delim,
        inner,
        title,
        location,
        open_delimiter_location: Some(open_delimiter_location),
        close_delimiter_location,
    })
}

/// `====` example block, or an admonition when carrying an admonition style.
fn build_example_block<'input>(
    state: &mut ParserState<'input>,
    block_metadata: &BlockParsingMetadata<'input>,
    mut metadata: BlockMetadata<'input>,
    p: &DelimitedParams<'input>,
    location: Location,
    open_delimiter_location: Location,
    close_delimiter_location: Option<Location>,
) -> Result<Block<'input>, Error> {
    metadata.move_positional_attributes_to_attributes();
    let blocks = parse_block_content(
        state,
        block_metadata,
        p.content,
        p.content_start,
        p.offset,
        "Error parsing example content as blocks in example block",
    )?;
    // An admonition style (NOTE/TIP/…) turns the example block into an admonition.
    if let Some(style) = block_metadata.metadata.style
        && let Ok(variant) = style.parse::<AdmonitionVariant>()
    {
        metadata.style = None;
        return Ok(Block::Admonition(
            Admonition::new(variant, blocks, location)
                .with_metadata(metadata)
                .with_title(block_metadata.title.clone()),
        ));
    }
    Ok(assemble_delimited(
        metadata,
        p.open_delim,
        DelimitedBlockType::DelimitedExample(blocks),
        block_metadata.title.clone(),
        location,
        open_delimiter_location,
        close_delimiter_location,
    ))
}

/// `////` comment block: the raw inner text, rendered nowhere.
fn comment_inner<'input>(
    state: &ParserState<'input>,
    metadata: &mut BlockMetadata<'input>,
    p: &DelimitedParams<'input>,
) -> DelimitedBlockType<'input> {
    metadata.move_positional_attributes_to_attributes();
    let content_location = state.create_block_location(p.content_start, p.content_end, p.offset);
    DelimitedBlockType::DelimitedComment(vec![InlineNode::PlainText(Plain {
        content: p.content,
        location: content_location,
        escaped: false,
    })])
}

/// Verbatim block (`----` listing or `....` literal, including the Markdown fence):
/// resolves callouts and records the verbatim state for a following callout list.
fn verbatim_inner<'input>(
    state: &mut ParserState<'input>,
    block_metadata: &BlockParsingMetadata<'input>,
    metadata: &mut BlockMetadata<'input>,
    p: &DelimitedParams<'input>,
) -> DelimitedBlockType<'input> {
    // A Markdown fence language becomes a positional `source` style so language
    // detection works like `[source,lang]` (never set for `....`/`----`).
    if let Some(language) = p.lang {
        metadata.positional_attributes.insert(
            0,
            PositionalAttribute {
                value: language,
                substitutions: false,
                location: None,
            },
        );
        metadata.style = Some("source");
    }
    extract_source_attributes(state, metadata);
    metadata.move_positional_attributes_to_attributes();
    let content_location = state.create_block_location(p.content_start, p.content_end, p.offset);
    let (inlines, callouts) = resolve_verbatim_callouts(
        state,
        p.content,
        content_location,
        block_metadata
            .substitutions
            .enabled(&Substitution::Callouts),
    );
    state.last_block_was_verbatim = true;
    state.last_verbatim_callouts = callouts;
    if p.kind == DelimitedKind::Literal {
        DelimitedBlockType::DelimitedLiteral(inlines)
    } else {
        DelimitedBlockType::DelimitedListing(inlines)
    }
}

/// `--` open block, or a non-rendering comment when carrying a `[comment]` style.
fn open_inner<'input>(
    state: &mut ParserState<'input>,
    block_metadata: &BlockParsingMetadata<'input>,
    metadata: &mut BlockMetadata<'input>,
    p: &DelimitedParams<'input>,
) -> Result<DelimitedBlockType<'input>, Error> {
    metadata.move_positional_attributes_to_attributes();
    if block_metadata.metadata.style == Some("comment") {
        metadata.style = None;
        let content_location =
            state.create_block_location(p.content_start, p.content_end, p.offset);
        return Ok(DelimitedBlockType::DelimitedComment(vec![
            InlineNode::PlainText(Plain {
                content: p.content,
                location: content_location,
                escaped: false,
            }),
        ]));
    }
    let blocks = parse_block_content(
        state,
        block_metadata,
        p.content,
        p.content_start,
        p.offset,
        "Error parsing content as blocks in open block",
    )?;
    Ok(DelimitedBlockType::DelimitedOpen(blocks))
}

/// `****` sidebar block.
fn sidebar_inner<'input>(
    state: &mut ParserState<'input>,
    block_metadata: &BlockParsingMetadata<'input>,
    metadata: &mut BlockMetadata<'input>,
    p: &DelimitedParams<'input>,
) -> Result<DelimitedBlockType<'input>, Error> {
    metadata.move_positional_attributes_to_attributes();
    let blocks = parse_block_content(
        state,
        block_metadata,
        p.content,
        p.content_start,
        p.offset,
        "Error parsing sidebar content as blocks",
    )?;
    Ok(DelimitedBlockType::DelimitedSidebar(blocks))
}

/// `++++` passthrough block, or a stem block when carrying a `[stem]` style.
fn pass_inner<'input>(
    state: &ParserState<'input>,
    metadata: &mut BlockMetadata<'input>,
    p: &DelimitedParams<'input>,
) -> DelimitedBlockType<'input> {
    if metadata.style == Some("stem") {
        let notation = drain_positional_slots(metadata, 1)
            .first()
            .filter(|value| !value.is_empty())
            .and_then(|value| value.parse::<StemNotation>().ok())
            .or_else(|| {
                state
                    .document_attributes
                    .get("stem")
                    .and_then(|value| match value {
                        AttributeValue::String(value) => value.parse::<StemNotation>().ok(),
                        AttributeValue::Bool(_) | AttributeValue::None => None,
                    })
            })
            .unwrap_or(StemNotation::Latexmath);
        metadata.move_positional_attributes_to_attributes();
        metadata.style = None;
        DelimitedBlockType::DelimitedStem(StemContent {
            content: p.content,
            notation,
        })
    } else {
        metadata.move_positional_attributes_to_attributes();
        let content_location =
            state.create_block_location(p.content_start, p.content_end, p.offset);
        DelimitedBlockType::DelimitedPass(vec![InlineNode::RawText(Raw {
            content: p.content,
            location: content_location,
            subs: vec![],
        })])
    }
}

/// Parse a `____` quote block body as nested blocks, or preserve verse text.
fn quote_inner<'input>(
    state: &mut ParserState<'input>,
    block_metadata: &BlockParsingMetadata<'input>,
    metadata: &mut BlockMetadata<'input>,
    p: &DelimitedParams<'input>,
) -> Result<DelimitedBlockType<'input>, Error> {
    metadata.move_positional_attributes_to_attributes();

    if metadata.style == Some("verse") {
        let content_location =
            state.create_block_location(p.content_start, p.content_end, p.offset);
        Ok(DelimitedBlockType::DelimitedVerse(vec![
            InlineNode::PlainText(Plain {
                content: p.content,
                location: content_location,
                escaped: false,
            }),
        ]))
    } else if metadata.style.is_some() {
        // A styled (non-verse) quote always parses its body, even when empty.
        let blocks = document_parser::blocks(
            p.content,
            state,
            p.content_start + p.offset,
            block_metadata.parent_section_level,
            None,
        )
        .unwrap_or_else(|e| {
            adjust_and_log_parse_error(
                &e,
                p.content,
                p.content_start + p.offset,
                state,
                "Error parsing example content as blocks in quote block",
            );
            Ok(Vec::new())
        })?;
        Ok(DelimitedBlockType::DelimitedQuote(blocks))
    } else {
        let blocks = parse_block_content(
            state,
            block_metadata,
            p.content,
            p.content_start,
            p.offset,
            "Error parsing content as blocks in quote block",
        )?;
        Ok(DelimitedBlockType::DelimitedQuote(blocks))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AttributeQuote {
    Unquoted,
    Single,
    Double,
}

#[derive(Debug)]
struct ScannedAttribute {
    name: Option<String>,
    value: String,
    quote: AttributeQuote,
    value_start: usize,
    value_end: usize,
}

#[derive(Clone, Copy)]
enum BlockAttributeMode {
    Block,
    Macro(MacroAttributeContext),
}

fn is_attribute_name_start(character: char) -> bool {
    character == '_' || character.is_alphanumeric()
}

fn is_attribute_name_continue(character: char) -> bool {
    is_attribute_name_start(character) || matches!(character, '-' | '.')
}

fn named_attribute_parts(value: &str) -> Option<(&str, usize)> {
    let mut characters = value.char_indices();
    let (_, first) = characters.next()?;
    if !is_attribute_name_start(first) {
        return None;
    }

    let mut name_end = first.len_utf8();
    for (index, character) in characters {
        if !is_attribute_name_continue(character) {
            break;
        }
        name_end = index + character.len_utf8();
    }

    let remainder = &value[name_end..];
    let equals_offset = remainder.len() - remainder.trim_start_matches([' ', '\t']).len();
    remainder
        .get(equals_offset..)
        .is_some_and(|remainder| remainder.starts_with('='))
        .then_some((&value[..name_end], name_end + equals_offset + 1))
}

fn closing_quote(value: &str, quote: char) -> Option<usize> {
    let mut escaped = false;
    for (index, character) in value.char_indices().skip(1) {
        if character == quote && !escaped {
            return Some(index);
        }
        escaped = character == '\\' && !escaped;
        if character != '\\' {
            escaped = false;
        }
    }
    None
}

fn unescape_attribute_quote(value: &str, quote: char) -> String {
    let escaped_quote = format!("\\{quote}");
    value.replace(&escaped_quote, &quote.to_string())
}

fn scan_attribute_list(source: &str) -> Vec<ScannedAttribute> {
    let mut attributes = Vec::new();
    let mut cursor = 0;

    loop {
        let slot_start = cursor;
        let remaining = &source[slot_start..];
        let leading = remaining.len() - remaining.trim_start_matches([' ', '\t']).len();
        let trimmed_start = slot_start + leading;
        let candidate = &source[trimmed_start..];
        let named = named_attribute_parts(candidate);
        let raw_value_start = named.map_or(trimmed_start, |(_, start)| trimmed_start + start);
        let value_leading = source[raw_value_start..].len()
            - source[raw_value_start..]
                .trim_start_matches([' ', '\t'])
                .len();
        let value_start = raw_value_start + value_leading;
        let first = source[value_start..].chars().next();

        if let Some(quote @ ('\'' | '"')) = first {
            let quoted = &source[value_start..];
            if let Some(close) = closing_quote(quoted, quote) {
                let after_close = value_start + close + quote.len_utf8();
                attributes.push(ScannedAttribute {
                    name: named.map(|(name, _)| name.to_string()),
                    value: unescape_attribute_quote(&quoted[quote.len_utf8()..close], quote),
                    quote: if quote == '\'' {
                        AttributeQuote::Single
                    } else {
                        AttributeQuote::Double
                    },
                    value_start: value_start + quote.len_utf8(),
                    value_end: value_start + close,
                });

                let trailing = &source[after_close..];
                let whitespace = trailing.len() - trailing.trim_start_matches([' ', '\t']).len();
                cursor = after_close + whitespace;
                if cursor == source.len() {
                    break;
                }
                if source[cursor..].starts_with(',') {
                    cursor += 1;
                    if cursor == source.len() {
                        attributes.push(ScannedAttribute {
                            name: None,
                            value: String::new(),
                            quote: AttributeQuote::Unquoted,
                            value_start: cursor,
                            value_end: cursor,
                        });
                        break;
                    }
                }
                continue;
            }
        }

        let comma = source[value_start..]
            .find(',')
            .map(|index| value_start + index);
        let end = comma.unwrap_or(source.len());
        let trimmed = source[value_start..end].trim_end();

        attributes.push(ScannedAttribute {
            name: named.map(|(name, _)| name.to_string()),
            value: trimmed.to_string(),
            quote: AttributeQuote::Unquoted,
            value_start,
            value_end: value_start + trimmed.len(),
        });

        let Some(comma) = comma else {
            break;
        };
        cursor = comma + 1;
        if cursor == source.len() {
            attributes.push(ScannedAttribute {
                name: None,
                value: String::new(),
                quote: AttributeQuote::Unquoted,
                value_start: cursor,
                value_end: cursor,
            });
            break;
        }
    }

    attributes
}

fn apply_block_style<'input>(
    state: &ParserState<'input>,
    metadata: &mut BlockMetadata<'input>,
    value: &'input str,
    location: Option<&Location>,
) -> bool {
    if value.is_empty() {
        return false;
    }
    if value.chars().any(char::is_whitespace)
        || !value
            .chars()
            .any(|character| matches!(character, '#' | '.' | '%'))
    {
        metadata.style = Some(value);
        return matches!(value, "discrete" | "float");
    }

    let mut kind = None;
    let mut start = 0;
    for (index, character) in value.char_indices() {
        let next_kind = match character {
            '#' => Some(StylePartKind::Id),
            '.' => Some(StylePartKind::Role),
            '%' => Some(StylePartKind::Option),
            _ => continue,
        };
        apply_style_part(
            state,
            metadata,
            kind,
            &value[start..index],
            start,
            index,
            location,
        );
        kind = next_kind;
        start = index + character.len_utf8();
    }
    apply_style_part(
        state,
        metadata,
        kind,
        &value[start..],
        start,
        value.len(),
        location,
    );
    matches!(metadata.style, Some("discrete" | "float"))
}

#[derive(Clone, Copy)]
enum StylePartKind {
    Id,
    Role,
    Option,
}

fn apply_style_part<'input>(
    state: &ParserState<'input>,
    metadata: &mut BlockMetadata<'input>,
    kind: Option<StylePartKind>,
    value: &'input str,
    start: usize,
    end: usize,
    location: Option<&Location>,
) {
    if value.is_empty() {
        return;
    }
    match kind {
        None => metadata.style = Some(value),
        Some(StylePartKind::Id) => {
            let location = location.map_or_else(Location::default, |location| {
                state.create_location(
                    location.absolute_start + start,
                    location.absolute_start + end,
                )
            });
            metadata.id = Some(Anchor {
                id: value,
                xreflabel: None,
                location,
                bibliography: false,
            });
        }
        Some(StylePartKind::Role) => metadata.roles.push(value),
        Some(StylePartKind::Option) => metadata.options.push(value),
    }
}

fn ensure_positional_slot(metadata: &mut BlockMetadata<'_>, slot: usize) {
    metadata
        .positional_attributes
        .resize(slot, PositionalAttribute::default());
}

fn drain_positional_slots<'input>(
    metadata: &mut BlockMetadata<'input>,
    count: usize,
) -> Vec<&'input str> {
    let drain = metadata.positional_attributes.len().min(count);
    metadata
        .positional_attributes
        .drain(..drain)
        .map(|attribute| attribute.value)
        .collect()
}

fn extract_source_attributes(state: &ParserState<'_>, metadata: &mut BlockMetadata<'_>) {
    if metadata.style != Some("source") {
        return;
    }

    if let Some(language) = metadata
        .positional_attributes
        .first()
        .map(|attribute| attribute.value)
        .filter(|value| !value.is_empty())
    {
        metadata.attributes.set(
            "language".into(),
            AttributeValue::String(Cow::Borrowed(language)),
        );
    }
    if let Some(linenums) = metadata
        .positional_attributes
        .get(1)
        .map(|attribute| attribute.value)
        .filter(|value| !value.is_empty())
    {
        metadata.attributes.set(
            "linenums".into(),
            AttributeValue::String(Cow::Borrowed(linenums)),
        );
    }
    if !metadata.attributes.contains_key("linenums")
        && (metadata.options.contains(&"linenums")
            || state.document_attributes.is_set("source-linenums-option"))
    {
        metadata
            .attributes
            .set("linenums".into(), AttributeValue::String(Cow::Borrowed("")));
    }
    if !metadata.options.contains(&"nowrap")
        && state.document_attributes.contains_key("prewrap")
        && !state.document_attributes.is_set("prewrap")
    {
        metadata.options.push("nowrap");
    }
}

fn store_named_block_attribute<'input>(
    state: &mut ParserState<'input>,
    metadata: &mut BlockMetadata<'input>,
    name: &'input str,
    value: &'input str,
    quote: AttributeQuote,
    location: Option<Location>,
    context: Option<MacroAttributeContext>,
) -> Option<(usize, usize)> {
    if quote == AttributeQuote::Unquoted && value == "None" {
        return None;
    }
    let title_position = if name == "title" {
        location
            .as_ref()
            .map(|location| (location.absolute_start, location.absolute_end))
    } else {
        None
    };
    match name {
        RESERVED_NAMED_ATTRIBUTE_ID if metadata.id.is_none() => {
            metadata.id = Some(Anchor {
                id: value,
                xreflabel: None,
                location: location.clone().unwrap_or_default(),
                bibliography: false,
            });
        }
        RESERVED_NAMED_ATTRIBUTE_ROLE | "roles" => {
            metadata
                .roles
                .extend(value.split_whitespace().filter(|role| !role.is_empty()));
        }
        RESERVED_NAMED_ATTRIBUTE_OPTIONS | "options" => {
            metadata
                .options
                .extend(parse_comma_separated_values(state, value));
        }
        "style" => metadata.style = Some(value),
        RESERVED_NAMED_ATTRIBUTE_SUBS => {
            #[cfg(feature = "pre-spec-subs")]
            {
                state.add_generic_warning_at(
                    "The subs= attribute may change when the AsciiDoc specification is finalized. See: https://gitlab.eclipse.org/eclipse/asciidoc-lang/asciidoc-lang/-/issues/16".to_string(),
                    location.clone().unwrap_or_default(),
                );
                metadata.substitutions = Some(parse_subs_attribute(value));
            }
            #[cfg(not(feature = "pre-spec-subs"))]
            state.add_generic_warning_at(
                "The subs= attribute is not honoured in this build (the `pre-spec-subs` feature is disabled). The draft AsciiDoc spec drops the substitution model in favour of an inline parsing grammar; this attribute will be silently ignored.".to_string(),
                location.clone().unwrap_or_default(),
            );
        }
        "attribution" => {
            metadata.attribution = Some(Attribution::new(plain_attribute_value(value, location)));
            metadata.attribution_substitutions = quote == AttributeQuote::Single;
        }
        "citetitle" => {
            metadata.citetitle = Some(CiteTitle::new(plain_attribute_value(value, location)));
            metadata.citetitle_substitutions = quote == AttributeQuote::Single;
        }
        _ => {
            if context == Some(MacroAttributeContext::Image) && name == "link" {
                metadata.attributes.set(
                    Cow::Borrowed(name),
                    AttributeValue::String(Cow::Borrowed(value)),
                );
            } else {
                metadata.attributes.insert(
                    Cow::Borrowed(name),
                    AttributeValue::String(Cow::Borrowed(value)),
                );
            }
        }
    }
    title_position
}

fn parse_block_attribute_list<'input>(
    state: &mut ParserState<'input>,
    source: &'input str,
    content_start: usize,
    fallback_end: usize,
    mode: BlockAttributeMode,
) -> (bool, BlockMetadata<'input>, Option<(usize, usize)>) {
    let substituted = substitute(
        source,
        &[Substitution::Attributes],
        &state.document_attributes,
    );
    let positions_exact = matches!(substituted, Cow::Borrowed(_));
    let attributes = scan_attribute_list(&substituted);
    let mut metadata = BlockMetadata::default();
    let mut discrete = false;
    let mut title_position = None;

    for (slot, attribute) in attributes.into_iter().enumerate() {
        let value = state.intern_str(&attribute.value);
        let name = attribute.name.as_deref().map(|name| state.intern_str(name));
        let location = positions_exact.then(|| {
            state.create_location(
                content_start + attribute.value_start,
                content_start + attribute.value_end,
            )
        });

        if let Some(name) = name {
            if slot > 0 {
                ensure_positional_slot(&mut metadata, slot);
            }
            title_position = store_named_block_attribute(
                state,
                &mut metadata,
                name,
                value,
                attribute.quote,
                location,
                match mode {
                    BlockAttributeMode::Block => None,
                    BlockAttributeMode::Macro(context) => Some(context),
                },
            )
            .or(title_position);
        } else if slot == 0 {
            match mode {
                BlockAttributeMode::Block => {
                    discrete = apply_block_style(state, &mut metadata, value, location.as_ref());
                }
                BlockAttributeMode::Macro(_) if !value.is_empty() => metadata.style = Some(value),
                BlockAttributeMode::Macro(_) => {}
            }
        } else {
            ensure_positional_slot(&mut metadata, slot);
            if let Some(positional) = metadata.positional_attributes.get_mut(slot - 1) {
                *positional = PositionalAttribute {
                    value,
                    substitutions: attribute.quote == AttributeQuote::Single,
                    location,
                };
            }
        }
    }

    if title_position.is_none() && metadata.attributes.get("title").is_some() {
        title_position = Some((content_start, fallback_end));
    }
    (discrete, metadata, title_position)
}

fn plain_attribute_value(value: &str, location: Option<Location>) -> Vec<InlineNode<'_>> {
    vec![InlineNode::PlainText(Plain {
        content: value,
        location: location.unwrap_or_default(),
        escaped: false,
    })]
}

fn extract_quote_attributes(metadata: &mut BlockMetadata<'_>) {
    if !matches!(metadata.style, Some("quote" | "verse")) {
        return;
    }

    let named_attribution = metadata.attribution.take();
    let named_citetitle = metadata.citetitle.take();
    let positional_attribution = metadata.positional_attributes.first().cloned();
    let positional_citetitle = metadata.positional_attributes.get(1).cloned();

    if let Some(attribute) = positional_attribution.filter(|value| !value.value.is_empty()) {
        metadata.attribution = Some(Attribution::new(plain_attribute_value(
            attribute.value,
            attribute.location,
        )));
        metadata.attribution_substitutions = attribute.substitutions;
    } else {
        metadata.attribution = named_attribution;
    }

    if let Some(attribute) = positional_citetitle.filter(|value| !value.value.is_empty()) {
        metadata.citetitle = Some(CiteTitle::new(plain_attribute_value(
            attribute.value,
            attribute.location,
        )));
        metadata.citetitle_substitutions = attribute.substitutions;
    } else {
        metadata.citetitle = named_citetitle;
    }

    let _ = drain_positional_slots(metadata, 2);
}

fn apply_quote_attribute_substitutions<'input>(
    state: &mut ParserState<'input>,
    metadata: &mut BlockMetadata<'input>,
    offset: usize,
    substitutions: SubstitutionPlan,
) -> Result<(), Error> {
    let block_metadata = BlockParsingMetadata {
        substitutions,
        ..BlockParsingMetadata::default()
    };

    if metadata.attribution_substitutions
        && let Some(InlineNode::PlainText(plain)) = metadata
            .attribution
            .as_deref()
            .and_then(|value| value.first())
    {
        let start = plain.location.absolute_start.saturating_sub(offset);
        let end = plain.location.absolute_end.saturating_sub(offset);
        let (inlines, _) =
            process_inlines(state, &block_metadata, start, end, offset, plain.content)?;
        metadata.attribution = Some(Attribution::new(inlines));
    }
    if metadata.citetitle_substitutions
        && let Some(InlineNode::PlainText(plain)) = metadata
            .citetitle
            .as_deref()
            .and_then(|value| value.first())
    {
        let start = plain.location.absolute_start.saturating_sub(offset);
        let end = plain.location.absolute_end.saturating_sub(offset);
        let (inlines, _) =
            process_inlines(state, &block_metadata, start, end, offset, plain.content)?;
        metadata.citetitle = Some(CiteTitle::new(inlines));
    }
    Ok(())
}

/// Parse a target's cross-reference label into inline nodes.
///
/// A displayed label takes the reference-text substitutions asciidoctor
/// documents — specialchars, quotes, and replacements — so `*Bold* label`
/// renders bold while a macro stays literal: a label cannot become a link, and
/// therefore cannot nest one inside the reference it labels. Only quotes are a
/// parse-time concern; converters apply specialchars and replacements to the
/// resulting text nodes. Attribute references were already substituted when
/// the block metadata was parsed.
fn parse_reference_label<'a>(
    state: &mut ParserState<'a>,
    label: Option<&'a str>,
    location: &Location,
) -> Option<Vec<InlineNode<'a>>> {
    let label = label?;
    if label.trim().is_empty() {
        return None;
    }
    let block_metadata = BlockParsingMetadata {
        substitutions: SubstitutionPlan::only(&Substitution::Quotes),
        ..BlockParsingMetadata::default()
    };
    match process_inlines(
        state,
        &block_metadata,
        location.absolute_start,
        location.absolute_end,
        0,
        label,
    ) {
        Ok((inlines, _)) if !inlines.is_empty() => Some(inlines),
        // A label that does not parse as inline content still reads as its
        // literal text.
        _ => Some(vec![InlineNode::PlainText(Plain {
            content: label,
            location: location.clone(),
            escaped: false,
        })]),
    }
}

/// Insert an anchor into the cross-reference catalog with optional reference text.
///
/// The first registration of an id wins, matching asciidoctor: when two elements
/// claim the same id, `<<id>>` uses the reference text of the one that claimed it
/// first.
fn insert_reference<'a>(
    state: &mut ParserState<'a>,
    refs: &mut HashMap<&'a str, Reference<'a>>,
    anchor: &Anchor<'a>,
    title: Option<Title<'a>>,
    caption: Option<Caption<'a>>,
) {
    if refs.contains_key(anchor.id) {
        return;
    }
    let mut xreflabel = parse_reference_label(state, anchor.xreflabel, &anchor.location);
    if anchor.is_bibliography()
        && let Some(label) = xreflabel.as_mut()
    {
        label.insert(
            0,
            InlineNode::PlainText(Plain {
                content: "[",
                location: anchor.location.clone(),
                escaped: false,
            }),
        );
        label.push(InlineNode::PlainText(Plain {
            content: "]",
            location: anchor.location.clone(),
            escaped: false,
        }));
    }
    refs.insert(
        anchor.id,
        Reference {
            xreflabel,
            title,
            location: anchor.location.clone(),
            caption,
            bibliography: anchor.is_bibliography(),
            automatic_citation: false,
        },
    );
}

struct CrossReferenceUse<'a> {
    target: &'a str,
    location: Location,
    automatic: bool,
    resolve_natural_target: bool,
}

fn insert_untitled_reference<'a>(
    refs: &mut HashMap<&'a str, Reference<'a>>,
    id: &'a str,
    location: &Location,
) {
    refs.entry(id).or_insert_with(|| Reference {
        xreflabel: None,
        title: None,
        location: location.clone(),
        caption: None,
        bibliography: false,
        automatic_citation: false,
    });
}

fn finalize_cross_references<'a>(
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

    walk_document_inline_nodes_mut(document, &mut |inline| {
        let InlineNode::Macro(InlineMacro::CrossReference(xref)) = inline else {
            return;
        };
        xref.target = resolve_xref_target(
            xref.target,
            xref.resolve_natural_target,
            reference_ids,
            natural_targets,
        );
        let Some(snapshot) = xref.caption_label_snapshot_id.take() else {
            return;
        };
        let Some(kind) = caption_kinds.get(xref.target) else {
            return;
        };
        xref.caption_label = state.xref_caption_label(snapshot, *kind);
    });
}

fn section_xreflabel<'a>(state: &ParserState<'a>, metadata: &BlockMetadata<'a>) -> Option<&'a str> {
    if let Some(reftext) = metadata.attributes.get_string("reftext") {
        return Some(state.intern_cow(reftext));
    }
    metadata.anchors.last().and_then(|anchor| anchor.xreflabel)
}

fn register_section_header<'a>(
    state: &mut ParserState<'a>,
    block_metadata: &BlockParsingMetadata<'a>,
    title: Title<'a>,
    natural_title: &'a str,
    level: SectionLevel,
    location: Location,
    direct_parent_section_kind: Option<SectionKind>,
) -> (Title<'a>, section::SectionNumbering) {
    let section_id = Section::generate_id(state.arena, &block_metadata.metadata, &title)
        .as_arena_str(state.arena);
    let xreflabel = section_xreflabel(state, &block_metadata.metadata);
    let reference_text = xreflabel
        .filter(|label| !label.trim().is_empty())
        .unwrap_or(natural_title);
    if !reference_text.is_empty() {
        state
            .natural_xref_targets
            .entry(reference_text)
            .or_insert(section_id);
    }

    let kind = SectionKind::from_style(block_metadata.metadata.style);
    state.toc_entries.push(TocEntry::for_section(
        section_id,
        title.clone(),
        level,
        xreflabel,
        kind,
        location.clone(),
    ));
    warn_for_nested_bibliography_section(state, direct_parent_section_kind, location);

    let numbering = section::SectionNumbering::from_attributes(&state.document_attributes);
    (title, numbering)
}

/// Finalize a cross-reference target after section IDs and title aliases are known.
///
/// Exact IDs are kept. Natural-title lookup applies only when enabled at the
/// reference's source position. Missing aliases leave the target unchanged.
fn resolve_xref_target<'a>(
    target: &'a str,
    resolve_natural_target: bool,
    reference_ids: &HashSet<&'a str>,
    natural_targets: &HashMap<&'a str, &'a str>,
) -> &'a str {
    if !resolve_natural_target || reference_ids.contains(target) {
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
    refs: &mut HashMap<&'a str, Reference<'a>>,
    xrefs: &mut Vec<CrossReferenceUse<'a>>,
) where
    T: MarkedText<'a, Content = Vec<InlineNode<'a>>>,
{
    if let Some(id) = text.id() {
        insert_untitled_reference(refs, id, text.location());
    }
    collect_inline_references(state, text.content(), refs, xrefs);
}

fn collect_link_references<'a>(
    state: &mut ParserState<'a>,
    attributes: &ElementAttributes<'a>,
    location: &Location,
    text: &[InlineNode<'a>],
    refs: &mut HashMap<&'a str, Reference<'a>>,
    xrefs: &mut Vec<CrossReferenceUse<'a>>,
) {
    if let Some(id) = attributes.get_string("id") {
        insert_untitled_reference(refs, state.intern_cow(id), location);
    }
    collect_inline_references(state, text, refs, xrefs);
}

fn header_reference_title<'a>(header: &Header<'a>) -> Title<'a> {
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

fn collect_metadata_references<'a>(
    state: &mut ParserState<'a>,
    metadata: &BlockMetadata<'a>,
    refs: &mut HashMap<&'a str, Reference<'a>>,
    xrefs: &mut Vec<CrossReferenceUse<'a>>,
) {
    if let Some(attribution) = &metadata.attribution {
        collect_inline_references(state, attribution, refs, xrefs);
    }
    if let Some(citetitle) = &metadata.citetitle {
        collect_inline_references(state, citetitle, refs, xrefs);
    }
}

/// Walk the final document tree to (1) populate the cross-reference catalog `refs` with
/// every anchor (block IDs, inline `[[id]]` anchors, formatted span IDs, and link IDs) and (2)
/// collect every `<<id>>` / `xref:id[]` use for unresolved-reference checking and
/// bibliography citation metadata. Titles and quote credits are part of this walk because
/// converters render their inline content too. A target with no title is still registered
/// (reference text `None`), so an `<<id>>` to it resolves to the literal `[id]` rather than
/// being treated as unresolved. Top-level section IDs are seeded from `toc_entries`; this walk
/// also catalogs nested-document sections that do not belong in the outer table of contents.
fn collect_references<'a>(
    state: &mut ParserState<'a>,
    blocks: &[Block<'a>],
    refs: &mut HashMap<&'a str, Reference<'a>>,
    xrefs: &mut Vec<CrossReferenceUse<'a>>,
) {
    for block in blocks {
        if !matches!(block, Block::Section(_))
            && let Some(anchor) = block.anchor()
        {
            let caption = block
                .metadata()
                .and_then(|metadata| metadata.caption.clone());
            insert_reference(state, refs, anchor, block.title().cloned(), caption);
        }
        if let Some(title) = block.title() {
            collect_inline_references(state, title, refs, xrefs);
        }
        if let Some(metadata) = block.metadata() {
            collect_metadata_references(state, metadata, refs, xrefs);
        }

        match block {
            Block::Section(s) => {
                // Top-level sections are already seeded from `toc_entries`.
                // Sections in an AsciiDoc-style table cell belong to its
                // nested document and are intentionally absent from that list,
                // but their destinations remain visible to outer xrefs.
                let id = Section::generate_id(state.arena, &s.metadata, &s.title)
                    .as_arena_str(state.arena);
                let xreflabel = section_xreflabel(state, &s.metadata);
                refs.entry(id).or_insert_with(|| Reference {
                    xreflabel: parse_reference_label(state, xreflabel, &s.location),
                    title: Some(s.title.clone()),
                    location: s.location.clone(),
                    caption: None,
                    bibliography: false,
                    automatic_citation: false,
                });
                collect_references(state, &s.content, refs, xrefs);
            }
            Block::Paragraph(p) => collect_inline_references(state, &p.content, refs, xrefs),
            // A simple admonition's content is a synthetic paragraph that shares
            // the admonition's anchor; the admonition registered it first, so its
            // reference text stands.
            Block::Admonition(a) => collect_references(state, &a.blocks, refs, xrefs),
            Block::UnorderedList(l) => {
                for item in &l.items {
                    collect_inline_references(state, &item.principal, refs, xrefs);
                    collect_references(state, &item.blocks, refs, xrefs);
                }
            }
            Block::OrderedList(l) => {
                for item in &l.items {
                    collect_inline_references(state, &item.principal, refs, xrefs);
                    collect_references(state, &item.blocks, refs, xrefs);
                }
            }
            Block::CalloutList(l) => {
                for item in &l.items {
                    collect_inline_references(state, &item.principal, refs, xrefs);
                    collect_references(state, &item.blocks, refs, xrefs);
                }
            }
            Block::DescriptionList(l) => {
                for item in &l.items {
                    for anchor in &item.anchors {
                        insert_reference(state, refs, anchor, None, None);
                    }
                    collect_inline_references(state, &item.term, refs, xrefs);
                    collect_inline_references(state, &item.principal_text, refs, xrefs);
                    collect_references(state, &item.description, refs, xrefs);
                }
            }
            Block::DelimitedBlock(d) => collect_delimited_references(state, &d.inner, refs, xrefs),
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

fn normalize_bibliography_lists<'a>(state: &ParserState<'a>, blocks: &mut [Block<'a>]) {
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
    inner: &DelimitedBlockType<'a>,
    refs: &mut HashMap<&'a str, Reference<'a>>,
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
                .iter()
                .chain(table.footer.iter())
                .chain(table.rows.iter())
            {
                for column in &row.columns {
                    collect_references(state, &column.content, refs, xrefs);
                }
            }
        }
        DelimitedBlockType::DelimitedStem(_) => {}
    }
}

/// Walk inline content for target IDs and cross-references, including nested inline content.
fn collect_inline_references<'a>(
    state: &mut ParserState<'a>,
    inlines: &[InlineNode<'a>],
    refs: &mut HashMap<&'a str, Reference<'a>>,
    xrefs: &mut Vec<CrossReferenceUse<'a>>,
) {
    for inline in inlines {
        match inline {
            InlineNode::InlineAnchor(anchor) => insert_reference(state, refs, anchor, None, None),
            InlineNode::Macro(InlineMacro::CrossReference(xref)) => {
                xrefs.push(CrossReferenceUse {
                    target: xref.target,
                    location: xref.location.clone(),
                    automatic: xref.text.is_empty(),
                    resolve_natural_target: xref.resolve_natural_target,
                });
                collect_inline_references(state, &xref.text, refs, xrefs);
            }
            InlineNode::Macro(InlineMacro::Footnote(footnote)) => {
                collect_inline_references(state, &footnote.content, refs, xrefs);
            }
            InlineNode::Macro(InlineMacro::Link(link)) => collect_link_references(
                state,
                &link.attributes,
                &link.location,
                &link.text,
                refs,
                xrefs,
            ),
            InlineNode::Macro(InlineMacro::Url(url)) => collect_link_references(
                state,
                &url.attributes,
                &url.location,
                &url.text,
                refs,
                xrefs,
            ),
            InlineNode::Macro(InlineMacro::Mailto(mailto)) => collect_link_references(
                state,
                &mailto.attributes,
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

/// Whether a cross-reference target is an internal id (a bare anchor name) as opposed to
/// an inter-document / external reference. Targets containing a fragment (`#`), path
/// separator (`/`), file extension (`.`), or scheme (`:`) address another resource and
/// are not validated against this document's catalog.
fn is_internal_reference(target: &str) -> bool {
    !target.is_empty() && !target.contains(['#', '.', '/', ':'])
}

fn get_literal_paragraph<'input>(
    state: &ParserState<'input>,
    content: &'input str,
    start: usize,
    end: usize,
    offset: usize,
    block_metadata: &BlockParsingMetadata<'input>,
) -> Block<'input> {
    tracing::debug!(
        content,
        "paragraph starts with a space - switching to literal block"
    );
    let mut metadata = block_metadata.metadata.clone();
    metadata.move_positional_attributes_to_attributes();
    metadata.style = Some("literal");
    let location = state.create_block_location(start, end, offset);

    // Strip leading space from each line ONLY if ALL lines consistently have leading space
    // This matches asciidoctor's behavior
    let all_lines_have_leading_space = content
        .lines()
        .all(|line| line.is_empty() || line.starts_with(' '));

    let content_ref: &'input str = if all_lines_have_leading_space {
        state.intern_join(
            content
                .lines()
                .map(|line| line.strip_prefix(' ').unwrap_or(line)),
            "\n",
        )
    } else {
        content
    };

    tracing::debug!(
        content = content_ref,
        all_lines_have_leading_space,
        "created literal paragraph"
    );
    Block::Paragraph(Paragraph {
        content: vec![InlineNode::PlainText(Plain {
            content: content_ref,
            location: location.clone(),
            escaped: false,
        })],
        metadata,
        title: block_metadata.title.clone(),
        location,
    })
}

/// Assembles principal text from first line and continuation lines. Used by list item
/// parsing rules to combine multi-line content. Produce the principal text for a list
/// item, interned into the arena.
///
/// When there are no continuation lines (the common case), this just returns the borrowed
/// `first_line` unchanged — zero allocation. Otherwise it writes `first_line` followed by
/// each continuation line (separated by `\n`) into a fresh arena string.
fn assemble_principal_text<'a>(
    state: &ParserState<'a>,
    first_line: &'a str,
    continuation_lines: &[&str],
) -> &'a str {
    if continuation_lines.is_empty() {
        first_line
    } else {
        let mut s = bumpalo::collections::String::new_in(state.arena);
        s.push_str(first_line);
        for line in continuation_lines {
            s.push('\n');
            s.push_str(line);
        }
        s.into_bump_str()
    }
}

/// Calculates the end position for a list item based on its principal text.
/// Returns `start` if empty, otherwise one less than `first_line_end`.
const fn calculate_item_end(
    principal_text_is_empty: bool,
    start: usize,
    first_line_end: usize,
) -> usize {
    if principal_text_is_empty {
        start
    } else {
        first_line_end.saturating_sub(1)
    }
}

/// Apply leveloffset to a section level.
///
/// This function combines two sources of leveloffset:
/// 1. Range-based offsets from include directives with `leveloffset=` attribute
/// 2. Document attribute `:leveloffset:` set directly in the document
///
/// The range-based offsets are checked first (based on byte position), and any
/// offset from document attributes is added on top.
///
/// Used by both `section_level` and `section_level_at_line_start` rules.
fn apply_leveloffset(
    base_level: SectionLevel,
    byte_offset: usize,
    leveloffset_ranges: &[LeveloffsetRange],
    document_attributes: &DocumentAttributes,
) -> SectionLevel {
    // Calculate offset from ranges (include directives)
    let range_offset = crate::model::calculate_leveloffset_at(leveloffset_ranges, byte_offset);

    // Get offset from document attributes (inline :leveloffset: settings)
    let attr_offset = document_attributes
        .get_string("leveloffset")
        .and_then(|s| s.parse::<isize>().ok())
        .unwrap_or(0);

    // Combine both offsets
    let total_offset = range_offset + attr_offset;

    if total_offset != 0 {
        let adjusted = isize::from(base_level) + total_offset;
        // Clamp to valid section levels (0-5)
        let clamped = adjusted.clamp(0, 5);
        // Safely converting the clamp ensures the value is in u8 range
        SectionLevel::try_from(clamped)
            .inspect_err(|error| {
                tracing::error!(
                    clamped,
                    ?error,
                    "not a valid section after applying leveloffset"
                );
            })
            .unwrap_or(0)
    } else {
        base_level
    }
}

/// Expected `parent_section_level` (one-based, i.e. own level + 1) for a
/// section's nested content.
///
/// A nestable level-0 special section in a book is rendered at level 1, so its
/// first subsection must be level 2 (`===`). A level-1 (`==`) heading closes the
/// special section instead of nesting under it. Every other section expects
/// children one level deeper than itself.
fn expected_child_level(level: SectionLevel, kind: SectionKind, is_book: bool) -> SectionLevel {
    if level == 0
        && is_book
        && matches!(
            kind,
            SectionKind::Preface | SectionKind::Abstract | SectionKind::Appendix
        )
    {
        2
    } else {
        level + 1
    }
}

fn warn_for_nested_bibliography_section(
    state: &ParserState<'_>,
    direct_parent_section_kind: Option<SectionKind>,
    heading_location: Location,
) {
    if direct_parent_section_kind == Some(SectionKind::Bibliography) {
        let location = state.create_error_source_location(heading_location);
        state.add_warning(crate::Warning::new(
            crate::WarningKind::NestedSectionInBibliography,
            Some(location),
        ));
    }
}

/// How the closing delimiter of a table block was resolved.
///
/// A `Terminated` variant carries the matched close delimiter and its start
/// position so callers can validate symmetry with the open delimiter and
/// record an accurate close-delimiter location. `Unterminated` means the
/// opening delimiter ran to end-of-input without a matching close — the
/// parser still assembles a table (matching asciidoctor's recovery) and
/// emits a warning.
#[derive(Clone, Copy)]
enum TableClosing<'a> {
    Terminated {
        close_delim: &'a str,
        close_start: usize,
    },
    Unterminated,
}

/// Parameters for parsing a table block, passed from delimiter-specific grammar rules
/// to the common parsing helper function.
struct TableParseParams<'a> {
    start: usize,
    offset: usize,
    table_start: usize,
    content_start: usize,
    content_end: usize,
    end: usize,
    open_delim: &'a str,
    content: &'a str,
    default_separator: &'a str,
    closing: TableClosing<'a>,
}

fn table_limit_error(
    state: &ParserState<'_>,
    location: Location,
    resource: &str,
    requested: impl std::fmt::Display,
    limit: usize,
) -> Error {
    Error::Parse(
        Box::new(state.create_error_source_location(location)),
        format!("table {resource} request of {requested} exceeds the maximum of {limit}"),
    )
}

/// Parse a table block from pre-extracted positions and content.
///
/// This helper function contains the common table parsing logic used by all
/// delimiter-specific table rules (pipe, exclamation, comma, colon).
#[allow(clippy::too_many_lines)]
fn parse_table_block_impl<'input>(
    params: &TableParseParams<'_>,
    state: &mut ParserState<'input>,
    block_metadata: &BlockParsingMetadata<'input>,
) -> Result<Block<'input>, Error> {
    let &TableParseParams {
        start,
        offset,
        table_start,
        content_start,
        content_end: _content_end,
        end,
        open_delim,
        content,
        default_separator,
        closing,
    } = params;

    let mut metadata = block_metadata.metadata.clone();
    metadata.move_positional_attributes_to_attributes();
    let presentation = TablePresentation::from_attributes(&metadata, &state.document_attributes);
    let location = state.create_block_location(start, end, offset);
    let table_location = state.create_block_location(table_start, end, offset);
    let open_delimiter_location = state.create_location(
        table_start + offset,
        table_start + offset + open_delim.len().saturating_sub(1),
    );
    let close_delimiter_location = match closing {
        TableClosing::Terminated {
            close_delim,
            close_start,
        } => {
            check_delimiters(
                open_delim,
                close_delim,
                "table",
                state.create_error_source_location(state.create_block_location(start, end, offset)),
            )?;
            Some(state.create_block_location(close_start, end, offset))
        }
        TableClosing::Unterminated => {
            state.add_warning(crate::Warning::new(
                crate::WarningKind::UnterminatedTable {
                    delimiter: open_delim.to_string(),
                },
                Some(state.create_error_source_location(open_delimiter_location.clone())),
            ));
            None
        }
    };

    let separator = if let Some(AttributeValue::String(sep)) =
        block_metadata.metadata.attributes.get("separator")
    {
        sep.to_string()
    } else if let Some(AttributeValue::String(format)) =
        block_metadata.metadata.attributes.get("format")
    {
        match &**format {
            "csv" => ",",
            "dsv" => ":",
            "tsv" => "\t",
            unknown_format => {
                state.add_warning(crate::Warning::new(
                    crate::WarningKind::TableUnknownFormat {
                        format: unknown_format.to_string(),
                    },
                    Some(state.create_error_source_location(table_location.clone())),
                ));
                default_separator
            }
        }
        .to_string()
    } else {
        default_separator.to_string()
    };

    let (ncols, column_formats) = if let Some(AttributeValue::String(cols)) =
        block_metadata.metadata.attributes.get("cols")
    {
        // Parse cols attribute
        // Full syntax: [multiplier*][halign][valign][width][style]
        // Examples: "3*", "^.>2a", "2*>.^1m", "<,^,>", "15%,30%,55%"
        let mut specs = Vec::new();

        for part in cols.split(',') {
            let s = strip_quotes(part.trim());

            // Check for "N*" notation (e.g., "3*" means 3 columns with same spec)
            let (multiplier, spec_str) = if let Some(pos) = s.find('*') {
                let mult_str = &s[..pos];
                let mult = mult_str.parse::<usize>().unwrap_or_else(|_| {
                    if !mult_str.is_empty() && mult_str.bytes().all(|b| b.is_ascii_digit()) {
                        MAX_TABLE_COLUMNS + 1
                    } else {
                        1
                    }
                });
                (mult, &s[pos + 1..])
            } else {
                (1, s)
            };

            let mut halign = crate::HorizontalAlignment::default();
            let mut valign = crate::VerticalAlignment::default();
            let mut width = crate::ColumnWidth::default();
            let mut style = crate::ColumnStyle::default();

            // Parse style (last character if it's a letter: a, d, e, h, l, m, s)
            let spec_str = if let Some(last_char) = spec_str.chars().last() {
                match last_char {
                    'a' => {
                        style = crate::ColumnStyle::AsciiDoc;
                        &spec_str[..spec_str.len() - 1]
                    }
                    'd' => {
                        style = crate::ColumnStyle::Default;
                        &spec_str[..spec_str.len() - 1]
                    }
                    'e' => {
                        style = crate::ColumnStyle::Emphasis;
                        &spec_str[..spec_str.len() - 1]
                    }
                    'h' => {
                        style = crate::ColumnStyle::Header;
                        &spec_str[..spec_str.len() - 1]
                    }
                    'l' => {
                        style = crate::ColumnStyle::Literal;
                        &spec_str[..spec_str.len() - 1]
                    }
                    'm' => {
                        style = crate::ColumnStyle::Monospace;
                        &spec_str[..spec_str.len() - 1]
                    }
                    's' => {
                        style = crate::ColumnStyle::Strong;
                        &spec_str[..spec_str.len() - 1]
                    }
                    _ => spec_str,
                }
            } else {
                spec_str
            };

            // Parse vertical alignment markers: .<, .^, .>
            if spec_str.contains(".<") {
                valign = crate::VerticalAlignment::Top;
            } else if spec_str.contains(".^") {
                valign = crate::VerticalAlignment::Middle;
            } else if spec_str.contains(".>") {
                valign = crate::VerticalAlignment::Bottom;
            }

            // Parse horizontal alignment markers: <, ^, > (not preceded by .)
            for (i, c) in spec_str.char_indices() {
                let prev_char = if i > 0 {
                    spec_str.chars().nth(i - 1)
                } else {
                    None
                };
                if prev_char == Some('.') {
                    continue; // This is a vertical alignment marker
                }
                match c {
                    '<' => halign = crate::HorizontalAlignment::Left,
                    '^' => halign = crate::HorizontalAlignment::Center,
                    '>' => halign = crate::HorizontalAlignment::Right,
                    _ => {}
                }
            }

            // Parse width: integer (proportional), percentage, or ~ (auto)
            // The ~ (tilde) for auto-width was added in Asciidoctor 1.5.7
            // See: https://github.com/asciidoctor/asciidoctor/issues/1844
            // Remove alignment markers to find the width
            let width_str: String = spec_str
                .chars()
                .filter(|c| !matches!(c, '<' | '^' | '>' | '.'))
                .collect();
            if !width_str.is_empty() {
                if width_str == "~" {
                    width = crate::ColumnWidth::Auto;
                } else if width_str.ends_with('%') {
                    if let Ok(pct) = width_str.trim_end_matches('%').parse::<u32>() {
                        width = crate::ColumnWidth::Percentage(pct);
                    }
                } else if let Ok(prop) = width_str.parse::<u32>() {
                    width = crate::ColumnWidth::Proportional(prop);
                }
            }

            // Add the spec for each column in the multiplier (including defaults)
            let spec = crate::ColumnFormat {
                halign,
                valign,
                width,
                style,
            };
            let column_count = specs.len().saturating_add(multiplier);
            if column_count > MAX_TABLE_COLUMNS {
                return Err(table_limit_error(
                    state,
                    table_location.clone(),
                    "column count",
                    column_count.to_string(),
                    MAX_TABLE_COLUMNS,
                ));
            }
            specs.extend(std::iter::repeat_n(spec, multiplier));
        }

        (Some(specs.len()), specs)
    } else {
        (None, Vec::new())
    };

    // Set this to true if the user mandates it!
    let mut has_header = block_metadata.metadata.options.contains(&"header");

    // If we find a partial row that cannot be completed, we're going to drop it.
    // Therefore, we capture the span of the dropped "cells" so that we can provide a nice
    // warning to the user.
    let mut dropped_span = None;
    let raw_rows = Table::parse_rows_with_positions(
        content,
        &separator,
        &mut has_header,
        content_start + offset,
        ncols,
        &mut dropped_span,
    )
    .map_err(|violation| {
        table_limit_error(
            state,
            state.create_location(violation.start, violation.end),
            violation.resource,
            violation.requested,
            violation.limit,
        )
    })?;

    if let Some((start, end)) = dropped_span {
        state.add_warning(crate::Warning::new(
            crate::WarningKind::TableIncompleteRow,
            Some(state.create_error_source_location(state.create_location(start, end))),
        ));
    }

    // If the user forces a `noheader`, we should not have a header, so after we've tried
    // to figure out if there are any headers, we should set it to false one last time.
    if block_metadata.metadata.options.contains(&"noheader") {
        has_header = false;
    }
    let has_footer = block_metadata.metadata.options.contains(&"footer");

    let mut header = None;
    let mut footer = None;
    // `rows` ends up with one entry per raw row (minus header/footer split).
    let mut rows = Vec::with_capacity(raw_rows.len());

    // Track rowspan state: maps column positions to remaining rowspan count.
    // When a cell has rowspan > 1, we track how many more rows it occupies.
    // Each entry: (column_position, remaining_rows, colspan_width)
    let mut active_rowspans: Vec<(usize, usize, usize)> = Vec::new();

    for (i, row) in raw_rows.iter().enumerate() {
        let is_header_row = has_header;
        // Each raw cell produces at least one `columns` entry; duplication
        // produces more but is bounded by the table's column limit.
        let mut columns = Vec::with_capacity(row.len());
        for cell in row {
            let cell_count = if cell.is_duplication {
                cell.duplication_count
            } else {
                1
            };
            for _ in 0..cell_count {
                // Column defaults follow generated cell order; spans do not
                // advance this source-row index in Asciidoctor.
                let column_index = columns.len();
                // Apply column format style if cell doesn't have explicit style
                let effective_cell = if is_header_row {
                    // A semantic header row always uses normal substitutions and
                    // header presentation, regardless of column or cell styles.
                    let mut cell_without_style = cell.clone();
                    cell_without_style.style = None;
                    cell_without_style
                } else if cell.style.is_none()
                    && let Some(col_format) = column_formats.get(column_index)
                    && col_format.style != crate::ColumnStyle::Default
                {
                    let mut cell_with_style = cell.clone();
                    cell_with_style.style = Some(col_format.style);
                    cell_with_style
                } else {
                    cell.clone()
                };

                // Cell content is owned by the ParsedCell; intern into the parser
                // arena so downstream block parsing can borrow at `'input`.
                let cell_content: &'input str = state.intern_str(&effective_cell.content);
                let parsed = parse_table_cell(
                    cell_content,
                    state,
                    effective_cell.content_start,
                    block_metadata.parent_section_level,
                    &effective_cell,
                )?;
                columns.push(parsed);
            }
        }

        // Row location from first cell (falls back to the table location
        // if the row is empty, which shouldn't happen in practice).
        let row_location = if let Some(first) = row.first() {
            state.create_location(first.start, first.end)
        } else {
            table_location.clone()
        };

        // Calculate occupied columns from active rowspans
        let occupied_from_rowspans: usize = active_rowspans
            .iter()
            .map(|(_pos, _remaining, width)| *width)
            .sum();

        // Logical column count = columns occupied by rowspans + colspans of new cells
        let logical_col_count: usize =
            occupied_from_rowspans + columns.iter().map(|c| c.colspan).sum::<usize>();
        if logical_col_count > MAX_TABLE_COLUMNS {
            return Err(table_limit_error(
                state,
                row_location.clone(),
                "column count",
                logical_col_count.to_string(),
                MAX_TABLE_COLUMNS,
            ));
        }

        if let Some(ncols) = ncols
            && logical_col_count != ncols
        {
            // Check if any cell's colspan exceeds the table width
            let has_overflow = columns.iter().any(|c| c.colspan > ncols);
            if has_overflow {
                state.add_warning(crate::Warning::new(
                    crate::WarningKind::TableCellOverflow {
                        actual: logical_col_count,
                        expected: ncols,
                    },
                    Some(state.create_error_source_location(row_location)),
                ));
            } else {
                state.add_warning(crate::Warning::new(
                    crate::WarningKind::TableColumnCount {
                        actual: logical_col_count,
                        expected: ncols,
                        occupied_from_rowspans,
                    },
                    Some(state.create_error_source_location(row_location)),
                ));
            }
            continue;
        }

        // Update active rowspans for this row:
        // 1. Decrement remaining count for existing rowspans
        // 2. Remove rowspans that are now exhausted
        active_rowspans.retain_mut(|(_pos, remaining, _width)| {
            *remaining -= 1;
            *remaining > 0
        });

        // 3. Add new rowspans from current row's cells
        let mut col_position = 0;
        for (_, active_pos, _, colspan) in active_rowspans.iter().map(|(p, r, c)| (*p, *p, *r, *c))
        {
            if col_position == active_pos {
                col_position += colspan;
            }
        }
        for cell in &columns {
            // Skip over positions occupied by rowspans
            while active_rowspans
                .iter()
                .any(|(pos, _, width)| col_position >= *pos && col_position < pos + width)
            {
                if let Some((_, _, width)) = active_rowspans
                    .iter()
                    .find(|(pos, _, w)| col_position >= *pos && col_position < pos + w)
                {
                    col_position += width;
                }
            }
            if cell.rowspan > 1 {
                active_rowspans.push((col_position, cell.rowspan - 1, cell.colspan));
            }
            col_position += cell.colspan;
        }

        // if we have a header, we need to add the columns we have to the header
        if is_header_row {
            header = Some(TableRow { columns });
            has_header = false;
            continue;
        }

        // if we have a footer, we need to add the columns we have to the footer
        if has_footer && i == raw_rows.len() - 1 {
            footer = Some(TableRow { columns });
            continue;
        }

        // if we get here, these columns are a row
        rows.push(TableRow { columns });
    }

    let table = Table::new(rows, table_location.clone())
        .with_header(header)
        .with_footer(footer)
        .with_columns(column_formats)
        .with_presentation(presentation);

    Ok(Block::DelimitedBlock(DelimitedBlock {
        metadata: metadata.clone(),
        delimiter: state.intern_str(open_delim),
        inner: DelimitedBlockType::DelimitedTable(table),
        title: block_metadata.title.clone(),
        location,
        open_delimiter_location: Some(open_delimiter_location),
        close_delimiter_location,
    }))
}

/// Scans `bytes[pos..]` for a description-list marker (`::`, `:::`, `::::`, or
/// `;;`) preceded by at least one term character and followed by EOL, space, or
/// (optionally) end-of-input. Returns `true` on the first complete marker, or
/// `false` if the bound is reached first.
///
/// `scan_across_eol = false` bounds the scan at the next `\n` (line-local).
/// `scan_across_eol = true` bounds it at the next blank line (`\n\n`).
/// `allow_eoi = true` treats end-of-input after the marker as valid context.
///
/// Replaces the per-byte PEG lookahead used by `check_start_of_description_list`,
/// `check_line_is_description_list`, and the inline negation in
/// `description_list_item`'s continuation pattern. The PEG version called
/// `description_list_marker()` (4 string-alts) at every byte, dominating CPU
/// time on macro-heavy paragraphs that don't actually contain dlist markers.
#[inline]
fn find_dlist_marker(bytes: &[u8], pos: usize, scan_across_eol: bool, allow_eoi: bool) -> bool {
    let mut i = pos;
    while let Some(&b) = bytes.get(i) {
        if b == b'\n' && (!scan_across_eol || bytes.get(i + 1) == Some(&b'\n')) {
            return false;
        }
        if i > pos && (b == b':' || b == b';') {
            let marker_len = if b == b':' {
                let mut k = 1;
                while k < 4 && bytes.get(i + k) == Some(&b':') {
                    k += 1;
                }
                if k >= 2 { k } else { 0 }
            } else if bytes.get(i + 1) == Some(&b';') {
                2
            } else {
                0
            };
            if marker_len > 0 {
                let after = bytes.get(i + marker_len).copied();
                if matches!(after, Some(b'\n' | b' ')) || (allow_eoi && after.is_none()) {
                    return true;
                }
            }
        }
        i += 1;
    }
    false
}

fn description_list_location(items: &[DescriptionListItem<'_>]) -> Location {
    let Some((first, rest)) = items.split_first() else {
        return Location::default();
    };
    let last = rest.last().unwrap_or(first);
    Location {
        absolute_start: first.location.absolute_start,
        absolute_end: last.location.absolute_end,
        start: first.location.start.clone(),
        end: last.location.end.clone(),
    }
}

fn nest_description_list_items<'input>(
    items: &mut VecDeque<DescriptionListItem<'input>>,
    delimiter: &'input str,
    ancestors: &mut Vec<&'input str>,
) -> Vec<DescriptionListItem<'input>> {
    let mut nested = Vec::new();

    while items
        .front()
        .is_some_and(|item| item.delimiter == delimiter)
    {
        let Some(mut item) = items.pop_front() else {
            break;
        };

        if let Some(child_delimiter) = items.front().map(|child| child.delimiter)
            && child_delimiter != delimiter
            && !ancestors.contains(&child_delimiter)
        {
            // A new delimiter starts one child level; only an ancestor delimiter unwinds it.
            ancestors.push(delimiter);
            let children = nest_description_list_items(items, child_delimiter, ancestors);
            ancestors.pop();
            let location = description_list_location(&children);
            item.location.absolute_end = location.absolute_end;
            item.location.end = location.end.clone();
            item.description
                .push(Block::DescriptionList(DescriptionList {
                    title: Title::default(),
                    metadata: BlockMetadata::default(),
                    items: children,
                    location,
                }));
        }

        nested.push(item);
    }

    nested
}

fn build_description_list_topology(
    items: Vec<DescriptionListItem<'_>>,
) -> Vec<DescriptionListItem<'_>> {
    if items
        .first()
        .is_none_or(|first| items.iter().all(|item| item.delimiter == first.delimiter))
    {
        return items;
    }

    let mut items = VecDeque::from(items);
    let delimiter = items.front().map_or("", |item| item.delimiter);
    nest_description_list_items(&mut items, delimiter, &mut Vec::new())
}

peg::parser! {
    pub(crate) grammar document_parser(state: &mut ParserState<'input>) for str {
        use std::str::FromStr;
        use crate::model::substitute;

        // Injected span endpoints — `span_start`/`span_end` are bound in every
        // action block to the byte range of the sequence leading up to it.
        // This replaces the boilerplate `start:position!() ... end:position!()`
        // captures that previously wrapped most rules. Pass-through bodies
        // keep this zero-cost; the heavy `offset_to_position` lookup is
        // still done explicitly via `state.create_location` only where a
        // `Location` is actually constructed.
        inject span_start(_input, l, _r) -> usize { l }
        inject span_end(_input, _l, r) -> usize { r }

        // We ignore empty lines before we set the start position of the document because
        // the asciidoc document should not consider empty lines at the beginning or end
        // of the file.
        //
        // We also ignore comments before the header - maybe we should change this but as
        // it stands in our current model, it makes no sense to have comments in the
        // blocks as it is a completely separate part of the document.
        pub(crate) rule document() -> Result<Document<'input>, Error>
        = eol()* start:position() comments_before_header:comment_line_block(0)* header_result:header() prepare_manpage_front_matter() blocks:blocks(0, None, None) end:position!() (eol()* / ![_]) {
            let header = header_result?;
            let mut blocks: Vec<Block<'_>> = comments_before_header.into_iter().collect::<Result<Vec<_>, Error>>()?.into_iter().chain(blocks?).collect();

            // Ensure end offset is on a valid UTF-8 boundary
            let mut document_end_offset = end;
            if document_end_offset > state.input.len() {
                document_end_offset = state.input.len();
            }
            // If not on a boundary, round forward to the next boundary
            while document_end_offset < state.input.len() && !state.input.is_char_boundary(document_end_offset) {
                document_end_offset += 1;
            }
            // Then decrement by one byte to get the last byte of content
            let document_end_offset = if document_end_offset == 0 {
                0
            } else {
                crate::grammar::utf8_utils::safe_decrement_offset(state.input, document_end_offset)
            };

            // Ensure the invariant: absolute_start <= absolute_end
            let (absolute_start, absolute_end) = if start.offset > document_end_offset {
                // This can happen with whitespace-only input where eol()* consumes all content
                // In this case, treat as an empty document at the start position
                (start.offset, start.offset)
            } else {
                (start.offset, document_end_offset)
            };

            // Special case for truly empty input: TCK expects column 0
            // Only for zero-byte input, not whitespace-only
            let (start_position, end_position) = if state.input.is_empty() || (absolute_start == 0 && absolute_end == 0) {
                // Whitespace-only documents should use column 1
                (crate::Position::new(1, 0), crate::Position::new(1, 0))
            } else {
                (
                    start.position,
                    state.line_map.offset_to_position(absolute_end, state.input)
                )
            };

            // Warn when a top-level section skips level 1 (e.g. a document that
            // jumps straight to `=== Heading`). Matches asciidoctor's "section
            // title out of sequence" check.
            //
            // The document root sits at level 0 — so every top-level section is
            // expected at level 1 — once it is "anchored" by a document title or
            // by preamble body content (a paragraph, list, ...) before the first
            // section. When anchored, asciidoctor flags *each* top-level section
            // deeper than level 1 (not just the first). A document that opens
            // directly with a section (no title, no preamble) is not anchored:
            // that first section sets the base level and neither it nor its
            // same-or-shallower siblings are out of sequence. Comments are
            // transparent and never anchor. Sections nested under another section
            // are validated separately, in the `section` rule itself.
            //
            // `toc_entries` is populated while parsing and is empty exactly when
            // the document has no sections — checking it first lets section-less
            // documents skip the body scan entirely. Otherwise we walk only the
            // top-level blocks (preamble + sibling sections, never nested content)
            // and stop at the first section in the un-anchored case.
            if !state.toc_entries.is_empty() {
                let mut anchored = header.as_ref().is_some_and(|h| !h.title.is_empty());
                let mut seen_section = false;
                for block in &blocks {
                    if let Block::Section(section) = block {
                        if !anchored {
                            // Un-anchored leading section: it establishes the base
                            // level, so neither it nor its siblings can be out of
                            // sequence. Nothing left to check.
                            break;
                        }
                        if section.level > 1 {
                            let location = state
                                .create_error_source_location(section.location.clone());
                            state.add_warning(crate::Warning::new(
                                crate::WarningKind::SectionLevelOutOfSequence {
                                    expected: 1,
                                    got: section.level,
                                },
                                Some(location),
                            ));
                        }
                        seen_section = true;
                    } else if !seen_section && !matches!(block, Block::Comment(_)) {
                        // Preamble content before the first section anchors the
                        // document at level 0.
                        anchored = true;
                    }
                }
            }

            // Assign caption ordinals over the finished tree. Numbering here cannot be
            // disturbed by PEG backtracking, and it runs before the reference catalog so that
            // catalog can later carry a target's caption label and ordinal.
            normalize_bibliography_lists(state, &mut blocks);
            caption::renumber_captions(&mut blocks);

            section::number_parsed_sections(
                &mut blocks,
                &mut state.toc_entries,
                is_book_doctype(&state.document_attributes),
            );

            // Build the id -> reference catalog for O(1) `<<id>>` resolution:
            // sections (already collected as toc_entries) plus a single walk over
            // the final tree for every other anchor (block IDs, inline `[[id]]`
            // anchors, and formatted span IDs). The same walk collects every
            // cross-reference so unresolved ones can be reported.
            let toc_entries = state.toc_entries.clone();
            let header_has_anchor = header.as_ref().is_some_and(|header| {
                header.metadata.id.is_some() || !header.metadata.anchors.is_empty()
            });
            let mut references: HashMap<&str, Reference<'_>> =
                HashMap::with_capacity(toc_entries.len() + usize::from(header_has_anchor));
            let mut xrefs = Vec::new();
            if let Some(header) = &header {
                if let Some(anchor) = header
                    .metadata
                    .id
                    .as_ref()
                    .or_else(|| header.metadata.anchors.last())
                {
                    insert_reference(
                        state,
                        &mut references,
                        anchor,
                        Some(header_reference_title(header)),
                        None,
                    );
                }
                collect_inline_references(
                    state,
                    header.title.as_ref(),
                    &mut references,
                    &mut xrefs,
                );
                if let Some(subtitle) = &header.subtitle {
                    collect_inline_references(
                        state,
                        subtitle.as_ref(),
                        &mut references,
                        &mut xrefs,
                    );
                }
                collect_metadata_references(
                    state,
                    &header.metadata,
                    &mut references,
                    &mut xrefs,
                );
            }
            for entry in &toc_entries {
                let xreflabel = parse_reference_label(state, entry.xreflabel, &entry.location);
                references.insert(
                    entry.id,
                    Reference {
                        xreflabel,
                        title: Some(entry.title.clone()),
                        location: entry.location.clone(),
                        caption: None,
                        bibliography: false,
                        automatic_citation: false,
                    },
                );
            }
            collect_references(state, &blocks, &mut references, &mut xrefs);

            let mut document = Document {
                header,
                // Built in preprocessed coordinates with no `file` on either boundary;
                // the post-parse remap then applies the per-boundary `file` model like
                // any node — a boundary in primary content stays `file: None`, one whose
                // content came from an `include::` gets that include chain. So a document
                // whose last block is a nested include legitimately ends in that file
                // (its start, in the primary, stays `None`). Matches the ASG's
                // per-`locationBoundary` `file` semantics.
                location: Location {
                    absolute_start,
                    absolute_end,
                    start: start_position,
                    end: end_position,
                },
                attributes: DocumentAttributes::clone(&state.document_attributes),
                blocks,
                footnotes: state.footnote_tracker.borrow().footnotes.clone(),
                toc_entries,
                references,
            };
            let reference_ids = document.references.keys().copied().collect::<HashSet<_>>();

            // An internal `<<id>>` whose target is absent from the catalog is an
            // unresolved (broken) reference. Inter-document/external targets
            // (those addressing another resource) are not validated here.
            for xref in xrefs {
                let target = resolve_xref_target(
                    xref.target,
                    xref.resolve_natural_target,
                    &reference_ids,
                    &state.natural_xref_targets,
                );
                if xref.automatic
                    && let Some(reference) = document.references.get_mut(target)
                    && reference.is_bibliography()
                {
                    reference.automatic_citation = true;
                }
                if is_internal_reference(target) && !reference_ids.contains(target) {
                    let source_location = state.create_error_source_location(xref.location);
                    state.add_warning(crate::Warning::new(
                        crate::WarningKind::UnresolvedReference {
                            target: target.to_string(),
                        },
                        Some(source_location),
                    ));
                }
            }
            finalize_cross_references(
                state,
                &mut document,
                &reference_ids,
                &state.natural_xref_targets,
            );
            Ok(document)
        }

        rule prepare_manpage_front_matter()
        = manpage_name_section_required() section:&manpage_name_section() {
            prepare_manpage_name_attributes(state, Some(section));
        }
        / {
            prepare_manpage_name_attributes(state, None);
        }

        rule manpage_name_section_required()
        = {?
            if is_manpage_doctype(&state.document_attributes)
                && !(state.document_attributes.get_string("manname").is_some()
                    && state.document_attributes.get_string("manpurpose").is_some())
            {
                Ok(())
            } else {
                Err("manpage name section is not required")
            }
        }

        // The first manpage section supplies header attributes. Inspect it without
        // consuming it so the regular block grammar still builds the AST.
        rule manpage_name_section() -> ManpageNameSection<'input>
        = eol()*
          metadata_attributes:(attribute:manpage_name_metadata() eol()* { attribute })*
          title:manpage_level_one_title()
          eol()*
          lines:manpage_name_body_line()+
        {?
            derive_name_section_attrs(lines)
                .map(|attributes| ManpageNameSection {
                    title,
                    attributes,
                    metadata_attributes: metadata_attributes.into_iter().flatten().collect(),
                })
                .ok_or("non-conforming manpage name section body")
        }

        rule manpage_level_one_title() -> &'input str
        = level:section_level(0, None) whitespace()+ title:$([^'\n']+) (eol() / ![_]) {?
            (level.1 == 1)
                .then_some(title.trim())
                .ok_or("not a level-one manpage name section")
        }
        / title:$([^'\n']+) eol()
          level:setext_section_level(title.trim().chars().count(), None) {?
            (level == 1 && !title_looks_like_description_list(title))
                .then_some(title.trim())
                .ok_or("not a level-one Setext manpage name section")
        }

        rule manpage_name_metadata() -> Option<AttributeEntry<'input>>
        = manpage_comment_block() { None }
        / "//" !"/" [^'\n']* (eol() / ![_]) { None }
        / "[[" (!"]]" [^'\n'])+ "]]" (eol() / ![_]) { None }
        / !empty_list_separator() !double_open_square_bracket()
          open_square_bracket() attribute_list_content() (eol() / ![_]) { None }
        / "." ![' ' | '\t' | '\n' | '\r' | '.'] [^'\n']* (eol() / ![_]) { None }
        / attribute:document_attribute_match() (eol() / ![_]) { Some(attribute) }

        rule manpage_comment_block()
        = delimiter:$(['/']*<4,>) eol()
          (!manpage_comment_delimiter(delimiter) [^'\n']* eol())*
          manpage_comment_delimiter(delimiter)

        rule manpage_comment_delimiter(delimiter: &str)
        = candidate:$(['/']*<4,>) (eol() / ![_]) {?
            (candidate == delimiter)
                .then_some(())
                .ok_or("not the matching comment delimiter")
        }

        rule manpage_name_body_line() -> Option<&'input str>
        = "//" !"/" [^'\n']* (eol() / ![_]) { None }
        / line:$([^'\n']+) (eol() / ![_]) { Some(line) }

        pub(crate) rule header() -> Result<Option<Header<'input>>, Error>
            = start:position!()
            ((document_attribute() / comment()) (eol()+ / ![_]))*
            // Parse header metadata (anchors and attributes) before the document title
            metadata:header_metadata()
            title_authors:(title_authors:title_authors() { title_authors })?
            (eol()+ (document_attribute() / comment()))*
            end:position!()
            (eol()*<,2> / ![_])
        {
            if let Some((title, subtitle, authors)) = title_authors {
                let mut location = state.create_location(start, end);
                // Decrement end by one character (for byte offset, use safe UTF-8 decrement)
                location.absolute_end = crate::grammar::utf8_utils::safe_decrement_offset(state.input, location.absolute_end);
                location.end.column = location.end.column.saturating_sub(1);
                let mut header = Header {
                    metadata,
                    title,
                    subtitle,
                    authors,
                    location,
                };

                // Derive author attributes bidirectionally
                derive_author_attrs(
                    state.arena,
                    &mut header,
                    Rc::make_mut(&mut state.document_attributes),
                );

                // Derive manpage attributes from header if doctype=manpage
                // This must happen during parsing so {mantitle} etc. work in body
                if is_manpage_doctype(&state.document_attributes) {
                    derive_manpage_header_attrs(
                        Some(&header),
                        Rc::make_mut(&mut state.document_attributes),
                        state.options.strict,
                        state.current_file.as_deref().map(std::path::PathBuf::as_path),
                    )?;
                }

                Ok(Some(header))
            } else {
                tracing::debug!("No title or authors found in the document header.");
                Ok(None)
            }
        }

        /// Parse block metadata lines (anchors and attributes) that can appear before a document title.
        /// Only consumes metadata if followed by a document title to avoid stealing attributes
        /// meant for the first block when there's no document title.
        rule header_metadata() -> BlockMetadata<'input>
            = lines:(
                anchor:anchor() { AttributeOrAnchorLine::Anchor(anchor) }
                / attr:attributes_line() { AttributeOrAnchorLine::Attributes((attr.0, Box::new(attr.1))) }
            )+ &document_title()
            {
                let mut metadata = BlockMetadata::default();

                for line in lines {
                    match line {
                        AttributeOrAnchorLine::Anchor(anchor) => metadata.anchors.push(anchor),
                        AttributeOrAnchorLine::Attributes((_, attr_metadata)) => {
                            let attr_metadata = *attr_metadata;
                            // Merge attribute metadata - last one wins for id/style
                            if attr_metadata.id.is_some() {
                                metadata.id = attr_metadata.id;
                            }
                            if attr_metadata.style.is_some() {
                                metadata.style = attr_metadata.style;
                            }
                            metadata.roles.extend(attr_metadata.roles);
                            metadata.options.extend(attr_metadata.options);
                            metadata.attributes = attr_metadata.attributes;
                            metadata.positional_attributes = attr_metadata.positional_attributes;
                        }
                    }
                }
                metadata
            }
            / { BlockMetadata::default() }

        pub(crate) rule title_authors() -> (Title<'input>, Option<Subtitle<'input>>, Vec<Author<'input>>)
        // Comment lines between the title and the author line are skipped (matching
        // `asciidoctor`). The first non-comment line is the author line only when it is
        // not an attribute entry (`:name:`). An attribute entry means there is no author
        // and it belongs to the header body.
        = title_and_subtitle:document_title() eol() (comment() eol())* !document_attribute_match() !comment() authors:authors_and_revision() &(eol()+ / ![_])
        {
            let (title, subtitle) = title_and_subtitle;
            tracing::debug!(?title, ?subtitle, ?authors, "Found title and authors in the document header.");
            (title, subtitle, authors)
        }
        / title_and_subtitle:document_title() &(eol() / ![_]) {
            let (title, subtitle) = title_and_subtitle;
            tracing::debug!(?title, ?subtitle, "Found title in the document header without authors.");
            (title, subtitle, vec![])
        }

        pub(crate) rule document_title() -> (Title<'input>, Option<Subtitle<'input>>)
        = document_title_atx()
        / document_title_setext()

        /// ATX-style document title: `= Title` or `# Title`
        rule document_title_atx() -> (Title<'input>, Option<Subtitle<'input>>)
        = document_title_token() whitespace() start:position!() title:$([^'\n']*) end:position!()
        {?
            tracing::debug!(?title, "Processing ATX document title");
            let block_metadata = BlockParsingMetadata::default();

            let (title_inlines, subtitle) = if let Some(colon_pos) = title.rfind(": ") {
                let subtitle_raw = &title[colon_pos + 1..];
                let subtitle_text = subtitle_raw.trim();
                if subtitle_text.is_empty() {
                    // Empty subtitle after colon, treat whole text as title
                    let (inlines, _) = process_inlines(state, &block_metadata, start, end, 0, title)
                        .map_err(|_| "could not process document title")?;
                    (inlines, None)
                } else {
                    // Title: trim trailing whitespace before colon
                    let title_raw = &title[..colon_pos];
                    let title_text = title_raw.trim_end();
                    let title_end = start + title_text.len();
                    let (inlines, _) = process_inlines(state, &block_metadata, start, title_end, 0, title_text)
                        .map_err(|_| "could not process document title")?;

                    // Subtitle: trim leading whitespace after colon
                    let sub_leading = subtitle_raw.len() - subtitle_raw.trim_start().len();
                    let sub_start_offset = start + colon_pos + 1 + sub_leading;
                    let subtitle_start = PositionWithOffset {
                        offset: sub_start_offset,
                        position: state.line_map.offset_to_position(sub_start_offset, state.input),
                    };
                    let sub_end = sub_start_offset + subtitle_text.len();
                    let (subtitle_inlines, _) = process_inlines(state, &block_metadata, subtitle_start.offset, sub_end, 0, subtitle_text)
                        .map_err(|_| "could not process document subtitle")?;

                    (inlines, Some(Subtitle::new(subtitle_inlines)))
                }
            } else {
                let (inlines, _) = process_inlines(state, &block_metadata, start, end, 0, title)
                    .map_err(|_| "could not process document title")?;
                (inlines, None)
            };

            Ok((Title::new(title_inlines), subtitle))
        }

        /// Setext-style document title: Title underlined with `=` characters
        ///
        /// ```text
        /// Document Title
        /// ==============
        /// ```
        ///
        /// The underline must be within ±2 characters of the title width.
        /// Only enabled when the setext feature is compiled in AND the runtime
        /// option is enabled.
        rule document_title_setext() -> (Title<'input>, Option<Subtitle<'input>>)
        = title:$([^'\n']+) end:position!() eol()
          underline:$("="+) &(eol() / ![_])
        {?
            // Check if setext mode is enabled
            if !setext::is_enabled(state) {
                return Err("setext mode not enabled");
            }

            let title_text = title.trim();
            let title_width = title_text.chars().count();
            let underline_width = underline.chars().count();

            // Check underline width tolerance (±2 characters)
            if !setext::width_ok(title_width, underline_width) {
                return Err("underline width out of tolerance");
            }

            // Check underline is level 0 (document title uses =)
            if !underline.starts_with('=') {
                return Err("document title must use = underline");
            }

            tracing::debug!(?title_text, "Processing setext document title");
            let block_metadata = BlockParsingMetadata::default();

            let (title_inlines, subtitle) = if let Some(colon_pos) = title.rfind(": ") {
                let subtitle_raw = &title[colon_pos + 1..];
                let subtitle_text = subtitle_raw.trim();
                if subtitle_text.is_empty() {
                    let (inlines, _) = process_inlines(state, &block_metadata, span_start, end, 0, title)
                        .map_err(|_| "could not process setext document title")?;
                    (inlines, None)
                } else {
                    // Title: trim trailing whitespace before colon
                    let title_raw = &title[..colon_pos];
                    let title_text = title_raw.trim_end();
                    let title_end = span_start + title_text.len();
                    let (inlines, _) = process_inlines(state, &block_metadata, span_start, title_end, 0, title_text)
                        .map_err(|_| "could not process setext document title")?;

                    // Subtitle: trim leading whitespace after colon
                    let sub_leading = subtitle_raw.len() - subtitle_raw.trim_start().len();
                    let sub_start_offset = span_start + colon_pos + 1 + sub_leading;
                    let subtitle_start = PositionWithOffset {
                        offset: sub_start_offset,
                        position: state.line_map.offset_to_position(sub_start_offset, state.input),
                    };
                    let sub_end = sub_start_offset + subtitle_text.len();
                    let (subtitle_inlines, _) = process_inlines(state, &block_metadata, subtitle_start.offset, sub_end, 0, subtitle_text)
                        .map_err(|_| "could not process setext document subtitle")?;

                    (inlines, Some(Subtitle::new(subtitle_inlines)))
                }
            } else {
                let (inlines, _) = process_inlines(state, &block_metadata, span_start, end, 0, title)
                    .map_err(|_| "could not process setext document title")?;
                (inlines, None)
            };

            Ok((Title::new(title_inlines), subtitle))
        }

        rule document_title_token() = "=" / "#"

        rule authors_and_revision() -> Vec<Author<'input>>
            // Capture the author line, substitute any attribute references, then parse
            = start:position!() author_line:$([^'\n']+) end:position!() (eol() (comment() eol())* revision_pre_substitution())? {?
                let substituted_cow = substitute(author_line.trim(), HEADER, &state.document_attributes);
                // Intern any owned substitution result so the downstream
                // `authors()` parse can yield `Author<'input>` that outlives
                // this action block.
                let substituted: &'input str = match substituted_cow {
                    Cow::Borrowed(s) => s,
                    Cow::Owned(s) => state.intern_str(&s),
                };
                tracing::debug!(?author_line, ?substituted, "Processing author line with substitution");

                // Parse the substituted content as authors
                let mut temp_state =
                    ParserState::for_inline_parsing(substituted, state, state.inline_ctx);

                // `asciidoctor` always consumes the line after the title as the author
                // line; when it doesn't parse as structured "firstname [middle] [last]
                // [<email>]" authors (e.g. it contains parentheses, commas, or an
                // "Author:" prefix), the whole line becomes a single author's full name.
                if let Ok(authors) = document_parser::authors(substituted, &mut temp_state) {
                    tracing::debug!(?authors, "Parsed authors from line");
                    Ok(authors)
                } else {
                    tracing::debug!(?substituted, "Author line did not parse structurally; using whole line as a single author");
                    let location = state.create_error_source_location(state.create_location(start, end));
                    state.add_warning(crate::Warning::new(
                        crate::WarningKind::NonStandardAuthorLine { line: substituted.to_string() },
                        Some(location),
                    ));
                    Ok(vec![Author::new(state.arena, substituted, None, None)])
                }
            }

        pub(crate) rule authors() -> Vec<Author<'input>>
            = authors:(author() ++ (";" whitespace()*)) {
                authors
            }

        /// Parse an author in various formats:
        /// - "First Middle Last <email>"
        /// - "First Last <email>"
        /// - "First <email>"
        /// - "First Last"
        pub(crate) rule author() -> Author<'input>
            = name:author_name() email:author_email()? {
                let mut author = name;
                if let Some(email_addr) = email {
                    author.email = Some(email_addr);
                }
                author
            }

        /// Parse author name in format: "First [Middle] Last" or just "First"
        rule author_name() -> Author<'input>
        = first:name_part() whitespace()+ middle:name_part() whitespace()+ last:$(name_part() ++ whitespace()) {
            Author::new(state.arena, first, Some(middle), Some(last))
        }
        / first:name_part() whitespace()+ last:name_part() {
            Author::new(state.arena, first, None, Some(last))
        }
        / first:name_part() {
            Author::new(state.arena, first, None, None)
        }

        /// Parse email address in format: " <email@domain>"
        rule author_email() -> &'input str
            = whitespace()* "<" email:$([^'>']*) ">" { email }

        rule name_part() -> &'input str
            = name:$([c if c.is_alphanumeric() || c == '.' || c == '-' || c == '\'']+ ("_" [c if c.is_alphanumeric() || c == '.' || c == '-' || c == '\'']+)*) {
                name
            }

        pub(crate) rule revision() -> ()
            = "v"? number:$(digits() ++ ".") date:revision_date()? remark:revision_remark()? {
                let revision_info = RevisionInfo {
                    number: Cow::Owned(number.to_string()),
                    date: date.map(|d| Cow::Owned(d.to_string())),
                    remark: remark.map(|r| Cow::Owned(r.to_string())),
                };
                if revision_info.number.is_empty() {
                    // No revision number found, nothing to do
                    return;
                }
                let revision_location = state.create_location(span_start, span_end);
                let ignored: IgnoredRevisionFields = {
                    let document_attributes = Rc::make_mut(&mut state.document_attributes);
                    process_revision_info(revision_info, document_attributes)
                };
                if ignored.number {
                    state.add_generic_warning_at(
                        "Revision number found in revision line but ignoring due to being set through attribute entries.".to_string(),
                        revision_location.clone(),
                    );
                }
                if ignored.date {
                    state.add_generic_warning_at(
                        "Revision date found in revision line but ignoring due to being set through attribute entries.".to_string(),
                        revision_location.clone(),
                    );
                }
                if ignored.remark {
                    state.add_generic_warning_at(
                        "Revision remark found in revision line but ignoring due to being set through attribute entries.".to_string(),
                        revision_location,
                    );
                }
            }

        /// Parse revision line with attribute reference support
        rule revision_pre_substitution() -> ()
            // Capture the revision line, substitute any attribute references, then parse
            = rev_line:$([^'\n']+) {?
                let substituted_cow = substitute(rev_line.trim(), HEADER, &state.document_attributes);
                let substituted: &'input str = match substituted_cow {
                    Cow::Borrowed(s) => s,
                    Cow::Owned(s) => state.intern_str(&s),
                };
                tracing::debug!(?rev_line, ?substituted, "Processing revision line with substitution");

                // Parse the substituted content as revision
                let mut temp_state =
                    ParserState::for_inline_parsing(substituted, state, state.inline_ctx);

                match document_parser::revision(substituted, &mut temp_state) {
                    Ok(()) => {
                        // Copy revision attributes from temp_state back to main state
                        for key in ["revnumber", "revdate", "revremark"] {
                            if let Some(value) = temp_state.document_attributes.get(key) {
                                Rc::make_mut(&mut state.document_attributes).insert(key.into(), value.clone());
                            }
                        }
                        tracing::debug!("Parsed revision from line");
                        Ok(())
                    }
                    Err(_) => Err("line did not parse as revision")
                }
            }

        rule revision_date() -> &'input str
            = ", " date:$([^ (':'|'\n')]+) {
                date
            }

        rule revision_remark() -> &'input str
            = ": " remark:$([^'\n']+) {
                remark
            }

        rule document_attribute() -> ()
        = att:document_attribute_match() (&eol() / ![_])
        {
            let AttributeEntry{key, value, set} = att;
            tracing::debug!(%set, %key, %value, "Found document attribute in the document header");
            state.apply_document_attribute(key.into(), value, set, true);
        }

        pub(crate) rule blocks(offset: usize, parent_section_level: Option<SectionLevel>, direct_parent_section_kind: Option<SectionKind>) -> Result<Vec<Block<'input>>, Error>
        = blocks:block(offset, parent_section_level, direct_parent_section_kind)*
        {
            let mut blocks = blocks.into_iter().collect::<Result<Vec<_>, Error>>()?;
            // A trusted attribute declared in document content is consumed as syntax
            // but has no semantic node in asciidoctor's AST.
            blocks.retain(|block| {
                !matches!(
                    block,
                    Block::DocumentAttribute(attribute)
                        if crate::constants::is_builtin_attribute_protected(&attribute.name)
                )
            });
            Ok(blocks)
        }

        /// Blocks for table cells without `AsciiDoc` style - excludes block types that require full parsing.
        /// Table cells use a simplified block parser that excludes sections, document attributes,
        /// and block types like lists, delimited blocks, toc, page breaks, and markdown blockquotes.
        pub(crate) rule blocks_for_table_cell(offset: usize, parent_section_level: Option<SectionLevel>) -> Result<Vec<Block<'input>>, Error>
        = eol()*
        blocks:(
            comment_line_block(offset) /
            block_generic_for_table_cell(offset, parent_section_level)
        )*
        {
            blocks.into_iter().collect::<Result<Vec<_>, Error>>()
        }

        pub(crate) rule block(offset: usize, parent_section_level: Option<SectionLevel>, direct_parent_section_kind: Option<SectionKind>) -> Result<Block<'input>, Error>
        = eol()*
        // First check: if we're at a same-or-higher-level section, fail the entire block
        // This prevents section content from consuming sibling/parent sections as paragraphs
        !same_or_higher_level_section(offset, parent_section_level)
        block:(
            comment_line_block(offset) /
            document_attribute_block(offset) /
            // A discrete heading is introduced by an attribute line (`[discrete]`,
            // `[#id,discrete]`, `[float]`, …) or an anchor preceding one, so only
            // attempt it when the block starts with `[`. The rule itself backtracks
            // to `section`/`block_generic` when the metadata isn't a discrete marker.
            &"[" dh:discrete_header(offset) { dh } /
            section:section(offset, parent_section_level, direct_parent_section_kind) { section } /
            // Try setext-style sections (only enabled with setext feature + runtime flag)
            section_setext:section_setext(offset, parent_section_level, direct_parent_section_kind) { section_setext } /
            block_generic(offset, parent_section_level)
        )
        { block }

        /// Single-line comment that becomes a block in the AST.
        /// Line comments begin with `//` (but not `///` or `////` which are block comment delimiters).
        rule comment_line_block(offset: usize) -> Result<Block<'input>, Error>
        = "//" !("/") content:$([^'\n']*) end:position!() (eol() / ![_])
        {
            // `end` is captured before consuming the trailing newline so the
            // comment's location doesn't include it.
            Ok(Block::Comment(Comment {
                kind: CommentKind::Line,
                content,
                location: state.create_location(span_start + offset, end + offset),
            }))
        }

        /// Like `comment_line_block` but leaves the trailing newline unconsumed
        /// (lookahead instead of consume). Used in list continuations so that a
        /// `+` continuation following the comment can still match, since
        /// continuation markers expect a leading newline before the `+`.
        rule comment_line_block_keep_eol(offset: usize) -> Result<Block<'input>, Error>
        = "//" !("/") content:$([^'\n']*) end:position!() &(eol() / ![_])
        {
            Ok(Block::Comment(Comment {
                kind: CommentKind::Line,
                content,
                location: state.create_location(span_start + offset, end + offset),
            }))
        }

        // Check if the upcoming content is a section at same or higher level (which
        // should not be parsed as content)
        //
        // This rule skips optional metadata (anchors, attributes, etc.) before checking
        // the section level, so that `[[anchor]]\n== Section` is correctly identified as
        // a sibling section.
        //
        // Checks both ATX-style (= or #) and setext-style (underlined) sections.
        rule same_or_higher_level_section(offset: usize, parent_section_level: Option<SectionLevel>) -> ()
        = (anchor() / attributes_line() / document_attribute_line() / title_line(offset))*
          (
            // ATX-style section check - require space after marker to avoid matching
            // description list items like `#term::` as sections
            level:section_level(offset, parent_section_level) &" "
            {?
                if let Some(parent_level) = parent_section_level {
                    let upcoming_level = level.1 + 1; // Convert to 1-based
                    if upcoming_level <= parent_level {
                        Ok(()) // This IS a same or higher level section
                    } else {
                        Err("not a same or higher level section")
                    }
                } else {
                    Err("no parent section level to compare")
                }
            }
            /
            // Setext-style section check (title followed by underline)
            &setext_section_lookahead(parent_section_level)
          )

        /// Lookahead rule to detect setext sections at same or higher level.
        /// Used by same_or_higher_level_section to properly terminate sections.
        /// Excludes description list items (e.g., `term:: content`) which would otherwise
        /// match as setext titles.
        rule setext_section_lookahead(parent_section_level: Option<SectionLevel>) -> ()
        = title:$([^'\n']+) eol() underline:$(['-' | '~' | '^' | '+']+) &(eol() / ![_])
        {?
            // Exclude description list items
            if title_looks_like_description_list(title) {
                return Err("title looks like a description list item");
            }
            // Only check if setext mode is enabled
            if !setext::is_enabled(state) {
                return Err("setext mode not enabled");
            }

            // Validate underline width
            let title_width = title.trim().chars().count();
            let underline_width = underline.chars().count();
            if !setext::width_ok(title_width, underline_width) {
                return Err("underline width out of tolerance");
            }

            // Get level from underline character
            let underline_char = underline.chars().next().ok_or("empty underline")?;
            let level = setext::char_to_level(underline_char).ok_or("invalid setext char")?;

            // Level 0 (=) is document title, not section — unless doctype is book (parts)
            if level == 0 && !is_book_doctype(&state.document_attributes) {
                return Err("not a section, seems like you're trying to define a document title");
            }

            // Check if this is a same-or-higher level section
            if let Some(parent_level) = parent_section_level {
                if level < parent_level {
                    Ok(()) // This IS a same or higher level setext section
                } else {
                    Err("not a same or higher level section")
                }
            } else {
                Err("no parent section level to compare")
            }
        }

        rule discrete_header(offset: usize) -> Result<Block<'input>, Error>
        = block_metadata:(bm:block_metadata(offset, None) {?
            let bm = bm.map_err(|e| {
                tracing::error!(?e, "error parsing block metadata in discrete_header");
                "block metadata parse error"
            })?;
            // Backtrack to the regular `section` rule unless the attribute line
            // actually marks this as a discrete heading; a discrete heading is
            // exempt from section-level sequencing, so it must not reach `section`.
            if !bm.discrete {
                return Err("not a discrete heading");
            }
            Ok(bm)
        })
        section_level:section_level(offset, None) whitespace()
        title_start:position!() title:section_title(offset, &block_metadata) title_end:position!() &(eol()*<1,2> / ![_])
        {
            let (title, _) = title?;
            tracing::debug!(?block_metadata, ?title, ?title_start, ?title_end, "parsing discrete header block");

            let level = section_level.1;
            // `span_end` lands at title_end here because the trailing `&(...)` is a
            // zero-width lookahead.
            let location = state.create_block_location(span_start, span_end, offset);

            // `float` is a legacy alias for the `discrete` block style (older
            // AsciiDoc called these "floating titles"). Surface its use so authors
            // can migrate to `discrete`. Only the style form reaches this rule.
            if block_metadata.metadata.style == Some("float") {
                let warning_location = state.create_error_source_location(
                    state.create_block_location(span_start, span_end, offset),
                );
                state.add_warning(crate::Warning::new(
                    crate::WarningKind::LegacyFloatDiscreteHeading,
                    Some(warning_location),
                ));
            }

            Ok(Block::DiscreteHeader(DiscreteHeader {
                metadata: block_metadata.metadata,
                title,
                level,
                location,
            }))
        }

        pub(crate) rule document_attribute_block(offset: usize) -> Result<Block<'input>, Error>
        = att:document_attribute_match()
        {
            let AttributeEntry{ key, value, set } = att;
            let value = state.apply_document_attribute(key.into(), value, set, false);
            Ok(Block::DocumentAttribute(DocumentAttribute {
                name: key.into(),
                value,
                location: state.create_location(span_start+offset, span_end+offset)
            }))
        }

        pub(crate) rule section(offset: usize, parent_section_level: Option<SectionLevel>, direct_parent_section_kind: Option<SectionKind>) -> Result<Block<'input>, Error>
        = block_metadata:(bm:block_metadata(offset, parent_section_level) {?
            bm.map_err(|e| {
                tracing::error!(?e, "error parsing block metadata in section");
                "block metadata parse error"
            })
        })
        section_level_start:position!()
        section_level:section_level(offset, parent_section_level)
        section_level_end:position!()
        whitespace()
        title_start:position!()
        section_header:(title:section_title(offset, &block_metadata) title_end:position!() &(eol()*<1,2> / ![_]) {
            let (title, natural_title) = title?;
            let location = state.create_block_location(section_level_start, title_end, offset);
            Ok::<(Title<'input>, section::SectionNumbering), Error>(register_section_header(
                state,
                &block_metadata,
                title,
                natural_title,
                section_level.1,
                location,
                direct_parent_section_kind,
            ))
        })
        content:section_content(offset, Some(expected_child_level(
            section_level.1,
            SectionKind::from_style(block_metadata.metadata.style),
            is_book_doctype(&state.document_attributes),
        )), Some(SectionKind::from_style(block_metadata.metadata.style)))?
        {
            let (title, numbering) = section_header?;
            tracing::debug!(?offset, ?block_metadata, ?title, "parsing section block");

            // Validate section level against parent section level if any is provided.
            if let Some(parent_level) = parent_section_level {
                if section_level.1 < parent_level || section_level.1 > 5 {
                    return Err(Error::NestedSectionLevelMismatch(
                        Box::new(state.create_error_source_location(state.create_block_location(section_level_start, section_level_end, offset))),
                        section_level.1+1,
                        parent_level + 1,
                    ));
                }
                // A section that skips a level (deeper than one below its parent)
                // is "out of sequence". asciidoctor warns but still renders it at
                // its literal level rather than aborting, so we do the same.
                if section_level.1 > parent_level {
                    let location = state.create_error_source_location(
                        state.create_block_location(section_level_start, section_level_end, offset),
                    );
                    state.add_warning(crate::Warning::new(
                        crate::WarningKind::SectionLevelOutOfSequence {
                            expected: parent_level,
                            got: section_level.1,
                        },
                        Some(location),
                    ));
                }
            }

            let level = section_level.1;
            let location = state.create_block_location(span_start, span_end, offset);

            // Classify the section before the post-parse numbering pass applies
            // special-section rules to the complete section tree.
            let kind = SectionKind::from_style(block_metadata.metadata.style);

            Ok(Block::Section(Section::parsed(
                block_metadata.metadata,
                title,
                level,
                content.unwrap_or(Ok(Vec::new()))?,
                kind,
                numbering,
                location,
            )))
        }

        /// Setext-style section header: Title underlined with `-`, `~`, `^`, or `+`
        ///
        /// ```text
        /// Section Title
        /// -------------
        /// ```
        ///
        /// The underline character determines the section level:
        /// - `-` = Level 1
        /// - `~` = Level 2
        /// - `^` = Level 3
        /// - `+` = Level 4
        ///
        /// The underline must be within ±2 characters of the title width.
        /// Only enabled when the setext feature is compiled in AND the runtime
        /// option is enabled.
        /// Parse a setext section level from the underline character.
        /// Returns the level (1-4) corresponding to -, ~, ^, +
        rule setext_section_level(title_width: usize, parent_section_level: Option<SectionLevel>) -> u8
        = underline:$(['-' | '~' | '^' | '+']+) &(eol() / ![_])
        {?
            // Check if setext mode is enabled
            if !setext::is_enabled(state) {
                return Err("setext mode not enabled");
            }

            let underline_width = underline.chars().count();

            // Check underline width tolerance (±2 characters)
            if !setext::width_ok(title_width, underline_width) {
                return Err("underline width out of tolerance");
            }

            // Get the underline character and determine section level
            let underline_char = underline.chars().next().ok_or("empty underline")?;
            let level = setext::char_to_level(underline_char).ok_or("invalid setext underline character")?;

            // Document title (level 0) uses =, not allowed here — unless doctype is book (parts)
            if level == 0 && !is_book_doctype(&state.document_attributes) {
                return Err("use = underline for document title, not section");
            }

            // Validate section level against parent section level if any is provided
            if let Some(parent_level) = parent_section_level
                && (level < parent_level || level > parent_level + 1 || level > 5)
            {
                return Err("section level mismatch with parent");
            }

            Ok(level)
        }

        /// Parse a setext-style section (title followed by underline).
        /// Excludes description list items (e.g., `term:: content`) which would otherwise
        /// match as setext titles.
        pub(crate) rule section_setext(offset: usize, parent_section_level: Option<SectionLevel>, direct_parent_section_kind: Option<SectionKind>) -> Result<Block<'input>, Error>
        = !check_line_is_description_list(offset)
        block_metadata:(bm:block_metadata(offset, parent_section_level) {?
            bm.map_err(|e| {
                tracing::error!(?e, "error parsing block metadata in section_setext");
                "block metadata parse error"
            })
        })
        title_start:position!() title:$([^'\n']+) title_end:position!() eol()
        setext_level:setext_section_level(title.trim().chars().count(), parent_section_level)
        section_header:({
            let (processed_title, natural_title) =
                process_inlines(state, &block_metadata, title_start, title_end, offset, title)?;
            let title = Title::new(processed_title);
            let location = state.create_block_location(title_start, title_end, offset);
            Ok::<(Title<'input>, section::SectionNumbering), Error>(register_section_header(
                state,
                &block_metadata,
                title,
                natural_title,
                setext_level,
                location,
                direct_parent_section_kind,
            ))
        })
        content:section_content(offset, Some(expected_child_level(
            setext_level,
            SectionKind::from_style(block_metadata.metadata.style),
            is_book_doctype(&state.document_attributes),
        )), Some(SectionKind::from_style(block_metadata.metadata.style)))?
        {
            let (title, numbering) = section_header?;
            let location = state.create_block_location(span_start, span_end, offset);

            // Classify the section by its style (see the ATX section rule).
            let kind = SectionKind::from_style(block_metadata.metadata.style);

            Ok(Block::Section(Section::parsed(
                block_metadata.metadata,
                title,
                setext_level,
                content.unwrap_or(Ok(Vec::new()))?,
                kind,
                numbering,
                location,
            )))
        }

        rule block_metadata(offset: usize, parent_section_level: Option<SectionLevel>) -> Result<BlockParsingMetadata<'input>, Error>
        = meta_start:position!() lines:(
            anchor:anchor() { Ok::<BlockMetadataLine<'input>, Error>(BlockMetadataLine::Anchor(anchor)) }
            / attr:attributes_line() { Ok::<BlockMetadataLine<'input>, Error>(BlockMetadataLine::Attributes((attr.0, Box::new(attr.1)))) }
            / doc_attr:document_attribute_line() { Ok::<BlockMetadataLine<'input>, Error>(BlockMetadataLine::DocumentAttribute(Cow::Borrowed(doc_attr.key), doc_attr.value, doc_attr.set)) }
            / title:title_line(offset) { title.map(BlockMetadataLine::Title) }
        )* meta_end:position!()
        {
            let mut metadata = BlockMetadata::default();
            let mut discrete = false;
            let mut title = Title::default();

            for line in lines {
                // Skip errors from title parsing (e.g., empty titles like "." + newline)
                let Ok(value) = line else {
                    state.add_generic_warning(format!("failed to parse block metadata line, skipping: {line:?}"));
                    continue
                };
                match value {
                    BlockMetadataLine::Anchor(value) => metadata.anchors.push(value),
                    BlockMetadataLine::Attributes((attr_discrete, attr_metadata)) => {
                        discrete = attr_discrete;
                        merge_attribute_metadata(&mut metadata, *attr_metadata);
                    },
                    BlockMetadataLine::DocumentAttribute(key, value, set) => {
                        // Set the document attribute immediately so it's available for
                        // subsequent attribute references (e.g., in title lines)
                        state.apply_document_attribute(key, value, set, false);
                    },
                    BlockMetadataLine::Title(inner) => {
                        title = inner;
                    }
                }
            }
            if meta_start != meta_end {
                metadata.location = Some(state.create_block_location(meta_start, meta_end, offset));
            }
            finish_block_parsing_metadata(
                state,
                metadata,
                title,
                parent_section_level,
                discrete,
                offset,
            )
        }

        // A title line can be a simple title or a section title
        //
        // A title line is a line that starts with a period (.) followed by a non-whitespace character
        rule title_line(offset: usize) -> Result<Title<'input>, Error>
        = period() start:position!() title:$(![' ' | '\t' | '\n' | '\r' | '.'] [^'\n']*) end:position!() eol()
        {
            tracing::debug!(?title, ?start, ?end, "Found title line in block metadata");
            let block_metadata = BlockParsingMetadata::default();
            let (title, _) = process_inlines(state, &block_metadata, start, end, offset, title)?;
            Ok(title.into())
        }

        // A document attribute line in block metadata context
        // This allows document attributes to be set between block attributes and the block content
        // Uses the same parsing logic as document attributes in the header
        rule document_attribute_line() -> AttributeEntry<'input>
        = attr:document_attribute_match() eol()
        {
            tracing::debug!(?attr, "Found document attribute in block metadata");
            attr
        }

        rule section_level(offset: usize, parent_section_level: Option<SectionLevel>) -> (&'input str, SectionLevel)
        = level:$(("=" / "#")*<1,6>)
        {
            let base_level: SectionLevel = level.len().try_into().unwrap_or(1) - 1;
            let byte_offset = span_start + offset;
            (level, apply_leveloffset(base_level, byte_offset, &state.leveloffset_ranges, &state.document_attributes))
        }

        rule section_level_at_line_start(offset: usize, parent_section_level: Option<SectionLevel>) -> (&'input str, SectionLevel)
        = level:$(("=" / "#")*<1,6>)
        {?
            // This rule is invoked as a negative lookahead from paragraph
            // parsing, so it runs speculatively on every continuation line.
            // The injected `span_start` is just a byte offset — the cheap line-start
            // byte check below rejects most speculations before any expensive
            // (line, column) materialisation happens.
            let absolute_pos = span_start + offset;
            let at_line_start = absolute_pos == 0 || {
                let prev_byte_pos = absolute_pos.saturating_sub(1);
                state.input.as_bytes().get(prev_byte_pos).is_some_and(|&b| b == b'\n')
            };

            if !at_line_start {
                return Err("section level must be at line start");
            }

            let base_level: SectionLevel = level.len().try_into().unwrap_or(1) - 1;
            let byte_offset = span_start + offset;
            Ok((level, apply_leveloffset(base_level, byte_offset, &state.leveloffset_ranges, &state.document_attributes)))
        }

        rule section_title(offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<(Title<'input>, &'input str), Error>
        = title:$([^'\n']*)
        {
            tracing::debug!(?title, title_start = span_start, title_end = span_end, offset, "Found section title");
            let (content, natural_title) = process_inlines(
                state,
                block_metadata,
                span_start,
                span_end,
                offset,
                title,
            )?;
            Ok((Title::new(content), natural_title))
        }

        rule section_content(offset: usize, parent_section_level: Option<SectionLevel>, direct_parent_section_kind: Option<SectionKind>) -> Result<Vec<Block<'input>>, Error>
        = blocks(offset, parent_section_level, direct_parent_section_kind) / { Ok(vec![]) }

        pub(crate) rule block_generic(offset: usize, parent_section_level: Option<SectionLevel>) -> Result<Block<'input>, Error>
        = start:position!()
        block_metadata:(bm:block_metadata(offset, parent_section_level) {?
            bm.map_err(|e| {
                tracing::error!(?e, "error parsing block metadata in block_generic");
                "block metadata parse error"
            })
        })
        block:(
            delimited_block:delimited_block(start, offset, &block_metadata) { delimited_block }
            / image:image(start, offset, &block_metadata) { image }
            / audio:audio(start, offset, &block_metadata) { audio }
            / video:video(start, offset, &block_metadata) { video }
            / toc:toc(start, offset, &block_metadata) { toc }
            / thematic_break:thematic_break(start, offset, &block_metadata) { thematic_break }
            / page_break:page_break(start, offset, &block_metadata) { page_break }
            / list:list(start, offset, &block_metadata) { list }
            / quoted_paragraph:quoted_paragraph(start, offset, &block_metadata) { quoted_paragraph }
            / markdown_blockquote:markdown_blockquote(start, offset, &block_metadata) { markdown_blockquote }
            / paragraph:paragraph(start, offset, &block_metadata) { paragraph }
        ) {
            let mut block = block?;
            assign_block_caption(state, &mut block);
            Ok(block)
        }

        // Block parsing for continuation context - lists inside continuations cannot consume
        // further continuations (those belong to the parent item that started the continuation)
        rule block_in_continuation(offset: usize, parent_section_level: Option<SectionLevel>) -> Result<Block<'input>, Error>
        = start:position!()
        block_metadata:(bm:block_metadata(offset, parent_section_level) {?
            bm.map_err(|e| {
                tracing::error!(?e, "error parsing block metadata in block_in_continuation");
                "block metadata parse error"
            })
        })
        block:(
            // A `//` line comment or `////` block comment in a continuation
            // produces a comment node (which renders to nothing), matching
            // asciidoctor. Absorb optional leading blank lines: a trailing `+`
            // leaves a blank-line newline before the comment, while an immediate
            // comment sits directly at the delimiter. Without this, the `+` would
            // backtrack and leak a stray `+` paragraph. Must precede `paragraph`,
            // which would otherwise gobble the `//` line.
            comment:(eol()* comment_start:position!() c:(
                comment_line_block_keep_eol(offset)
                / comment_block(comment_start, offset, &block_metadata)
            ) { c }) { comment }
            / delimited_block:delimited_block(start, offset, &block_metadata) { delimited_block }
            / image:image(start, offset, &block_metadata) { image }
            / audio:audio(start, offset, &block_metadata) { audio }
            / video:video(start, offset, &block_metadata) { video }
            / toc:toc(start, offset, &block_metadata) { toc }
            / thematic_break:thematic_break(start, offset, &block_metadata) { thematic_break }
            / page_break:page_break(start, offset, &block_metadata) { page_break }
            // Lists in continuation context cannot consume further continuations
            / list:list_with_continuation(start, offset, &block_metadata, false) { list }
            / quoted_paragraph:quoted_paragraph(start, offset, &block_metadata) { quoted_paragraph }
            / markdown_blockquote:markdown_blockquote(start, offset, &block_metadata) { markdown_blockquote }
            / paragraph:paragraph(start, offset, &block_metadata) { paragraph }
        ) {
            let mut block = block?;
            assign_block_caption(state, &mut block);
            Ok(block)
        }

        /// Block parsing for table cells without `AsciiDoc` style - excludes block types that require full parsing.
        /// Only `a` (`AsciiDoc`) style cells should have full block parsing.
        /// Excluded: delimited_block, list, toc, page_break, markdown_blockquote
        rule block_generic_for_table_cell(offset: usize, parent_section_level: Option<SectionLevel>) -> Result<Block<'input>, Error>
        = eol()*
        start:position!()
        block_metadata:(bm:block_metadata(offset, parent_section_level) {?
            bm.map_err(|e| {
                tracing::error!(?e, "error parsing block metadata in block_generic_for_table_cell");
                "block metadata parse error"
            })
        })
        block:(
            // NOTE: delimited_block is intentionally excluded - only valid with 'a' cell style
            image:image(start, offset, &block_metadata) { image }
            / audio:audio(start, offset, &block_metadata) { audio }
            / video:video(start, offset, &block_metadata) { video }
            / thematic_break:thematic_break(start, offset, &block_metadata) { thematic_break }
            / quoted_paragraph:quoted_paragraph(start, offset, &block_metadata) { quoted_paragraph }
            // NOTE: toc, page_break, list, markdown_blockquote are excluded - only valid with 'a' cell style
            / paragraph:paragraph(start, offset, &block_metadata) { paragraph }
        ) {
            let mut block = block?;
            assign_block_caption(state, &mut block);
            Ok(block)
        }

        rule delimited_block(
            start: usize,
            offset: usize,
            block_metadata: &BlockParsingMetadata<'input>,
        ) -> Result<Block<'input>, Error>
        = generic_delimited_block(start, offset, block_metadata)
        / table_block(start, offset, block_metadata)

        // Every non-table delimited block shares one open/content/optional-close
        // skeleton. `block_open` recognises which kind a delimiter introduces and
        // `build_delimited_block` constructs the right block — the same split tables
        // use (`*_table_block` rules + `parse_table_block_impl`). The optional close
        // and the `(eol() / ![_])` after the open delimiter let an opener that runs
        // to end of input still produce a block, closed at EOF (asciidoctor's
        // recovery; `build_delimited_block` emits the unterminated warning).
        rule generic_delimited_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
            = open_start:position!() open:block_open() (eol() / ![_])
              content_start:position!() content:until_block_close(open.1) content_end:position!()
              close:(eol() close_start:position!() close_delim:block_close_delim(open.1) { (close_start, close_delim) })?
        {
            build_delimited_block(state, block_metadata, &DelimitedParams {
                kind: open.0, open_delim: open.1, lang: open.2, content,
                open_start, start, content_start, content_end, end: span_end, offset, close,
            })
        }

        // Recognise a non-table delimited-block opening delimiter, returning its
        // kind, the literal delimiter, and (for a Markdown ``` fence) an optional
        // language. Listing (`-`×4+) is tried before open (`--`) so `----` is a
        // listing, not a too-short open block.
        rule block_open() -> (DelimitedKind, &'input str, Option<&'input str>)
            = d:comment_delimiter()  { (DelimitedKind::Comment, d, None) }
            / d:example_delimiter()  { (DelimitedKind::Example, d, None) }
            / d:listing_delimiter()  { (DelimitedKind::Listing, d, None) }
            / d:literal_delimiter()  { (DelimitedKind::Literal, d, None) }
            / d:open_delimiter()     { (DelimitedKind::Open, d, None) }
            / d:sidebar_delimiter()  { (DelimitedKind::Sidebar, d, None) }
            / d:pass_delimiter()     { (DelimitedKind::Pass, d, None) }
            / d:quote_delimiter()    { (DelimitedKind::Quote, d, None) }
            / d:markdown_code_delimiter() lang:markdown_language()? { (DelimitedKind::Listing, d, lang) }

        // Content up to (but not including) a closing delimiter line exactly equal
        // to `expected`, or end of input. Generic over delimiter type; the exact
        // comparison in `block_close_delim` keeps a different-length or
        // different-character run from closing the block.
        rule until_block_close(expected: &str) -> &'input str
            = content:$((!(eol() block_close_delim(expected)) [_])*) { content }

        // A maximal run of a single block-delimiter character equal to `expected`.
        // Per-character alternatives (not a mixed character class) so a run stops
        // at the first foreign character, exactly like the old per-type rules.
        rule block_close_delim(expected: &str) -> &'input str
            = delim:$("="+ / "/"+ / "-"+ / "."+ / "*"+ / "_"+ / "+"+ / "~"+ / "`"+)
              {? if delim == expected { Ok(delim) } else { Err("delimiter mismatch") } }

        // A `////` comment block specifically. The generic `delimited_block` covers
        // this in normal flow, but a list/description-list continuation needs to
        // match *only* a comment block (to absorb it after a `+`), so this gated
        // entry point reuses the shared skeleton and builder.
        rule comment_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
            = open_start:position!() open_delim:comment_delimiter() (eol() / ![_])
              content_start:position!() content:until_block_close(open_delim) content_end:position!()
              close:(eol() close_start:position!() close_delim:block_close_delim(open_delim) { (close_start, close_delim) })?
        {
            build_delimited_block(state, block_metadata, &DelimitedParams {
                kind: DelimitedKind::Comment, open_delim, lang: None, content,
                open_start, start, content_start, content_end, end: span_end, offset, close,
            })
        }

        // Delimiter recognition rules
        rule comment_delimiter() -> &'input str = delim:$("/"*<4,>) { delim }
        rule example_delimiter() -> &'input str = delim:$("="*<4,>) { delim }
        rule listing_delimiter() -> &'input str = delim:$("-"*<4,>) { delim }
        rule literal_delimiter() -> &'input str = delim:$("."*<4,>) { delim }
        rule open_delimiter() -> &'input str = delim:$("-"*<2,2> / "~"*<4,>) { delim }
        rule sidebar_delimiter() -> &'input str = delim:$("*"*<4,>) { delim }
        rule table_delimiter() -> &'input str = delim:$((['|' | ',' | ':' | '!'] "="*<3,>)) { delim }

        // Delimiter-specific table delimiter rules for nested table support.
        // PEG negative lookahead can't accept runtime parameters, so we need
        // separate rules for each delimiter type to correctly parse nested tables.
        rule pipe_table_delimiter() -> &'input str = delim:$("|" "="*<3,>) { delim }
        rule excl_table_delimiter() -> &'input str = delim:$("!" "="*<3,>) { delim }
        rule comma_table_delimiter() -> &'input str = delim:$("," "="*<3,>) { delim }
        rule colon_table_delimiter() -> &'input str = delim:$(":" "="*<3,>) { delim }

        rule pass_delimiter() -> &'input str = delim:$("+"*<4,>) { delim }
        rule markdown_code_delimiter() -> &'input str = delim:$("`"*<3,>) { delim }
        rule quote_delimiter() -> &'input str = delim:$("_"*<4,>) { delim }

        rule until_table_delimiter() -> &'input str
        = content:$((!(eol() table_delimiter()) [_])*) { content }

        // Delimiter-specific content rules for nested table support.
        // Each rule only looks ahead for its specific delimiter, allowing
        // nested tables with different delimiters to be parsed correctly.
        rule until_pipe_table_delimiter() -> &'input str
        = content:$((!(eol() pipe_table_delimiter()) [_])*) { content }

        rule until_excl_table_delimiter() -> &'input str
        = content:$((!(eol() excl_table_delimiter()) [_])*) { content }

        rule until_comma_table_delimiter() -> &'input str
        = content:$((!(eol() comma_table_delimiter()) [_])*) { content }

        rule until_colon_table_delimiter() -> &'input str
        = content:$((!(eol() colon_table_delimiter()) [_])*) { content }

        rule markdown_language() -> &'input str
        = lang:$((['a'..='z'] / ['A'..='Z'] / ['0'..='9'] / "_" / "+" / "-")+) { lang }

        // Table block dispatcher - tries each delimiter-specific variant in order.
        // This enables nested tables: |=== outer can contain !=== inner because
        // each rule only looks for its own closing delimiter.
        //
        // Terminated variants are tried first; unterminated fallbacks only match
        // when an opening delimiter runs to end-of-input without a close.
        rule table_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
            = pipe_table_block(start, offset, block_metadata)
            / excl_table_block(start, offset, block_metadata)
            / comma_table_block(start, offset, block_metadata)
            / colon_table_block(start, offset, block_metadata)
            / unterminated_pipe_table_block(start, offset, block_metadata)
            / unterminated_excl_table_block(start, offset, block_metadata)
            / unterminated_comma_table_block(start, offset, block_metadata)
            / unterminated_colon_table_block(start, offset, block_metadata)

        rule pipe_table_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
            = table_start:position!() open_delim:pipe_table_delimiter() eol()
              content_start:position!() content:until_pipe_table_delimiter() content_end:position!()
              eol() close_start:position!() close_delim:pipe_table_delimiter()
        {
            parse_table_block_impl(
                &TableParseParams {
                    start, offset, table_start, content_start, content_end, end: span_end,
                    open_delim, content, default_separator: "|",
                    closing: TableClosing::Terminated { close_delim, close_start },
                },
                state,
                block_metadata,
            )
        }

        rule excl_table_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
            = table_start:position!() open_delim:excl_table_delimiter() eol()
              content_start:position!() content:until_excl_table_delimiter() content_end:position!()
              eol() close_start:position!() close_delim:excl_table_delimiter()
        {
            parse_table_block_impl(
                &TableParseParams {
                    start, offset, table_start, content_start, content_end, end: span_end,
                    open_delim, content, default_separator: "!",
                    closing: TableClosing::Terminated { close_delim, close_start },
                },
                state,
                block_metadata,
            )
        }

        rule comma_table_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
            = table_start:position!() open_delim:comma_table_delimiter() eol()
              content_start:position!() content:until_comma_table_delimiter() content_end:position!()
              eol() close_start:position!() close_delim:comma_table_delimiter()
        {
            parse_table_block_impl(
                &TableParseParams {
                    start, offset, table_start, content_start, content_end, end: span_end,
                    open_delim, content, default_separator: ",",
                    closing: TableClosing::Terminated { close_delim, close_start },
                },
                state,
                block_metadata,
            )
        }

        rule colon_table_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
            = table_start:position!() open_delim:colon_table_delimiter() eol()
              content_start:position!() content:until_colon_table_delimiter() content_end:position!()
              eol() close_start:position!() close_delim:colon_table_delimiter()
        {
            parse_table_block_impl(
                &TableParseParams {
                    start, offset, table_start, content_start, content_end, end: span_end,
                    open_delim, content, default_separator: ":",
                    closing: TableClosing::Terminated { close_delim, close_start },
                },
                state,
                block_metadata,
            )
        }

        // Unterminated table fallbacks: match an opening table delimiter
        // that runs to end-of-input without a closing delimiter. These
        // alternatives are tried only after all terminated variants fail,
        // so a document with a valid close never takes this path. When
        // taken, `parse_table_block_impl` emits an `UnterminatedTable`
        // warning and still produces a table, matching asciidoctor's
        // recovery behavior.
        //
        // The `(eol() / ![_])` after the open delimiter accepts both
        // `|===\n...` and `|===<EOF>`: the preprocessor's `normalize`
        // strips a single trailing newline (mirroring `str::lines`), so a
        // file ending with just `|===\n` reaches the grammar as `|===`.
        rule unterminated_pipe_table_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
            = table_start:position!() open_delim:pipe_table_delimiter() (eol() / ![_])
              content_start:position!() content:until_pipe_table_delimiter() content_end:position!()
              ![_]
        {
            parse_table_block_impl(
                &TableParseParams {
                    start, offset, table_start, content_start, content_end, end: span_end,
                    open_delim, content, default_separator: "|",
                    closing: TableClosing::Unterminated,
                },
                state,
                block_metadata,
            )
        }

        rule unterminated_excl_table_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
            = table_start:position!() open_delim:excl_table_delimiter() (eol() / ![_])
              content_start:position!() content:until_excl_table_delimiter() content_end:position!()
              ![_]
        {
            parse_table_block_impl(
                &TableParseParams {
                    start, offset, table_start, content_start, content_end, end: span_end,
                    open_delim, content, default_separator: "!",
                    closing: TableClosing::Unterminated,
                },
                state,
                block_metadata,
            )
        }

        rule unterminated_comma_table_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
            = table_start:position!() open_delim:comma_table_delimiter() (eol() / ![_])
              content_start:position!() content:until_comma_table_delimiter() content_end:position!()
              ![_]
        {
            parse_table_block_impl(
                &TableParseParams {
                    start, offset, table_start, content_start, content_end, end: span_end,
                    open_delim, content, default_separator: ",",
                    closing: TableClosing::Unterminated,
                },
                state,
                block_metadata,
            )
        }

        rule unterminated_colon_table_block(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
            = table_start:position!() open_delim:colon_table_delimiter() (eol() / ![_])
              content_start:position!() content:until_colon_table_delimiter() content_end:position!()
              ![_]
        {
            parse_table_block_impl(
                &TableParseParams {
                    start, offset, table_start, content_start, content_end, end: span_end,
                    open_delim, content, default_separator: ":",
                    closing: TableClosing::Unterminated,
                },
                state,
                block_metadata,
            )
        }

        rule toc(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
        = "toc::" attributes:attributes() end:position!()
          trailing:$([^'\n']*)
        {
            let (_discrete, metadata_from_attributes, _title_position) = attributes;
            let mut metadata = block_metadata.metadata.clone();
            metadata.merge(&metadata_from_attributes);
            metadata.move_positional_attributes_to_attributes();
            state.warn_trailing_macro_content("toc", trailing, end, offset);
            tracing::debug!("Found Table of Contents block");
            Ok(Block::TableOfContents(TableOfContents {
                metadata,
                location: state.create_location(start+offset, end+offset),
            }))
        }

        rule image_macro_expands(block_metadata: &BlockParsingMetadata<'input>)
        = {?
            if matches!(
                block_metadata.metadata.style,
                Some("listing" | "source" | "literal" | "verse")
            ) {
                Err("verbatim paragraph style")
            } else {
                Ok(())
            }
        }

        rule image(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
        = image_macro_expands(block_metadata)
          "image::" source:source() attributes:image_macro_attributes() end:position!()
          trailing:$([^'\n']*)
        {
            state.warn_trailing_macro_content("image", trailing, end, offset);
            let (_discrete, metadata_from_attributes, _title_position) = attributes;
            let title = block_metadata.title.clone();
            let mut metadata = block_metadata.metadata.clone();
            metadata.merge(&metadata_from_attributes);
            if let Some(style) = metadata.style {
                metadata.style = None; // Clear style to avoid confusion
                metadata
                    .attributes
                    .set("alt".into(), AttributeValue::String(Cow::Borrowed(style)));
            }
            let slots = drain_positional_slots(&mut metadata, 2);
            if let Some(width) = slots.first().filter(|value| !value.is_empty()) {
                metadata.attributes.set(
                    "width".into(),
                    AttributeValue::String(Cow::Borrowed(width)),
                );
            }
            if let Some(height) = slots.get(1).filter(|value| !value.is_empty()) {
                metadata.attributes.set(
                    "height".into(),
                    AttributeValue::String(Cow::Borrowed(height)),
                );
            }
            metadata.move_positional_attributes_to_attributes();
            Ok(Block::Image(Image {
                title,
                source,
                metadata,
                location: state.create_block_location(start, end, offset),

            }))
        }

        rule audio(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
        = "audio::" source:source() attributes:macro_attributes() end:position!()
          trailing:$([^'\n']*)
        {
            state.warn_trailing_macro_content("audio", trailing, end, offset);
            let (_discrete, metadata_from_attributes, _title_position) = attributes;
            let title = block_metadata.title.clone();
            let mut metadata = block_metadata.metadata.clone();
            metadata.merge(&metadata_from_attributes);
            metadata.move_positional_attributes_to_attributes();
            Ok(Block::Audio(Audio {
                title,
                source,
                metadata,
                location: state.create_block_location(start, end, offset),
            }))
        }

        // The video block is similar to the audio and image blocks, but it supports
        // multiple sources. This is for example to allow passing multiple youtube video
        // ids to form a playlist.
        rule video(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
        = "video::" sources:(source() ** comma()) attributes:macro_attributes() end:position!()
          trailing:$([^'\n']*)
        {
            state.warn_trailing_macro_content("video", trailing, end, offset);
            let (_discrete, metadata_from_attributes, _title_position) = attributes;
            let title = block_metadata.title.clone();
            let mut metadata = block_metadata.metadata.clone();
            metadata.merge(&metadata_from_attributes);
            if let Some(style) = metadata.style {
                metadata.style = None;
                if style == "youtube" || style == "vimeo" {
                    tracing::debug!(?metadata, "transforming video metadata style into attribute");
                    metadata
                        .attributes
                        .set(Cow::Borrowed(style), AttributeValue::Bool(true));
                } else {
                    // assume poster
                    tracing::debug!(?metadata, "transforming video metadata style into attribute, assuming poster");
                    metadata.attributes.set(
                        "poster".into(),
                        AttributeValue::String(Cow::Borrowed(style)),
                    );
                }
            }
            let slots = drain_positional_slots(&mut metadata, 2);
            if let Some(width) = slots.first().filter(|value| !value.is_empty()) {
                metadata.attributes.set(
                    "width".into(),
                    AttributeValue::String(Cow::Borrowed(width)),
                );
            }
            if let Some(height) = slots.get(1).filter(|value| !value.is_empty()) {
                metadata.attributes.set(
                    "height".into(),
                    AttributeValue::String(Cow::Borrowed(height)),
                );
            }
            metadata.move_positional_attributes_to_attributes();
            Ok(Block::Video(Video {
                title,
                sources,
                metadata,
                location: state.create_block_location(start, end, offset),
            }))
        }

        rule thematic_break(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
            = ("'''"
               // Below are the markdown-style thematic breaks
               / "---"
               / "- - -"
               / "***"
               / "* * *"
            )
        {
            tracing::debug!("Found thematic break block");
            Ok(Block::ThematicBreak(ThematicBreak {
                anchors: block_metadata.metadata.anchors.clone(), // TODO(nlopes): should this simply be metadata?
                title: block_metadata.title.clone(),
                location: state.create_block_location(start, span_end, offset),
            }))
        }

        rule page_break(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
            = "<<<" &eol()*<2,2>
        {
            tracing::debug!("Found page break block");
            let mut metadata = block_metadata.metadata.clone();
            metadata.move_positional_attributes_to_attributes();

            Ok(Block::PageBreak(PageBreak {
                title: block_metadata.title.clone(),
                metadata,
                location: state.create_location(start+offset, span_end+offset),
            }))
        }

        rule list(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
        = list_with_continuation(start, offset, block_metadata, true)

        // Parameterized list rule - allow_continuation controls whether list items can consume
        // explicit continuations. Set to false when parsing lists inside continuation blocks
        // to prevent nested lists from consuming parent-level continuations.
        rule list_with_continuation(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>, allow_continuation: bool) -> Result<Block<'input>, Error>
        = callout_list(start, offset, block_metadata)
        / unordered_list(start, offset, block_metadata, None, allow_continuation, false)
        / ordered_list(start, offset, block_metadata, None, allow_continuation, false)
        / description_list(start, offset, block_metadata, allow_continuation)

        rule unordered_list_marker() -> &'input str = $("*"+ / "-")

        rule ordered_list_marker() -> &'input str = $(digits()? "."+)

        rule description_list_marker() -> &'input str = $("::::" / ":::" / "::" / ";;")

        rule callout_list_marker() -> &'input str = $("<" (digits() / ".") ">")

        rule section_level_marker() -> &'input str = $(("=" / "#")+)

        // This restricted form excludes titles and document attributes, which
        // asciidoctor does not assign to an automatically nested list.
        rule nested_list_metadata(offset: usize, parent_section_level: Option<SectionLevel>) -> Result<BlockParsingMetadata<'input>, Error>
        = meta_start:position!() lines:(
            anchor:anchor() { AttributeOrAnchorLine::Anchor(anchor) }
            / attr:attributes_line() { AttributeOrAnchorLine::Attributes((attr.0, Box::new(attr.1))) }
        )+ meta_end:position!()
        {
            let mut metadata = BlockMetadata::default();
            let mut discrete = false;
            for line in lines {
                match line {
                    AttributeOrAnchorLine::Anchor(anchor) => metadata.anchors.push(anchor),
                    AttributeOrAnchorLine::Attributes((attr_discrete, attr_metadata)) => {
                        discrete = attr_discrete;
                        merge_attribute_metadata(&mut metadata, *attr_metadata);
                    }
                }
            }
            metadata.location = Some(state.create_block_location(meta_start, meta_end, offset));
            finish_block_parsing_metadata(
                state,
                metadata,
                Title::default(),
                parent_section_level,
                discrete,
                offset,
            )
        }

        rule parsed_nested_list_metadata(offset: usize, parent_section_level: Option<SectionLevel>) -> BlockParsingMetadata<'input>
        = metadata:nested_list_metadata(offset, parent_section_level) {?
            metadata.map_err(|error| {
                tracing::error!(?error, "error parsing nested list metadata");
                "nested list metadata parse error"
            })
        }

        // Syntactic counterparts of `nested_list_metadata`; these are used in
        // lookahead and therefore do not run metadata actions speculatively.
        rule nested_list_attribute_line_match()
        = !empty_list_separator() !double_open_square_bracket()
          open_square_bracket() attribute_list_content() eol()

        rule nested_list_anchor_line_match()
        = inline_anchor_match() eol()

        rule nested_list_metadata_line_match()
        = nested_list_anchor_line_match() / nested_list_attribute_line_match()

        rule nested_list_metadata_gap()
        = eol() / comment_line()

        rule nested_unordered_child_after_metadata(current_marker: &str, parent_ordered_marker: Option<&'input str>)
        = nested_list_metadata_line_match()+ nested_list_metadata_gap()* (
            !at_ancestor_ordered_marker(parent_ordered_marker)
            &(whitespace()* ordered_list_marker() whitespace())
            / &at_deeper_unordered_marker(current_marker)
        )

        rule nested_ordered_child_after_metadata(current_marker: &str, parent_unordered_marker: Option<&'input str>)
        = nested_list_metadata_line_match()+ nested_list_metadata_gap()* (
            !at_ancestor_unordered_marker(parent_unordered_marker)
            &(whitespace()* unordered_list_marker() whitespace())
            / &at_deeper_ordered_marker(current_marker)
        )

        // Helper rule to check if we're at the start of a new list item (lookahead)
        rule at_list_item_start() = whitespace()* (unordered_list_marker() / ordered_list_marker()) whitespace()

        // Helper rule to check if we're at the start of a section heading (lookahead)
        // This is used to terminate list continuations when a section follows
        rule at_section_start() = (anchor() / attributes_line())* ("=" / "#")+ " "

        // Helper rule to check if we're at an ordered list marker ahead (after newlines)
        rule at_ordered_marker_ahead() = eol()+ whitespace()* ordered_list_marker()

        // Helper rule to check if we're at an unordered list marker ahead (after newlines)
        rule at_unordered_marker_ahead() = eol()+ whitespace()* unordered_list_marker()

        // Helper rule to check if we're at an ancestor-level ordered marker
        // Used in cross-type nesting to prevent consuming sibling ordered markers
        // that belong to a parent ordered list context
        rule at_ancestor_ordered_marker(ancestor: Option<&'input str>)
        = whitespace()* marker:ordered_list_marker() whitespace() {?
            match ancestor {
                Some(m) if marker.len() <= m.len() => Ok(()),
                _ => Err("not ancestor")
            }
        }

        // Helper rule to check if we're at an ancestor-level unordered marker
        // Used in cross-type nesting to prevent consuming sibling unordered markers
        // that belong to a parent unordered list context
        rule at_ancestor_unordered_marker(ancestor: Option<&'input str>)
        = whitespace()* marker:unordered_list_marker() whitespace() {?
            match ancestor {
                Some(m) if marker.len() <= m.len() => Ok(()),
                _ => Err("not ancestor")
            }
        }

        // Helper rule to check if we're at a shallower unordered marker
        // Used to terminate nested lists when a blank line precedes a shallower item
        // Same-level markers continue the list as siblings; only shallower markers end it
        rule at_shallower_unordered_marker(base_marker: &str)
        = whitespace()* marker:unordered_list_marker() whitespace() {?
            if marker.len() < base_marker.len() { Ok(()) } else { Err("same-or-deeper") }
        }

        // Helper rule to check if we're at a shallower ordered marker
        // Used to terminate nested lists when a blank line precedes a shallower item
        // Same-level markers continue the list as siblings; only shallower markers end it
        rule at_shallower_ordered_marker(base_marker: &str)
        = whitespace()* marker:ordered_list_marker() whitespace() {?
            if marker.len() < base_marker.len() { Ok(()) } else { Err("same-or-deeper") }
        }

        // Helper rule to check if we're at a deeper unordered marker (for nested same-type lists)
        // Used by unordered_list_item_nested_content to detect nested unordered lists
        rule at_deeper_unordered_marker(base_marker: &str)
        = whitespace()* marker:unordered_list_marker() whitespace() {?
            if marker.len() > base_marker.len() { Ok(()) } else { Err("same-or-shallower") }
        }

        // Helper rule to check if we're at a deeper ordered marker (for nested same-type lists)
        // Used by ordered_list_item_nested_content to detect nested ordered lists
        rule at_deeper_ordered_marker(base_marker: &str)
        = whitespace()* marker:ordered_list_marker() whitespace() {?
            if marker.len() > base_marker.len() { Ok(()) } else { Err("same-or-shallower") }
        }

        // Helper rule to check if we're at a list separator (forces list termination)
        // Matches either a line comment (//) or empty block attributes ([]) on their own line
        // Note: Separator must be preceded by at least one blank line (2+ newlines)
        // Without a blank line before it, a comment is just skipped, not a separator
        rule at_list_separator()
        = eol()*<2,> at_list_separator_content()

        // Helper rule to check for separator content at current position (no leading newlines)
        // Used by continuation_lines to stop at separators
        rule at_list_separator_content()
        = "//" [^'\n']* (&eol() / ![_])  // Line comment separator
        / whitespace()* "[" whitespace()* "]" whitespace()* (&eol() / ![_])  // Empty block attributes

        rule unordered_list_principal_continuation(current_marker: &str, parent_ordered_marker: Option<&'input str>) -> &'input str
        = eol()
          !(
              &eol()
              / &at_list_item_start()
              / &"+"
              / &at_section_start()
              / &at_list_separator_content()
              / &nested_unordered_child_after_metadata(current_marker, parent_ordered_marker)
          )
          line:$((!eol() [_])*) { line }

        rule ordered_list_principal_continuation(current_marker: &str, parent_unordered_marker: Option<&'input str>) -> &'input str
        = eol()
          !(
              &eol()
              / &at_list_item_start()
              / &"+"
              / &at_section_start()
              / &at_list_separator_content()
              / &nested_ordered_child_after_metadata(current_marker, parent_unordered_marker)
          )
          line:$((!eol() [_])*) { line }

        // Block metadata in column one after a blank line starts a new description list.
        // Indented metadata-like text remains part of the current item.
        rule at_dlist_block_boundary()
        = eol()*<2,> &(
            ("[" ![']' | '['] [^']' | '\n']+ "]" whitespace()* eol())
            / ("[[" [^']']+ "]]" whitespace()* eol())
            / ("." ![' ' | '\t' | '\n' | '\r' | '.'] [^'\n']* eol())
        )

        rule unordered_list(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>, parent_ordered_marker: Option<&'input str>, allow_continuation: bool, is_nested: bool) -> Result<Block<'input>, Error>
        // Parse whitespace + marker first to capture base_marker for rest items
        // marker_start captures position before marker for correct first item location
        = whitespace()* marker_start:position!() base_marker:$(unordered_list_marker()) &whitespace()
        first:unordered_list_item_after_marker(offset, block_metadata, allow_continuation, base_marker, marker_start, parent_ordered_marker)
        rest:(unordered_list_rest_item(offset, block_metadata, parent_ordered_marker, allow_continuation, base_marker))*
        {
            tracing::debug!("Found unordered list block");
            let mut content = vec![first?];
            for item in rest {
                content.push(item?);
            }
            let end = content.last().map_or(span_end, |(_, item_end)| *item_end);
            let items: Vec<ListItem<'input>> = content.into_iter().map(|(item, _)| item).collect();
            let marker = items.first().map_or("", |item| item.marker);

            Ok(Block::UnorderedList(UnorderedList {
                title: if is_nested { Title::default() } else { block_metadata.title.clone() },
                metadata: if is_nested { BlockMetadata::default() } else { block_metadata.metadata.clone() },
                items,
                marker,
                location: state.create_location(start+offset, end+offset),
            }))
        }

        // Parse first item content after marker has been consumed by unordered_list
        // marker_start is the position where the marker began, for correct location tracking
        rule unordered_list_item_after_marker(offset: usize, block_metadata: &BlockParsingMetadata<'input>, allow_continuation: bool, marker: &'input str, marker_start: usize, parent_ordered_marker: Option<&'input str>) -> Result<(ListItem<'input>, usize), Error>
        = item:unordered_list_item_with_continuation_after_marker(offset, block_metadata, marker, marker_start, parent_ordered_marker) {? if allow_continuation { Ok(item) } else { Err("skip") } }
        / item:unordered_list_item_no_continuation_after_marker(offset, block_metadata, marker, marker_start, parent_ordered_marker) { item }

        // Zero-cost guards for the front-of-alternative branch selector in
        // `*_list_rest_item`. Keeps the expensive item parse out of the branch
        // whose trailing semantic action would have just discarded it.
        rule parent_is_some(parent: Option<&'input str>) -> ()
        = {? if parent.is_some() { Ok(()) } else { Err("parent_is_none") } }

        rule parent_is_none(parent: Option<&'input str>) -> ()
        = {? if parent.is_none() { Ok(()) } else { Err("parent_is_some") } }

        rule unordered_list_rest_item(offset: usize, block_metadata: &BlockParsingMetadata<'input>, parent_ordered_marker: Option<&'input str>, allow_continuation: bool, base_marker: &str) -> Result<(ListItem<'input>, usize), Error>
        // `parent_ordered_marker` is fixed for the whole `unordered_list` call, so
        // rather than parse the (expensive) item first and reject via a trailing
        // `{? }` action on three of four alternatives, guard each alternative at
        // the front with a zero-cost check and only parse when the branch applies.
        // The `!at_ordered_marker_ahead()` lookahead is kept only in the
        // `parent_ordered_marker.is_some()` branch where it actually pays off.
        // See fixtures: nested_unordered_in_ordered.adoc, nested_ordered_in_unordered.adoc
        //
        // Branch: parent is ordered
        = parent_is_some(parent_ordered_marker) !at_list_separator() !eol() comment_line()* !at_ordered_marker_ahead() item:unordered_list_item(offset, block_metadata, allow_continuation, parent_ordered_marker)
          { item }
        / parent_is_some(parent_ordered_marker) !at_list_separator() eol()+ comment_line()* !at_shallower_unordered_marker(base_marker) !at_ordered_marker_ahead() item:unordered_list_item(offset, block_metadata, allow_continuation, parent_ordered_marker)
          { item }
        // Branch: no ordered parent
        / parent_is_none(parent_ordered_marker) !at_list_separator() !eol() comment_line()* item:unordered_list_item(offset, block_metadata, allow_continuation, parent_ordered_marker)
          { item }
        / parent_is_none(parent_ordered_marker) !at_list_separator() eol()+ comment_line()* !at_shallower_unordered_marker(base_marker) item:unordered_list_item(offset, block_metadata, allow_continuation, parent_ordered_marker)
          { item }

        rule ordered_list(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>, parent_unordered_marker: Option<&'input str>, allow_continuation: bool, is_nested: bool) -> Result<Block<'input>, Error>
        // Parse whitespace + marker first to capture base_marker for rest items
        // marker_start captures position before marker for correct first item location
        = whitespace()* marker_start:position!() base_marker:$(ordered_list_marker()) &whitespace()
        first:ordered_list_item_after_marker(offset, block_metadata, allow_continuation, base_marker, marker_start, parent_unordered_marker)
        rest:(ordered_list_rest_item(offset, block_metadata, parent_unordered_marker, allow_continuation, base_marker))*
        {
            tracing::debug!("Found ordered list block");
            let mut content = vec![first?];
            for item in rest {
                content.push(item?);
            }
            let end = content.last().map_or(span_end, |(_, item_end)| *item_end);
            let items: Vec<ListItem<'input>> = content.into_iter().map(|(item, _)| item).collect();
            let marker = items.first().map_or("", |item| item.marker);

            Ok(Block::OrderedList(OrderedList {
                title: if is_nested { Title::default() } else { block_metadata.title.clone() },
                metadata: if is_nested { BlockMetadata::default() } else { block_metadata.metadata.clone() },
                items,
                marker,
                location: state.create_location(start+offset, end+offset),
            }))
        }

        // Parse first item content after marker has been consumed by ordered_list
        // marker_start is the position where the marker began, for correct location tracking
        rule ordered_list_item_after_marker(offset: usize, block_metadata: &BlockParsingMetadata<'input>, allow_continuation: bool, marker: &'input str, marker_start: usize, parent_unordered_marker: Option<&'input str>) -> Result<(ListItem<'input>, usize), Error>
        = item:ordered_list_item_with_continuation_after_marker(offset, block_metadata, marker, marker_start, parent_unordered_marker) {? if allow_continuation { Ok(item) } else { Err("skip") } }
        / item:ordered_list_item_no_continuation_after_marker(offset, block_metadata, marker, marker_start, parent_unordered_marker) { item }

        rule ordered_list_rest_item(offset: usize, block_metadata: &BlockParsingMetadata<'input>, parent_unordered_marker: Option<&'input str>, allow_continuation: bool, base_marker: &str) -> Result<(ListItem<'input>, usize), Error>
        // Mirror of `unordered_list_rest_item`'s front-guard structure. See that
        // rule's comment for the rationale.
        //
        // Branch: parent is unordered
        = parent_is_some(parent_unordered_marker) !at_list_separator() !eol() comment_line()* !at_unordered_marker_ahead() item:ordered_list_item(offset, block_metadata, allow_continuation, parent_unordered_marker)
          { item }
        / parent_is_some(parent_unordered_marker) !at_list_separator() eol()+ comment_line()* !at_shallower_ordered_marker(base_marker) !at_unordered_marker_ahead() item:ordered_list_item(offset, block_metadata, allow_continuation, parent_unordered_marker)
          { item }
        // Branch: no unordered parent
        / parent_is_none(parent_unordered_marker) !at_list_separator() !eol() comment_line()* item:ordered_list_item(offset, block_metadata, allow_continuation, parent_unordered_marker)
          { item }
        / parent_is_none(parent_unordered_marker) !at_list_separator() eol()+ comment_line()* !at_shallower_ordered_marker(base_marker) item:ordered_list_item(offset, block_metadata, allow_continuation, parent_unordered_marker)
          { item }

        // Note: The `*_with_continuation` and `*_no_continuation` variants exist because
        // PEG parsers are greedy - nested items must NOT consume explicit continuations
        // that belong to their parent. Attempting to handle this in semantic actions
        // (by always parsing continuations then discarding them) would consume input
        // needed by the parent rule. This structural duplication is intentional.
        rule unordered_list_item(offset: usize, block_metadata: &BlockParsingMetadata<'input>, allow_continuation: bool, parent_ordered_marker: Option<&'input str>) -> Result<(ListItem<'input>, usize), Error>
        = item:unordered_list_item_with_continuation(offset, block_metadata, parent_ordered_marker) {? if allow_continuation { Ok(item) } else { Err("skip") } }
        / item:unordered_list_item_no_continuation(offset, block_metadata, parent_ordered_marker) { item }

        rule unordered_list_item_with_continuation(offset: usize, block_metadata: &BlockParsingMetadata<'input>, parent_ordered_marker: Option<&'input str>) -> Result<(ListItem<'input>, usize), Error>
        = whitespace()*
        marker:unordered_list_marker()
        whitespace()
        checked:checklist_item()?
        first_line_start:position!()
        // Parse first line (principal text)
        first_line:$((!(eol()) [_])*)
        // Parse continuation lines that are part of the same paragraph
        // Stop at: blank line, list item start, explicit continuation marker, section heading, or list separator
        continuation_lines:unordered_list_principal_continuation(marker, parent_ordered_marker)*
        first_line_end:position!()
        // Try to parse nested list (ordered, or unordered with deeper markers)
        // Don't consume newlines if we're at a list separator (comment or [])
        // Nested items cannot consume parent-level continuations (allow_continuation: false)
        // NOTE: nested_content is NOT optional here - if no nested content matches, the entire
        // alternative fails and backtracks, leaving eol() unconsumed for explicit_continuation
        nested:(!at_list_separator() nested_content:unordered_list_item_nested_after_principal(offset, block_metadata, marker, parent_ordered_marker) { nested_content })?
        // Try to parse explicit continuations (+ marker)
        // Don't consume newlines if we're at a list separator (comment or [])
        // Parent items accept both:
        // - Immediate continuations (0 empty lines) for content directly after principal text
        // - Ancestor continuations (1+ empty lines) for content that bubbles up from nested items
        // Use * to match a mixed sequence of immediate and ancestor continuations
        explicit_continuations:(!at_list_separator() cont:(
            list_explicit_continuation_immediate(offset, block_metadata)
            / list_explicit_continuation_ancestor(offset, block_metadata)
        ) { cont })*
        list_dangling_continuation()?
        {
            tracing::debug!(%first_line, ?continuation_lines, %marker, ?checked, "found unordered list item");
            let level = ListLevel::try_from(ListItem::parse_depth_from_marker(marker).unwrap_or(1))?;
            let principal_text: &'input str = assemble_principal_text(state, first_line, &continuation_lines);
            let item_end = calculate_item_end(principal_text.is_empty(), span_start, first_line_end);

            // Process principal text as inline nodes
            let principal = if principal_text.trim().is_empty() {
                vec![]
            } else {
                let (principal, _) =
                    process_inlines(state, block_metadata, first_line_start, first_line_end, offset, principal_text)?;
                principal
            };

            let mut blocks = Vec::new();
            // nested_content is no longer optional in the grammar, so one less Some level
            if let Some(Some(Ok(nested_list))) = nested {
                blocks.push(nested_list);
            }
            // Collect all continuation blocks (each is a Result<Block<'input>, Error>)
            blocks.extend(explicit_continuations.into_iter().flatten());

            // Use end position after all blocks if we have any, otherwise use item_end
            let actual_end = if blocks.is_empty() { item_end } else { span_end.saturating_sub(1) };

            Ok((ListItem {
                principal,
                blocks,
                level,
                marker,
                checked,
                location: state.create_location(span_start+offset, actual_end+offset),
            }, actual_end))
        }

        // Version with immediate continuations only (for nested items)
        // Nested items consume continuations with 0 empty lines (immediate attachment).
        // Continuations with 1+ empty lines bubble up to ancestor items.
        rule unordered_list_item_no_continuation(offset: usize, block_metadata: &BlockParsingMetadata<'input>, parent_ordered_marker: Option<&'input str>) -> Result<(ListItem<'input>, usize), Error>
        = whitespace()*
        marker:unordered_list_marker()
        whitespace()
        checked:checklist_item()?
        first_line_start:position!()
        first_line:$((!(eol()) [_])*)
        continuation_lines:unordered_list_principal_continuation(marker, parent_ordered_marker)*
        first_line_end:position!()
        // Nested items can still have nested lists, but those also cannot consume parent continuations
        // NOTE: nested_content is NOT optional here - if no nested content matches, the entire
        // alternative fails and backtracks, leaving eol() unconsumed for immediate_continuation
        nested:(!at_list_separator() nested_content:unordered_list_item_nested_after_principal(offset, block_metadata, marker, parent_ordered_marker) { nested_content })?
        // Parse immediate continuations (0 empty lines) - these attach to this item
        // Ancestor continuations (1+ empty lines) bubble up to parent items
        immediate_continuations:(!at_list_separator() cont:list_explicit_continuation_immediate(offset, block_metadata) { cont })*
        {
            tracing::debug!(%first_line, ?continuation_lines, %marker, ?checked, "found unordered list item (immediate continuation only)");
            let level = ListLevel::try_from(ListItem::parse_depth_from_marker(marker).unwrap_or(1))?;
            let principal_text: &'input str = assemble_principal_text(state, first_line, &continuation_lines);
            let item_end = calculate_item_end(principal_text.is_empty(), span_start, first_line_end);

            let principal = if principal_text.trim().is_empty() {
                vec![]
            } else {
                let (principal, _) =
                    process_inlines(state, block_metadata, first_line_start, first_line_end, offset, principal_text)?;
                principal
            };

            let mut blocks = Vec::new();
            // nested_content is no longer optional in the grammar, so one less Some level
            if let Some(Some(Ok(nested_list))) = nested {
                blocks.push(nested_list);
            }
            // Collect all immediate continuation blocks
            blocks.extend(immediate_continuations.into_iter().flatten());

            // Use end position after all blocks if we have any, otherwise use item_end
            let actual_end = if blocks.is_empty() { item_end } else { span_end.saturating_sub(1) };

            Ok((ListItem {
                principal,
                blocks,
                level,
                marker,
                checked,
                location: state.create_location(span_start+offset, actual_end+offset),
            }, actual_end))
        }

        // After-marker variants: used when marker has already been consumed by parent rule
        // These are identical to the regular variants except they take marker as a parameter
        // instead of parsing it, and start after the marker position
        rule unordered_list_item_with_continuation_after_marker(offset: usize, block_metadata: &BlockParsingMetadata<'input>, marker: &'input str, marker_start: usize, parent_ordered_marker: Option<&'input str>) -> Result<(ListItem<'input>, usize), Error>
        = whitespace()
        checked:checklist_item()?
        first_line_start:position!()
        first_line:$((!(eol()) [_])*)
        continuation_lines:unordered_list_principal_continuation(marker, parent_ordered_marker)*
        first_line_end:position!()
        nested:(!at_list_separator() nested_content:unordered_list_item_nested_after_principal(offset, block_metadata, marker, parent_ordered_marker) { nested_content })?
        explicit_continuations:(!at_list_separator() cont:(
            list_explicit_continuation_immediate(offset, block_metadata)
            / list_explicit_continuation_ancestor(offset, block_metadata)
        ) { cont })*
        list_dangling_continuation()?
        {
            tracing::debug!(%first_line, ?continuation_lines, %marker, ?checked, "found unordered list item (after marker)");
            let level = ListLevel::try_from(ListItem::parse_depth_from_marker(marker).unwrap_or(1))?;
            let principal_text: &'input str = assemble_principal_text(state, first_line, &continuation_lines);
            let item_end = calculate_item_end(principal_text.is_empty(), span_start, first_line_end);

            let principal = if principal_text.trim().is_empty() {
                vec![]
            } else {
                let (principal, _) =
                    process_inlines(state, block_metadata, first_line_start, first_line_end, offset, principal_text)?;
                principal
            };

            let mut blocks = Vec::new();
            if let Some(Some(Ok(nested_list))) = nested {
                blocks.push(nested_list);
            }
            blocks.extend(explicit_continuations.into_iter().flatten());

            let actual_end = if blocks.is_empty() { item_end } else { span_end.saturating_sub(1) };

            Ok((ListItem {
                principal,
                blocks,
                level,
                marker,
                checked,
                location: state.create_location(marker_start+offset, actual_end+offset),
            }, actual_end))
        }

        rule unordered_list_item_no_continuation_after_marker(offset: usize, block_metadata: &BlockParsingMetadata<'input>, marker: &'input str, marker_start: usize, parent_ordered_marker: Option<&'input str>) -> Result<(ListItem<'input>, usize), Error>
        = whitespace()
        checked:checklist_item()?
        first_line_start:position!()
        first_line:$((!(eol()) [_])*)
        continuation_lines:unordered_list_principal_continuation(marker, parent_ordered_marker)*
        first_line_end:position!()
        nested:(!at_list_separator() nested_content:unordered_list_item_nested_after_principal(offset, block_metadata, marker, parent_ordered_marker) { nested_content })?
        immediate_continuations:(!at_list_separator() cont:list_explicit_continuation_immediate(offset, block_metadata) { cont })*
        {
            tracing::debug!(%first_line, ?continuation_lines, %marker, ?checked, "found unordered list item (after marker, immediate only)");
            let level = ListLevel::try_from(ListItem::parse_depth_from_marker(marker).unwrap_or(1))?;
            let principal_text: &'input str = assemble_principal_text(state, first_line, &continuation_lines);
            let item_end = calculate_item_end(principal_text.is_empty(), span_start, first_line_end);

            let principal = if principal_text.trim().is_empty() {
                vec![]
            } else {
                let (principal, _) =
                    process_inlines(state, block_metadata, first_line_start, first_line_end, offset, principal_text)?;
                principal
            };

            let mut blocks = Vec::new();
            if let Some(Some(Ok(nested_list))) = nested {
                blocks.push(nested_list);
            }
            blocks.extend(immediate_continuations.into_iter().flatten());

            let actual_end = if blocks.is_empty() { item_end } else { span_end.saturating_sub(1) };

            Ok((ListItem {
                principal,
                blocks,
                level,
                marker,
                checked,
                location: state.create_location(marker_start+offset, actual_end+offset),
            }, actual_end))
        }

        /// Parse nested content within an unordered list item (e.g., nested ordered or unordered list)
        /// Note: allow_continuation is false to prevent nested items from consuming parent-level continuations
        /// current_marker: the marker of the parent unordered list item (e.g., "*" or "**")
        /// parent_ordered_marker: the marker of an ancestor ordered list (if any), to prevent
        /// consuming sibling ordered markers that belong to a parent ordered list context
        rule unordered_list_item_nested_after_principal(offset: usize, block_metadata: &BlockParsingMetadata<'input>, current_marker: &'input str, parent_ordered_marker: Option<&'input str>) -> Option<Result<Block<'input>, Error>>
        = eol() nested:(
            unordered_list_item_nested_content_with_metadata(offset, block_metadata, current_marker, parent_ordered_marker)
            / unordered_list_item_nested_content(offset, block_metadata, current_marker, parent_ordered_marker)
          ) { nested }
        / eol()+ nested:unordered_list_item_nested_content(offset, block_metadata, current_marker, parent_ordered_marker) { nested }

        rule unordered_list_item_nested_content_with_metadata(offset: usize, block_metadata: &BlockParsingMetadata<'input>, current_marker: &'input str, parent_ordered_marker: Option<&'input str>) -> Option<Result<Block<'input>, Error>>
        = nested_start:position!()
          metadata:parsed_nested_list_metadata(offset, block_metadata.parent_section_level)
          nested_list_metadata_gap()*
          list:(
              !at_ancestor_ordered_marker(parent_ordered_marker)
              list:ordered_list(nested_start, offset, &metadata, Some(current_marker), false, false) { list }
              / &at_deeper_unordered_marker(current_marker)
                list:unordered_list_nested(nested_start, offset, &metadata, current_marker, parent_ordered_marker, true) { list }
          )
        {
            Some(list)
        }

        rule unordered_list_item_nested_content(offset: usize, block_metadata: &BlockParsingMetadata<'input>, current_marker: &'input str, parent_ordered_marker: Option<&'input str>) -> Option<Result<Block<'input>, Error>>
        // !at_ancestor_ordered_marker() prevents sibling ordered markers from a parent
        // ordered list context from being consumed by this nested unordered item.
        = !at_ancestor_ordered_marker(parent_ordered_marker) nested_start:position!() list:ordered_list(nested_start, offset, block_metadata, Some(current_marker), false, true) {
            Some(list)
        }
        // Nested unordered list with deeper markers (e.g., ** inside *)
        // Uses unordered_list_nested which only parses items deeper than current_marker
        / &at_deeper_unordered_marker(current_marker)
          nested_start:position!()
          list:unordered_list_nested(nested_start, offset, block_metadata, current_marker, parent_ordered_marker, false)
        {
            Some(list)
        }

        /// Parse a nested unordered list where all items have markers deeper than parent_marker.
        /// This is used to parse same-type nesting (e.g., ** inside *) as hierarchical content
        /// rather than flat siblings, enabling proper ancestor continuation handling.
        /// Uses allow_continuation=false to prevent nested items from consuming parent continuations.
        rule unordered_list_nested(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>, parent_marker: &str, parent_ordered_marker: Option<&'input str>, has_own_metadata: bool) -> Result<Block<'input>, Error>
        // Parse first item - must have a deeper marker than parent_marker
        = &at_deeper_unordered_marker(parent_marker)
          whitespace()* marker_start:position!() base_marker:$(unordered_list_marker()) &whitespace()
          first:unordered_list_item_after_marker(offset, block_metadata, false, base_marker, marker_start, parent_ordered_marker)
          // Parse rest items - only those at same level as base_marker (not deeper, not shallower than parent)
          rest:(unordered_list_nested_rest_item(offset, block_metadata, parent_marker, base_marker, parent_ordered_marker))*
        {
            tracing::debug!(?parent_marker, ?base_marker, "Found nested unordered list block");
            let mut content = vec![first?];
            for item in rest {
                content.push(item?);
            }
            let end = content.last().map_or(span_end, |(_, item_end)| *item_end);
            let items: Vec<ListItem<'input>> = content.into_iter().map(|(item, _)| item).collect();
            let marker = items.first().map_or("", |item| item.marker);

            Ok(Block::UnorderedList(UnorderedList {
                title: if has_own_metadata { block_metadata.title.clone() } else { Title::default() },
                metadata: if has_own_metadata { block_metadata.metadata.clone() } else { BlockMetadata::default() },
                items,
                marker,
                location: state.create_location(start+offset, end+offset),
            }))
        }

        /// Parse rest items in a nested unordered list.
        /// Items must be deeper than parent_marker and at same-or-deeper level as base_marker.
        /// Stops when we encounter a marker at or shallower than parent_marker.
        rule unordered_list_nested_rest_item(offset: usize, block_metadata: &BlockParsingMetadata<'input>, parent_marker: &str, base_marker: &str, parent_ordered_marker: Option<&'input str>) -> Result<(ListItem<'input>, usize), Error>
        // Case 1: No blank lines - accept same-level or deeper items
        = !at_list_separator() !eol() comment_line()*
          // Must not be at shallower-or-equal to parent (that would end the nested list)
          !at_shallower_or_equal_unordered_marker(parent_marker)
          item:unordered_list_item(offset, block_metadata, false, parent_ordered_marker)
        { item }
        // Case 2: Blank lines present - only accept same-level items (deeper would be its own nesting)
        / !at_list_separator() eol()+ comment_line()*
          // Must not be at shallower-or-equal to parent
          !at_shallower_or_equal_unordered_marker(parent_marker)
          // Must not be deeper than base (that would be nested inside this item)
          !at_deeper_unordered_marker(base_marker)
          item:unordered_list_item(offset, block_metadata, false, parent_ordered_marker)
        { item }

        // Helper rule to check if we're at a marker that's shallower than or equal to parent_marker
        // Used to terminate nested lists when encountering parent-level or ancestor-level items
        rule at_shallower_or_equal_unordered_marker(parent_marker: &str)
        = whitespace()* marker:unordered_list_marker() whitespace() {?
            if marker.len() <= parent_marker.len() { Ok(()) } else { Err("deeper") }
        }

        // See comment on unordered_list_item for why *_with/without_continuation variants exist.
        rule ordered_list_item(offset: usize, block_metadata: &BlockParsingMetadata<'input>, allow_continuation: bool, parent_unordered_marker: Option<&'input str>) -> Result<(ListItem<'input>, usize), Error>
        = item:ordered_list_item_with_continuation(offset, block_metadata, parent_unordered_marker) {? if allow_continuation { Ok(item) } else { Err("skip") } }
        / item:ordered_list_item_no_continuation(offset, block_metadata, parent_unordered_marker) { item }

        rule ordered_list_item_with_continuation(offset: usize, block_metadata: &BlockParsingMetadata<'input>, parent_unordered_marker: Option<&'input str>) -> Result<(ListItem<'input>, usize), Error>
        = whitespace()*
        marker:ordered_list_marker()
        whitespace()
        first_line_start:position!()
        // Parse first line (principal text)
        first_line:$((!(eol()) [_])*)
        // Parse continuation lines that are part of the same paragraph
        // Stop at: blank line, list item start, explicit continuation marker, section heading, or list separator
        continuation_lines:ordered_list_principal_continuation(marker, parent_unordered_marker)*
        first_line_end:position!()
        // Try to parse nested list (unordered, or ordered with deeper markers)
        // Don't consume newlines if we're at a list separator (comment or [])
        // Nested items cannot consume parent-level continuations (allow_continuation: false)
        // NOTE: nested_content is NOT optional here - if no nested content matches, the entire
        // alternative fails and backtracks, leaving eol() unconsumed for explicit_continuation
        nested:(!at_list_separator() nested_content:ordered_list_item_nested_after_principal(offset, block_metadata, marker, parent_unordered_marker) { nested_content })?
        // Try to parse explicit continuations (+ marker)
        // Don't consume newlines if we're at a list separator (comment or [])
        // Parent items accept both:
        // - Immediate continuations (0 empty lines) for content directly after principal text
        // - Ancestor continuations (1+ empty lines) for content that bubbles up from nested items
        // Use * to match a mixed sequence of immediate and ancestor continuations
        explicit_continuations:(!at_list_separator() cont:(
            list_explicit_continuation_immediate(offset, block_metadata)
            / list_explicit_continuation_ancestor(offset, block_metadata)
        ) { cont })*
        list_dangling_continuation()?
        {
            tracing::debug!(%first_line, ?continuation_lines, %marker, "found ordered list item");
            let level = ListLevel::try_from(ListItem::parse_depth_from_marker(marker).unwrap_or(1))?;
            let principal_text: &'input str = assemble_principal_text(state, first_line, &continuation_lines);
            let item_end = calculate_item_end(principal_text.is_empty(), span_start, first_line_end);

            // Process principal text as inline nodes
            let principal = if principal_text.trim().is_empty() {
                vec![]
            } else {
                let (principal, _) =
                    process_inlines(state, block_metadata, first_line_start, first_line_end, offset, principal_text)?;
                principal
            };

            let mut blocks = Vec::new();
            // nested_content is no longer optional in the grammar, so one less Some level
            if let Some(Some(Ok(nested_list))) = nested {
                blocks.push(nested_list);
            }
            // Collect all continuation blocks (each is a Result<Block<'input>, Error>)
            blocks.extend(explicit_continuations.into_iter().flatten());

            // Use end position after all blocks if we have any, otherwise use item_end
            let actual_end = if blocks.is_empty() { item_end } else { span_end.saturating_sub(1) };

            Ok((ListItem {
                principal,
                blocks,
                level,
                marker,
                checked: None,
                location: state.create_location(span_start+offset, actual_end+offset),
            }, actual_end))
        }

        // Version with immediate continuations only (for nested items)
        // Nested items consume continuations with 0 empty lines (immediate attachment).
        // Continuations with 1+ empty lines bubble up to ancestor items.
        rule ordered_list_item_no_continuation(offset: usize, block_metadata: &BlockParsingMetadata<'input>, parent_unordered_marker: Option<&'input str>) -> Result<(ListItem<'input>, usize), Error>
        = whitespace()*
        marker:ordered_list_marker()
        whitespace()
        first_line_start:position!()
        first_line:$((!(eol()) [_])*)
        continuation_lines:ordered_list_principal_continuation(marker, parent_unordered_marker)*
        first_line_end:position!()
        // Nested items can still have nested lists, but those also cannot consume parent continuations
        // NOTE: nested_content is NOT optional here - if no nested content matches, the entire
        // alternative fails and backtracks, leaving eol() unconsumed for immediate_continuation
        nested:(!at_list_separator() nested_content:ordered_list_item_nested_after_principal(offset, block_metadata, marker, parent_unordered_marker) { nested_content })?
        // Parse immediate continuations (0 empty lines) - these attach to this item
        // Ancestor continuations (1+ empty lines) bubble up to parent items
        immediate_continuations:(!at_list_separator() cont:list_explicit_continuation_immediate(offset, block_metadata) { cont })*
        {
            tracing::debug!(%first_line, ?continuation_lines, %marker, "found ordered list item (immediate continuation only)");
            let level = ListLevel::try_from(ListItem::parse_depth_from_marker(marker).unwrap_or(1))?;
            let principal_text: &'input str = assemble_principal_text(state, first_line, &continuation_lines);
            let item_end = calculate_item_end(principal_text.is_empty(), span_start, first_line_end);

            let principal = if principal_text.trim().is_empty() {
                vec![]
            } else {
                let (principal, _) =
                    process_inlines(state, block_metadata, first_line_start, first_line_end, offset, principal_text)?;
                principal
            };

            let mut blocks = Vec::new();
            // nested_content is no longer optional in the grammar, so one less Some level
            if let Some(Some(Ok(nested_list))) = nested {
                blocks.push(nested_list);
            }
            // Collect all immediate continuation blocks
            blocks.extend(immediate_continuations.into_iter().flatten());

            // Use end position after all blocks if we have any, otherwise use item_end
            let actual_end = if blocks.is_empty() { item_end } else { span_end.saturating_sub(1) };

            Ok((ListItem {
                principal,
                blocks,
                level,
                marker,
                checked: None,
                location: state.create_location(span_start+offset, actual_end+offset),
            }, actual_end))
        }

        // After-marker variants for ordered lists: used when marker has already been consumed by parent rule
        rule ordered_list_item_with_continuation_after_marker(offset: usize, block_metadata: &BlockParsingMetadata<'input>, marker: &'input str, marker_start: usize, parent_unordered_marker: Option<&'input str>) -> Result<(ListItem<'input>, usize), Error>
        = whitespace()
        first_line_start:position!()
        first_line:$((!(eol()) [_])*)
        continuation_lines:ordered_list_principal_continuation(marker, parent_unordered_marker)*
        first_line_end:position!()
        nested:(!at_list_separator() nested_content:ordered_list_item_nested_after_principal(offset, block_metadata, marker, parent_unordered_marker) { nested_content })?
        explicit_continuations:(!at_list_separator() cont:(
            list_explicit_continuation_immediate(offset, block_metadata)
            / list_explicit_continuation_ancestor(offset, block_metadata)
        ) { cont })*
        list_dangling_continuation()?
        {
            tracing::debug!(%first_line, ?continuation_lines, %marker, "found ordered list item (after marker)");
            let level = ListLevel::try_from(ListItem::parse_depth_from_marker(marker).unwrap_or(1))?;
            let principal_text: &'input str = assemble_principal_text(state, first_line, &continuation_lines);
            let item_end = calculate_item_end(principal_text.is_empty(), span_start, first_line_end);

            let principal = if principal_text.trim().is_empty() {
                vec![]
            } else {
                let (principal, _) =
                    process_inlines(state, block_metadata, first_line_start, first_line_end, offset, principal_text)?;
                principal
            };

            let mut blocks = Vec::new();
            if let Some(Some(Ok(nested_list))) = nested {
                blocks.push(nested_list);
            }
            blocks.extend(explicit_continuations.into_iter().flatten());

            let actual_end = if blocks.is_empty() { item_end } else { span_end.saturating_sub(1) };

            Ok((ListItem {
                principal,
                blocks,
                level,
                marker,
                checked: None,
                location: state.create_location(marker_start+offset, actual_end+offset),
            }, actual_end))
        }

        rule ordered_list_item_no_continuation_after_marker(offset: usize, block_metadata: &BlockParsingMetadata<'input>, marker: &'input str, marker_start: usize, parent_unordered_marker: Option<&'input str>) -> Result<(ListItem<'input>, usize), Error>
        = whitespace()
        first_line_start:position!()
        first_line:$((!(eol()) [_])*)
        continuation_lines:ordered_list_principal_continuation(marker, parent_unordered_marker)*
        first_line_end:position!()
        nested:(!at_list_separator() nested_content:ordered_list_item_nested_after_principal(offset, block_metadata, marker, parent_unordered_marker) { nested_content })?
        immediate_continuations:(!at_list_separator() cont:list_explicit_continuation_immediate(offset, block_metadata) { cont })*
        {
            tracing::debug!(%first_line, ?continuation_lines, %marker, "found ordered list item (after marker, immediate only)");
            let level = ListLevel::try_from(ListItem::parse_depth_from_marker(marker).unwrap_or(1))?;
            let principal_text: &'input str = assemble_principal_text(state, first_line, &continuation_lines);
            let item_end = calculate_item_end(principal_text.is_empty(), span_start, first_line_end);

            let principal = if principal_text.trim().is_empty() {
                vec![]
            } else {
                let (principal, _) =
                    process_inlines(state, block_metadata, first_line_start, first_line_end, offset, principal_text)?;
                principal
            };

            let mut blocks = Vec::new();
            if let Some(Some(Ok(nested_list))) = nested {
                blocks.push(nested_list);
            }
            blocks.extend(immediate_continuations.into_iter().flatten());

            let actual_end = if blocks.is_empty() { item_end } else { span_end.saturating_sub(1) };

            Ok((ListItem {
                principal,
                blocks,
                level,
                marker,
                checked: None,
                location: state.create_location(marker_start+offset, actual_end+offset),
            }, actual_end))
        }

        /// Parse nested content within an ordered list item (e.g., nested unordered or ordered list)
        /// Note: allow_continuation is false to prevent nested items from consuming parent-level continuations
        /// current_marker: the marker of the parent ordered list item (e.g., "." or "..")
        /// parent_unordered_marker: the marker of an ancestor unordered list (if any), to prevent
        /// consuming sibling unordered markers that belong to a parent unordered list context
        rule ordered_list_item_nested_after_principal(offset: usize, block_metadata: &BlockParsingMetadata<'input>, current_marker: &'input str, parent_unordered_marker: Option<&'input str>) -> Option<Result<Block<'input>, Error>>
        = eol() nested:(
            ordered_list_item_nested_content_with_metadata(offset, block_metadata, current_marker, parent_unordered_marker)
            / ordered_list_item_nested_content(offset, block_metadata, current_marker, parent_unordered_marker)
          ) { nested }
        / eol()+ nested:ordered_list_item_nested_content(offset, block_metadata, current_marker, parent_unordered_marker) { nested }

        rule ordered_list_item_nested_content_with_metadata(offset: usize, block_metadata: &BlockParsingMetadata<'input>, current_marker: &'input str, parent_unordered_marker: Option<&'input str>) -> Option<Result<Block<'input>, Error>>
        = nested_start:position!()
          metadata:parsed_nested_list_metadata(offset, block_metadata.parent_section_level)
          nested_list_metadata_gap()*
          list:(
              !at_ancestor_unordered_marker(parent_unordered_marker)
              list:unordered_list(nested_start, offset, &metadata, Some(current_marker), false, false) { list }
              / &at_deeper_ordered_marker(current_marker)
                list:ordered_list_nested(nested_start, offset, &metadata, current_marker, parent_unordered_marker, true) { list }
          )
        {
            Some(list)
        }

        rule ordered_list_item_nested_content(offset: usize, block_metadata: &BlockParsingMetadata<'input>, current_marker: &'input str, parent_unordered_marker: Option<&'input str>) -> Option<Result<Block<'input>, Error>>
        // !at_ancestor_unordered_marker() prevents sibling unordered markers from a parent
        // unordered list context from being consumed by this nested ordered item.
        = !at_ancestor_unordered_marker(parent_unordered_marker) nested_start:position!() list:unordered_list(nested_start, offset, block_metadata, Some(current_marker), false, true) {
            Some(list)
        }
        // Nested ordered list with deeper markers (e.g., .. inside .)
        // Uses ordered_list_nested which only parses items deeper than current_marker
        / &at_deeper_ordered_marker(current_marker)
          nested_start:position!()
          list:ordered_list_nested(nested_start, offset, block_metadata, current_marker, parent_unordered_marker, false)
        {
            Some(list)
        }

        /// Parse a nested ordered list where all items have markers deeper than parent_marker.
        /// This is used to parse same-type nesting (e.g., .. inside .) as hierarchical content
        /// rather than flat siblings, enabling proper ancestor continuation handling.
        /// Uses allow_continuation=false to prevent nested items from consuming parent continuations.
        rule ordered_list_nested(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>, parent_marker: &str, parent_unordered_marker: Option<&'input str>, has_own_metadata: bool) -> Result<Block<'input>, Error>
        // Parse first item - must have a deeper marker than parent_marker
        = &at_deeper_ordered_marker(parent_marker)
          whitespace()* marker_start:position!() base_marker:$(ordered_list_marker()) &whitespace()
          first:ordered_list_item_after_marker(offset, block_metadata, false, base_marker, marker_start, parent_unordered_marker)
          // Parse rest items - only those at same level as base_marker (not deeper, not shallower than parent)
          rest:(ordered_list_nested_rest_item(offset, block_metadata, parent_marker, base_marker, parent_unordered_marker))*
        {
            tracing::debug!(?parent_marker, ?base_marker, "Found nested ordered list block");
            let mut content = vec![first?];
            for item in rest {
                content.push(item?);
            }
            let end = content.last().map_or(span_end, |(_, item_end)| *item_end);
            let items: Vec<ListItem<'input>> = content.into_iter().map(|(item, _)| item).collect();
            let marker = items.first().map_or("", |item| item.marker);

            Ok(Block::OrderedList(OrderedList {
                title: if has_own_metadata { block_metadata.title.clone() } else { Title::default() },
                metadata: if has_own_metadata { block_metadata.metadata.clone() } else { BlockMetadata::default() },
                items,
                marker,
                location: state.create_location(start+offset, end+offset),
            }))
        }

        /// Parse rest items in a nested ordered list.
        /// Items must be deeper than parent_marker and at same-or-deeper level as base_marker.
        /// Stops when we encounter a marker at or shallower than parent_marker.
        rule ordered_list_nested_rest_item(offset: usize, block_metadata: &BlockParsingMetadata<'input>, parent_marker: &str, base_marker: &str, parent_unordered_marker: Option<&'input str>) -> Result<(ListItem<'input>, usize), Error>
        // Case 1: No blank lines - accept same-level or deeper items
        = !at_list_separator() !eol() comment_line()*
          // Must not be at shallower-or-equal to parent (that would end the nested list)
          !at_shallower_or_equal_ordered_marker(parent_marker)
          item:ordered_list_item(offset, block_metadata, false, parent_unordered_marker)
        { item }
        // Case 2: Blank lines present - only accept same-level items (deeper would be its own nesting)
        / !at_list_separator() eol()+ comment_line()*
          // Must not be at shallower-or-equal to parent
          !at_shallower_or_equal_ordered_marker(parent_marker)
          // Must not be deeper than base (that would be nested inside this item)
          !at_deeper_ordered_marker(base_marker)
          item:ordered_list_item(offset, block_metadata, false, parent_unordered_marker)
        { item }

        // Helper rule to check if we're at a marker that's shallower than or equal to parent_marker
        // Used to terminate nested lists when encountering parent-level or ancestor-level items
        rule at_shallower_or_equal_ordered_marker(parent_marker: &str)
        = whitespace()* marker:ordered_list_marker() whitespace() {?
            if marker.len() <= parent_marker.len() { Ok(()) } else { Err("deeper") }
        }

        /// Predicate rule that succeeds when we're NOT after a verbatim block
        /// Used with negative lookahead to ensure callout lists only match after verbatim blocks
        rule not_after_verbatim_block() -> ()
        = {?
            if state.last_block_was_verbatim {
                Err("is_after_verbatim")
            } else {
                Ok(())
            }
        }

        rule callout_list(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
        // !not_after_verbatim_block(): callout lists only make sense after source/listing
        // blocks The double negative succeeds only when last_block_was_verbatim is true
        = !not_after_verbatim_block()
        // OPTIMIZATION: This positive lookahead fails fast when not at a callout marker
        // (<1>, <.>, etc.) Without it, callout_list_item would be called and fail - same
        // result, just slower
        &(whitespace()* callout_list_marker() whitespace())
        first:callout_list_item(offset, block_metadata)
        rest:(callout_list_rest_item(offset, block_metadata))*
        {
            tracing::debug!("Found callout list block");
            let mut content = vec![first?];
            for item in rest {
                content.push(item?);
            }
            let end = content.last().map_or(span_end, |(_, _, item_end)| *item_end);

            // Resolve auto-numbered callouts and collect items
            let mut auto_number = 1usize;
            let mut items: Vec<CalloutListItem> = Vec::with_capacity(content.len());

            for (mut item, marker, _end) in content {
                // Resolve auto-numbered callouts
                if marker == "<.>" {
                    item.callout = CalloutRef::auto(auto_number, item.callout.location.clone());
                    auto_number += 1;
                }
                items.push(item);
            }

            // Validate callout list items
            for (expected_number, item) in (1..).zip(items.iter()) {
                let actual_number = item.callout.number;

                // Check sequential order
                if actual_number != expected_number {
                    state.add_generic_warning_at(
                        format!(
                            "callout list item index: expected {expected_number}, got {actual_number}"
                        ),
                        item.location.clone(),
                    );
                }

                // Check if the EXPECTED callout exists in the verbatim block
                // (This warns when sequence is broken and the expected number is missing)
                let callout_exists = state
                    .last_verbatim_callouts
                    .iter()
                    .any(|c| c.number == expected_number);
                if !callout_exists {
                    state.add_generic_warning_at(
                        format!("no callout found for <{expected_number}>"),
                        item.location.clone(),
                    );
                }
            }

            // Reset the flag after successfully parsing the callout list
            state.last_block_was_verbatim = false;
            state.last_verbatim_callouts.clear();

            Ok(Block::CalloutList(CalloutList {
                title: block_metadata.title.clone(),
                metadata: block_metadata.metadata.clone(),
                items,
                location: state.create_location(start+offset, end+offset),
            }))
        }

        rule callout_list_rest_item(offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<(CalloutListItem<'input>, String, usize), Error>
        = eol()+ item:callout_list_item(offset, block_metadata)
        {?
            Ok(item)
        }

        rule callout_list_item(offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<(CalloutListItem<'input>, String, usize), Error>
        = whitespace()*
        marker:callout_list_marker()
        whitespace()
        first_line_start:position!()
        // Parse first line (principal text)
        first_line:$((!(eol()) [_])*)
        // Parse continuation lines that are part of the same paragraph
        // Stop at list markers, explicit continuations, blank lines, section
        // headers, or block attributes.
        continuation_lines:(
            eol()
            !(whitespace()* (callout_list_marker() / unordered_list_marker() / ordered_list_marker() / section_level_marker() whitespace() / "[" / "+" whitespace()* eol() / eol()))
            line:$((!(eol()) [_])*)
            { line }
        )*
        first_line_end:position!()
        explicit_continuations:(!at_list_separator() cont:(
            list_explicit_continuation_immediate(offset, block_metadata)
            / list_explicit_continuation_ancestor(offset, block_metadata)
        ) { cont })*
        list_dangling_continuation()?
        {
            // Combine first line and continuation lines
            let principal_text_owned = if continuation_lines.is_empty() {
                first_line.to_string()
            } else {
                let mut text = first_line.to_string();
                for cont_line in continuation_lines {
                    text.push('\n');
                    text.push_str(cont_line);
                }
                text
            };
            let principal_text: &'input str = state.intern_str(&principal_text_owned);

            // The end position for the list item should be at the last character of content
            let item_end = if principal_text.is_empty() {
                span_start
            } else {
                first_line_end.saturating_sub(1)
            };

            // Process principal text as inline nodes
            let principal = if principal_text.trim().is_empty() {
                vec![]
            } else {
                let (principal, _) =
                    process_inlines(state, block_metadata, first_line_start, first_line_end, offset, principal_text)?;
                principal
            };

            let blocks = explicit_continuations.into_iter().flatten().collect::<Vec<_>>();

            let location = state.create_location(span_start+offset, item_end+offset);

            // Create a placeholder callout - will be resolved in callout_list
            // We pass the marker string to the parent rule for resolution
            let callout = if marker == "<.>" {
                CalloutRef::auto(0, location.clone()) // Number will be resolved later
            } else {
                let number = extract_callout_number(marker).unwrap_or(0);
                CalloutRef::explicit(number, location.clone())
            };

            let actual_end = if blocks.is_empty() { item_end } else { span_end.saturating_sub(1) };

            Ok((CalloutListItem {
                callout,
                principal,
                blocks,
                location: state.create_location(span_start+offset, actual_end+offset),
            }, marker.to_string(), actual_end))
        }

        rule checklist_item() -> ListItemCheckedStatus
            = checked:(("[x]" / "[X]" / "[*]") { ListItemCheckedStatus::Checked } / "[ ]" { ListItemCheckedStatus::Unchecked }) whitespace()
        {
            checked
        }

        rule check_start_of_description_list(offset: usize)
        = pos:position!() {?
            if find_dlist_marker(state.input.as_bytes(), pos + offset, true, true) {
                Ok(())
            } else {
                Err("no dlist marker before next blank line")
            }
        }

        /// Like check_start_of_description_list but restricted to the current line.
        /// Used by setext section rules to avoid false positives when a description
        /// list marker (::, ;;) appears later in the document but not on the current line.
        rule check_line_is_description_list(offset: usize)
        = pos:position!() {?
            if find_dlist_marker(state.input.as_bytes(), pos + offset, false, true) {
                Ok(())
            } else {
                Err("no dlist marker on current line")
            }
        }

        rule check_start_of_description_list_in_context(offset: usize, scan_across_eol: bool)
        = pos:position!() {?
            if find_dlist_marker(state.input.as_bytes(), pos + offset, scan_across_eol, true) {
                Ok(())
            } else {
                Err("no dlist marker in this block context")
            }
        }

        rule description_list(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>, scan_across_eol: bool) -> Result<Block<'input>, Error>
        = check_start_of_description_list_in_context(offset, scan_across_eol)
        first_item:description_list_item(offset, block_metadata)
        additional_items:description_list_additional_items(offset, block_metadata)*
        {
            tracing::debug!("Found description list block with auto-attachment support");
            let mut items = vec![first_item?];

            for additional in additional_items {
                items.push(additional?);
            }

            let actual_end = items.last().map_or(span_end, |item| {
                let loc_end = item.location.absolute_end;
                loc_end - offset
            });

            Ok(Block::DescriptionList(DescriptionList {
                title: block_metadata.title.clone(),
                metadata: block_metadata.metadata.clone(),
                items: build_description_list_topology(items),
                location: state.create_location(start+offset, actual_end+offset),
            }))
        }

        // Parse additional description list items (after potential auto-attached content)
        //
        // !at_dlist_block_boundary() prevents continuing the list when a blank line is
        // followed by block attributes. This allows attributes to apply to a new list.
        rule description_list_additional_items(offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<DescriptionListItem<'input>, Error>
        = !at_dlist_block_boundary()
        eol()*
        check_start_of_description_list(offset)
        item:description_list_item(offset, block_metadata)
        {
            tracing::debug!("Found additional description list item");
            item
        }

        rule description_list_item(offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<DescriptionListItem<'input>, Error>
        = term_start:position!()
        term:$((!(description_list_marker() (eol() / " " / ![_]) / eol()*<2,2>) [_])+)
        term_end:position!()
        delim_start:position!() delimiter:description_list_marker() delim_end:position!()
        whitespace()?
        principal_start:position!()
        principal_content:$(
            (!eol() [_])*
            // Implicit text continuation: consume subsequent non-blank lines that
            // aren't new dlist entries, list items, continuation markers, or block
            // delimiters. This mirrors paragraph multi-line handling but with
            // dlist-specific stop conditions.
            (eol()
             !eol()                                    // not a blank line
             !check_line_is_description_list(offset)
             !(whitespace()* (unordered_list_marker() / ordered_list_marker()) whitespace())  // not a list item
             !("+" (whitespace() / eol() / ![_]))      // not a continuation marker
             !example_delimiter()                      // not a block delimiter
             !listing_delimiter()
             !literal_delimiter()
             !sidebar_delimiter()
             !quote_delimiter()
             !pass_delimiter()
             !comment_delimiter()
             !table_delimiter()
             !(open_delimiter() (whitespace()* eol()))
             !markdown_code_delimiter()
             !attributes_line()                           // not a block attributes line
             !((anchor() / attributes_line())* section_level_at_line_start(offset, None) (whitespace() / eol() / ![_]))  // not a section heading
             (!eol() [_])+                             // continuation line content
            )*
        )
        // Now handle auto-attachment and explicit continuation
        attached_content:description_list_attached_content(offset, block_metadata)*
        {
            tracing::debug!(%term, %delimiter, "parsing description list item with auto-attachment");

            let trimmed_term = term.trim();
            let leading_whitespace = term.len() - term.trim_start().len();
            let term_start = term_start + leading_whitespace;
            let term_end = term_end - (term.len() - term.trim_end().len());
            let (term, _) = process_inlines(
                state,
                block_metadata,
                term_start,
                term_end,
                offset,
                trimmed_term,
            )?;

            let principal_end = principal_start + principal_content.len();
            let principal_text = if principal_content.trim().is_empty() {
                Vec::new()
            } else {
                // Parse as inline content with attribute substitution
                let (principal, _) = process_inlines(
                    state,
                    block_metadata,
                    principal_start,
                    principal_end,
                    offset,
                    principal_content.trim(),
                )?;
                principal
            };

            // Collect all attached blocks (auto-attached and explicitly continued)
            let mut description = Vec::with_capacity(attached_content.len());
            for content in attached_content {
                match content {
                    Ok(blocks) => description.extend(blocks),
                    Err(e) => {
                        tracing::error!(?e, "Error processing attached content");
                    }
                }
            }

            // Calculate actual end from last attached block, or fall back to end of principal/term.
            // The injected `span_end` captures position after consuming blank lines looking for more
            // continuations (start of the next item), so it's not the right end either — we want
            // the actual content end.
            let actual_end = description.last().map_or_else(
                || {
                    // No attached content: use end of principal text line
                    if principal_content.is_empty() {
                        // Just term + delimiter
                        principal_start
                    } else {
                        principal_start + principal_content.len()
                    }
                },
                |b| {
                    let loc = b.location();
                    loc.absolute_end - offset
                },
            );

            let delimiter_location = state.create_block_location(delim_start, delim_end, offset);
            Ok(DescriptionListItem {
                anchors: vec![],
                term,
                delimiter,
                delimiter_location: Some(delimiter_location),
                principal_text,
                description,
                location: state.create_location(span_start+offset, actual_end+offset),
            })
        }

        rule description_list_attached_content(offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Vec<Block<'input>>, Error>
        = eol() content:(
            // Explicit continuation - this uses +, allows any content including delimited
            // blocks
            description_list_explicit_continuation(offset, block_metadata)
            // Auto-attach lists (even with blank lines before them)
            / description_list_auto_attached_list(offset, block_metadata)
        )
        {
            content
        }

        rule description_list_auto_attached_list(offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Vec<Block<'input>>, Error>
        = eol()* // Consume any blank lines before the list
        &(whitespace()* (unordered_list_marker() / ordered_list_marker()) whitespace())
        list_start:position!()
        list:(unordered_list(list_start, offset, block_metadata, None, true, true) / ordered_list(list_start, offset, block_metadata, None, true, true))
        {
            tracing::debug!("Auto-attaching list to description list item");
            Ok(vec![list?])
        }

        // Parse one or more explicit continuations for description lists
        // Same pattern as list_explicit_continuation: + marker followed by a single block
        // Uses block_in_continuation to prevent lists inside continuations from consuming
        // further continuations that belong to the parent item
        rule description_list_explicit_continuation(offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Vec<Block<'input>>, Error>
        = continuations:(
            eol()* "+" eol()
            block:block_in_continuation(offset, block_metadata.parent_section_level)
            { block }
          )+
        {
            tracing::debug!(count = continuations.len(), "Description list explicit continuation blocks");
            Ok(continuations.into_iter().filter_map(Result::ok).collect())
        }

        // Parse a single immediate continuation (0 empty lines before +)
        // These attach to the current (most recent) list item per AsciiDoc spec.
        // Uses block_in_continuation to prevent lists inside continuations from consuming
        // further continuations that belong to the parent item.
        // Pattern: exactly one newline before + (content\n+\nblock)
        rule list_explicit_continuation_immediate(offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
        = eol() !eol() "+" eol()
          block:block_in_continuation(offset, block_metadata.parent_section_level)
        {
            tracing::debug!("List immediate continuation block (0 empty lines)");
            block
        }

        // Parse a single ancestor continuation (1+ empty lines before +)
        // Per AsciiDoc spec: each empty line before + moves attachment up one nesting level.
        // 1 empty line = parent, 2 empty lines = grandparent, etc.
        // Uses block_in_continuation to prevent lists inside continuations from consuming
        // further continuations that belong to the parent item.
        // Pattern: two or more newlines before + (content\n\n+\nblock)
        rule list_explicit_continuation_ancestor(offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
        = eol() eol()+ "+" eol()
          block:block_in_continuation(offset, block_metadata.parent_section_level)
        {
            tracing::debug!("List ancestor continuation block (1+ empty lines)");
            block
        }

        // Consume a "dangling" list continuation marker: a `+` on its own line with no
        // attachable block after it (the following line is blank, or the input ends).
        // asciidoctor silently drops such a marker; without this the leftover `+` is
        // parsed as a standalone paragraph and prematurely terminates the list. Only
        // tried after the immediate/ancestor continuation rules, so a `+` with real
        // content still attaches.
        //
        // We consume up to and including the marker's own line terminator, then assert a
        // blank line or end of input follows (`&(eol() / ![_])`) — that lookahead is what
        // makes it "dangling": a `+` with real content on the next line is left alone. Any
        // following blank line is left unconsumed so the list resumes with the next item.
        rule list_dangling_continuation()
        = eol()+ "+" whitespace()* eol()? &(eol() / ![_])
        {
            tracing::debug!("Dropped dangling list continuation marker");
        }

        // Parse a quoted paragraph: "content" followed by `-- attribution[, citation]`
        //
        // This matches the AsciiDoc shorthand syntax for blockquotes:
        // ```
        // "I hold it that a little rebellion now and then is a good thing."
        // -- Thomas Jefferson, Papers of Thomas Jefferson
        // ```
        rule quoted_paragraph(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
        = content_start:position!()
          "\"" quoted_content:$((!"\"" [_])+) "\""
          eol()
          "-- " attr_start:position!() attribution_line:$([^'\n']+)
        {
            tracing::debug!(?quoted_content, ?attribution_line, "found quoted paragraph");

            // Parse attribution line: "Author Name, Source Title" or just "Author Name"
            // Intern the slices into the parser arena so downstream inline parsing
            // can produce nodes with the `'input` lifetime.
            let (attr_str, cite_str): (&'input str, Option<&'input str>) = match attribution_line.split_once(',') {
                Some((attr, cite)) => (state.intern_str(attr.trim()), Some(state.intern_str(cite.trim()))),
                None => (state.intern_str(attribution_line.trim()), None),
            };

            // Parse attribution through inline pipeline
            let attr_end_offset = attr_start + attr_str.len();
            let (attr_inlines, _) = process_inlines(
                state,
                block_metadata,
                attr_start,
                attr_end_offset,
                offset,
                attr_str,
            )?;

            // Parse citation through inline pipeline if present
            let cite_inlines = if let Some(cite) = cite_str {
                let cite_offset_in_line = attribution_line.find(',').unwrap_or(0) + 1;
                let cite_raw_start = attr_start + cite_offset_in_line + (attribution_line[cite_offset_in_line..].len() - attribution_line[cite_offset_in_line..].trim_start().len());
                let cite_pos = PositionWithOffset {
                    offset: cite_raw_start,
                    position: state.line_map.offset_to_position(cite_raw_start, state.input),
                };
                let (cite_inlines, _) = process_inlines(
                    state,
                    block_metadata,
                    cite_pos.offset,
                    cite_raw_start + cite.len(),
                    offset,
                    cite,
                )?;
                Some(cite_inlines)
            } else {
                None
            };

            // Parse the quoted content as blocks
            let blocks = document_parser::blocks(quoted_content, state, content_start + offset, block_metadata.parent_section_level, None).unwrap_or_else(|e| {
                adjust_and_log_parse_error(&e, quoted_content, content_start + offset, state, "Error parsing content as blocks in quoted paragraph");
                Ok(Vec::new())
            })?;

            // Build metadata with quote style and attribution
            let mut metadata = block_metadata.metadata.clone();
            metadata.style = Some("quote");
            metadata.attribution = Some(Attribution::new(attr_inlines));
            if let Some(inlines) = cite_inlines {
                metadata.citetitle = Some(CiteTitle::new(inlines));
            }

            Ok(Block::DelimitedBlock(DelimitedBlock {
                metadata,
                delimiter: "\"",
                inner: DelimitedBlockType::DelimitedQuote(blocks),
                title: block_metadata.title.clone(),
                location: state.create_block_location(start, span_end, offset),
                open_delimiter_location: None,
                close_delimiter_location: None,
            }))
        }

        /// Parse a markdown-style blockquote: lines starting with `> `
        ///
        /// This matches the Markdown-compatible syntax for blockquotes:
        /// ```
        /// > I hold it that a little rebellion now and then is a good thing,
        /// > and as necessary in the political world as storms in the physical.
        /// > -- Thomas Jefferson, Papers of Thomas Jefferson: Volume 11
        /// ```
        ///
        /// The content after `> ` on each line is joined and parsed as blocks.
        /// Attribution is extracted from a line matching `> -- Author[, Citation]`.
        rule markdown_blockquote(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
        = lines:markdown_blockquote_content_line()+ attribution:markdown_blockquote_attribution()?
        {
            tracing::debug!(?lines, ?attribution, "found markdown blockquote");

            let content: &'input str = state.intern_join(lines.iter(), "\n");
            let content_start = start;

            // Build metadata with quote style and attribution
            let mut metadata = block_metadata.metadata.clone();
            metadata.style = Some("quote");
            if let Some((author, author_start, citation)) = attribution {
                let author: &'input str = state.intern_str(&author);
                // Parse author through inline pipeline
                let author_pos = PositionWithOffset {
                    offset: author_start,
                    position: state.line_map.offset_to_position(author_start, state.input),
                };
                let attr_end_offset = author_start + author.len();
                let (attr_inlines, _) = process_inlines(
                    state,
                    block_metadata,
                    author_pos.offset,
                    attr_end_offset,
                    offset,
                    author,
                )?;
                metadata.attribution = Some(Attribution::new(attr_inlines));

                if let Some((cite, cite_start)) = citation {
                    let cite: &'input str = state.intern_str(&cite);
                    // Parse citation through inline pipeline
                    let cite_pos = PositionWithOffset {
                        offset: cite_start,
                        position: state.line_map.offset_to_position(cite_start, state.input),
                    };
                    let (cite_inlines, _) = process_inlines(
                        state,
                        block_metadata,
                        cite_pos.offset,
                        cite_start + cite.len(),
                        offset,
                        cite,
                    )?;
                    metadata.citetitle = Some(CiteTitle::new(cite_inlines));
                }
            }

            let location = state.create_block_location(start, span_end, offset);

            // Parse the content as blocks
            let blocks = if content.trim().is_empty() {
                Vec::new()
            } else {
                document_parser::blocks(content, state, content_start + offset, block_metadata.parent_section_level, None).unwrap_or_else(|e| {
                    adjust_and_log_parse_error(&e, content, content_start + offset, state, "Error parsing content as blocks in markdown blockquote");
                    Ok(Vec::new())
                })?
            };

            Ok(Block::DelimitedBlock(DelimitedBlock {
                metadata,
                delimiter: ">",
                inner: DelimitedBlockType::DelimitedQuote(blocks),
                title: block_metadata.title.clone(),
                location,
                open_delimiter_location: None,
                close_delimiter_location: None,
            }))
        }

        /// Match a content line of a markdown-style blockquote
        /// A line is content if:
        /// 1. It's followed by another `>` line (so `> -- ...` mid-blockquote is content)
        /// 2. OR it doesn't start with `-- ` (so it can't be attribution)
        rule markdown_blockquote_content_line() -> &'input str
        = "> " content:$([^'\n']*) eol() &">" { content }
        / "> " !("-- ") content:$([^'\n']*) (eol() / ![_]) { content }
        / ">" eol() &">" { "" }
        / ">" eol() { "" }
        / ">" ![_] { "" }

        /// Match an attribution line: `> -- Author[, Citation]`
        /// Only matches at the END of a blockquote (not followed by more `>` lines)
        /// Returns (author, author_start, Option<(citation, cite_start)>)
        rule markdown_blockquote_attribution() -> (String, usize, Option<(String, usize)>)
        = "> -- " author_start:position!() author:$([^(',' | '\n')]+) ", " cite_start:position!() citation:$([^'\n']+) ((eol() !">") / ![_]) {
            (author.trim().to_string(), author_start, Some((citation.trim().to_string(), cite_start)))
        }
        / "> -- " author_start:position!() author:$([^'\n']+) ((eol() !">") / ![_]) {
            (author.trim().to_string(), author_start, None)
        }

        rule paragraph(start: usize, offset: usize, block_metadata: &BlockParsingMetadata<'input>) -> Result<Block<'input>, Error>
        = admonition:admonition()?
        content_start:position!()
        content:$((
            "[[" (!eol() [_])*
            / !(
            eol()*<2,>
            / eol()* ![_]
            / eol() &attributes_line()
            / eol() example_delimiter()
            / eol() listing_delimiter()
            / eol() literal_delimiter()
            / eol() sidebar_delimiter()
            / eol() quote_delimiter()
            / eol() pass_delimiter()
            / eol() table_delimiter()
            / eol() markdown_code_delimiter()
            / eol() comment_delimiter()
            / eol() open_delimiter() &(whitespace()* eol())
            / eol() !not_after_verbatim_block() &(whitespace()* callout_list_marker() whitespace())
            / eol() list(start, offset, block_metadata)
            / eol() &("+" (whitespace() / eol() / ![_]))  // Stop at list continuation marker
            / eol()* &((anchor() / attributes_line())* section_level_at_line_start(offset, None) (whitespace() / eol() / ![_]))
            ) [_]
        )+)
        {
            let is_styled_verbatim = matches!(
                block_metadata.metadata.style,
                Some("source" | "listing" | "literal")
            );

            // Reset the verbatim flag unless this paragraph establishes a new
            // callout scope below.
            state.last_block_was_verbatim = false;

            // A `[comment]`-styled paragraph is a comment that produces no
            // output; keep its raw text on the `Comment` for tooling.
            if block_metadata.metadata.style == Some("comment") {
                return Ok(Block::Comment(Comment {
                    kind: CommentKind::Paragraph,
                    content,
                    location: state.create_block_location(start, span_end, offset),
                }));
            }

            // Check if this is a literal paragraph BEFORE preprocessing
            //
            // Literal paragraphs start with a space and should not have inline
            // preprocessing applied
            if content.starts_with(' ')
                && !matches!(
                    block_metadata.metadata.style,
                    Some("source" | "listing" | "literal")
                )
            {
                return Ok(get_literal_paragraph(state, content, start, span_end, offset, block_metadata));
            }

            let content = if is_styled_verbatim {
                let content_location =
                    state.create_block_location(content_start, span_end, offset);
                let (verbatim_content, callouts) = resolve_verbatim_callouts(
                    state,
                    content,
                    content_location,
                    block_metadata
                        .substitutions
                        .enabled(&Substitution::Callouts),
                );
                let content = if callouts.is_empty() {
                    process_inlines(
                        state,
                        block_metadata,
                        content_start,
                        span_end,
                        offset,
                        content,
                    )?
                    .0
                } else {
                    verbatim_content
                };
                state.last_block_was_verbatim = true;
                state.last_verbatim_callouts = callouts;
                content
            } else {
                process_inlines(
                    state,
                    block_metadata,
                    content_start,
                    span_end,
                    offset,
                    content,
                )?
                .0
            };

            // Title should either be an attribute named title, or the title parsed from the block metadata
            let title: Title = if let Some(AttributeValue::String(title)) = block_metadata.metadata.attributes.get("title") {
                vec![InlineNode::PlainText(Plain {
                    content: state.intern_cow(title.clone()),
                    location: state.create_location(start+offset, (start+offset).saturating_add(title.len()).saturating_sub(1)),
                    escaped: false,
                })].into()
            } else {
                block_metadata.title.clone()
            };

            if let Some((variant, admonition_start, admonition_end)) = admonition {
                let Ok(parsed_variant) = AdmonitionVariant::from_str(&variant) else {
                    tracing::error!(%variant, "invalid admonition variant");
                    return Err(Error::InvalidAdmonitionVariant(
                        Box::new(state.create_error_source_location(state.create_location(admonition_start + offset, admonition_end + offset - 1))),
                        variant
                    ));
                };
                tracing::debug!(%variant, "found admonition block with variant");
                Ok(Block::Admonition(Admonition{
                    metadata: block_metadata.metadata.clone(),
                    title,
                    blocks: vec![Block::Paragraph(Paragraph {
                        content,
                        metadata: block_metadata.metadata.clone(),
                        title: Title::default(),
                        location: state.create_block_location(content_start, span_end, offset),
                    })],
                    location: state.create_block_location(start, span_end, offset),
                    variant: parsed_variant,

                }))
            } else {
                let mut metadata = block_metadata.metadata.clone();
                metadata.move_positional_attributes_to_attributes();

                tracing::debug!(?content, "found paragraph block");
                Ok(Block::Paragraph(Paragraph {
                    content,
                    metadata,
                    title,
                    location: state.create_block_location(start, span_end, offset),
                }))
            }
        }

        rule admonition() -> (String, usize, usize)
            = variant:$("NOTE" / "WARNING" / "TIP" / "IMPORTANT" / "CAUTION") ": "
        {
            (variant.to_string(), span_start, span_end)
        }

        // Lookahead rule that warns about anchor ID-like patterns containing whitespace.
        //
        // This uses negative lookahead and emits a warning if it detects whitespace. It
        // does not consume the input.
        rule warn_anchor_id_with_whitespace() -> ()
        = &(
            id:$([^'\'' | ',' | ']' | '.' | '#']+)
            {?
                if id.chars().any(char::is_whitespace) {
                    let location = state.create_location(span_start, span_end);
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

        rule anchor() -> Anchor<'input>
        = result:(
            // Double-bracket [[id]] syntax - allows dots in ID since no role shorthand
            // possible.
            //
            // Whitespace is excluded per AsciiDoc documentation at
            // https://docs.asciidoctor.org/asciidoc/latest/attributes/id/#valid-id-characters
            double_open_square_bracket() warn_anchor_id_with_whitespace()? id:$([^'\'' | ',' | ']' | ' ' | '\t' | '\n' | '\r']+) comma() reftext:$([^']']+) double_close_square_bracket() {
                (id, Some(reftext))
            } /
            double_open_square_bracket() warn_anchor_id_with_whitespace()? id:$([^'\'' | ',' | ']' | ' ' | '\t' | '\n' | '\r']+) double_close_square_bracket() {
                (id, None)
            } /
            // Single-bracket [#id] shorthand - exclude '.', '%' as they start role/option
            // shorthands.
            //
            // Only the bare `[#id]` form is an anchor here; `[#id,...]` is NOT — the
            // comma introduces further block attributes (e.g. `[#id,discrete]`), so it
            // must fall through to the attribute-line parser where `#id` becomes the id
            // and the rest are positional/named attributes. Unlike `[[id,reftext]]`, a
            // single-bracket comma does not set a reftext (matching asciidoctor).
            //
            // Whitespace is excluded per AsciiDoc documentation at
            // https://docs.asciidoctor.org/asciidoc/latest/attributes/id/#valid-id-characters
            open_square_bracket() "#" warn_anchor_id_with_whitespace()? id:$([^'\'' | ',' | ']' | '.' | '%' | ' ' | '\t' | '\n' | '\r']+) close_square_bracket() {
                (id, None)
            }
        )
        end:position!()
        eol()
        {
            let (id, reftext) = result;
            let substituted_id = state.intern_cow(substitute(id, HEADER, &state.document_attributes));
            let substituted_reftext = reftext.map(|rt| state.intern_cow(substitute(rt, HEADER, &state.document_attributes)));
            // `end` is captured before the trailing eol() so the anchor's
            // location doesn't include the newline.
            Anchor {
                id: substituted_id,
                xreflabel: substituted_reftext,
                location: state.create_location(span_start, end),
                bibliography: false,
            }
        }

        rule inline_anchor(offset: usize) -> InlineNode<'input>
        = double_open_square_bracket()
        // Whitespace is excluded - IDs must not contain spaces
        warn_anchor_id_with_whitespace()?
        id:$([^'\'' | ',' | ']' | '[' | ' ' | '\t' | '\n' | '\r']+)
        reftext:(
            comma() reftext:$([^']']+) {
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
                location: state.create_block_location(span_start, span_end, offset),
                bibliography: false,
            })
        }

        rule inline_anchor_match() -> ()
        = double_open_square_bracket() [^'\'' | ',' | ']' | '[' | ' ' | '\t' | '\n' | '\r']+ (comma() [^']']+)? double_close_square_bracket()

        rule invalid_bibliography_anchor(offset: usize) -> InlineNode<'input>
        = syntax:$("[[[" [^']' | '\n']* "]]]") {?
            let body = &syntax[3..syntax.len() - 3];
            let id = body.split_once(',').map_or(body, |(id, _)| id);
            if is_valid_bibliography_id(id) {
                Err("valid bibliography anchor")
            } else {
                Ok(InlineNode::PlainText(Plain {
                    content: syntax,
                    location: state.create_block_location(span_start, span_end, offset),
                    escaped: false,
                }))
            }
        }

        rule attributes_line() -> (bool, BlockMetadata<'input>)
            // Don't match empty [] followed by blank line - that's a list separator, not
            // block attributes. Without this, `[]\n\n` would be parsed as an empty
            // attributes line, breaking list separation
            = !empty_list_separator() attributes:attributes() eol() {
                let (discrete, metadata, _title_position) = attributes;
                (discrete, metadata)
            }

        // Empty brackets followed by a blank line is a list separator
        rule empty_list_separator()
            = whitespace()* "[" whitespace()* "]" whitespace()* eol() eol()

        pub(crate) rule attributes() -> (bool, BlockMetadata<'input>, Option<(usize, usize)>)
            = !double_open_square_bracket()
              open_square_bracket()
              content_start:position!()
              content:attribute_list_content()
            {
                parse_block_attribute_list(
                    state,
                    content,
                    content_start,
                    span_end,
                    BlockAttributeMode::Block,
                )
            }

        /// Macro attribute parsing - simpler than block attributes.
        ///
        /// Does NOT support shorthand syntax (.role, #id, %option).
        /// Shorthands are only valid in block-level attributes, not inside macro brackets.
        ///
        /// Asciidoctor behavior:
        /// - `image::photo.jpg[.role]` -> alt=".role" (literal text, NOT a role)
        /// - `image::photo.jpg[Diablo 4 picture of Lilith.]` -> alt="Diablo 4 picture of Lilith."
        pub(crate) rule macro_attributes() -> (bool, BlockMetadata<'input>, Option<(usize, usize)>)
            = macro_attributes_for(MacroAttributeContext::General)

        rule image_macro_attributes() -> (bool, BlockMetadata<'input>, Option<(usize, usize)>)
            = macro_attributes_for(MacroAttributeContext::Image)

        rule macro_attributes_for(context: MacroAttributeContext) -> (bool, BlockMetadata<'input>, Option<(usize, usize)>)
            = open_square_bracket()
              content_start:position!()
              content:attribute_list_content()
            {
                parse_block_attribute_list(
                    state,
                    content,
                    content_start,
                    span_end,
                    BlockAttributeMode::Macro(context),
                )
            }

        rule open_square_bracket() = "["
        rule close_square_bracket() = "]"
        rule attribute_list_content() -> &'input str
            = content:$((!last_close_square_bracket() [^'\n' | '\r'])*) close_square_bracket() { content }
        rule last_close_square_bracket()
            = &("]" [^']' | '\n' | '\r']* (eol() / ![_]))
        rule double_open_square_bracket() = "[["
        rule double_close_square_bracket() = "]]"
        rule comma() = ","
        rule period() = "."
        /// URL rule matches both web URLs (proto://) and mailto: URLs
        pub rule url() -> String =
        proto:$("https" / "http" / "ftp" / "irc") "://" path:url_path() { format!("{proto}://{path}") }
        / "mailto:" email:email_address() { format!("mailto:{email}") }

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
        rule email_address() -> String
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

            Ok(format!("{local}@{domain}"))
        }

        /// URL target content following `://`.
        /// Supports query parameters, fragments, and percent escapes while excluding
        /// brackets that delimit the macro attributes.
        /// Spaces must be internal to the target.
        rule url_path() -> String = path:$(url_path_char() (url_path_char() / internal_url_path_spaces())*)
        {?
            let inline_state = InlinePreprocessorParserState::new_all_enabled(
                path,
                state.line_map.clone(),
                state.input,
                state.arena,
            );
            let processed = inline_preprocessing::run(path, &state.document_attributes, &inline_state)
            .map_err(|e| {
                tracing::error!(?e, "could not preprocess url path");
                "could not preprocess url path"
            })?;
            // Strip backslash escapes before URL parsing to prevent the url crate
            // from normalizing backslashes to forward slashes
            let result = strip_url_backslash_escapes(&processed.text).into_owned();
            let warnings = inline_state.drain_warnings();
            drop(inline_state);
            for warning in warnings {
                state.add_inline_preprocessor_warning(warning);
            }
            Ok(result)
        }

        rule url_path_char() = ['A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '.' | '_' | '~' | ':' | '/' | '?' | '#' | '@' | '!' | '$' | '&' | '\'' | '(' | ')' | '*' | '+' | ',' | ';' | '=' | '%' | '\\' ]
        rule internal_url_path_spaces() = [' ']+ &url_path_char()

        /// URL for bare autolinks — avoids capturing trailing sentence punctuation
        /// (., ;, !, etc.) by only consuming punctuation when more URL chars follow.
        rule bare_url() -> String =
        proto:$("https" / "http" / "ftp" / "irc") "://" path:bare_url_path()
        { format!("{proto}://{path}") }

        /// URL path for bare autolinks. Like url_path() but:
        /// - Trailing punctuation (. , ; ! ? : ' *) only consumed when followed by more URL chars.
        /// - `)` only consumed as part of a balanced `(...)` group, preventing capture of
        ///   sentence-level parens like `(see http://example.com)`.
        rule bare_url_path() -> String = path:$(
            bare_url_safe_char()
            ( bare_url_safe_char()
            / bare_url_paren_group()
            / "("
            / bare_url_trailing_char() &bare_url_char()
            )*
        )
        {?
            let inline_state = InlinePreprocessorParserState::new_all_enabled(
                path,
                state.line_map.clone(),
                state.input,
                state.arena,
            );
            let processed = inline_preprocessing::run(path, &state.document_attributes, &inline_state)
                .map_err(|e| {
                    tracing::error!(?e, "could not preprocess bare url path");
                    "could not preprocess bare url path"
                })?;
            let result = strip_url_backslash_escapes(&processed.text).into_owned();
            let warnings = inline_state.drain_warnings();
            drop(inline_state);
            for warning in warnings {
                state.add_inline_preprocessor_warning(warning);
            }
            Ok(result)
        }

        /// Balanced parenthesized group in a URL path.
        /// Handles nested parens: `http://example.com/wiki/Foo_(bar_(baz))`
        /// Only `)` consumed via this rule — unbalanced `)` is never captured.
        rule bare_url_paren_group()
        = "(" (bare_url_safe_char() / bare_url_trailing_char() / bare_url_paren_group() / "(")* ")"

        /// URL chars that are safe to end a bare URL — won't be confused with sentence punctuation.
        /// Excludes `(` and `)` which are handled separately via `bare_url_paren_group`.
        rule bare_url_safe_char() = ['A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '~'
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

        /// Fragment identifier for URLs and cross-references (e.g., `#section-id`)
        /// Only used by `xref:` and `link:` macros — other macros (`image::`, `video::`, etc.) do not support fragments
        rule path_fragment() -> String
            = "#" fragment:$(['a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-']+)
        {
            format!("#{fragment}")
        }

        /// Filesystem path accepted by block macros.
        ///
        /// ASCII input uses a conservative filename set. Non-ASCII Unicode characters
        /// are accepted unchanged, and `{`/`}` permit `AsciiDoc` attribute substitution.
        /// Existing percent escapes and internal spaces are preserved.
        pub rule path() -> String = path:$(path_char() (path_char() / internal_path_spaces())*)
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
            let result = processed.text.into_owned();
            let warnings = inline_state.drain_warnings();
            drop(inline_state);
            for warning in warnings {
                state.add_inline_preprocessor_warning(warning);
            }
            Ok(result)
        }

        rule path_char() = ['A'..='Z' | 'a'..='z' | '0'..='9' | '{' | '}' | '_' | '-' | '.' | '/' | '\\' | '%' | '\u{80}'..='\u{10FFFF}' ]
        rule internal_path_spaces() = [' ']+ &path_char()


        pub rule source() -> Source<'input>
            = source:
        (
            u:url() {?
                let interned = state.intern_str(&u);
                Source::from_str_borrowed(interned).map_err(|_| "failed to parse URL")
            }
            / p:path() {?
                let interned = state.intern_str(&p);
                Source::from_str_borrowed(interned).map_err(|_| "failed to parse path")
            }
        )
        { source }

        rule digits() = ['0'..='9']+

        rule whitespace() = quiet!{ " " / "\t" }
        rule eol() = quiet!{ "\n" }

        rule comment_line() = quiet!{ comment() (eol() / ![_]) }
        rule comment() = quiet!{ "//" [^'\n']+ (&eol() / ![_]) }

        // Value parsing for document attributes
        // Handles both single-line values and values with continuation markers (" \" or " + \")
        // The preprocessor preserves these markers for the parser to handle
        rule document_attribute_value() -> String
        = " " lines:document_attribute_value_lines()
        {
            lines.join("\n")
        }

        // Parse value lines, continuing while lines end with backslash
        rule document_attribute_value_lines() -> Vec<&'input str>
        = backslash_continuation_lines() / single_line:$([^'\n']+) { vec![single_line] }

        // Lines ending with backslash continuation - keeps consuming lines until one doesn't end with backslash
        rule backslash_continuation_lines() -> Vec<&'input str>
        = lines:(line:$((!(" \\" eol()) [^'\n'])+ " \\") eol() { line })+
          last:$([^'\n']+)?
        {
            let mut result = lines;
            if let Some(l) = last {
                result.push(l);
            }
            result
        }

        // Document attribute parsing
        // Works identically in both header and block metadata contexts
        rule document_attribute_match() -> AttributeEntry<'input>
        = ":"
        key_entry:(
            "!" key:$([^':']+) { (false, key) }
            / key:$([^('!' | ':')]+) "!" { (false, key) }
            / key:$([^':']+) { (true, key) }
        )
        ":" &" "?
        value:document_attribute_value()?
        {
            let (set, key) = key_entry;
            let attr_value = if !set {
                AttributeValue::Bool(false)
            } else if let Some(v) = value {
                let trimmed = v.trim();
                match trimmed {
                    "true" => AttributeValue::Bool(true),
                    _ => AttributeValue::String(Cow::Owned(v)),
                }
            } else {
                AttributeValue::Bool(true)
            };
            AttributeEntry { set, key, value: attr_value }
        }
        / expected!("document attribute key starting with ':'")

        rule position() -> PositionWithOffset = offset:position!() {
            PositionWithOffset {
                offset,
                position: state.line_map.offset_to_position(offset, state.input)
            }
        }

    }
}

/// Splits trailing callout sequences into text and structured references with exact locations.
///
/// Escaped markers remain literal and do not consume an automatic number. XML comment guards
/// remain as adjacent text so each converter can apply its own presentation rule.
fn resolve_verbatim_callouts<'a>(
    state: &ParserState<'a>,
    text: &str,
    base_location: Location,
    callouts_enabled: bool,
) -> (Vec<InlineNode<'a>>, Vec<CalloutRef>) {
    let arena = state.arena;
    if !callouts_enabled {
        return verbatim_without_callouts(state, text, base_location);
    }
    let mut inlines = Vec::new();
    let mut callouts = Vec::new();
    let mut auto_number = 1usize;
    // Build text directly in the arena: each flush hands ownership of the
    // current `BumpString` to the AST via `into_bump_str()`, then we start
    // fresh in the same arena. Avoids the heap-`String`-then-arena-copy
    // round-trip per `VerbatimText` node.
    let mut segment = VerbatimSegment::new(arena);
    let mut line_start = 0;
    let lines = text.split_inclusive('\n');

    for raw_line in lines {
        let line = raw_line.strip_suffix('\n').unwrap_or(raw_line);
        let line = line.strip_suffix('\r').unwrap_or(line);

        if let Some(first) = first_trailing_callout_marker(line) {
            segment.push(
                &line[..first.source_start],
                line_start,
                line_start + first.source_start,
            );

            let mut previous_end = first.source_start;
            let mut marker = first;
            loop {
                segment.push(
                    &line[previous_end..marker.source_start],
                    line_start + previous_end,
                    line_start + marker.source_start,
                );
                if marker.escaped {
                    segment.push(
                        &line[marker.marker_start..marker.end],
                        line_start + marker.source_start,
                        line_start + marker.end,
                    );
                } else {
                    if marker.xml {
                        segment.push(
                            "<!--",
                            line_start + marker.marker_start,
                            line_start + marker.marker_start + 4,
                        );
                    }
                    if let Some(text) = segment.flush(state, base_location.absolute_start) {
                        inlines.push(text);
                    }
                    let callout_start = if marker.xml {
                        marker.marker_start + 4
                    } else {
                        marker.marker_start
                    };
                    let callout_end = if marker.xml {
                        marker.end - 3
                    } else {
                        marker.end
                    };
                    let location = state.create_block_location(
                        line_start + callout_start,
                        line_start + callout_end,
                        base_location.absolute_start,
                    );
                    let callout_ref = match marker.number {
                        ParsedCalloutNumber::Auto => {
                            let callout = CalloutRef::auto(auto_number, location);
                            auto_number += 1;
                            callout
                        }
                        ParsedCalloutNumber::Explicit(number) => {
                            CalloutRef::explicit(number, location)
                        }
                    };
                    inlines.push(InlineNode::CalloutRef(callout_ref.clone()));
                    callouts.push(callout_ref);
                    if marker.xml {
                        segment.push("-->", line_start + marker.end - 3, line_start + marker.end);
                    }
                }
                previous_end = marker.end;
                let Some(next) = next_callout_marker(line, previous_end) else {
                    break;
                };
                marker = next;
            }
            segment.push(
                &line[previous_end..],
                line_start + previous_end,
                line_start + line.len(),
            );
        } else {
            segment.push(line, line_start, line_start + line.len());
        }

        if raw_line.ends_with('\n') {
            segment.push("\n", line_start + line.len(), line_start + raw_line.len());
        }
        line_start += raw_line.len();
    }

    if let Some(text) = segment.flush(state, base_location.absolute_start) {
        inlines.push(text);
    }

    (inlines, callouts)
}

fn verbatim_without_callouts<'a>(
    state: &ParserState<'a>,
    text: &str,
    base_location: Location,
) -> (Vec<InlineNode<'a>>, Vec<CalloutRef>) {
    let location = if text.is_empty() {
        base_location
    } else {
        state.create_block_location(0, text.len(), base_location.absolute_start)
    };
    let mut content = bumpalo::collections::String::new_in(state.arena);
    content.push_str(text);
    (
        vec![InlineNode::VerbatimText(Verbatim {
            content: content.into_bump_str(),
            location,
        })],
        Vec::new(),
    )
}

struct VerbatimSegment<'a> {
    content: bumpalo::collections::String<'a>,
    source_start: Option<usize>,
    source_end: usize,
}

impl<'a> VerbatimSegment<'a> {
    fn new(arena: &'a bumpalo::Bump) -> Self {
        Self {
            content: bumpalo::collections::String::new_in(arena),
            source_start: None,
            source_end: 0,
        }
    }

    fn push(&mut self, content: &str, source_start: usize, source_end: usize) {
        if content.is_empty() {
            return;
        }
        debug_assert!(source_start < source_end);
        debug_assert!(self.source_start.is_none() || self.source_end == source_start);
        self.source_start.get_or_insert(source_start);
        self.source_end = source_end;
        self.content.push_str(content);
    }

    fn flush(&mut self, state: &ParserState<'a>, base_offset: usize) -> Option<InlineNode<'a>> {
        let source_start = self.source_start.take()?;
        let source_end = std::mem::take(&mut self.source_end);
        let content = std::mem::replace(
            &mut self.content,
            bumpalo::collections::String::new_in(state.arena),
        );
        Some(InlineNode::VerbatimText(Verbatim {
            content: content.into_bump_str(),
            location: state.create_block_location(source_start, source_end, base_offset),
        }))
    }
}

#[derive(Clone, Copy)]
enum ParsedCalloutNumber {
    Auto,
    Explicit(usize),
}

#[derive(Clone, Copy)]
struct ParsedCalloutMarker {
    source_start: usize,
    marker_start: usize,
    end: usize,
    number: ParsedCalloutNumber,
    escaped: bool,
    xml: bool,
}

fn first_trailing_callout_marker(line: &str) -> Option<ParsedCalloutMarker> {
    let mut marker = parse_callout_marker_ending_at(line, line.trim_end().len())?;

    loop {
        let adjacent = parse_callout_marker_ending_at(line, marker.source_start);
        let spaced = marker
            .source_start
            .checked_sub(1)
            .filter(|index| line.as_bytes().get(*index) == Some(&b' '))
            .and_then(|end| parse_callout_marker_ending_at(line, end));
        let Some(previous) = adjacent.or(spaced) else {
            break;
        };
        marker = previous;
    }

    Some(marker)
}

fn next_callout_marker(line: &str, previous_end: usize) -> Option<ParsedCalloutMarker> {
    parse_callout_marker_starting_at(line, previous_end).or_else(|| {
        previous_end
            .checked_add(1)
            .filter(|_| line.as_bytes().get(previous_end) == Some(&b' '))
            .and_then(|start| parse_callout_marker_starting_at(line, start))
    })
}

fn parse_callout_marker_starting_at(
    line: &str,
    source_start: usize,
) -> Option<ParsedCalloutMarker> {
    let escaped = line.as_bytes().get(source_start) == Some(&b'\\');
    let marker_start = source_start + usize::from(escaped);
    let marker = line.get(marker_start..)?;
    let (number, end, xml) = if let Some(value) = marker.strip_prefix("<!--") {
        let close = value.find("-->")?;
        (
            parse_callout_number(value.get(..close)?)?,
            marker_start + 4 + close + 3,
            true,
        )
    } else {
        let value = marker.strip_prefix('<')?;
        let close = value.find('>')?;
        (
            parse_callout_number(value.get(..close)?)?,
            marker_start + 1 + close + 1,
            false,
        )
    };

    Some(ParsedCalloutMarker {
        source_start,
        marker_start,
        end,
        number,
        escaped,
        xml,
    })
}

fn parse_callout_marker_ending_at(line: &str, end: usize) -> Option<ParsedCalloutMarker> {
    let prefix = line.get(..end)?;
    let marker_start = prefix.rfind('<')?;
    let marker = prefix.get(marker_start..)?;
    let (number, xml) = if marker.starts_with("<!--") && marker.ends_with("-->") {
        (
            parse_callout_number(marker.get(4..marker.len().checked_sub(3)?)?)?,
            true,
        )
    } else if marker.starts_with('<') && marker.ends_with('>') {
        (
            parse_callout_number(marker.get(1..marker.len().checked_sub(1)?)?)?,
            false,
        )
    } else {
        return None;
    };
    let source_start = marker_start
        .checked_sub(1)
        .filter(|index| line.as_bytes().get(*index) == Some(&b'\\'))
        .unwrap_or(marker_start);

    Some(ParsedCalloutMarker {
        source_start,
        marker_start,
        end,
        number,
        escaped: source_start != marker_start,
        xml,
    })
}

fn parse_callout_number(value: &str) -> Option<ParsedCalloutNumber> {
    if value == "." {
        Some(ParsedCalloutNumber::Auto)
    } else {
        value.parse().ok().map(ParsedCalloutNumber::Explicit)
    }
}

/// Extract callout number from a line ending with <N>
fn extract_callout_number(line: &str) -> Option<usize> {
    if line.ends_with('>')
        && let Some(start) = line.rfind('<')
    {
        let number_str = &line[start + 1..line.len() - 1];
        number_str.parse().ok()
    } else {
        None
    }
}

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::unreachable
)]
mod tests {
    use super::*;

    #[test]
    #[tracing_test::traced_test]
    fn test_document() -> Result<(), Error> {
        let input = "// this comment line is ignored
= Document Title
Lorn_Kismet R. Lee <kismet@asciidoctor.org>; Norberto M. Lopes <nlopesml@gmail.com>
v2.9, 01-09-2024: Fall incarnation
:description: The document's description.
:sectanchors:
:url-repo: https://my-git-repo.com";
        let mut state = ParserState::new_for_test(input);
        let result = document_parser::document(input, &mut state)??;
        let header = result.header.expect("document has a header");
        assert_eq!(header.title.len(), 1);
        assert_eq!(
            header.title[0],
            InlineNode::PlainText(Plain {
                content: "Document Title",
                location: Location {
                    absolute_start: 34,
                    absolute_end: 47,
                    start: crate::Position::new(2, 3),
                    end: crate::Position::new(2, 16),
                },
                escaped: false,
            })
        );
        assert_eq!(header.authors.len(), 2);
        assert_eq!(header.authors[0].first_name, "Lorn Kismet");
        assert_eq!(header.authors[0].middle_name, Some("R."));
        assert_eq!(header.authors[0].last_name, "Lee");
        assert_eq!(header.authors[0].initials, "LRL");
        assert_eq!(header.authors[0].email, Some("kismet@asciidoctor.org"));
        assert_eq!(header.authors[1].first_name, "Norberto");
        assert_eq!(header.authors[1].middle_name, Some("M."));
        assert_eq!(header.authors[1].last_name, "Lopes");
        assert_eq!(header.authors[1].initials, "NML");
        assert_eq!(header.authors[1].email, Some("nlopesml@gmail.com"));
        assert_eq!(
            state.document_attributes.get("revnumber"),
            Some(&AttributeValue::String("2.9".into()))
        );
        assert_eq!(
            state.document_attributes.get("revdate"),
            Some(&AttributeValue::String("01-09-2024".into()))
        );
        assert_eq!(
            state.document_attributes.get("revremark"),
            Some(&AttributeValue::String("Fall incarnation".into()))
        );
        assert_eq!(
            state.document_attributes.get("description"),
            Some(&AttributeValue::String(
                "The document's description.".into()
            ))
        );
        assert_eq!(
            state.document_attributes.get("sectanchors"),
            Some(&AttributeValue::Bool(true))
        );
        assert_eq!(
            state.document_attributes.get("url-repo"),
            Some(&AttributeValue::String("https://my-git-repo.com".into()))
        );
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_authors() -> Result<(), Error> {
        let input =
            "Lorn_Kismet R. Lee <kismet@asciidoctor.org>; Norberto M. Lopes <nlopesml@gmail.com>";
        let mut state = ParserState::new_for_test(input);
        let result = document_parser::authors(input, &mut state)?;

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].first_name, "Lorn Kismet");
        assert_eq!(result[0].middle_name, Some("R."));
        assert_eq!(result[0].last_name, "Lee");
        assert_eq!(result[0].initials, "LRL");
        assert_eq!(result[0].email, Some("kismet@asciidoctor.org"));
        assert_eq!(result[1].first_name, "Norberto");
        assert_eq!(result[1].middle_name, Some("M."));
        assert_eq!(result[1].last_name, "Lopes");
        assert_eq!(result[1].initials, "NML");
        assert_eq!(result[1].email, Some("nlopesml@gmail.com"));
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_author() -> Result<(), Error> {
        let input = "Norberto M. Lopes supa dough <nlopesml@gmail.com>";
        let mut state = ParserState::new_for_test(input);
        let result = document_parser::author(input, &mut state)?;
        assert_eq!(result.first_name, "Norberto");
        assert_eq!(result.middle_name, Some("M."));
        assert_eq!(result.last_name, "Lopes supa dough");
        assert_eq!(result.initials, "NML");
        assert_eq!(result.email, Some("nlopesml@gmail.com"));
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_compound_first_name() -> Result<(), Error> {
        let input = "Ann_Marie Jenson";
        let mut state = ParserState::new_for_test(input);
        let result = document_parser::author(input, &mut state)?;
        assert_eq!(result.first_name, "Ann Marie");
        assert_eq!(result.middle_name, None);
        assert_eq!(result.last_name, "Jenson");
        assert_eq!(result.initials, "AJ");
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_compound_last_name() -> Result<(), Error> {
        let input = "Tomás López_del_Toro";
        let mut state = ParserState::new_for_test(input);
        let result = document_parser::author(input, &mut state)?;
        assert_eq!(result.first_name, "Tomás");
        assert_eq!(result.middle_name, None);
        assert_eq!(result.last_name, "López del Toro");
        assert_eq!(result.initials, "TL");
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_compound_middle_name() -> Result<(), Error> {
        let input = "First Middle_Name Last";
        let mut state = ParserState::new_for_test(input);
        let result = document_parser::author(input, &mut state)?;
        assert_eq!(result.first_name, "First");
        assert_eq!(result.middle_name, Some("Middle Name"));
        assert_eq!(result.last_name, "Last");
        assert_eq!(result.initials, "FML");
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_multiple_compound_authors() -> Result<(), Error> {
        let input = "Ann_Marie Jenson; Tomás López_del_Toro";
        let mut state = ParserState::new_for_test(input);
        let result = document_parser::authors(input, &mut state)?;
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].first_name, "Ann Marie");
        assert_eq!(result[0].last_name, "Jenson");
        assert_eq!(result[0].initials, "AJ");
        assert_eq!(result[1].first_name, "Tomás");
        assert_eq!(result[1].last_name, "López del Toro");
        assert_eq!(result[1].initials, "TL");
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_unicode_author_name() -> Result<(), Error> {
        let input = "Tomás Müller";
        let mut state = ParserState::new_for_test(input);
        let result = document_parser::author(input, &mut state)?;
        assert_eq!(result.first_name, "Tomás");
        assert_eq!(result.last_name, "Müller");
        assert_eq!(result.initials, "TM");
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_revision_full() -> Result<(), Error> {
        let input = "v2.9, 01-09-2024: Fall incarnation";
        let mut state = ParserState::new_for_test(input);
        document_parser::revision(input, &mut state)?;
        assert_eq!(
            state.document_attributes.get("revnumber"),
            Some(&AttributeValue::String("2.9".into()))
        );
        assert_eq!(
            state.document_attributes.get("revdate"),
            Some(&AttributeValue::String("01-09-2024".into()))
        );
        assert_eq!(
            state.document_attributes.get("revremark"),
            Some(&AttributeValue::String("Fall incarnation".into()))
        );
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_revision_with_date_no_remark() -> Result<(), Error> {
        let input = "v2.9, 01-09-2024";
        let mut state = ParserState::new_for_test(input);
        document_parser::revision(input, &mut state)?;
        assert_eq!(
            state.document_attributes.get("revnumber"),
            Some(&AttributeValue::String("2.9".into()))
        );
        assert_eq!(
            state.document_attributes.get("revdate"),
            Some(&AttributeValue::String("01-09-2024".into()))
        );
        assert_eq!(state.document_attributes.get("revremark"), None);
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_revision_no_date_with_remark() -> Result<(), Error> {
        let input = "v2.9: Fall incarnation";
        let mut state = ParserState::new_for_test(input);
        document_parser::revision(input, &mut state)?;
        assert_eq!(
            state.document_attributes.get("revnumber"),
            Some(&AttributeValue::String("2.9".into()))
        );
        assert_eq!(state.document_attributes.get("revdate"), None);
        assert_eq!(
            state.document_attributes.get("revremark"),
            Some(&AttributeValue::String("Fall incarnation".into()))
        );
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_revision_no_date_no_remark() -> Result<(), Error> {
        let input = "v2.9";
        let mut state = ParserState::new_for_test(input);
        document_parser::revision(input, &mut state)?;
        assert_eq!(
            state.document_attributes.get("revnumber"),
            Some(&AttributeValue::String("2.9".into()))
        );
        assert_eq!(state.document_attributes.get("revdate"), None);
        assert_eq!(state.document_attributes.get("revremark"), None);
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_comment_between_author_and_revision() -> Result<(), Error> {
        // asciidoctor skips a line comment between the author line and the
        // revision line and still reads the revision (and following attributes).
        let input = "= T
Roberto Avanzi
// a comment
v2.0, 2026-01-15: rel
:foo: bar";
        let mut state = ParserState::new_for_test(input);
        let result = document_parser::document(input, &mut state)??;
        let header = result.header.expect("document has a header");
        assert_eq!(header.authors.len(), 1);
        assert_eq!(header.authors[0].first_name, "Roberto");
        assert_eq!(
            state.document_attributes.get("revnumber"),
            Some(&AttributeValue::String("2.0".into()))
        );
        assert_eq!(
            state.document_attributes.get("revdate"),
            Some(&AttributeValue::String("2026-01-15".into()))
        );
        assert_eq!(
            state.document_attributes.get("foo"),
            Some(&AttributeValue::String("bar".into()))
        );
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_authorcount_defaults_to_zero_without_author() -> Result<(), Error> {
        let input = "= T\n\nbody";
        let mut state = ParserState::new_for_test(input);
        document_parser::document(input, &mut state)??;
        assert_eq!(
            state.document_attributes.get("authorcount"),
            Some(&AttributeValue::String("0".into()))
        );
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_document_title() -> Result<(), Error> {
        let input = "= Document Title";
        let mut state = ParserState::new_for_test(input);
        let result = document_parser::document_title(input, &mut state)?;
        assert_eq!(result.0.len(), 1);
        assert_eq!(
            result.0[0],
            InlineNode::PlainText(Plain {
                content: "Document Title",
                location: Location {
                    absolute_start: 2,
                    absolute_end: 15,
                    start: crate::Position::new(1, 3),
                    end: crate::Position::new(1, 16),
                },
                escaped: false,
            })
        );
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_document_title_and_subtitle() -> Result<(), Error> {
        let input = "= Document Title: And a subtitle";
        let mut state = ParserState::new_for_test(input);
        let result = document_parser::document_title(input, &mut state)?;
        assert_eq!(
            result,
            (
                Title::new(vec![InlineNode::PlainText(Plain {
                    content: "Document Title",
                    location: Location {
                        absolute_start: 2,
                        absolute_end: 15,
                        start: crate::Position::new(1, 3),
                        end: crate::Position::new(1, 16),
                    },
                    escaped: false,
                })]),
                Some(Subtitle::new(vec![InlineNode::PlainText(Plain {
                    content: "And a subtitle",
                    location: Location {
                        absolute_start: 18,
                        absolute_end: 31,
                        start: crate::Position::new(1, 19),
                        end: crate::Position::new(1, 32),
                    },
                    escaped: false,
                })]))
            )
        );
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_header_with_title_and_authors() -> Result<(), Error> {
        let input = "= Document Title
Lorn_Kismet R. Lee <kismet@asciidoctor.org>; Norberto M. Lopes <nlopesml@gmail.com>";
        let mut state = ParserState::new_for_test(input);
        let result =
            document_parser::header(input, &mut state)??.expect("header should be present");
        assert_eq!(result.title.len(), 1);
        assert_eq!(
            result.title[0],
            InlineNode::PlainText(Plain {
                content: "Document Title",
                location: Location {
                    absolute_start: 2,
                    absolute_end: 15,
                    start: crate::Position::new(1, 3),
                    end: crate::Position::new(1, 16),
                },
                escaped: false,
            })
        );
        assert_eq!(result.authors.len(), 2);
        assert_eq!(result.authors[0].first_name, "Lorn Kismet");
        assert_eq!(result.authors[0].middle_name, Some("R."));
        assert_eq!(result.authors[0].last_name, "Lee");
        assert_eq!(result.authors[0].initials, "LRL");
        assert_eq!(result.authors[0].email, Some("kismet@asciidoctor.org"));
        assert_eq!(result.authors[1].first_name, "Norberto");
        assert_eq!(result.authors[1].middle_name, Some("M."));
        assert_eq!(result.authors[1].last_name, "Lopes");
        assert_eq!(result.authors[1].initials, "NML");
        assert_eq!(result.authors[1].email, Some("nlopesml@gmail.com"));
        Ok(())
    }

    /// A document whose only content is a title (no body, no following blank
    /// line) is recognised as the doctitle, not a level-0 section. The
    /// preprocessor strips the trailing newline, so the title sits at EOF — the
    /// `title_authors` rule must accept end-of-input, not only a following `\n`.
    /// Matches asciidoctor, which treats a lone `= Title` as the doctitle.
    #[test]
    fn test_title_only_document_is_doctitle() -> Result<(), Error> {
        // No trailing newline: mirrors the post-preprocessor buffer for a
        // single-line `= Doc Title\n` source.
        let input = "= Doc Title";
        let mut state = ParserState::new_for_test(input);
        let doc = document_parser::document(input, &mut state)??;
        let header = doc.header.expect("title-only doc should have a header");
        assert_eq!(header.title.len(), 1);
        assert_eq!(
            header.title[0],
            InlineNode::PlainText(Plain {
                content: "Doc Title",
                location: Location {
                    absolute_start: 2,
                    absolute_end: 10,
                    start: crate::Position::new(1, 3),
                    end: crate::Position::new(1, 11),
                },
                escaped: false,
            })
        );
        assert!(
            doc.blocks.is_empty(),
            "title-only doc should have no body blocks, got: {:?}",
            doc.blocks
        );
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_document_empty_attribute_list() -> Result<(), Error> {
        let input = "[]";
        let mut state = ParserState::new_for_test(input);
        let (discrete, metadata, _title_position) = document_parser::attributes(input, &mut state)?;
        assert!(!discrete); // Not discrete
        assert_eq!(metadata.id, None);
        assert_eq!(metadata.style, None);
        assert!(metadata.roles.is_empty());
        assert!(metadata.options.is_empty());
        assert!(metadata.attributes.is_empty());
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_document_empty_attribute_list_with_discrete() -> Result<(), Error> {
        let input = "[discrete]";
        let mut state = ParserState::new_for_test(input);
        let (discrete, metadata, _title_position) = document_parser::attributes(input, &mut state)?;
        assert!(discrete); // Should be discrete
        assert_eq!(metadata.id, None);
        // The `discrete` style is retained so a discrete heading renders it as a class.
        assert_eq!(metadata.style, Some("discrete"));
        assert!(metadata.roles.is_empty());
        assert!(metadata.options.is_empty());
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_document_attribute_with_id() -> Result<(), Error> {
        let input = "[id=my-id,role=admin,options=read,options=write]";
        let mut state = ParserState::new_for_test(input);
        let (discrete, metadata, _title_position) = document_parser::attributes(input, &mut state)?;
        assert!(!discrete); // Not discrete
        assert_eq!(
            metadata.id,
            Some(Anchor {
                id: "my-id",
                xreflabel: None,
                location: Location {
                    absolute_start: 4,
                    absolute_end: 9,
                    start: crate::Position::new(1, 5),
                    end: crate::Position::new(1, 10),
                },
                bibliography: false,
            })
        );
        assert_eq!(metadata.style, None);
        assert!(metadata.roles.contains(&"admin"));
        assert!(metadata.options.contains(&"read"));
        assert!(metadata.options.contains(&"write"));
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_document_attribute_with_id_mixed() -> Result<(), Error> {
        let input = "[astyle#myid.admin,options=read,options=write]";
        let mut state = ParserState::new_for_test(input);
        let (discrete, metadata, _title_position) = document_parser::attributes(input, &mut state)?;
        assert!(!discrete); // Not discrete
        assert_eq!(
            metadata.id,
            Some(Anchor {
                id: "myid",
                xreflabel: None,
                location: Location {
                    absolute_start: 8,
                    absolute_end: 12,
                    start: crate::Position::new(1, 9),
                    end: crate::Position::new(1, 13),
                },
                bibliography: false,
            })
        );
        assert_eq!(metadata.style, Some("astyle"));
        assert!(metadata.roles.contains(&"admin"));
        assert!(metadata.options.contains(&"read"));
        assert!(metadata.options.contains(&"write"));
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_document_attribute_with_id_mixed_with_quotes() -> Result<(), Error> {
        let input = "[astyle#myid.admin,options=\"read,write\"]";
        let mut state = ParserState::new_for_test(input);
        let (discrete, metadata, _title_position) = document_parser::attributes(input, &mut state)?;
        assert!(!discrete); // Not discrete
        assert_eq!(
            metadata.id,
            Some(Anchor {
                id: "myid",
                xreflabel: None,
                location: Location {
                    absolute_start: 8,
                    absolute_end: 12,
                    start: crate::Position::new(1, 9),
                    end: crate::Position::new(1, 13),
                },
                bibliography: false,
            })
        );
        assert_eq!(metadata.style, Some("astyle"));
        assert!(metadata.roles.contains(&"admin"));
        assert!(metadata.options.contains(&"read"));
        assert!(metadata.options.contains(&"write"));
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_shorthand_id_role_combined() -> Result<(), Error> {
        // Test [#id.role] syntax - ID with role, no style
        let input = "[#bracket-id.some-role]";
        let mut state = ParserState::new_for_test(input);
        let (discrete, metadata, _title_position) = document_parser::attributes(input, &mut state)?;
        assert!(!discrete);
        assert_eq!(
            metadata.id,
            Some(Anchor {
                id: "bracket-id",
                xreflabel: None,
                location: Location {
                    absolute_start: 2,
                    absolute_end: 12,
                    start: crate::Position::new(1, 3),
                    end: crate::Position::new(1, 13),
                },
                bibliography: false,
            })
        );
        assert_eq!(metadata.style, None);
        assert!(metadata.roles.contains(&"some-role"));
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_shorthand_id_role_option_combined() -> Result<(), Error> {
        // Test [#id.role%option] syntax - ID with role and option
        let input = "[#my-id.my-role%my-option]";
        let mut state = ParserState::new_for_test(input);
        let (discrete, metadata, _title_position) = document_parser::attributes(input, &mut state)?;
        assert!(!discrete);
        assert_eq!(
            metadata.id,
            Some(Anchor {
                id: "my-id",
                xreflabel: None,
                location: Location {
                    absolute_start: 2,
                    absolute_end: 7,
                    start: crate::Position::new(1, 3),
                    end: crate::Position::new(1, 8),
                },
                bibliography: false,
            })
        );
        assert_eq!(metadata.style, None);
        assert!(metadata.roles.contains(&"my-role"));
        assert!(metadata.options.contains(&"my-option"));
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_shorthand_multiple_roles() -> Result<(), Error> {
        // Test [#id.role1.role2] syntax - ID with multiple roles
        let input = "[#my-id.role-one.role-two]";
        let mut state = ParserState::new_for_test(input);
        let (discrete, metadata, _title_position) = document_parser::attributes(input, &mut state)?;
        assert!(!discrete);
        assert_eq!(metadata.id.as_ref().map(|a| a.id), Some("my-id"));
        assert!(metadata.roles.contains(&"role-one"));
        assert!(metadata.roles.contains(&"role-two"));
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_shorthand_style_id_role() -> Result<(), Error> {
        // Test [style#id.role] syntax - already tested in test_document_attribute_with_id_mixed
        // but let's verify it still works
        let input = "[quote#my-id.my-role]";
        let mut state = ParserState::new_for_test(input);
        let (discrete, metadata, _title_position) = document_parser::attributes(input, &mut state)?;
        assert!(!discrete);
        assert_eq!(metadata.id.as_ref().map(|a| a.id), Some("my-id"));
        assert_eq!(metadata.style, Some("quote"));
        assert!(metadata.roles.contains(&"my-role"));
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_shorthand_just_roles() -> Result<(), Error> {
        // Test [.role1.role2] syntax - just roles, no ID
        let input = "[.role-one.role-two]";
        let mut state = ParserState::new_for_test(input);
        let (discrete, metadata, _title_position) = document_parser::attributes(input, &mut state)?;
        assert!(!discrete);
        assert_eq!(metadata.id, None);
        assert!(metadata.roles.contains(&"role-one"));
        assert!(metadata.roles.contains(&"role-two"));
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_toc_simple() -> Result<(), Error> {
        let input =
            "= Document Title\n\n== Section 1\n\nSome content.\n\n== Section 2\n\nMore content.";
        let mut state = ParserState::new_for_test(input);
        let result = document_parser::document(input, &mut state)??;

        // Check that TOC entries were generated
        assert_eq!(result.toc_entries.len(), 2);
        assert_eq!(result.toc_entries[0].level, 1);
        assert_eq!(result.toc_entries[0].id, "_section_1");
        assert_eq!(result.toc_entries[1].level, 1);
        assert_eq!(result.toc_entries[1].id, "_section_2");
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_toc_tree() -> Result<(), Error> {
        let input = "= Document Title\n\n== Section A\n\nContent A.\n\n=== Section A.1\n\nContent A.1\n\n== Section B\n\nContent B.";
        let mut state = ParserState::new_for_test(input);
        let result = document_parser::document(input, &mut state)??;

        // Check that TOC entries were generated and ordered correctly
        assert_eq!(result.toc_entries.len(), 3);
        assert_eq!(result.toc_entries[0].id, "_section_a");
        assert_eq!(result.toc_entries[1].id, "_section_a_1");
        assert_eq!(result.toc_entries[2].id, "_section_b");
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_section_kind_classifies_special_sections() -> Result<(), Error> {
        // A plain subsection keeps its own `Normal` kind. The numbering pass
        // handles any suppression inherited from a special parent.
        let input = "= Title\n\n[preface]\n== Introduction\n\nintro\n\n=== Features\n\nfeatures\n\n== Real Chapter\n\ntext";
        let mut state = ParserState::new_for_test(input);
        let result = document_parser::document(input, &mut state)??;

        let mut sections = Vec::new();
        fn collect<'a, 'b>(blocks: &'b [Block<'a>], out: &mut Vec<&'b Section<'a>>) {
            for block in blocks {
                if let Block::Section(s) = block {
                    out.push(s);
                    collect(&s.content, out);
                }
            }
        }
        collect(&result.blocks, &mut sections);
        assert_eq!(sections.len(), 3);
        assert_eq!(sections[0].kind, SectionKind::Preface); // Introduction
        assert_eq!(sections[1].kind, SectionKind::Normal); // Features (plain subsection)
        assert_eq!(sections[2].kind, SectionKind::Normal); // Real Chapter

        // The flat TOC list carries the same per-section kinds.
        assert_eq!(result.toc_entries.len(), 3);
        assert_eq!(result.toc_entries[0].kind, SectionKind::Preface);
        assert_eq!(result.toc_entries[1].kind, SectionKind::Normal);
        assert_eq!(result.toc_entries[2].kind, SectionKind::Normal);
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_section_kind_appendix() -> Result<(), Error> {
        // `[appendix]` is classified as Appendix; its plain subsection is Normal.
        let input = "= Title\n:doctype: book\n\n[appendix]\n== App\n\napp\n\n=== App Sub\n\nsub";
        let mut state = ParserState::new_for_test(input);
        let result = document_parser::document(input, &mut state)??;

        assert_eq!(result.toc_entries.len(), 2);
        assert_eq!(result.toc_entries[0].kind, SectionKind::Appendix);
        assert_eq!(result.toc_entries[1].kind, SectionKind::Normal);
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_toc_empty_document() -> Result<(), Error> {
        let input = "= Document Title\n\nJust some content without sections.";
        let mut state = ParserState::new_for_test(input);
        let result = document_parser::document(input, &mut state)??;
        assert_eq!(result.toc_entries.len(), 0);
        Ok(())
    }

    #[cfg(feature = "setext")]
    #[test]
    #[tracing_test::traced_test]
    fn test_setext_document_title() -> Result<(), Error> {
        let input = "Document Title
==============

Some content.
";
        let mut state = ParserState::new_for_test(input);
        std::rc::Rc::make_mut(&mut state.options).setext = true;
        let result = document_parser::document(input, &mut state)??;
        let header = result.header.expect("document has a header");
        assert_eq!(header.title.len(), 1);
        assert!(
            matches!(&header.title[0], InlineNode::PlainText(Plain { content, .. }) if *content == "Document Title")
        );
        Ok(())
    }

    #[cfg(feature = "setext")]
    #[test]
    #[tracing_test::traced_test]
    fn test_setext_section() -> Result<(), Error> {
        let input = "= Document Title

Section One
-----------

Content.
";
        let mut state = ParserState::new_for_test(input);
        std::rc::Rc::make_mut(&mut state.options).setext = true;
        let result = document_parser::document(input, &mut state)??;

        // Find the section
        let section = result.blocks.iter().find_map(|b| {
            if let Block::Section(s) = b {
                Some(s)
            } else {
                None
            }
        });
        let section = section.expect("should have a section");
        assert_eq!(section.level, 1);
        assert!(
            matches!(&section.title[0], InlineNode::PlainText(Plain { content, .. }) if *content == "Section One")
        );
        Ok(())
    }

    #[cfg(feature = "setext")]
    #[test]
    #[tracing_test::traced_test]
    fn test_setext_disabled_by_default() {
        let input = "Document Title
==============

Some content.
";
        let mut state = ParserState::new_for_test(input);
        // setext is disabled by default
        assert!(!state.options.setext);
        // Should not parse as setext title when disabled
        let result = document_parser::document(input, &mut state);
        // The document will be parsed but without recognizing the setext title
        // The title line will be parsed as a paragraph or similar
        if let Ok(Ok(doc)) = result {
            // No header should be found when setext is disabled
            assert!(doc.header.is_none());
        }
    }

    #[cfg(feature = "setext")]
    #[test]
    #[tracing_test::traced_test]
    fn test_setext_single_section_per_level() -> Result<(), Error> {
        // Test a single setext section with document title
        // Note: Multiple same-level setext sections currently nest incorrectly
        // (tracked as known limitation). This test verifies basic functionality.
        let input = "Document Title
==============

Section One
-----------

Content here.
";
        let mut state = ParserState::new_for_test(input);
        std::rc::Rc::make_mut(&mut state.options).setext = true;
        let result = document_parser::document(input, &mut state)??;

        // Check document title (level 0)
        let header = result.header.expect("document has a header");
        assert!(
            matches!(&header.title[0], InlineNode::PlainText(Plain { content, .. }) if *content == "Document Title")
        );

        // Find the section
        let section = result
            .blocks
            .iter()
            .find_map(|b| {
                if let Block::Section(s) = b {
                    Some(s)
                } else {
                    None
                }
            })
            .expect("should have a section");

        assert_eq!(section.level, 1);
        assert!(
            matches!(&section.title[0], InlineNode::PlainText(Plain { content, .. }) if *content == "Section One")
        );

        Ok(())
    }

    #[cfg(feature = "setext")]
    #[test]
    #[tracing_test::traced_test]
    fn test_setext_sibling_sections() -> Result<(), Error> {
        // Test that multiple same-level setext sections are parsed as siblings, not nested
        let input = "Document Title
==============

Section A
---------

Content A.

Section B
---------

Content B.

Section C
---------

Content C.
";
        let mut state = ParserState::new_for_test(input);
        std::rc::Rc::make_mut(&mut state.options).setext = true;
        let result = document_parser::document(input, &mut state)??;

        // Check document title
        let header = result.header.expect("document has a header");
        assert!(
            matches!(&header.title[0], InlineNode::PlainText(Plain { content, .. }) if *content == "Document Title")
        );

        // All three sections should be at the top level (siblings, not nested)
        let sections: Vec<&Section> = result
            .blocks
            .iter()
            .filter_map(|b| {
                if let Block::Section(s) = b {
                    Some(s)
                } else {
                    None
                }
            })
            .collect();

        assert_eq!(
            sections.len(),
            3,
            "should have 3 top-level sibling sections"
        );

        // Verify all are level 1
        for (i, section) in sections.iter().enumerate() {
            assert_eq!(section.level, 1, "section {i} should be level 1");
        }

        // Verify titles
        assert!(
            matches!(&sections[0].title[0], InlineNode::PlainText(Plain { content, .. }) if *content == "Section A")
        );
        assert!(
            matches!(&sections[1].title[0], InlineNode::PlainText(Plain { content, .. }) if *content == "Section B")
        );
        assert!(
            matches!(&sections[2].title[0], InlineNode::PlainText(Plain { content, .. }) if *content == "Section C")
        );

        Ok(())
    }

    #[cfg(feature = "setext")]
    #[test]
    #[tracing_test::traced_test]
    fn test_setext_all_underline_characters() -> Result<(), Error> {
        // Test each setext underline character individually
        // = → level 0 (document title)
        // - → level 1
        // ~ → level 2
        // ^ → level 3
        // + → level 4

        // Test level 1 with -
        let input = "= Doc\n\nLevel One\n---------\n\nContent.\n";
        let mut state = ParserState::new_for_test(input);
        std::rc::Rc::make_mut(&mut state.options).setext = true;
        let result = document_parser::document(input, &mut state)??;
        let section = result
            .blocks
            .iter()
            .find_map(|b| {
                if let Block::Section(s) = b {
                    Some(s)
                } else {
                    None
                }
            })
            .expect("level 1 section");
        assert_eq!(section.level, 1);

        // Test level 2 with ~
        let input = "= Doc\n\nLevel Two\n~~~~~~~~~\n\nContent.\n";
        let mut state = ParserState::new_for_test(input);
        std::rc::Rc::make_mut(&mut state.options).setext = true;
        let result = document_parser::document(input, &mut state)??;
        let section = result
            .blocks
            .iter()
            .find_map(|b| {
                if let Block::Section(s) = b {
                    Some(s)
                } else {
                    None
                }
            })
            .expect("level 2 section");
        assert_eq!(section.level, 2);

        // Test level 3 with ^
        let input = "= Doc\n\nLevel Three\n^^^^^^^^^^^\n\nContent.\n";
        let mut state = ParserState::new_for_test(input);
        std::rc::Rc::make_mut(&mut state.options).setext = true;
        let result = document_parser::document(input, &mut state)??;
        let section = result
            .blocks
            .iter()
            .find_map(|b| {
                if let Block::Section(s) = b {
                    Some(s)
                } else {
                    None
                }
            })
            .expect("level 3 section");
        assert_eq!(section.level, 3);

        // Test level 4 with +
        let input = "= Doc\n\nLevel Four\n++++++++++\n\nContent.\n";
        let mut state = ParserState::new_for_test(input);
        std::rc::Rc::make_mut(&mut state.options).setext = true;
        let result = document_parser::document(input, &mut state)??;
        let section = result
            .blocks
            .iter()
            .find_map(|b| {
                if let Block::Section(s) = b {
                    Some(s)
                } else {
                    None
                }
            })
            .expect("level 4 section");
        assert_eq!(section.level, 4);

        Ok(())
    }

    #[cfg(feature = "setext")]
    #[test]
    #[tracing_test::traced_test]
    fn test_setext_manpage_style_document() -> Result<(), Error> {
        let input = "gitdatamodel(7)\n===============\n\nNAME\n----\ngitdatamodel - Git's core data model\n\nSYNOPSIS\n--------\ngitdatamodel\n";
        let mut state = ParserState::new_for_test(input);
        std::rc::Rc::make_mut(&mut state.options).setext = true;
        let result = document_parser::document(input, &mut state)??;

        // Verify document title parsed
        let header = result.header.expect("document has a header");
        assert!(
            matches!(&header.title[0], InlineNode::PlainText(Plain { content, .. }) if content.contains("gitdatamodel"))
        );

        // Verify NAME and SYNOPSIS are level-1 sections
        let sections: Vec<&Section> = result
            .blocks
            .iter()
            .filter_map(|b| {
                if let Block::Section(s) = b {
                    Some(s)
                } else {
                    None
                }
            })
            .collect();

        assert_eq!(
            sections.len(),
            2,
            "should have 2 top-level sections (NAME and SYNOPSIS)"
        );
        assert_eq!(sections[0].level, 1);
        assert_eq!(sections[1].level, 1);
        assert!(
            matches!(&sections[0].title[0], InlineNode::PlainText(Plain { content, .. }) if *content == "NAME")
        );
        assert!(
            matches!(&sections[1].title[0], InlineNode::PlainText(Plain { content, .. }) if *content == "SYNOPSIS")
        );

        Ok(())
    }

    #[cfg(feature = "setext")]
    #[test]
    #[tracing_test::traced_test]
    fn test_setext_with_description_lists() -> Result<(), Error> {
        // Regression: description list markers (::) anywhere in the document
        // used to cause setext sections to fail because the lookahead
        // `check_start_of_description_list` scanned the entire remaining input
        let input = "\
gitdatamodel(7)
===============

NAME
----
gitdatamodel - description

SYNOPSIS
--------
gitdatamodel

OBJECTS
-------

commit::
    A commit.

REFERENCES
----------

References.
";
        let options = crate::Options::builder().with_setext().build();
        let parsed = crate::parse(input, &options)?;
        let result = parsed.document();

        let header = result.header.as_ref().expect("document has a header");
        assert!(
            matches!(&header.title[0], InlineNode::PlainText(Plain { content, .. }) if content.contains("gitdatamodel"))
        );

        let sections: Vec<&Section> = result
            .blocks
            .iter()
            .filter_map(|b| {
                if let Block::Section(s) = b {
                    Some(s)
                } else {
                    None
                }
            })
            .collect();

        assert_eq!(
            sections.len(),
            4,
            "should have 4 sections (NAME, SYNOPSIS, OBJECTS, REFERENCES)"
        );
        for section in &sections {
            assert_eq!(section.level, 1);
        }

        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_index_term_flow() -> Result<(), Error> {
        use crate::InlineMacro;

        let input = "= Test\n\nThis is about ((Arthur)) the king.\n";
        let mut state = ParserState::new_for_test(input);
        let result = document_parser::document(input, &mut state)??;

        // Find the paragraph
        let paragraph = result
            .blocks
            .iter()
            .find_map(|b| {
                if let Block::Paragraph(p) = b {
                    Some(p)
                } else {
                    None
                }
            })
            .expect("paragraph exists");

        // Check that the index term was parsed
        let has_index_term = paragraph.content.iter().any(|inline| {
            matches!(
                inline,
                InlineNode::Macro(InlineMacro::IndexTerm(it))
                    if it.is_visible()
                        && matches!(it.term(), [InlineNode::PlainText(text)] if text.content == "Arthur")
            )
        });

        assert!(
            has_index_term,
            "Expected to find visible index term 'Arthur', but found: {:?}",
            paragraph.content
        );
        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_index_term_concealed() -> Result<(), Error> {
        use crate::InlineMacro;

        let input = "= Test\n\n(((Sword, Broadsword)))This is a concealed index term.\n";
        let mut state = ParserState::new_for_test(input);
        let result = document_parser::document(input, &mut state)??;

        // Find the paragraph
        let paragraph = result
            .blocks
            .iter()
            .find_map(|b| {
                if let Block::Paragraph(p) = b {
                    Some(p)
                } else {
                    None
                }
            })
            .expect("paragraph exists");

        // Check that the concealed index term was parsed
        let has_concealed_term = paragraph.content.iter().any(|inline| {
            matches!(
                inline,
                InlineNode::Macro(InlineMacro::IndexTerm(it))
                    if !it.is_visible()
                        && matches!(it.term(), [InlineNode::PlainText(text)] if text.content == "Sword")
            )
        });

        assert!(
            has_concealed_term,
            "Expected to find concealed index term 'Sword', but found: {:?}",
            paragraph.content
        );
        Ok(())
    }

    /// Test that macro attributes (like `image::`) correctly allow . # % as literal characters.
    ///
    /// This verifies the fix for the issue where `image::photo.jpg[Diablo 4 picture of Lilith.]`
    /// would fail because the trailing `.` was interpreted as a role shorthand prefix.
    ///
    /// In asciidoctor, shorthand syntax (.role, #id, %option) is only valid in block-level
    /// attributes, NOT inside macro brackets. Macro brackets should treat these characters
    /// as literal content.
    #[test]
    #[tracing_test::traced_test]
    fn test_macro_attributes_allow_literal_special_chars() -> Result<(), Error> {
        // Helper to extract the first Image block from a document
        fn get_image<'a>(doc: &'a Document<'a>) -> &'a Image<'a> {
            doc.blocks
                .iter()
                .find_map(|b| {
                    if let Block::Image(img) = b {
                        Some(img)
                    } else {
                        None
                    }
                })
                .expect("document should have an image block")
        }

        // Test trailing period in alt text
        let input = "image::photo.jpg[Diablo 4 picture of Lilith.]";
        let mut state = ParserState::new_for_test(input);
        let result = document_parser::document(input, &mut state)??;
        let img = get_image(&result);
        assert_eq!(
            img.metadata.attributes.get("alt"),
            Some(&AttributeValue::String(
                "Diablo 4 picture of Lilith.".into()
            )),
            "Trailing period should be preserved in alt text"
        );

        // Test .role as literal text (not a shorthand)
        let input = "image::photo.jpg[.role]";
        let mut state = ParserState::new_for_test(input);
        let result = document_parser::document(input, &mut state)??;
        let img = get_image(&result);
        assert_eq!(
            img.metadata.attributes.get("alt"),
            Some(&AttributeValue::String(".role".into())),
            ".role should be literal alt text, not a CSS class"
        );
        assert!(
            img.metadata.roles.is_empty(),
            "roles should be empty - .role is literal text"
        );

        // Test #id as literal text (not a shorthand)
        let input = "image::photo.jpg[Issue #42]";
        let mut state = ParserState::new_for_test(input);
        let result = document_parser::document(input, &mut state)??;
        let img = get_image(&result);
        assert_eq!(
            img.metadata.attributes.get("alt"),
            Some(&AttributeValue::String("Issue #42".into())),
            "#42 should be preserved as literal text"
        );
        assert!(
            img.metadata.id.is_none(),
            "id should be empty - #42 is literal text"
        );

        // Test named role= attribute still works
        let input = "image::photo.jpg[role=thumbnail]";
        let mut state = ParserState::new_for_test(input);
        let result = document_parser::document(input, &mut state)??;
        let img = get_image(&result);
        assert_eq!(
            img.metadata.roles,
            vec![std::borrow::Cow::Borrowed("thumbnail")],
            "Named role= attribute should work"
        );

        Ok(())
    }

    #[test]
    #[tracing_test::traced_test]
    fn test_block_macro_uses_last_closing_bracket() -> Result<(), Error> {
        let input = "image::foo.svg[role=inline][100,100]\n\n[.lead]\nHello\n";
        let mut state = ParserState::new_for_test(input);
        let result = document_parser::document(input, &mut state)??;

        let image = result.blocks.iter().find_map(|block| {
            if let Block::Image(image) = block {
                Some(image)
            } else {
                None
            }
        });
        assert!(image.is_some(), "document should contain an image block");
        if let Some(image) = image {
            assert_eq!(image.metadata.roles, ["inline][100"]);
            assert_eq!(
                image.metadata.attributes.get("width"),
                Some(&AttributeValue::String("100".into()))
            );
        }
        assert!(
            result
                .blocks
                .iter()
                .any(|b| matches!(b, Block::Paragraph(_))),
            "document should contain a paragraph block"
        );
        assert!(state.warnings.borrow().is_empty());
        Ok(())
    }

    /// When `source_ranges` are set, `warn_trailing_macro_content` should resolve
    /// the correct file name and line number from the included file.
    #[test]
    fn test_trailing_content_warning_resolves_source_range() {
        use crate::model::SourceRange;
        use std::path::PathBuf;

        // Simulate: lines 0..30 are from the main file, lines 30..80 are from
        // "sponsor.adoc" (included), and the trailing content is at byte 45.
        let input = "a]b\n".repeat(20); // 80 bytes total (4 bytes per line)
        let mut state = ParserState::new_for_test(&input);
        state.current_file = Some(PathBuf::from("/docs/main.adoc").into());
        state.source_ranges = vec![SourceRange {
            start_offset: 28, // byte 28 starts the included region
            end_offset: 60,
            file: Some(PathBuf::from("/docs/sponsor.adoc")),
            file_chain: vec!["sponsor.adoc".to_string()],
            start_line: 1,
            source_start_offset: 0,
            column_shift: 0,
        }];

        // Trigger warning at byte offset 40 (inside the included range)
        // 40 - 28 = 12 bytes into the included content = 3 newlines = line 4
        state.warn_trailing_macro_content("image", "[100,100]", 40, 0);

        let warnings = state.warnings.borrow();
        assert_eq!(warnings.len(), 1);
        let loc = warnings[0]
            .source_location()
            .expect("warning should have a location");
        assert_eq!(
            loc.file.as_deref(),
            Some(std::path::Path::new("/docs/sponsor.adoc")),
            "should reference the included file, got: {:?}",
            loc.file,
        );
        let position_line = loc.location.start.line;
        assert_eq!(
            position_line, 4,
            "should reference line 4 in included file, got line {position_line}",
        );
    }

    /// When offset is outside any `source_range`, `warn_trailing_macro_content`
    /// should fall back to the entry-point file.
    #[test]
    fn test_trailing_content_warning_falls_back_to_entry_file() {
        use crate::model::SourceRange;
        use std::path::PathBuf;

        let input = "image::x.png[alt]extra\nsecond line\n";
        let mut state = ParserState::new_for_test(input);
        state.current_file = Some(PathBuf::from("/docs/main.adoc").into());
        state.source_ranges = vec![SourceRange {
            start_offset: 100, // well beyond input - shouldn't match
            end_offset: 200,
            file: Some(PathBuf::from("/docs/other.adoc")),
            file_chain: vec!["other.adoc".to_string()],
            start_line: 1,
            source_start_offset: 0,
            column_shift: 0,
        }];

        state.warn_trailing_macro_content("image", "extra", 17, 0);

        let warnings = state.warnings.borrow();
        assert_eq!(warnings.len(), 1);
        let loc = warnings[0]
            .source_location()
            .expect("warning should have a location");
        assert_eq!(
            loc.file.as_deref(),
            Some(std::path::Path::new("/docs/main.adoc")),
            "should reference the entry-point file, got: {:?}",
            loc.file,
        );
    }

    /// When the document has a title and the first section skips level 1,
    /// the parser should warn (asciidoctor's "section title out of sequence").
    #[test]
    fn test_first_section_not_level_1_emits_warning() -> Result<(), Error> {
        let input = "= Doc Title\n\n=== Starts at level 2\n\nContent\n";
        let mut state = ParserState::new_for_test(input);
        let _ = document_parser::document(input, &mut state)??;
        let warnings = state.warnings.borrow();
        let warning = warnings
            .iter()
            .find(|w| {
                matches!(
                    &w.kind,
                    crate::WarningKind::SectionLevelOutOfSequence { got: 2, .. },
                )
            })
            .expect("expected out-of-sequence warning");
        // The warning should carry the location of the offending section
        // (byte 13 = line 3 in the test input).
        let loc = warning
            .source_location()
            .expect("warning should carry a location");
        assert_eq!(loc.location.start.line, 3);
        Ok(())
    }

    /// A level-0 `[appendix]` is rendered at level 1, so its first subsection
    /// must be a level-2 (`===`) section — that is in sequence and must NOT warn,
    /// matching asciidoctor.
    #[test]
    fn test_level0_appendix_level2_subsection_no_warning() -> Result<(), Error> {
        let input = "= Book\n:doctype: book\n\n= Part One\n\n== Chapter\n\nbody\n\n[appendix]\n= App Part\n\nintro\n\n=== First Subsection\n\nbody\n";
        let mut state = ParserState::new_for_test(input);
        let _ = document_parser::document(input, &mut state)??;
        let warnings = state.warnings.borrow();
        assert!(
            !warnings.iter().any(|w| matches!(
                &w.kind,
                crate::WarningKind::SectionLevelOutOfSequence { .. },
            )),
            "level-2 subsection of a level-0 appendix is in sequence, got: {warnings:?}"
        );
        Ok(())
    }

    #[test]
    fn test_level0_preface_level2_subsection_no_warning() -> Result<(), Error> {
        let input =
            "= Book\n:doctype: book\n\n[preface]\n= Preface\n\nintro\n\n=== Background\n\nbody\n";
        let mut state = ParserState::new_for_test(input);
        let _ = document_parser::document(input, &mut state)??;
        let warnings = state.warnings.borrow();
        assert!(
            !warnings.iter().any(|warning| matches!(
                &warning.kind,
                crate::WarningKind::SectionLevelOutOfSequence { .. },
            )),
            "level-2 subsection of a level-0 preface is in sequence, got: {warnings:?}"
        );
        Ok(())
    }

    /// A level-0 `[appendix]`'s children are expected at level 2, so a level-3
    /// (`====`) child that skips level 2 still warns — with `expected: 2`,
    /// matching asciidoctor.
    #[test]
    fn test_level0_appendix_level3_child_still_warns() -> Result<(), Error> {
        let input = "= Book\n:doctype: book\n\n= Part One\n\n== Chapter\n\nbody\n\n[appendix]\n= App Part\n\nintro\n\n==== Too Deep\n\nbody\n";
        let mut state = ParserState::new_for_test(input);
        let _ = document_parser::document(input, &mut state)??;
        let warnings = state.warnings.borrow();
        assert!(
            warnings.iter().any(|w| matches!(
                &w.kind,
                crate::WarningKind::SectionLevelOutOfSequence {
                    expected: 2,
                    got: 3
                },
            )),
            "level-3 child skipping level 2 should warn, got: {warnings:?}"
        );
        Ok(())
    }

    /// A titleless document whose first section skips level 1 still warns when
    /// preamble body content (here a description list) precedes it — the
    /// preamble anchors the document at level 0. Matches asciidoctor.
    #[test]
    fn test_titleless_preamble_then_deep_section_emits_warning() -> Result<(), Error> {
        let input = "term:: desc\n\n===== Deep\n\ntext\n";
        let mut state = ParserState::new_for_test(input);
        let _ = document_parser::document(input, &mut state)??;
        let warnings = state.warnings.borrow();
        assert!(
            warnings.iter().any(|w| matches!(
                &w.kind,
                crate::WarningKind::SectionLevelOutOfSequence {
                    expected: 1,
                    got: 4
                },
            )),
            "expected out-of-sequence warning, got: {warnings:?}"
        );
        Ok(())
    }

    /// A titleless document whose very first block is a deeper-than-1 section
    /// (no doctitle, no preamble) does not warn — matches asciidoctor.
    #[test]
    fn test_titleless_bare_deep_section_no_warning() -> Result<(), Error> {
        let input = "===== Deep\n\ntext\n";
        let mut state = ParserState::new_for_test(input);
        let _ = document_parser::document(input, &mut state)??;
        let warnings = state.warnings.borrow();
        assert!(
            !warnings.iter().any(|w| matches!(
                &w.kind,
                crate::WarningKind::SectionLevelOutOfSequence { .. },
            )),
            "expected no out-of-sequence warning, got: {warnings:?}"
        );
        Ok(())
    }

    /// Once anchored (here by a doctitle), asciidoctor flags *every* top-level
    /// section that skips level 1, not just the first. Two sibling `=====`
    /// sections must each produce a warning.
    #[test]
    fn test_multiple_top_level_sections_each_warn() -> Result<(), Error> {
        let input = "= Doc Title\n\n===== One\n\ntext\n\n===== Two\n\ntext\n";
        let mut state = ParserState::new_for_test(input);
        let _ = document_parser::document(input, &mut state)??;
        let warnings = state.warnings.borrow();
        let count = warnings
            .iter()
            .filter(|w| {
                matches!(
                    &w.kind,
                    crate::WarningKind::SectionLevelOutOfSequence {
                        expected: 1,
                        got: 4
                    },
                )
            })
            .count();
        assert_eq!(
            count, 2,
            "expected one warning per sibling, got: {warnings:?}"
        );
        Ok(())
    }

    /// An un-anchored document that opens with a deep section establishes that
    /// section's level as the base, so same-level siblings are not flagged.
    #[test]
    fn test_bare_deep_section_siblings_no_warning() -> Result<(), Error> {
        let input = "===== One\n\ntext\n\n===== Two\n\ntext\n";
        let mut state = ParserState::new_for_test(input);
        let _ = document_parser::document(input, &mut state)??;
        let warnings = state.warnings.borrow();
        assert!(
            !warnings.iter().any(|w| matches!(
                &w.kind,
                crate::WarningKind::SectionLevelOutOfSequence { .. },
            )),
            "expected no out-of-sequence warning, got: {warnings:?}"
        );
        Ok(())
    }

    /// A `[comment]`-styled block produces no output. The `--` open block
    /// becomes a `DelimitedComment` (kept distinct from a `////` block by its
    /// `--` delimiter); the paragraph becomes a `Comment` of kind `Paragraph`.
    /// The following blank-separated paragraph is kept.
    #[test]
    fn test_comment_style_block_dropped() -> Result<(), Error> {
        let input = "[comment]\n--\nhidden\n\n== Hidden heading\n--\n\n[comment]\nhidden para.\n\nVisible.\n";
        let mut state = ParserState::new_for_test(input);
        let doc = document_parser::document(input, &mut state)??;
        assert_eq!(doc.blocks.len(), 3);

        // The open block: a `--`-delimited DelimitedComment (no leftover
        // `comment` style) retaining its raw inner text.
        assert!(
            matches!(
                &doc.blocks[0],
                Block::DelimitedBlock(delimited)
                if matches!(&delimited.inner, DelimitedBlockType::DelimitedComment(nodes)
                    if matches!(&nodes[0], InlineNode::PlainText(text)
                        if text.content.contains("Hidden heading")))
                        && delimited.delimiter == "--"
                        && delimited.metadata.style.is_none()
            ),
            "the [comment] open block should be a `--` DelimitedComment"
        );

        // The paragraph: a `Comment` of kind `Paragraph`.
        assert!(
            matches!(&doc.blocks[1], Block::Comment(comment) if comment.kind == CommentKind::Paragraph),
            "the [comment] paragraph should be a Comment of kind Paragraph"
        );

        // The trailing blank-separated paragraph is normal content.
        assert!(
            matches!(&doc.blocks[2], Block::Paragraph(para)
                if matches!(&para.content[..], [InlineNode::PlainText(text)]
                    if text.content == "Visible.")),
            "the trailing paragraph should survive"
        );
        Ok(())
    }

    /// `[comment]` only suppresses open blocks and paragraphs. On any other
    /// block (e.g. a listing) `asciidoctor` ignores the style and renders the
    /// block, so it must stay a normal `DelimitedListing`, not become a comment.
    #[test]
    fn test_comment_style_on_listing_renders() -> Result<(), Error> {
        let input = "[comment]\n----\nvisible\n----\n";
        let mut state = ParserState::new_for_test(input);
        let doc = document_parser::document(input, &mut state)??;
        assert_eq!(doc.blocks.len(), 1);
        assert!(
            matches!(
                &doc.blocks[0],
                Block::DelimitedBlock(delimited)
                    if matches!(delimited.inner, DelimitedBlockType::DelimitedListing(_))
            ),
            "a [comment]-styled listing must still render as a listing"
        );
        Ok(())
    }

    /// An `<<id>>` whose target is defined nowhere is an unresolved reference
    /// and warns, pointing at the cross-reference.
    #[test]
    fn test_unresolved_reference_warns() -> Result<(), Error> {
        let input = "A paragraph.\n\nSee <<missing>>.\n";
        let mut state = ParserState::new_for_test(input);
        let _ = document_parser::document(input, &mut state)??;
        let warnings = state.warnings.borrow();
        assert!(
            warnings.iter().any(|w| matches!(
                &w.kind,
                crate::WarningKind::UnresolvedReference { target } if target == "missing"
            )),
            "expected an unresolved-reference warning for `missing`"
        );
        Ok(())
    }

    /// An `<<id>>` pointing at an inline `[[id]]` anchor resolves (the catalog
    /// includes inline anchors), so it does not warn.
    #[test]
    fn test_inline_anchor_reference_resolves() -> Result<(), Error> {
        let input = "Some text [[here]] in a paragraph.\n\nSee <<here>>.\n";
        let mut state = ParserState::new_for_test(input);
        let doc = document_parser::document(input, &mut state)??;
        assert!(doc.references.contains_key("here"));
        let warnings = state.warnings.borrow();
        assert!(
            !warnings
                .iter()
                .any(|w| matches!(&w.kind, crate::WarningKind::UnresolvedReference { .. })),
            "a reference to an existing inline anchor must not warn"
        );
        Ok(())
    }

    /// IDs attached to formatted spans are reference targets in the same way
    /// as explicit inline anchors.
    #[test]
    fn test_formatted_inline_ids_resolve_cross_references() -> Result<(), Error> {
        let input = r#"A [#bold-id]*bold*, [#italic-id]_italic_, [#mono-id]`mono`, [#mark-id]#mark#, [#sub-id]~sub~, [#super-id]^super^, [#double-id]"`double`", and [#single-id]'`single`'.

See <<bold-id>>, <<italic-id>>, <<mono-id>>, <<mark-id>>, <<sub-id>>, <<super-id>>, <<double-id>>, and <<single-id>>.
"#;
        let mut state = ParserState::new_for_test(input);
        let doc = document_parser::document(input, &mut state)??;

        for id in [
            "bold-id",
            "italic-id",
            "mono-id",
            "mark-id",
            "sub-id",
            "super-id",
            "double-id",
            "single-id",
        ] {
            assert!(
                doc.references.contains_key(id),
                "formatted inline ID `{id}` must be a reference target"
            );
        }
        let warnings = state.warnings.borrow();
        assert!(
            !warnings.iter().any(|warning| matches!(
                warning.kind,
                crate::WarningKind::UnresolvedReference { .. }
            )),
            "formatted inline references should resolve: {warnings:?}"
        );
        Ok(())
    }

    #[test]
    fn test_link_ids_resolve_cross_references_without_using_link_text() -> Result<(), Error> {
        let input = r"Before: <<link-id>>, <<url-id>>, <<mailto-id>>, and <<bare-id>>.

link:https://example.com[Link text,id=link-id,role=hot]

https://example.org[URL text,id=url-id]

mailto:person@example.com[Mail text,id=mailto-id]

link:https://example.net[,id=bare-id]

After: <<link-id>>, <<url-id>>, <<mailto-id>>, and <<bare-id>>.
";
        let mut state = ParserState::new_for_test(input);
        let doc = document_parser::document(input, &mut state)??;

        for id in ["link-id", "url-id", "mailto-id", "bare-id"] {
            let Some(reference) = doc.references.get(id) else {
                unreachable!("link ID `{id}` must be a reference target");
            };
            assert!(reference.xreflabel.is_none());
            assert!(reference.title.is_none());
        }
        let warnings = state.warnings.borrow();
        assert!(
            !warnings.iter().any(|warning| matches!(
                warning.kind,
                crate::WarningKind::UnresolvedReference { .. }
            )),
            "link references should resolve: {warnings:?}"
        );
        Ok(())
    }

    #[test]
    fn test_link_id_catalog_keeps_first_definition_and_ignores_positional_text() -> Result<(), Error>
    {
        let input = r"link:https://example.com[First,id=duplicate]

link:https://example.org[Second,id=duplicate]

link:https://example.net[Text,positional-id]
";
        let mut state = ParserState::new_for_test(input);
        let doc = document_parser::document(input, &mut state)??;

        assert_eq!(
            doc.references
                .get("duplicate")
                .expect("the first link ID must be catalogued")
                .location
                .start
                .line,
            1
        );
        assert!(!doc.references.contains_key("positional-id"));
        Ok(())
    }

    /// An inline `[[id]]` anchor inside a callout-list item's text is catalogued
    /// (callout lists are walked like other list containers), so a reference to
    /// it resolves.
    #[test]
    fn test_callout_item_inline_anchor_resolves() -> Result<(), Error> {
        let input = "----\ncode <1>\n----\n<1> Note with an [[cnote]] anchor.\n\nSee <<cnote>>.\n";
        let mut state = ParserState::new_for_test(input);
        let doc = document_parser::document(input, &mut state)??;
        assert!(doc.references.contains_key("cnote"));
        let warnings = state.warnings.borrow();
        assert!(
            !warnings
                .iter()
                .any(|w| matches!(&w.kind, crate::WarningKind::UnresolvedReference { .. })),
            "a reference to an anchor inside a callout item must not warn"
        );
        Ok(())
    }

    #[test]
    fn test_callout_item_explicit_continuation_attaches_block() -> Result<(), Error> {
        let input = "----\nfirst <1>\nsecond <2>\n----\n<1> First explanation.\n+\nAttached paragraph.\n<2> Second explanation.\n";
        let mut state = ParserState::new_for_test(input);
        let doc = document_parser::document(input, &mut state)??;
        let list = doc
            .blocks
            .iter()
            .find_map(|block| {
                if let Block::CalloutList(list) = block {
                    Some(list)
                } else {
                    None
                }
            })
            .expect("callout list must be parsed");

        assert_eq!(list.items.len(), 2);
        assert_eq!(list.items[0].blocks.len(), 1);
        assert!(matches!(list.items[0].blocks[0], Block::Paragraph(_)));
        assert!(list.items[1].blocks.is_empty());
        assert!(state.warnings.borrow().is_empty());
        Ok(())
    }

    /// A titled block with an id is collected into `references` so a `<<id>>`
    /// reference can resolve to its title.
    #[test]
    fn test_titled_block_collected_in_references() -> Result<(), Error> {
        let input = "[[data-table]]\n.Important Data\n[cols=\"1,1\"]\n|===\n| a | b\n|===\n";
        let mut state = ParserState::new_for_test(input);
        let doc = document_parser::document(input, &mut state)??;
        let entry = doc
            .references
            .get("data-table")
            .expect("titled block should be a reference target");
        let title = entry
            .title
            .as_ref()
            .expect("titled block has reference text");
        assert!(
            matches!(&title[..], [InlineNode::PlainText(text)] if text.content == "Important Data")
        );
        // The location points at the anchor on line 1 (for LSP navigation).
        assert_eq!(entry.location.start.line, 1);
        Ok(())
    }

    #[test]
    fn natural_title_cross_references_resolve_to_section_ids() -> Result<(), Error> {
        let input = "Generated: <<Syntax Highlighting>>.\n\nCustom: <<Syntax Highlighting,section>>.\n\nExplicit: <<explicit-id>>.\n\nExplicit title: <<Explicit Title>>.\n\nMissing: <<Missing Title>>.\n\n== Syntax Highlighting\n\n[#explicit-id]\n== Explicit Title\n";
        let mut state = ParserState::new_for_test(input);
        let doc = document_parser::document(input, &mut state)??;
        let xrefs = doc
            .blocks
            .iter()
            .filter_map(|block| {
                let Block::Paragraph(paragraph) = block else {
                    return None;
                };
                paragraph.content.iter().find_map(|inline| {
                    let InlineNode::Macro(InlineMacro::CrossReference(xref)) = inline else {
                        return None;
                    };
                    Some(xref.target)
                })
            })
            .collect::<Vec<_>>();

        assert_eq!(
            xrefs,
            [
                "_syntax_highlighting",
                "_syntax_highlighting",
                "explicit-id",
                "explicit-id",
                "Missing Title",
            ]
        );
        let warnings = state.warnings.borrow();
        assert_eq!(
            warnings
                .iter()
                .filter_map(|warning| {
                    let crate::WarningKind::UnresolvedReference { target } = &warning.kind else {
                        return None;
                    };
                    Some(target.as_str())
                })
                .collect::<Vec<_>>(),
            ["Missing Title"]
        );
        Ok(())
    }

    #[test]
    fn named_section_reftext_populates_catalog_toc_and_warnings()
    -> Result<(), Box<dyn std::error::Error>> {
        let input =
            include_str!("../../fixtures/tests/named_section_reftext_cross_references.adoc");
        let mut state = ParserState::new_for_test(input);
        let doc = document_parser::document(input, &mut state)??;
        let reference = doc.references.get("id").ok_or("missing id reference")?;
        assert!(
            matches!(
                reference.xreflabel.as_deref(),
                Some([InlineNode::PlainText(text)]) if text.content == "Custom Label"
            ),
            "{:?}",
            reference.xreflabel
        );
        assert_eq!(
            doc.toc_entries
                .iter()
                .find(|entry| entry.id == "id")
                .and_then(|entry| entry.xreflabel),
            Some("Custom Label")
        );
        let formatted = doc
            .references
            .get("formatted")
            .ok_or("missing formatted reference")?;
        assert!(
            matches!(
                formatted.xreflabel.as_deref(),
                Some([
                    InlineNode::PlainText(prefix),
                    InlineNode::BoldText(bold),
                    InlineNode::PlainText(suffix),
                ]) if prefix.content == "Custom "
                    && suffix.content == " Label"
                    && matches!(
                        &bold.content[..],
                        [InlineNode::PlainText(text)] if text.content == "Formatted"
                    )
            ),
            "{:?}",
            formatted.xreflabel
        );
        assert_eq!(
            state
                .warnings
                .borrow()
                .iter()
                .filter_map(|warning| {
                    let crate::WarningKind::UnresolvedReference { target } = &warning.kind else {
                        return None;
                    };
                    Some(target.as_str())
                })
                .collect::<Vec<_>>(),
            ["Actual Title", "Generated Title", "Custom Formatted Label"]
        );
        Ok(())
    }

    #[test]
    fn passthrough_xref_warnings_use_restored_targets_and_source_locations() -> Result<(), Error> {
        let input = include_str!(
            "../../fixtures/tests/natural_title_cross_references_with_passthrough.adoc"
        );
        let result = crate::parse(input, &crate::Options::default())?;

        let warnings = result
            .warnings()
            .iter()
            .filter_map(|warning| {
                let crate::WarningKind::UnresolvedReference { target } = &warning.kind else {
                    return None;
                };
                let location = warning.source_location()?;
                Some((
                    target.as_str(),
                    location.location.start.line,
                    location.location.start.column,
                ))
            })
            .collect::<Vec<_>>();
        assert_eq!(
            warnings,
            [
                ("Target raw Title", 3, 15),
                ("Target raw Title", 4, 14),
                ("Missing raw Title", 5, 16),
                ("Missing raw Title", 6, 15),
            ]
        );
        Ok(())
    }

    #[test]
    fn compat_mode_skips_natural_title_cross_reference_resolution() -> Result<(), Error> {
        let input = "= Document\n:compat-mode:\n\nNatural: <<Syntax Highlighting>>.\n\nExplicit: <<_syntax_highlighting>>.\n\n== Syntax Highlighting\n";
        let mut state = ParserState::new_for_test(input);
        let doc = document_parser::document(input, &mut state)??;
        let targets = doc
            .blocks
            .iter()
            .filter_map(|block| {
                let Block::Paragraph(paragraph) = block else {
                    return None;
                };
                paragraph.content.iter().find_map(|inline| {
                    let InlineNode::Macro(InlineMacro::CrossReference(xref)) = inline else {
                        return None;
                    };
                    Some(xref.target)
                })
            })
            .collect::<Vec<_>>();

        assert_eq!(targets, ["Syntax Highlighting", "_syntax_highlighting"]);
        assert!(doc.references.contains_key("_syntax_highlighting"));
        assert_eq!(
            state
                .warnings
                .borrow()
                .iter()
                .filter_map(|warning| {
                    let crate::WarningKind::UnresolvedReference { target } = &warning.kind else {
                        return None;
                    };
                    Some(target.as_str())
                })
                .collect::<Vec<_>>(),
            ["Syntax Highlighting"]
        );
        Ok(())
    }

    #[test]
    fn compat_mode_natural_reference_resolution_follows_source_position() -> Result<(), Error> {
        let input = "Before set: <<First Natural>>.\n\n:compat-mode:\n\nAfter set: <<Second Natural>>.\n\n:compat-mode!:\n\nAfter unset: <<Third Natural>>.\n\n== First Natural\n\n== Second Natural\n\n== Third Natural\n";
        let mut state = ParserState::new_for_test(input);
        let doc = document_parser::document(input, &mut state)??;
        let targets = doc
            .blocks
            .iter()
            .filter_map(|block| {
                let Block::Paragraph(paragraph) = block else {
                    return None;
                };
                paragraph.content.iter().find_map(|inline| {
                    let InlineNode::Macro(InlineMacro::CrossReference(xref)) = inline else {
                        return None;
                    };
                    Some(xref.target)
                })
            })
            .collect::<Vec<_>>();

        assert_eq!(
            targets,
            ["_first_natural", "Second Natural", "_third_natural"]
        );
        assert_eq!(
            state
                .warnings
                .borrow()
                .iter()
                .filter_map(|warning| {
                    let crate::WarningKind::UnresolvedReference { target } = &warning.kind else {
                        return None;
                    };
                    Some(target.as_str())
                })
                .collect::<Vec<_>>(),
            ["Second Natural"]
        );
        Ok(())
    }

    #[test]
    fn interdocument_xref_macro_targets_are_not_naturally_resolved() -> Result<(), Error> {
        let input = "Empty: xref:Other.adoc[].\n\nExplicit: xref:Other.adoc[Other].\n\nShorthand: <<Other.adoc>>.\n\nFragment: xref:Foo#Bar[].\n\n== Other.adoc\n\n== Foo#Bar\n";
        let mut state = ParserState::new_for_test(input);
        let doc = document_parser::document(input, &mut state)??;
        let targets = doc
            .blocks
            .iter()
            .filter_map(|block| {
                let Block::Paragraph(paragraph) = block else {
                    return None;
                };
                paragraph.content.iter().find_map(|inline| {
                    let InlineNode::Macro(InlineMacro::CrossReference(xref)) = inline else {
                        return None;
                    };
                    Some(xref.target)
                })
            })
            .collect::<Vec<_>>();

        assert_eq!(
            targets,
            ["Other.adoc", "Other.adoc", "_other_adoc", "Foo#Bar"]
        );
        assert!(
            state.warnings.borrow().is_empty(),
            "interdocument targets and the resolved shorthand must not warn"
        );
        Ok(())
    }

    #[cfg(feature = "setext")]
    #[test]
    fn natural_title_cross_references_resolve_setext_section_ids() -> Result<(), Error> {
        let input = "See <<Setext Title>>.\n\nSetext Title\n------------\n";
        let mut state = ParserState::new_for_test(input);
        std::rc::Rc::make_mut(&mut state.options).setext = true;
        let doc = document_parser::document(input, &mut state)??;
        let xrefs = doc
            .blocks
            .iter()
            .filter_map(|block| {
                let Block::Paragraph(paragraph) = block else {
                    return None;
                };
                paragraph.content.iter().find_map(|inline| {
                    let InlineNode::Macro(InlineMacro::CrossReference(xref)) = inline else {
                        return None;
                    };
                    Some(xref.target)
                })
            })
            .collect::<Vec<_>>();

        assert_eq!(xrefs, ["_setext_title"]);
        Ok(())
    }

    #[test]
    fn captioned_references_keep_source_order_xrefstyle_and_target_caption() -> Result<(), Error> {
        let input = "= Caption references\n:xrefstyle: short\n\nShort: <<figure-target>>.\n\n:table-caption: ReferenceTable\n:xrefstyle: full\n\nFull: <<table-target>>.\n\n:xrefstyle: basic\n\nBasic: <<figure-target>>.\n\n:table-caption:\n:xrefstyle: short\n\nNumber only: <<table-target>>.\n\n:table-caption!:\n\nTarget label: <<table-target>>.\n\n:table-caption: TargetTable\n\n[[figure-target]]\n.A figure\nimage::figure.svg[]\n\n[[table-target]]\n.A table\n|===\n|Cell\n|===\n";
        let mut state = ParserState::new_for_test(input);
        let doc = document_parser::document(input, &mut state)??;
        let xrefs = doc
            .blocks
            .iter()
            .filter_map(|block| {
                let Block::Paragraph(paragraph) = block else {
                    return None;
                };
                paragraph.content.iter().find_map(|inline| {
                    let InlineNode::Macro(InlineMacro::CrossReference(xref)) = inline else {
                        return None;
                    };
                    Some((xref.target, xref.xrefstyle, xref.caption_label))
                })
            })
            .collect::<Vec<_>>();

        assert_eq!(
            xrefs,
            [
                (
                    "figure-target",
                    crate::XrefStyle::Short,
                    crate::XrefCaptionLabel::AtTarget,
                ),
                (
                    "table-target",
                    crate::XrefStyle::Full,
                    crate::XrefCaptionLabel::AtReference("ReferenceTable"),
                ),
                (
                    "figure-target",
                    crate::XrefStyle::Basic,
                    crate::XrefCaptionLabel::AtTarget,
                ),
                (
                    "table-target",
                    crate::XrefStyle::Short,
                    crate::XrefCaptionLabel::NumberOnly,
                ),
                (
                    "table-target",
                    crate::XrefStyle::Short,
                    crate::XrefCaptionLabel::AtTarget,
                ),
            ]
        );
        assert!(matches!(
            doc.references
                .get("figure-target")
                .and_then(|reference| reference.caption.as_ref()),
            Some(Caption::Numbered {
                kind: CaptionKind::Figure,
                label,
                number: Some(number),
            }) if label == "Figure" && number.get() == 1
        ));
        assert!(matches!(
            doc.references
                .get("table-target")
                .and_then(|reference| reference.caption.as_ref()),
            Some(Caption::Numbered {
                kind: CaptionKind::Table,
                label,
                number: Some(number),
            }) if label == "TargetTable" && number.get() == 1
        ));
        Ok(())
    }

    #[test]
    fn test_titled_single_line_admonition_keeps_reference_title() -> Result<(), Error> {
        let input = "[[notice]]\n.Admonition *Title*\nNOTE: note\n\nSee <<notice>>.\n";
        let mut state = ParserState::new_for_test(input);
        let doc = document_parser::document(input, &mut state)??;
        let entry = doc
            .references
            .get("notice")
            .expect("titled admonition should be a reference target");
        let title = entry
            .title
            .as_ref()
            .expect("titled admonition should keep its reference text");

        assert!(
            matches!(
                &title[..],
                [InlineNode::PlainText(prefix), InlineNode::BoldText(bold)]
                    if prefix.content == "Admonition "
                        && matches!(
                            &bold.content[..],
                            [InlineNode::PlainText(text)] if text.content == "Title"
                        )
            ),
            "{title:?}"
        );
        assert!(
            !state.warnings.borrow().iter().any(|warning| matches!(
                warning.kind,
                crate::WarningKind::UnresolvedReference { .. }
            ))
        );
        Ok(())
    }

    /// A block with an id but no title is still a reference target — present in
    /// the catalog with no reference text (`title: None`). This distinguishes a
    /// resolvable-but-untitled id (renders `[id]`) from an absent/unresolved id.
    #[test]
    fn test_untitled_block_in_references_without_reftext() -> Result<(), Error> {
        let input = "[[untitled]]\n[cols=\"1,1\"]\n|===\n| a | b\n|===\n";
        let mut state = ParserState::new_for_test(input);
        let doc = document_parser::document(input, &mut state)??;
        let entry = doc
            .references
            .get("untitled")
            .expect("untitled block with an id is still a reference target");
        assert!(
            entry.title.is_none(),
            "untitled block has no reference text"
        );
        Ok(())
    }

    /// The first element to claim an id owns its reference text, matching
    /// asciidoctor: a later element with the same id (here a formatted span,
    /// which carries no title) does not take the text away from the titled
    /// block that registered first.
    #[test]
    fn test_duplicate_id_keeps_first_reference_text() -> Result<(), Error> {
        let input = "[[dup]]\n.Titled Block\n====\nbody\n====\n\nA [#dup]*bold* span.\n";
        let mut state = ParserState::new_for_test(input);
        let doc = document_parser::document(input, &mut state)??;
        let entry = doc
            .references
            .get("dup")
            .expect("the id is a reference target");
        let title = entry
            .title
            .as_ref()
            .expect("the first registration keeps its reference text");
        assert!(
            matches!(
                &title[..],
                [InlineNode::PlainText(text)] if text.content == "Titled Block"
            ),
            "{title:?}"
        );
        Ok(())
    }

    /// A reference label is inline content, so `[[id,*Bold* label]]` reaches
    /// converters as parsed inline nodes rather than literal asterisks.
    #[test]
    fn test_reference_label_is_parsed_as_inlines() -> Result<(), Error> {
        let input = "Some [[labelled,*Bold* label]]text.\n";
        let mut state = ParserState::new_for_test(input);
        let doc = document_parser::document(input, &mut state)??;
        let label = doc
            .references
            .get("labelled")
            .expect("the id is a reference target")
            .xreflabel
            .as_ref()
            .expect("the anchor has a label");
        assert!(
            matches!(
                &label[..],
                [InlineNode::BoldText(bold), InlineNode::PlainText(rest)]
                    if rest.content == " label"
                        && matches!(
                            &bold.content[..],
                            [InlineNode::PlainText(text)] if text.content == "Bold"
                        )
            ),
            "{label:?}"
        );
        Ok(())
    }

    #[test]
    fn test_reference_label_restores_passthrough_syntax() -> Result<(), Error> {
        let input = "Some [[labelled,+++<mark>Label</mark>+++]]text.\n";
        let mut state = ParserState::new_for_test(input);
        let doc = document_parser::document(input, &mut state)??;
        let label = doc
            .references
            .get("labelled")
            .expect("the id is a reference target")
            .xreflabel
            .as_ref()
            .expect("the anchor has a label");
        assert!(
            matches!(
                &label[..],
                [InlineNode::PlainText(text)]
                    if text.content == "+++<mark>Label</mark>+++"
            ),
            "{label:?}"
        );
        Ok(())
    }

    #[test]
    fn test_nested_passthrough_retains_substitution_policy() -> Result<(), Error> {
        let input = "*before +++<mark>nested</mark>+++ after*\n";
        let mut state = ParserState::new_for_test(input);
        let doc = document_parser::document(input, &mut state)??;
        assert!(
            matches!(
                &doc.blocks[..],
                [Block::Paragraph(paragraph)]
                    if matches!(
                        &paragraph.content[..],
                        [InlineNode::BoldText(bold)]
                            if matches!(
                                &bold.content[..],
                                [InlineNode::PlainText(before), InlineNode::RawText(raw), InlineNode::PlainText(after)]
                                    if before.content == "before "
                                        && raw.content == "<mark>nested</mark>"
                                        && raw.subs.is_empty()
                                        && after.content == " after"
                            )
                    )
            ),
            "{:?}",
            doc.blocks
        );
        Ok(())
    }

    /// An author line that doesn't parse as structured authors is kept as a
    /// single author, and the parser warns (acdc-only heads-up; asciidoctor is
    /// silent). The warning points at the author line.
    #[test]
    fn test_non_standard_author_line_emits_warning() -> Result<(), Error> {
        let input = "= Doc Title\nAuthor: Roberto Avanzi (Lead), Ruud Derwig\n:foo: bar\n\nBody.\n";
        let mut state = ParserState::new_for_test(input);
        let _ = document_parser::document(input, &mut state)??;
        let warnings = state.warnings.borrow();
        let warning = warnings
            .iter()
            .find(|w| {
                matches!(
                    &w.kind,
                    crate::WarningKind::NonStandardAuthorLine { line }
                        if line == "Author: Roberto Avanzi (Lead), Ruud Derwig"
                )
            })
            .expect("expected non-standard author line warning");
        // The warning points at the author line (line 2 of the input).
        let loc = warning
            .source_location()
            .expect("warning should carry a location");
        assert_eq!(loc.location.start.line, 2);
        Ok(())
    }

    /// A discrete heading marked with the legacy `float` block style warns so
    /// authors can migrate to `discrete`.
    #[test]
    fn test_legacy_float_discrete_heading_warns() -> Result<(), Error> {
        let input = "== Parent\n\n[float]\n==== Floating\n";
        let mut state = ParserState::new_for_test(input);
        let _ = document_parser::document(input, &mut state)??;
        let warnings = state.warnings.borrow();
        assert!(
            warnings
                .iter()
                .any(|w| matches!(&w.kind, crate::WarningKind::LegacyFloatDiscreteHeading)),
            "`[float]` discrete heading should warn"
        );
        Ok(())
    }

    /// `float` only marks a discrete heading as a block *style*. The preferred
    /// `[discrete]`, a table's `float=` layout attribute, and a bare `float`
    /// positional (which leaves the block an ordinary section) must NOT raise the
    /// legacy-`float` warning.
    #[test]
    fn test_no_legacy_float_warning() -> Result<(), Error> {
        for input in [
            "== Parent\n\n[discrete]\n==== Disc\n",
            "[float=\"center\",cols=\"1,1\"]\n|===\n| a | b\n|===\n",
            "= Doc\n\n[#f,float]\n=== Ordinary Section\n",
        ] {
            let mut state = ParserState::new_for_test(input);
            let _ = document_parser::document(input, &mut state)??;
            let warnings = state.warnings.borrow();
            assert!(
                !warnings
                    .iter()
                    .any(|w| matches!(&w.kind, crate::WarningKind::LegacyFloatDiscreteHeading)),
                "input {input:?} should not raise the legacy-float warning"
            );
        }
        Ok(())
    }

    /// A plain `Firstname Lastname` author line parses structurally and must
    /// NOT raise the non-standard-author warning.
    #[test]
    fn test_standard_author_line_no_warning() -> Result<(), Error> {
        let input = "= Doc Title\nRoberto Avanzi\n:foo: bar\n\nBody.\n";
        let mut state = ParserState::new_for_test(input);
        let _ = document_parser::document(input, &mut state)??;
        let warnings = state.warnings.borrow();
        assert!(
            !warnings
                .iter()
                .any(|w| matches!(&w.kind, crate::WarningKind::NonStandardAuthorLine { .. })),
            "structured author line should not warn"
        );
        Ok(())
    }

    /// A trailing partial row that cannot fill a complete row is dropped, and
    /// the parser warns at the location of the dropped cell — matching
    /// asciidoctor's "dropping cells from incomplete row" message.
    #[test]
    fn test_incomplete_final_row_emits_dropping_warning() -> Result<(), Error> {
        // The lone `|g` on line 5 cannot complete a 3-column row.
        let input = "[cols=\"3*\"]\n|===\n|a |b |c\n|d |e |f\n|g\n|===\n";
        let mut state = ParserState::new_for_test(input);
        let _ = document_parser::document(input, &mut state)??;
        let warnings = state.warnings.borrow();
        let warning = warnings
            .iter()
            .find(|w| matches!(&w.kind, crate::WarningKind::TableIncompleteRow))
            .expect("expected dropping-cells warning");
        let loc = warning
            .source_location()
            .expect("warning should carry a location");
        assert_eq!(loc.location.start.line, 5);
        Ok(())
    }

    /// Without a document title, the first-section-level check is silent
    /// (matches asciidoctor's behavior).
    #[test]
    fn test_first_section_without_doc_title_does_not_warn() -> Result<(), Error> {
        let input = "=== No title above me\n\nContent\n";
        let mut state = ParserState::new_for_test(input);
        let _ = document_parser::document(input, &mut state)??;
        let warnings = state.warnings.borrow();
        assert!(
            !warnings.iter().any(|w| matches!(
                &w.kind,
                crate::WarningKind::SectionLevelOutOfSequence { .. }
            )),
            "should not warn without doc title, got: {warnings:?}",
        );
        Ok(())
    }

    /// Valid structure (doc title + level 1 first section) must not warn.
    #[test]
    fn test_first_section_level_1_no_warning() -> Result<(), Error> {
        let input = "= Doc Title\n\n== Good\n\n=== Nested\n";
        let mut state = ParserState::new_for_test(input);
        let _ = document_parser::document(input, &mut state)??;
        let warnings = state.warnings.borrow();
        assert!(
            !warnings.iter().any(|w| matches!(
                &w.kind,
                crate::WarningKind::SectionLevelOutOfSequence { .. }
            )),
            "should not warn for valid structure, got: {warnings:?}",
        );
        Ok(())
    }

    /// An opened table that never closes before EOF emits an
    /// `UnterminatedTable { separator, equals }` warning and still
    /// produces a table (matching asciidoctor's recovery).
    #[test]
    fn test_unterminated_pipe_table_emits_warning() -> Result<(), Error> {
        let input = "|===\n| A | B\n| C | D\n";
        let mut state = ParserState::new_for_test(input);
        let doc = document_parser::document(input, &mut state)??;

        let warnings = state.warnings.borrow();
        let warning = warnings
            .iter()
            .find(|w| {
                matches!(
                    &w.kind,
                    crate::WarningKind::UnterminatedTable { delimiter } if delimiter == "|===",
                )
            })
            .expect("expected unterminated table warning");
        let loc = warning
            .source_location()
            .expect("warning should carry a location");
        // Warning should point to the opening `|===` on line 1.
        assert_eq!(loc.location.start.line, 1);

        // The document should still contain a table block.
        let has_table = doc.blocks.iter().any(|b| {
            matches!(
                b,
                Block::DelimitedBlock(DelimitedBlock {
                    inner: DelimitedBlockType::DelimitedTable(_),
                    ..
                })
            )
        });
        assert!(has_table, "expected a table block in the document");
        Ok(())
    }

    /// The `!===` (exclamation) table delimiter is also covered by the
    /// unterminated fallback, and the warning carries the actual opening
    /// delimiter so consumers can distinguish between delimiter variants.
    #[test]
    fn test_unterminated_excl_table_emits_warning() -> Result<(), Error> {
        let input = "!===\n! A ! B\n";
        let mut state = ParserState::new_for_test(input);
        let _ = document_parser::document(input, &mut state)??;
        let warnings = state.warnings.borrow();
        assert!(
            warnings.iter().any(|w| matches!(
                &w.kind,
                crate::WarningKind::UnterminatedTable { delimiter } if delimiter == "!===",
            )),
            "expected unterminated table warning with `!===` delimiter, got: {warnings:?}",
        );
        Ok(())
    }

    /// Diagnostics emitted from inside an `a`-style cell must point at the
    /// offending token within the cell, not at the cell's `a|` style prefix.
    /// Repro for the case where a nested `!===` is left unterminated:
    /// the warning's reported line should match the line of `!===`, not the
    /// line of `a|`.
    #[test]
    fn test_warning_in_ascii_cell_points_at_inner_token() -> Result<(), Error> {
        // Lines:
        //   1: `[cols="1a"]`
        //   2: `|===`
        //   3: `a|`           <- cell style prefix
        //   4: `!===`         <- offending unterminated inner table
        //   5: `! Inner A ! Inner B`
        //   6: `|===`
        let input = "[cols=\"1a\"]\n|===\na|\n!===\n! Inner A ! Inner B\n|===\n";
        let mut state = ParserState::new_for_test(input);
        let _ = document_parser::document(input, &mut state)??;
        let warnings = state.warnings.borrow();
        let warning = warnings
            .iter()
            .find(|w| {
                matches!(
                    &w.kind,
                    crate::WarningKind::UnterminatedTable { delimiter } if delimiter == "!===",
                )
            })
            .expect("expected unterminated inner-table warning");
        let loc = warning
            .source_location()
            .expect("warning should carry a location");
        let line = loc.location.start.line;
        assert_eq!(
            line, 4,
            "warning should point at line 4 (the `!===`), not the `a|` line; got {line}",
        );
        Ok(())
    }

    /// A properly closed table must not emit an unterminated warning.
    #[test]
    fn test_terminated_table_does_not_warn() -> Result<(), Error> {
        let input = "|===\n| A | B\n|===\n";
        let mut state = ParserState::new_for_test(input);
        let _ = document_parser::document(input, &mut state)??;
        let warnings = state.warnings.borrow();
        assert!(
            !warnings
                .iter()
                .any(|w| matches!(&w.kind, crate::WarningKind::UnterminatedTable { .. })),
            "should not warn for a properly closed table, got: {warnings:?}",
        );
        Ok(())
    }

    /// Degenerate case: the document is just an opening delimiter with no
    /// content and no close. Asciidoctor still warns ("unterminated table
    /// block"). The unterminated fallback rule should match and produce an
    /// empty table rather than falling through to paragraph parsing.
    #[test]
    fn test_unterminated_pipe_table_with_no_content_emits_warning() -> Result<(), Error> {
        let input = "|===\n";
        let mut state = ParserState::new_for_test(input);
        let _ = document_parser::document(input, &mut state)??;
        let warnings = state.warnings.borrow();
        assert!(
            warnings.iter().any(|w| matches!(
                &w.kind,
                crate::WarningKind::UnterminatedTable { delimiter } if delimiter == "|===",
            )),
            "expected unterminated table warning for empty open, got: {warnings:?}",
        );
        Ok(())
    }

    /// Same as above but exercised through the public `parse` entry point
    /// (which runs the preprocessor first). Catches the case where the
    /// preprocessor normalises the input in a way that breaks the
    /// unterminated fallback.
    #[test]
    fn test_unterminated_pipe_table_empty_through_parse_entry() {
        let opts = crate::Options::default();
        let res = crate::parse("|===\n", &opts).expect("parse should succeed");
        let has_warning = res.warnings().iter().any(|w| {
            matches!(
                &w.kind,
                crate::WarningKind::UnterminatedTable { delimiter } if delimiter == "|===",
            )
        });
        assert!(
            has_warning,
            "expected unterminated table warning through parse(), got: {:?}",
            res.warnings(),
        );
    }

    /// Every delimited block whose opening delimiter runs to end of input
    /// without a close is still produced (closed at EOF) and emits an
    /// `UnterminatedDelimitedBlock` warning carrying the block kind and the
    /// literal opening delimiter — matching asciidoctor's recovery.
    #[test]
    fn test_unterminated_delimited_blocks_emit_warning() -> Result<(), Error> {
        // (delimiter line + content, expected kind, expected opening delimiter).
        // A leading `para\n\n` keeps the delimiter in the document body — a
        // `////` at the very start would otherwise be eaten by the header's
        // leading-comment scan.
        let cases = [
            ("====\ntext", "example", "===="),
            ("----\ntext", "listing", "----"),
            ("....\ntext", "literal", "...."),
            ("****\ntext", "sidebar", "****"),
            ("____\ntext", "quote", "____"),
            ("--\ntext", "open", "--"),
            ("////\ntext", "comment", "////"),
            ("++++\ntext", "pass", "++++"),
            ("```\ntext", "listing", "```"),
        ];
        for (block, want_kind, want_delim) in cases {
            let input = &format!("para\n\n{block}");
            let mut state = ParserState::new_for_test(input);
            let doc = document_parser::document(input, &mut state)??;
            let warnings = state.warnings.borrow();
            assert!(
                warnings.iter().any(|w| matches!(
                    &w.kind,
                    crate::WarningKind::UnterminatedDelimitedBlock { kind, delimiter }
                        if *kind == want_kind && delimiter == want_delim,
                )),
                "expected unterminated {want_kind} warning for input {input:?}, got: {warnings:?}",
            );
            // The block is still produced and recorded as unterminated (no
            // closing delimiter location).
            assert!(
                doc.blocks.iter().any(|b| matches!(
                    b,
                    Block::DelimitedBlock(d) if d.close_delimiter_location.is_none(),
                )),
                "expected an unterminated delimited block for input {input:?}, got: {:?}",
                doc.blocks,
            );
        }
        Ok(())
    }

    /// A properly closed delimited block must not emit the unterminated warning.
    #[test]
    fn test_terminated_delimited_block_no_warning() -> Result<(), Error> {
        let input = "====\ntext\n====";
        let mut state = ParserState::new_for_test(input);
        let _ = document_parser::document(input, &mut state)??;
        let warnings = state.warnings.borrow();
        assert!(
            !warnings.iter().any(|w| matches!(
                &w.kind,
                crate::WarningKind::UnterminatedDelimitedBlock { .. },
            )),
            "a closed example block should not warn, got: {warnings:?}",
        );
        Ok(())
    }

    /// Exercised through the public `parse` entry point (which runs the
    /// preprocessor, stripping the trailing newline) so a lone `====\n`
    /// source still reaches the grammar as an unterminated block.
    #[test]
    fn test_unterminated_example_through_parse_entry() {
        let opts = crate::Options::default();
        let res = crate::parse("====\ntext\n", &opts).expect("parse should succeed");
        let has_warning = res.warnings().iter().any(|w| {
            matches!(
                &w.kind,
                crate::WarningKind::UnterminatedDelimitedBlock { kind, delimiter }
                    if *kind == "example" && delimiter == "====",
            )
        });
        assert!(
            has_warning,
            "expected unterminated example warning through parse(), got: {:?}",
            res.warnings(),
        );
    }
}
