//! Delimited block construction and delimiter recovery.

use crate::{
    Admonition, AdmonitionForm, AdmonitionVariant, Block, DelimitedComment, Error, Example,
    InlineNode, Location, Open, Passthrough, Plain, Quote, Raw, Sidebar, SourceLocation, StemBlock,
    StemNotation, Substitution, Verse, Warning, WarningKind,
    grammar::{
        ParserState,
        document::verbatim::build_delimited_verbatim,
        document_parser,
        helpers::BlockParsingMetadata,
        inline_processing::{adjust_and_log_parse_error, process_inlines},
        state::BlockContext,
    },
    model::Delimited,
    model::SectionLevel,
    model::StemContent,
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

impl<'a> ContentRequest<'a> {
    /// An enclosing section ends the child sequence before the quote closes.
    /// Report the heading itself regardless of how the PEG input was split.
    pub(super) fn section_boundary_warning(
        &self,
        tail: &'a str,
        offset: usize,
        state: &mut ParserState<'a>,
    ) -> Option<crate::Warning> {
        if !matches!(self.kind, ContentKind::Blocks) || self.context != BlockContext::Document {
            return None;
        }
        let position = document_parser::content_section_boundary(
            tail,
            state,
            offset,
            self.parent_section_level,
        )
        .ok()?;
        let location =
            state.create_error_source_location(state.create_location(position, position));
        Some(crate::Warning::new(
            crate::WarningKind::ContentRecovery {
                message: "A section at the enclosing level or higher interrupts the quote. Its body was discarded.".into(),
            },
            Some(location),
        ))
    }

    pub(super) fn warn_recovery(
        &self,
        error: &super::RecognitionError,
        state: &mut ParserState<'a>,
    ) {
        let offset = error.location.offset;
        if let Some(warning) = self
            .source
            .get(offset..)
            .and_then(|tail| self.section_boundary_warning(tail, self.offset + offset, state))
        {
            state.add_warning(warning);
            return;
        }
        adjust_and_log_parse_error(error, self.offset, state, self.error_context);
    }
}

pub(super) trait ContentParser<'a> {
    /// Buffered cells have already passed through the table decoder.
    fn raw_cell_separator(&self) -> Option<char> {
        None
    }

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
        let toc_start = state.toc_entries.len();
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
            state.toc_entries.truncate(toc_start);
            request.warn_recovery(&error, state);
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
pub(super) fn resolve_delimited_close<'input>(
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
    pub(super) fn has_compound_body(self, style: Option<&str>) -> bool {
        matches!(
            self.with_style(style),
            Self::Example | Self::Open | Self::Sidebar | Self::Quote
        ) && !matches!(style, Some("comment" | "verse"))
    }

    pub(super) fn raw_context(self) -> Option<crate::extensions::RawBlockContext> {
        use crate::extensions::RawBlockContext;
        match self {
            Self::Listing => Some(RawBlockContext::Listing),
            Self::Literal => Some(RawBlockContext::Literal),
            Self::Comment
            | Self::Example
            | Self::Open
            | Self::Sidebar
            | Self::Passthrough
            | Self::Quote => None,
        }
    }

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
    // Empty source can be a streamed opener. Its builder still needs the
    // child request to know the parsing context and where the body starts.
    if !content.is_empty() && content.trim().is_empty() {
        return Ok(Vec::new());
    }
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

/// Fields every delimited kind shares, assembled before a builder attaches its body.
pub(super) type DelimitedShell<'input> = Delimited<'input, ()>;

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
    let shell = Delimited {
        metadata: block_metadata.metadata.clone(),
        body: (),
        delimiter: p.open_delim,
        title: block_metadata.title.clone(),
        location: state.create_block_location(p.start, p.end, p.offset),
        open_delimiter_location: Some(state.create_location(
            p.open_start + p.offset,
            p.open_start + p.offset + p.open_delim.len().saturating_sub(1),
        )),
        close_delimiter_location,
        source_text: Some(p.source_text),
    };

    match p.kind {
        DelimitedKind::Example => build_delimited_example(state, block_metadata, shell, p, parser),
        DelimitedKind::Comment => Ok(build_delimited_comment(state, shell, p)),
        DelimitedKind::Listing | DelimitedKind::Literal => {
            build_delimited_verbatim(state, block_metadata, shell, p)
        }
        DelimitedKind::Open => build_delimited_open(state, block_metadata, shell, p, parser),
        DelimitedKind::Sidebar => build_delimited_sidebar(state, block_metadata, shell, p, parser),
        DelimitedKind::Passthrough => Ok(build_delimited_pass(state, shell, p)),
        DelimitedKind::Quote => build_delimited_quote(state, block_metadata, shell, p, parser),
    }
}

/// `====` example block, or an admonition when carrying an admonition style.
fn build_delimited_example<'input>(
    state: &mut ParserState<'input>,
    block_metadata: &BlockParsingMetadata<'input>,
    mut shell: DelimitedShell<'input>,
    p: &DelimitedParams<'input>,
    parser: &mut impl ContentParser<'input>,
) -> Result<Block<'input>, Error> {
    shell.metadata.move_positional_attributes_to_attributes();
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
        shell.metadata.style = None;
        let mut admonition = Admonition::new(variant, blocks, shell.location)
            .with_form(AdmonitionForm::Delimited(p.open_delim))
            .with_title(shell.title);
        admonition.metadata = shell.metadata;
        return Ok(Block::Admonition(admonition));
    }
    Ok(Block::DelimitedExample(Example(shell.with_body(blocks))))
}

/// The raw text of a comment body, kept as one plain node.
fn comment_body<'input>(
    state: &ParserState<'input>,
    p: &DelimitedParams<'input>,
) -> Vec<InlineNode<'input>> {
    vec![InlineNode::Plain(Plain {
        content: p.content,
        location: state.create_block_location(p.content_start, p.content_end, p.offset),
        escaped: false,
    })]
}

/// `////` comment block: the raw inner text, rendered nowhere.
fn build_delimited_comment<'input>(
    state: &ParserState<'input>,
    mut shell: DelimitedShell<'input>,
    p: &DelimitedParams<'input>,
) -> Block<'input> {
    shell.metadata.move_positional_attributes_to_attributes();
    Block::DelimitedComment(DelimitedComment(shell.with_body(comment_body(state, p))))
}

/// `--` open block, or a non-rendering comment when carrying a `[comment]` style.
fn build_delimited_open<'input>(
    state: &mut ParserState<'input>,
    block_metadata: &BlockParsingMetadata<'input>,
    mut shell: DelimitedShell<'input>,
    p: &DelimitedParams<'input>,
    parser: &mut impl ContentParser<'input>,
) -> Result<Block<'input>, Error> {
    shell.metadata.move_positional_attributes_to_attributes();
    if block_metadata.metadata.style == Some("comment") {
        shell.metadata.style = None;
        let body = comment_body(state, p);
        return Ok(Block::DelimitedComment(DelimitedComment(
            shell.with_body(body),
        )));
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
    Ok(Block::DelimitedOpen(Open(shell.with_body(blocks))))
}

/// `****` sidebar block.
fn build_delimited_sidebar<'input>(
    state: &mut ParserState<'input>,
    block_metadata: &BlockParsingMetadata<'input>,
    mut shell: DelimitedShell<'input>,
    p: &DelimitedParams<'input>,
    parser: &mut impl ContentParser<'input>,
) -> Result<Block<'input>, Error> {
    shell.metadata.move_positional_attributes_to_attributes();
    let blocks = parse_block_content(
        state,
        block_metadata,
        p.content,
        p.content_start,
        p.offset,
        "Error parsing sidebar content as blocks",
        parser,
    )?;
    Ok(Block::DelimitedSidebar(Sidebar(shell.with_body(blocks))))
}

/// `++++` passthrough block, or math when its style selects STEM notation.
fn build_delimited_pass<'input>(
    state: &ParserState<'input>,
    mut shell: DelimitedShell<'input>,
    p: &DelimitedParams<'input>,
) -> Block<'input> {
    let metadata = &mut shell.metadata;
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
        Block::DelimitedStem(StemBlock(
            shell.with_body(StemContent { content, notation }),
        ))
    } else {
        metadata.move_positional_attributes_to_attributes();
        let content_location =
            state.create_block_location(p.content_start, p.content_end, p.offset);
        let content = super::super::passthrough_processing::process_block_passthrough(
            Raw {
                metadata: None,
                content: p.content,
                location: content_location,
                subs: Vec::new(),
            },
            metadata.substitutions.as_ref(),
            state,
        );
        Block::DelimitedPassthrough(Passthrough(shell.with_body(content)))
    }
}

/// Parse a `____` quote body as blocks, or a verse body as inline content.
fn build_delimited_quote<'input>(
    state: &mut ParserState<'input>,
    block_metadata: &BlockParsingMetadata<'input>,
    mut shell: DelimitedShell<'input>,
    p: &DelimitedParams<'input>,
    parser: &mut impl ContentParser<'input>,
) -> Result<Block<'input>, Error> {
    shell.metadata.move_positional_attributes_to_attributes();

    if shell.metadata.style == Some("verse") {
        let (inlines, _) = process_inlines(
            state,
            block_metadata,
            p.content_start,
            p.content_end,
            p.offset,
            p.content,
        )?;
        Ok(Block::DelimitedVerse(Verse(shell.with_body(inlines))))
    } else if shell.metadata.style.is_some() {
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
        Ok(Block::DelimitedQuote(Quote(shell.with_body(blocks))))
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
        Ok(Block::DelimitedQuote(Quote(shell.with_body(blocks))))
    }
}
