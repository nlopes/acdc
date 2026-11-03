//! Verbatim block and paragraph content.

use crate::{
    AttributeValue, Block, BlockMetadata, DelimitedBlockType, Error, InlineNode, Paragraph, Plain,
    Verbatim,
    grammar::{
        ParserState,
        document::{
            callouts::resolve_verbatim_callouts,
            delimited::{DelimitedKind, DelimitedParams},
            paragraph::common_indent,
        },
        helpers::BlockParsingMetadata,
        inline_processing::process_verbatim_inlines,
        location_walk::walk_inline_nodes_mut,
    },
    model::{
        Substitution,
        substitution::{SubstitutionPlan, VERBATIM},
    },
};

/// Verbatim block (`----` listing or `....` literal, including the Markdown fence):
/// preserves whitespace while resolving enabled inline substitutions and callouts.
pub(super) fn verbatim_inner<'input>(
    state: &mut ParserState<'input>,
    block_metadata: &BlockParsingMetadata<'input>,
    metadata: &mut BlockMetadata<'input>,
    p: &DelimitedParams<'input>,
) -> Result<DelimitedBlockType<'input>, Error> {
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
    if let Some(spec) = &metadata.metadata.substitutions {
        return SubstitutionPlan::from_substitutions(&spec.resolve(VERBATIM));
    }
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

pub(super) fn resolve_verbatim_inlines<'a>(
    state: &mut ParserState<'a>,
    metadata: &BlockParsingMetadata<'_>,
    inlines: Vec<InlineNode<'a>>,
) -> Result<Vec<InlineNode<'a>>, Error> {
    let substitutions = verbatim_substitutions(metadata);
    if !needs_verbatim_inlines(substitutions) {
        return Ok(inlines);
    }
    let comment_prefix = metadata.metadata.attributes.get("line-comment");
    let metadata = BlockParsingMetadata {
        substitutions,
        ..BlockParsingMetadata::default()
    };
    let mut resolved = Vec::new();
    let mut inlines = inlines.into_iter().peekable();
    let mut xml_tail = false;
    while let Some(node) = inlines.next() {
        if let InlineNode::VerbatimText(text) = node {
            let prefix = if xml_tail && text.content.starts_with("-->") {
                3
            } else {
                0
            };
            xml_tail = false;
            if prefix != 0 {
                resolved.push(InlineNode::VerbatimText(Verbatim {
                    content: &text.content[..prefix],
                    location: state.create_block_location(
                        text.location.absolute_start,
                        text.location.absolute_start + prefix,
                        0,
                    ),
                }));
            }
            let guard = (substitutions
                .precedes(&Substitution::Callouts, &Substitution::Replacements)
                && matches!(inlines.peek(), Some(InlineNode::CalloutRef(_))))
            .then(|| callout_guard(text.content, comment_prefix))
            .flatten();
            let end = guard.unwrap_or(text.content.len());
            resolved.extend(process_verbatim_inlines(
                state,
                &metadata,
                text.location.absolute_start + prefix,
                &text.content[prefix..end],
            )?);
            if guard.is_some() {
                // A callout guard is already consumed when later replacements run.
                let start = match resolved.last() {
                    Some(InlineNode::VerbatimText(last))
                        if last.location.absolute_end + 1 == text.location.absolute_start + end
                            && text.content[..end].ends_with(last.content) =>
                    {
                        let start = end - last.content.len();
                        resolved.pop();
                        start
                    }
                    _ => end,
                };
                resolved.push(InlineNode::VerbatimText(Verbatim {
                    content: &text.content[start..],
                    location: state.create_block_location(
                        text.location.absolute_start + start,
                        text.location.absolute_start + text.content.len(),
                        0,
                    ),
                }));
            }
        } else {
            xml_tail = substitutions.precedes(&Substitution::Callouts, &Substitution::Replacements)
                && matches!(node, InlineNode::CalloutRef(_))
                && matches!(resolved.last(), Some(InlineNode::VerbatimText(text)) if text.content.ends_with("<!--"));
            resolved.push(node);
        }
    }
    Ok(resolved)
}

fn callout_guard(text: &str, prefix: Option<&AttributeValue<'_>>) -> Option<usize> {
    if let Some(body) = text.strip_suffix("<!--") {
        return Some(body.len());
    }
    let candidate = text.strip_suffix(' ').unwrap_or(text);
    let strip = |prefix: &str| {
        if prefix.is_empty() {
            return None;
        }
        candidate
            .strip_suffix(prefix)
            .or_else(|| text.strip_suffix(prefix))
            .map(str::len)
    };
    match prefix {
        Some(AttributeValue::String(prefix)) => strip(prefix),
        Some(_) => None,
        None => ["//", "#", "--", ";;"].into_iter().find_map(strip),
    }
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

    let indent = common_indent(content);
    if indent != 0 {
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
                            line.get(..indent)
                                .filter(|prefix| {
                                    prefix.bytes().all(|byte| byte == b' ' || byte == b'\t')
                                })
                                .and_then(|_| line.get(indent..))
                                .unwrap_or(line)
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
