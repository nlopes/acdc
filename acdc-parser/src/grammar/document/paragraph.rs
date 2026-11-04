//! Interpret paragraph styles after their source boundaries have been matched.

use std::str::FromStr;

use crate::{
    Admonition, AdmonitionVariant, Block, Comment, CommentKind, CompatibilityMode, Error,
    InlineNode, Paragraph, Raw,
    grammar::{
        ParserState,
        document::{
            callouts::resolve_verbatim_callouts,
            verbatim::{get_literal_paragraph, resolve_verbatim_inlines, verbatim_substitutions},
        },
        helpers::BlockParsingMetadata,
        inline_preprocessor::{ProcessedKind, SourceMap},
        inline_processing::{process_inlines, process_mapped_inlines},
        passthrough_processing::process_block_passthrough,
    },
    model::Substitution,
};

pub(super) fn indented_style(mode: CompatibilityMode, style: Option<&str>) -> Option<&str> {
    match style {
        Some("paragraph") if mode == CompatibilityMode::Draft => Some("normal"),
        Some(
            "normal" | "source" | "listing" | "literal" | "verse" | "quote" | "sidebar" | "example"
            | "abstract" | "partintro" | "pass" | "comment" | "NOTE" | "TIP" | "IMPORTANT"
            | "CAUTION" | "WARNING",
        ) => style,
        _ => None,
    }
}

pub(super) fn process_normal_indented<'a>(
    state: &ParserState<'a>,
    metadata: &BlockParsingMetadata<'a>,
    content: &'a str,
    absolute_start: usize,
) -> Result<Vec<InlineNode<'a>>, Error> {
    let indent = common_indent(content);
    let mut text = String::with_capacity(content.len());
    let mut map = SourceMap::default();
    let mut start = 0;
    for line in content.split_inclusive('\n') {
        let removed = line
            .bytes()
            .take(indent)
            .take_while(|byte| *byte == b' ' || *byte == b'\t')
            .count();
        if removed != 0 {
            map.add_replacement(start, start + removed, 0, ProcessedKind::Indentation);
        }
        text.push_str(line.get(removed..).unwrap_or_default());
        start += line.len();
    }
    let map = map.with_origin(SourceMap::default(), absolute_start);
    process_mapped_inlines(
        state,
        state.intern_str(&text),
        &map,
        metadata,
        &state.document_attributes,
        crate::grammar::state::InlineRules::default(),
    )
}

pub(super) fn common_indent(content: &str) -> usize {
    content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            line.bytes()
                .take_while(|byte| *byte == b' ' || *byte == b'\t')
                .count()
        })
        .min()
        .unwrap_or(0)
}

pub(super) struct ParagraphParams<'a> {
    pub(super) start: usize,
    pub(super) content_start: usize,
    pub(super) end: usize,
    pub(super) offset: usize,
    pub(super) content: &'a str,
    pub(super) admonition: Option<(String, usize, usize)>,
}

pub(super) fn build_paragraph<'a>(
    state: &mut ParserState<'a>,
    block_metadata: &BlockParsingMetadata<'a>,
    params: ParagraphParams<'a>,
) -> Result<Block<'a>, Error> {
    let ParagraphParams {
        start,
        content_start,
        end,
        offset,
        content,
        admonition,
    } = params;
    let is_styled_verbatim = matches!(
        block_metadata.metadata.style,
        Some("source" | "listing" | "literal")
    );

    // A `[comment]`-styled paragraph is a comment that produces no
    // output; keep its raw text on the `Comment` for tooling.
    if block_metadata.metadata.style == Some("comment") {
        return Ok(Block::Comment(Comment {
            kind: CommentKind::Paragraph,
            content,
            location: state.create_block_location(start, end, offset),
        }));
    }

    let selected_style = if content.starts_with([' ', '\t']) {
        let Some(style) = indented_style(
            state.options.compatibility_mode,
            block_metadata.metadata.style,
        ) else {
            return get_literal_paragraph(
                state,
                content,
                start,
                content_start,
                end,
                offset,
                block_metadata,
            );
        };
        Some(style)
    } else {
        block_metadata.metadata.style
    };

    let source_text = content;
    let content = if selected_style == Some("pass") {
        process_block_passthrough(
            Raw {
                metadata: None,
                content,
                location: state.create_block_location(content_start, end, offset),
                subs: Vec::new(),
            },
            block_metadata.metadata.substitutions.as_ref(),
            state,
        )
    } else if content.starts_with([' ', '\t']) && selected_style == Some("normal") {
        process_normal_indented(state, block_metadata, content, content_start + offset)?
    } else if is_styled_verbatim {
        process_verbatim_paragraph(state, block_metadata, content, content_start, end, offset)?
    } else {
        process_inlines(state, block_metadata, content_start, end, offset, content)?.0
    };

    let title = block_metadata.title.clone();

    let admonition = admonition.or_else(|| {
        block_metadata
            .metadata
            .style
            .filter(|style| matches!(*style, "NOTE" | "TIP" | "IMPORTANT" | "CAUTION" | "WARNING"))
            .map(|style| (style.to_string(), content_start, content_start))
    });
    if let Some((variant, admonition_start, admonition_end)) = admonition {
        let Ok(parsed_variant) = AdmonitionVariant::from_str(&variant) else {
            tracing::error!("invalid admonition variant");
            return Err(Error::InvalidAdmonitionVariant(
                Box::new(state.create_error_source_location(
                    state.create_location(admonition_start + offset, admonition_end + offset - 1),
                )),
                variant,
            ));
        };
        tracing::debug!("found admonition block with variant");
        Ok(Block::Admonition(Admonition::from_paragraph(
            parsed_variant,
            Paragraph {
                source_text: Some(source_text),
                content,
                metadata: block_metadata.metadata.clone(),
                title,
                location: state.create_block_location(content_start, end, offset),
            },
            state.create_block_location(start, end, offset),
        )))
    } else {
        let mut metadata = block_metadata.metadata.clone();
        metadata.move_positional_attributes_to_attributes();
        metadata.style = selected_style;

        tracing::debug!(node_count = content.len(), "found paragraph block");
        Ok(Block::Paragraph(Paragraph {
            source_text: Some(source_text),
            content,
            metadata,
            title,
            location: state.create_block_location(start, end, offset),
        }))
    }
}

fn process_verbatim_paragraph<'a>(
    state: &mut ParserState<'a>,
    block_metadata: &BlockParsingMetadata<'a>,
    content: &'a str,
    content_start: usize,
    end: usize,
    offset: usize,
) -> Result<Vec<InlineNode<'a>>, Error> {
    let content_location = state.create_block_location(content_start, end, offset);
    let (verbatim_content, callouts) = resolve_verbatim_callouts(
        state,
        content,
        content_location,
        block_metadata
            .substitutions
            .enabled(&Substitution::Callouts),
        !block_metadata
            .metadata
            .attributes
            .contains_key("line-comment"),
    );
    let content = if callouts.is_empty() {
        let verbatim_metadata = BlockParsingMetadata {
            substitutions: verbatim_substitutions(block_metadata),
            ..BlockParsingMetadata::default()
        };
        process_inlines(
            state,
            &verbatim_metadata,
            content_start,
            end,
            offset,
            content,
        )?
        .0
    } else {
        resolve_verbatim_inlines(state, block_metadata, verbatim_content)?
    };
    state.pending_callouts.extend(callouts);
    Ok(content)
}
