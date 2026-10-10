//! Verbatim block and paragraph content.

use crate::{
    Block, BlockMetadata, DelimitedBlockType, Error, InlineNode, Paragraph, Plain,
    grammar::{
        ParserState,
        document::{
            callouts::resolve_verbatim_callouts,
            delimited::{DelimitedKind, DelimitedParams},
            metadata::extract_source_attributes,
        },
        helpers::BlockParsingMetadata,
        location_walk::walk_inline_nodes_mut,
    },
    model::{
        PositionalAttribute, Substitution,
        substitution::{SubstitutionPlan, VERBATIM},
    },
};

#[cfg(feature = "pre-spec-subs")]
use crate::grammar::inline_processing::process_verbatim_inlines;

/// Verbatim block (`----` listing or `....` literal, including the Markdown fence):
/// preserves whitespace while resolving enabled inline substitutions and callouts.
pub(super) fn verbatim_inner<'input>(
    state: &mut ParserState<'input>,
    block_metadata: &BlockParsingMetadata<'input>,
    metadata: &mut BlockMetadata<'input>,
    p: &DelimitedParams<'input>,
) -> Result<DelimitedBlockType<'input>, Error> {
    // A Markdown fence language becomes a positional `source` style so language
    // detection works like `[source,lang]` (never set for `....`/`----`).
    if let Some(language) = p.lang {
        metadata.prepend_synthetic_positional_attribute(PositionalAttribute {
            value: language,
            substitutions: false,
            location: None,
        });
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
        !metadata.attributes.contains_key("line-comment"),
    );
    state.pending_callouts.extend(callouts);
    let inlines = resolve_verbatim_inlines(state, block_metadata, inlines)?;
    Ok(if p.kind == DelimitedKind::Literal {
        DelimitedBlockType::DelimitedLiteral(inlines)
    } else {
        DelimitedBlockType::DelimitedListing(inlines)
    })
}

pub(super) fn verbatim_substitutions(metadata: &BlockParsingMetadata<'_>) -> SubstitutionPlan {
    #[cfg(feature = "pre-spec-subs")]
    if let Some(spec) = &metadata.metadata.substitutions {
        return SubstitutionPlan::from_substitutions(&spec.resolve(VERBATIM));
    }
    let _ = metadata;
    SubstitutionPlan::from_substitutions(VERBATIM)
}

fn needs_verbatim_inlines(substitutions: SubstitutionPlan) -> bool {
    [
        Substitution::Quotes,
        Substitution::Attributes,
        Substitution::Macros,
    ]
    .iter()
    .any(|substitution| substitutions.enabled(substitution))
}

#[cfg(feature = "pre-spec-subs")]
pub(super) fn resolve_verbatim_inlines<'a>(
    state: &mut ParserState<'a>,
    metadata: &BlockParsingMetadata<'_>,
    inlines: Vec<InlineNode<'a>>,
) -> Result<Vec<InlineNode<'a>>, Error> {
    let substitutions = verbatim_substitutions(metadata);
    if !needs_verbatim_inlines(substitutions) {
        return Ok(inlines);
    }
    let metadata = BlockParsingMetadata {
        substitutions,
        ..BlockParsingMetadata::default()
    };
    let mut resolved = Vec::new();
    for node in inlines {
        if let InlineNode::VerbatimText(text) = node {
            resolved.extend(process_verbatim_inlines(
                state,
                &metadata,
                text.location.absolute_start,
                text.content,
            )?);
        } else {
            resolved.push(node);
        }
    }
    Ok(resolved)
}

#[cfg(not(feature = "pre-spec-subs"))]
pub(super) fn resolve_verbatim_inlines<'a>(
    _state: &mut ParserState<'a>,
    _metadata: &BlockParsingMetadata<'_>,
    inlines: Vec<InlineNode<'a>>,
) -> Result<Vec<InlineNode<'a>>, Error> {
    Ok(inlines)
}

pub(super) fn get_literal_paragraph<'input>(
    state: &mut ParserState<'input>,
    content: &'input str,
    start: usize,
    content_start: usize,
    end: usize,
    offset: usize,
    block_metadata: &BlockParsingMetadata<'input>,
) -> Result<Block<'input>, Error> {
    tracing::debug!(
        input_len = content.len(),
        "paragraph starts with a space - switching to literal block"
    );
    let mut metadata = block_metadata.metadata.clone();
    metadata.move_positional_attributes_to_attributes();
    metadata.style = Some("literal");
    let location = state.create_block_location(start, end, offset);

    let substitutions = verbatim_substitutions(block_metadata);
    let (mut inlines, callouts) = resolve_verbatim_callouts(
        state,
        content,
        state.create_block_location(content_start, end, offset),
        substitutions.enabled(&Substitution::Callouts),
        !metadata.attributes.contains_key("line-comment"),
    );
    if callouts.is_empty() && !needs_verbatim_inlines(substitutions) {
        // Keep the existing plain-text AST for paragraphs without structured inlines.
        for node in &mut inlines {
            if let InlineNode::VerbatimText(text) = node {
                *node = InlineNode::PlainText(Plain {
                    content: text.content,
                    location: location.clone(),
                    escaped: false,
                });
            }
        }
    } else {
        inlines = resolve_verbatim_inlines(state, block_metadata, inlines)?;
    }
    state.pending_callouts.extend(callouts);

    // Mixed indentation preserves all leading spaces, as in Asciidoctor.
    let all_lines_have_leading_space = content
        .lines()
        .all(|line| line.is_empty() || line.starts_with(' '));
    if all_lines_have_leading_space {
        for node in &mut inlines {
            // Parse before dedenting so callouts and nested formatting keep source locations.
            walk_inline_nodes_mut(node, &mut |node| {
                let starts_line = node.location().start.column == 1;
                let content = if let InlineNode::VerbatimText(text) = node {
                    &mut text.content
                } else if let InlineNode::PlainText(text) = node {
                    &mut text.content
                } else {
                    return;
                };
                let mut first = true;
                *content = state.intern_join(
                    content.split_inclusive('\n').map(|line| {
                        let strip = !first || starts_line;
                        first = false;
                        if strip {
                            line.strip_prefix(' ').unwrap_or(line)
                        } else {
                            line
                        }
                    }),
                    "",
                );
            });
        }
    }
    Ok(Block::Paragraph(Paragraph {
        source_text: Some(content),
        content: inlines,
        metadata,
        title: block_metadata.title.clone(),
        location,
    }))
}
