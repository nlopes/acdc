//! Shared builders for shorthand and Markdown blockquotes.

use crate::{
    Attribution, Block, CiteTitle, DelimitedBlock, DelimitedBlockType, Error,
    grammar::{ParserState, helpers::BlockParsingMetadata, inline_processing::process_inlines},
};

use super::delimited::{ContentKind, ContentParser, ContentRequest, GrammarContent, parse_content};

pub(super) struct QuoteAttribution<'a> {
    pub(super) author: &'a str,
    pub(super) author_start: usize,
    pub(super) citation: Option<(&'a str, usize)>,
}

impl<'a> QuoteAttribution<'a> {
    pub(super) fn from_line(source: &'a str, start: usize) -> Self {
        let (author, citation) =
            source
                .split_once(',')
                .map_or((source.trim(), None), |(author, tail)| {
                    let citation_start =
                        start + author.len() + 1 + tail.len() - tail.trim_start().len();
                    (author.trim(), Some((tail.trim(), citation_start)))
                });
        Self {
            author,
            author_start: start,
            citation,
        }
    }
}

pub(super) enum QuoteSource<'a> {
    Shorthand {
        content: &'a str,
        content_start: usize,
    },
    Markdown(Vec<&'a str>),
}

pub(super) struct QuoteParams<'a> {
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) offset: usize,
    pub(super) source: QuoteSource<'a>,
    pub(super) attribution: Option<QuoteAttribution<'a>>,
}

pub(super) fn build_quote<'a>(
    state: &mut ParserState<'a>,
    metadata: &BlockParsingMetadata<'a>,
    params: &QuoteParams<'a>,
) -> Result<Block<'a>, Error> {
    build_quote_with_parser(state, metadata, params, &mut GrammarContent)
}

pub(super) fn build_quote_with_parser<'a>(
    state: &mut ParserState<'a>,
    block_metadata: &BlockParsingMetadata<'a>,
    params: &QuoteParams<'a>,
    parser: &mut impl ContentParser<'a>,
) -> Result<Block<'a>, Error> {
    let mut metadata = block_metadata.metadata.clone();
    metadata.style = Some("quote");
    if let Some(attribution) = &params.attribution {
        let (author, _) = process_inlines(
            state,
            block_metadata,
            attribution.author_start,
            attribution.author_start + attribution.author.len(),
            params.offset,
            attribution.author,
        )?;
        metadata.attribution = Some(Attribution::new(author));
        if let Some((citation, start)) = attribution.citation {
            let (citation, _) = process_inlines(
                state,
                block_metadata,
                start,
                start + citation.len(),
                params.offset,
                citation,
            )?;
            metadata.citetitle = Some(CiteTitle::new(citation));
        }
    }
    let (content, content_start, delimiter, skip_empty, error_context) = match &params.source {
        QuoteSource::Shorthand {
            content,
            content_start,
        } => (
            *content,
            *content_start,
            "\"",
            false,
            "Error parsing content as blocks in quoted paragraph",
        ),
        QuoteSource::Markdown(lines) => (
            state.intern_join(lines.iter(), "\n"),
            params.start,
            ">",
            true,
            "Error parsing content as blocks in markdown blockquote",
        ),
    };
    let blocks = if skip_empty && content.trim().is_empty() {
        Vec::new()
    } else {
        let request = ContentRequest {
            source: content,
            offset: content_start + params.offset,
            parent_section_level: block_metadata.parent_section_level,
            context: state.block_context,
            kind: ContentKind::Blocks,
            error_context,
        };
        parse_content(state, request, parser)?
    };
    Ok(Block::DelimitedBlock(DelimitedBlock {
        source_text: None,
        metadata,
        delimiter,
        inner: DelimitedBlockType::DelimitedQuote(blocks),
        title: block_metadata.title.clone(),
        location: state.create_block_location(params.start, params.end, params.offset),
        open_delimiter_location: None,
        close_delimiter_location: None,
    }))
}
