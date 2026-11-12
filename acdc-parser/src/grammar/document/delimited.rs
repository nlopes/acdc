//! Delimited block construction and delimiter recovery.

use crate::{
    Admonition, AdmonitionForm, AdmonitionVariant, Block, BlockMetadata, DelimitedBlock,
    DelimitedBlockType, Error, InlineNode, Location, Plain, Raw, SourceLocation, StemContent,
    StemNotation, Substitution, Title, Warning, WarningKind,
    grammar::{
        ParserState,
        document::verbatim::verbatim_inner,
        document_parser,
        helpers::BlockParsingMetadata,
        inline_processing::{adjust_and_log_parse_error, process_inlines},
        state::BlockContext,
    },
    model::SectionLevel,
};

use super::{RecognitionError, table::InitialAttributes};

#[derive(Clone, Copy)]
pub(super) enum ContentKind {
    Compound,
    Blocks,
}

/// A child parse requested by a committed block's existing builder.
#[derive(Clone, Copy)]
pub(super) struct ContentRequest<'a> {
    pub(super) source: &'a str,
    pub(super) offset: usize,
    pub(super) parent_section_level: Option<SectionLevel>,
    pub(super) context: BlockContext,
    pub(super) kind: ContentKind,
    pub(super) error_context: &'static str,
}

pub(super) trait ContentParser<'a> {
    fn parse(
        &mut self,
        state: &mut ParserState<'a>,
        request: ContentRequest<'a>,
    ) -> Result<Vec<Block<'a>>, Error>;

    fn nested_document(
        &mut self,
        state: &mut ParserState<'a>,
        source: &'a str,
        offset: usize,
        initial_attributes: &mut InitialAttributes<'a>,
    ) -> Result<Result<Vec<Block<'a>>, Error>, RecognitionError> {
        document_parser::nested_document_blocks(source, state, offset, initial_attributes)
    }
}

pub(super) struct GrammarContent;

impl<'a> ContentParser<'a> for GrammarContent {
    fn parse(
        &mut self,
        state: &mut ParserState<'a>,
        request: ContentRequest<'a>,
    ) -> Result<Vec<Block<'a>>, Error> {
        let result = match request.kind {
            ContentKind::Compound => document_parser::compound_blocks(
                request.source,
                state,
                request.offset,
                request.parent_section_level,
            ),
            ContentKind::Blocks => document_parser::blocks(
                request.source,
                state,
                request.offset,
                request.parent_section_level,
                None,
            ),
        };
        result.unwrap_or_else(|error| {
            adjust_and_log_parse_error(
                &error,
                request.source,
                request.offset,
                state,
                request.error_context,
            );
            Ok(Vec::new())
        })
    }
}

pub(super) fn parse_content<'a>(
    state: &mut ParserState<'a>,
    request: ContentRequest<'a>,
    parser: &mut impl ContentParser<'a>,
) -> Result<Vec<Block<'a>>, Error> {
    let outer_context = std::mem::replace(&mut state.block_context, request.context);
    let outer_scope = state.source_scope_offset;
    if state.input_origin.is_some() {
        state.source_scope_offset = request.offset;
    }
    let result = parser.parse(state, request);
    state.block_context = outer_context;
    state.source_scope_offset = outer_scope;
    result
}

/// Return an error if the closing delimiter does not match the opening delimiter.
pub(super) fn check_delimiters(
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

/// Validate a closing delimiter and return its location.
/// If the block has no closing delimiter, warn at the opening delimiter and
/// return `None`. The caller keeps the block through end of input.
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
        state.add_warning(Warning::new(
            WarningKind::UnterminatedDelimitedBlock {
                kind: p.kind.name(),
                delimiter: p.open_delim.to_string(),
            },
            Some(state.create_error_source_location(open_delimiter_location)),
        ));
        Ok(None)
    }
}

/// The block kind selected by the opening delimiter.
/// Markdown fences select `Listing` and can also specify a language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DelimitedKind {
    Example,
    Comment,
    Listing,
    Literal,
    Open,
    Sidebar,
    Passthrough,
    Quote,
}

impl DelimitedKind {
    pub(super) fn with_style(self, style: Option<&str>) -> Self {
        match (self, style) {
            (Self::Open | Self::Literal, Some("source" | "listing")) => Self::Listing,
            (Self::Open | Self::Listing, Some("literal")) => Self::Literal,
            (
                Self::Open,
                Some("example" | "NOTE" | "TIP" | "IMPORTANT" | "CAUTION" | "WARNING"),
            ) => Self::Example,
            (Self::Open, Some("quote" | "verse")) => Self::Quote,
            (Self::Open, Some("sidebar")) => Self::Sidebar,
            (Self::Open, Some("pass")) => Self::Passthrough,
            (kind, _) => kind,
        }
    }

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
            DelimitedKind::Passthrough => "pass",
            DelimitedKind::Quote => "quote",
        }
    }
}

/// Parse a compound body without allowing section blocks.
/// Empty or invalid content produces an empty block list. Invalid content also
/// produces a warning at its source location.
fn parse_block_content<'input>(
    state: &mut ParserState<'input>,
    block_metadata: &BlockParsingMetadata<'input>,
    content: &'input str,
    content_start: usize,
    offset: usize,
    error_context: &'static str,
    parser: &mut impl ContentParser<'input>,
) -> Result<Vec<Block<'input>>, Error> {
    if content.trim().is_empty() {
        Ok(Vec::new())
    } else {
        parse_content(
            state,
            ContentRequest {
                source: content,
                offset: content_start + offset,
                parent_section_level: block_metadata.parent_section_level,
                context: BlockContext::Compound,
                kind: ContentKind::Compound,
                error_context,
            },
            parser,
        )
    }
}

/// Source ranges and delimiters for a block.
/// `close` is `None` when the block reaches end of input without a closing delimiter.
#[derive(Clone, Copy, Debug)]
pub(super) struct DelimitedParams<'input> {
    pub(super) kind: DelimitedKind,
    /// The opening delimiter as it appeared in source (e.g. `"===="`).
    pub(super) open_delim: &'input str,
    pub(super) content: &'input str,
    pub(super) source_text: &'input str,
    pub(super) open_start: usize,
    pub(super) start: usize,
    pub(super) content_start: usize,
    pub(super) content_end: usize,
    /// End offset of the whole block (`span_end`).
    pub(super) end: usize,
    pub(super) offset: usize,
    pub(super) close: Option<(usize, &'input str)>,
}

struct DelimitedLocations {
    block: Location,
    open: Location,
    close: Option<Location>,
}

/// Build a block from its matched delimiters and content.
/// If the closing delimiter is missing, keep the block and warn, as in asciidoctor.
pub(super) fn build_delimited_block<'input>(
    state: &mut ParserState<'input>,
    block_metadata: &BlockParsingMetadata<'input>,
    p: &DelimitedParams<'input>,
) -> Result<Block<'input>, Error> {
    build_delimited_block_with_parser(state, block_metadata, p, &mut GrammarContent)
}

pub(super) fn build_delimited_block_with_parser<'input>(
    state: &mut ParserState<'input>,
    block_metadata: &BlockParsingMetadata<'input>,
    p: &DelimitedParams<'input>,
    parser: &mut impl ContentParser<'input>,
) -> Result<Block<'input>, Error> {
    let close_delimiter_location = resolve_delimited_close(state, p)?;
    let resolved = DelimitedParams {
        kind: p.kind.with_style(block_metadata.metadata.style),
        ..*p
    };
    let p = &resolved;
    let locations = DelimitedLocations {
        block: state.create_block_location(p.start, p.end, p.offset),
        open: state.create_location(
            p.open_start + p.offset,
            p.open_start + p.offset + p.open_delim.len().saturating_sub(1),
        ),
        close: close_delimiter_location,
    };
    let mut metadata = block_metadata.metadata.clone();

    let inner = match p.kind {
        // An example block can become an admonition (a different `Block` variant),
        // so it constructs and returns the whole block itself.
        DelimitedKind::Example => {
            return build_example_block(state, block_metadata, metadata, p, locations, parser);
        }
        DelimitedKind::Comment => comment_inner(state, &mut metadata, p),
        DelimitedKind::Listing | DelimitedKind::Literal => {
            verbatim_inner(state, block_metadata, &mut metadata, p)?
        }
        DelimitedKind::Open => open_inner(state, block_metadata, &mut metadata, p, parser)?,
        DelimitedKind::Sidebar => sidebar_inner(state, block_metadata, &mut metadata, p, parser)?,
        DelimitedKind::Passthrough => pass_inner(state, &mut metadata, p),
        DelimitedKind::Quote => quote_inner(state, block_metadata, &mut metadata, p, parser)?,
    };

    Ok(assemble_delimited(
        metadata,
        p,
        inner,
        block_metadata.title.clone(),
        locations,
    ))
}

/// Assemble the common `Block::DelimitedBlock` shell shared by every kind.
fn assemble_delimited<'input>(
    metadata: BlockMetadata<'input>,
    p: &DelimitedParams<'input>,
    inner: DelimitedBlockType<'input>,
    title: Title<'input>,
    locations: DelimitedLocations,
) -> Block<'input> {
    Block::DelimitedBlock(DelimitedBlock {
        metadata,
        delimiter: p.open_delim,
        inner,
        title,
        location: locations.block,
        open_delimiter_location: Some(locations.open),
        close_delimiter_location: locations.close,
        source_text: Some(p.source_text),
    })
}

/// `====` example block, or an admonition when carrying an admonition style.
fn build_example_block<'input>(
    state: &mut ParserState<'input>,
    block_metadata: &BlockParsingMetadata<'input>,
    mut metadata: BlockMetadata<'input>,
    p: &DelimitedParams<'input>,
    locations: DelimitedLocations,
    parser: &mut impl ContentParser<'input>,
) -> Result<Block<'input>, Error> {
    metadata.move_positional_attributes_to_attributes();
    let blocks = parse_block_content(
        state,
        block_metadata,
        p.content,
        p.content_start,
        p.offset,
        "Error parsing example content as blocks in example block",
        parser,
    )?;
    // An admonition style (NOTE/TIP/…) turns the example block into an admonition.
    if let Some(style) = block_metadata.metadata.style
        && let Ok(variant) = style.parse::<AdmonitionVariant>()
    {
        metadata.style = None;
        let mut admonition = Admonition::new(variant, blocks, locations.block)
            .with_form(AdmonitionForm::Delimited(p.open_delim))
            .with_title(block_metadata.title.clone());
        admonition.metadata = metadata;
        return Ok(Block::Admonition(admonition));
    }
    Ok(assemble_delimited(
        metadata,
        p,
        DelimitedBlockType::DelimitedExample(blocks),
        block_metadata.title.clone(),
        locations,
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

/// `--` open block, or a non-rendering comment when carrying a `[comment]` style.
fn open_inner<'input>(
    state: &mut ParserState<'input>,
    block_metadata: &BlockParsingMetadata<'input>,
    metadata: &mut BlockMetadata<'input>,
    p: &DelimitedParams<'input>,
    parser: &mut impl ContentParser<'input>,
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
        parser,
    )?;
    Ok(DelimitedBlockType::DelimitedOpen(blocks))
}

/// `****` sidebar block.
fn sidebar_inner<'input>(
    state: &mut ParserState<'input>,
    block_metadata: &BlockParsingMetadata<'input>,
    metadata: &mut BlockMetadata<'input>,
    p: &DelimitedParams<'input>,
    parser: &mut impl ContentParser<'input>,
) -> Result<DelimitedBlockType<'input>, Error> {
    metadata.move_positional_attributes_to_attributes();
    let blocks = parse_block_content(
        state,
        block_metadata,
        p.content,
        p.content_start,
        p.offset,
        "Error parsing sidebar content as blocks",
        parser,
    )?;
    Ok(DelimitedBlockType::DelimitedSidebar(blocks))
}

/// `++++` passthrough block, or math when its style selects STEM notation.
fn pass_inner<'input>(
    state: &ParserState<'input>,
    metadata: &mut BlockMetadata<'input>,
    p: &DelimitedParams<'input>,
) -> DelimitedBlockType<'input> {
    if matches!(metadata.style, Some("stem" | "latexmath" | "asciimath")) {
        let notation = metadata.take_next_positional_attribute();
        let notation = match metadata.style {
            Some("latexmath") => StemNotation::Latexmath,
            Some("asciimath") => StemNotation::Asciimath,
            _ => StemNotation::resolve(
                notation
                    .filter(|attribute| !attribute.value.is_empty())
                    .map(|attribute| attribute.value)
                    .or_else(|| state.document_attributes.text("stem")),
            ),
        };
        let content = p.content;
        let content = metadata.substitutions.as_ref().map_or(content, |spec| {
            let substitutions = spec.resolve(&[Substitution::SpecialChars]);
            let unsupported = substitutions
                .iter()
                .filter(|substitution| {
                    !matches!(substitution, Substitution::SpecialChars | Substitution::Attributes)
                })
                .map(ToString::to_string)
                .collect::<Vec<_>>();
            if !unsupported.is_empty() {
                state.add_warning(Warning::new(
                    WarningKind::ContentRecovery {
                        message: format!(
                            "STEM blocks do not support these substitutions: {}. Formula text is preserved; supported substitutions still apply.",
                            unsupported.join(", ")
                        ).into(),
                    },
                    Some(state.create_error_source_location(
                        state.create_block_location(p.content_start, p.content_end, p.offset)
                    )),
                ));
            }
            state.intern_cow(crate::apply_attribute_substitutions(content, &substitutions, &state.document_attributes))
        });
        metadata.move_positional_attributes_to_attributes();
        metadata.style = None;
        DelimitedBlockType::DelimitedStem(StemContent { content, notation })
    } else {
        metadata.move_positional_attributes_to_attributes();
        let content_location =
            state.create_block_location(p.content_start, p.content_end, p.offset);
        DelimitedBlockType::DelimitedPass(
            super::super::passthrough_processing::process_block_passthrough(
                Raw {
                    metadata: None,
                    content: p.content,
                    location: content_location,
                    subs: Vec::new(),
                },
                metadata.substitutions.as_ref(),
                state,
            ),
        )
    }
}

/// Parse a `____` quote body as blocks, or a verse body as inline content.
fn quote_inner<'input>(
    state: &mut ParserState<'input>,
    block_metadata: &BlockParsingMetadata<'input>,
    metadata: &mut BlockMetadata<'input>,
    p: &DelimitedParams<'input>,
    parser: &mut impl ContentParser<'input>,
) -> Result<DelimitedBlockType<'input>, Error> {
    metadata.move_positional_attributes_to_attributes();

    if metadata.style == Some("verse") {
        let (inlines, _) = process_inlines(
            state,
            block_metadata,
            p.content_start,
            p.content_end,
            p.offset,
            p.content,
        )?;
        Ok(DelimitedBlockType::DelimitedVerse(inlines))
    } else if metadata.style.is_some() {
        // A styled (non-verse) quote always parses its body, even when empty.
        let request = ContentRequest {
            source: p.content,
            offset: p.content_start + p.offset,
            parent_section_level: block_metadata.parent_section_level,
            context: state.block_context,
            kind: ContentKind::Blocks,
            error_context: "Error parsing example content as blocks in quote block",
        };
        let blocks = parse_content(state, request, parser)?;
        Ok(DelimitedBlockType::DelimitedQuote(blocks))
    } else {
        let blocks = parse_block_content(
            state,
            block_metadata,
            p.content,
            p.content_start,
            p.offset,
            "Error parsing content as blocks in quote block",
            parser,
        )?;
        Ok(DelimitedBlockType::DelimitedQuote(blocks))
    }
}
