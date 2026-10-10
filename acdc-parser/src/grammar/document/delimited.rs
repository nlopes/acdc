//! Delimited block construction and delimiter recovery.

use crate::{
    Admonition, AdmonitionVariant, Block, BlockMetadata, DelimitedBlock, DelimitedBlockType, Error,
    InlineNode, Location, Plain, Raw, SourceLocation, StemContent, StemNotation, Title, Warning,
    WarningKind,
    grammar::{
        ParserState,
        document::verbatim::verbatim_inner,
        document_parser,
        helpers::BlockParsingMetadata,
        inline_processing::{adjust_and_log_parse_error, process_inlines},
        state::BlockContext,
    },
};

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

/// Parse a compound body without allowing section blocks.
/// Empty or invalid content produces an empty block list. Invalid content also
/// produces a warning at its source location.
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
        let outer_context = std::mem::replace(&mut state.block_context, BlockContext::Compound);
        let result = document_parser::compound_blocks(
            content,
            state,
            content_start + offset,
            block_metadata.parent_section_level,
        );
        state.block_context = outer_context;
        result.unwrap_or_else(|e| {
            adjust_and_log_parse_error(&e, content, content_start + offset, state, error_context);
            Ok(Vec::new())
        })
    }
}

/// Source ranges and delimiters for a block.
/// `close` is `None` when the block reaches end of input without a closing delimiter.
pub(super) struct DelimitedParams<'input> {
    pub(super) kind: DelimitedKind,
    /// The opening delimiter as it appeared in source (e.g. `"===="`).
    pub(super) open_delim: &'input str,
    /// Language captured after a Markdown ```` ``` ```` fence, if any.
    pub(super) lang: Option<&'input str>,
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

/// Build a block from its matched delimiters and content.
/// If the closing delimiter is missing, keep the block and warn, as in asciidoctor.
pub(super) fn build_delimited_block<'input>(
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
            verbatim_inner(state, block_metadata, &mut metadata, p)?
        }
        DelimitedKind::Open => open_inner(state, block_metadata, &mut metadata, p)?,
        DelimitedKind::Sidebar => sidebar_inner(state, block_metadata, &mut metadata, p)?,
        DelimitedKind::Pass => pass_inner(state, &mut metadata, p),
        DelimitedKind::Quote => quote_inner(state, block_metadata, &mut metadata, p)?,
    };

    Ok(assemble_delimited(
        metadata,
        p,
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
    p: &DelimitedParams<'input>,
    inner: DelimitedBlockType<'input>,
    title: Title<'input>,
    location: Location,
    open_delimiter_location: Location,
    close_delimiter_location: Option<Location>,
) -> Block<'input> {
    Block::DelimitedBlock(DelimitedBlock {
        metadata,
        delimiter: p.open_delim,
        inner,
        title,
        location,
        open_delimiter_location: Some(open_delimiter_location),
        close_delimiter_location,
        source_text: Some(p.source_text),
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
        p,
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
        let [notation] = metadata.take_positional_attributes::<1>();
        let notation = notation
            .filter(|attribute| !attribute.value.is_empty())
            .and_then(|attribute| attribute.value.parse::<StemNotation>().ok())
            .or_else(|| {
                state
                    .document_attributes
                    .text("stem")
                    .and_then(|value| value.parse::<StemNotation>().ok())
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

/// Parse a `____` quote body as blocks, or a verse body as inline content.
fn quote_inner<'input>(
    state: &mut ParserState<'input>,
    block_metadata: &BlockParsingMetadata<'input>,
    metadata: &mut BlockMetadata<'input>,
    p: &DelimitedParams<'input>,
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
