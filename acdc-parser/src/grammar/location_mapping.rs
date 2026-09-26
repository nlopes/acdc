use std::{borrow::Cow, mem::take};

use crate::{
    AttributeValue, Error, Form, IndexTermKind, IndexTermRelationship, InlineMacro, InlineNode,
    Location, PassthroughKind, Plain, ProcessedContent, Source,
};

use super::{
    ParserState,
    marked_text::map_marked_text_locations,
    passthrough_processing::replace_passthrough_placeholders,
    utf8_utils::{self, RoundDirection, snap_to_boundary},
};

/// Offset to skip the opening delimiter in constrained formatting.
/// For `*s*`, position 1 is where 's' starts.
const CONSTRAINED_CONTENT_START_OFFSET: usize = 1;

/// End offset for single character content after opening delimiter.
/// For `*s*`, position 2 is where 's' ends.
const CONSTRAINED_CONTENT_END_OFFSET: usize = 2;

/// Clamp a Location's byte offsets to valid bounds within the input string
/// and ensure they fall on UTF-8 character boundaries.
///
/// This only operates on `absolute_start/end` - the canonical byte offsets.
/// `Position` fields (line/column) are not modified.
pub(crate) fn clamp_location_bounds(location: &mut Location, input: &str) {
    let input_len = input.len();

    // Clamp to input bounds
    location.absolute_start = location.absolute_start.min(input_len);
    location.absolute_end = location.absolute_end.min(input_len);

    // Ensure start is on a valid UTF-8 boundary (round backward)
    location.absolute_start = utf8_utils::ensure_char_boundary(input, location.absolute_start);

    // Ensure end is on a valid UTF-8 boundary (round forward)
    location.absolute_end = utf8_utils::ensure_char_boundary_forward(input, location.absolute_end);

    // Ensure start <= end
    if location.absolute_start > location.absolute_end {
        location.absolute_end = location.absolute_start;
    }
}

/// Recursively clamp all locations in an `InlineNode` to valid bounds.
pub(crate) fn clamp_inline_node_locations(node: &mut InlineNode, input: &str) {
    super::location_walk::walk_inline_locations_mut(node, &mut |loc| {
        clamp_location_bounds(loc, input);
    });
}

/// Source text and substitutions for inline locations relative to one base.
pub(crate) struct LocationMappingContext<'a, 'b> {
    pub state: &'a ParserState<'b>,
    pub processed: &'a ProcessedContent<'b>,
    pub base_location: &'a Location,
}

impl<'a> LocationMappingContext<'_, 'a> {
    fn restore_passthrough_text(&self, text: &str) -> Option<&'a str> {
        contains_passthrough_placeholders(text, self.processed).then(|| {
            self.state
                .intern_str(&replace_passthrough_placeholders(text, self.processed))
        })
    }

    /// Map a preprocessed location to source coordinates, preserving delimiter
    /// handling for constrained formatting and the end bias of substitutions.
    pub(crate) fn map_location(
        &self,
        loc: &Location,
        form: Option<&Form>,
    ) -> Result<Location, Error> {
        let Self {
            state,
            processed,
            base_location,
        } = *self;
        tracing::info!(?base_location, ?loc, "mapping inline location");

        // Convert processed-relative absolute offsets into document-absolute offsets
        let mut processed_abs_start = base_location.absolute_start + loc.absolute_start;
        let mut processed_abs_end = base_location.absolute_start + loc.absolute_end;

        // A newline already occupies one inclusive source position; extending it
        // would include the first character of the next line.
        if loc.absolute_start == loc.absolute_end
            && processed.text.as_bytes().get(loc.absolute_start) != Some(&b'\n')
        {
            if loc.absolute_start == 0 && base_location.absolute_start < base_location.absolute_end
            {
                // Special case: single character inside constrained formatting like "*s*"
                // (constrained formatting has single-character delimiters).
                if matches!(form, Some(Form::Constrained)) {
                    // Constrained formatting: skip single-char delimiter to point at content
                    processed_abs_start =
                        base_location.absolute_start + CONSTRAINED_CONTENT_START_OFFSET;
                    processed_abs_end =
                        base_location.absolute_start + CONSTRAINED_CONTENT_END_OFFSET;
                } else {
                    // Advance in processed coordinates; source UTF-8 boundaries are
                    // restored after mapping the range back to the input.
                    processed_abs_end += 1;
                }
            } else {
                processed_abs_end += 1;
            }
        }

        // Map those through the preprocessor source map back to original source
        let mut mapped_abs_start = processed.source_map.map_position(processed_abs_start)?;
        let mut mapped_abs_end = processed.source_map.map_end_position(processed_abs_end)?;

        // Clamp to input bounds - preprocessor expansion can produce positions beyond input length
        let input_len = state.input.len();
        mapped_abs_start = mapped_abs_start.min(input_len);
        mapped_abs_end = mapped_abs_end.min(input_len);

        // Ensure mapped positions are on valid UTF-8 boundaries
        mapped_abs_start =
            snap_to_boundary(state.input, mapped_abs_start, RoundDirection::Backward);
        mapped_abs_end = snap_to_boundary(state.input, mapped_abs_end, RoundDirection::Forward);

        // Compute human positions from the document's line map
        let start_pos = state
            .line_map
            .offset_to_position(mapped_abs_start, state.input);
        let mut end_pos = state
            .line_map
            .offset_to_position(mapped_abs_end, state.input);

        let is_single_char_fix = mapped_abs_end == mapped_abs_start + 1
            && loc.absolute_start == 0
            && base_location.absolute_start < base_location.absolute_end;
        // For single-character content inside constrained formatting, point both
        // start and end column at the same character.
        if is_single_char_fix && matches!(form, Some(Form::Constrained)) {
            end_pos.column = start_pos.column;
        }

        Ok(Location {
            absolute_start: mapped_abs_start,
            absolute_end: mapped_abs_end,
            start: start_pos,
            end: end_pos,
        })
    }
}

/// Expand a collapsed location to its original attribute reference span.
pub(crate) fn extend_attribute_location_if_needed(
    state: &ParserState<'_>,
    processed: &ProcessedContent<'_>,
    mut location: Location,
) -> Location {
    // Check if location is collapsed and we have attribute replacements to consider
    if location.absolute_start == location.absolute_end
        && !processed.source_map.replacements.is_empty()
    {
        // Find the attribute replacement that contains this collapsed location
        if let Some(attr_replacement) = processed.source_map.replacements.iter().find(|rep| {
            rep.kind == crate::grammar::inline_preprocessor::ProcessedKind::Attribute
                && location.absolute_start >= rep.absolute_start
                && location.absolute_start < rep.absolute_end
        }) {
            tracing::debug!(from=?location, to=?attr_replacement,
                "Extending collapsed location to full attribute span",
            );

            // Extend location to cover the full original attribute
            let start_pos = state
                .line_map
                .offset_to_position(attr_replacement.absolute_start, state.input);
            let end_pos = state
                .line_map
                .offset_to_position(attr_replacement.absolute_end, state.input);
            location = Location {
                absolute_start: attr_replacement.absolute_start,
                absolute_end: attr_replacement.absolute_end,
                start: start_pos,
                end: end_pos,
            };
        }
    }
    location
}

/// Map formatted text's inner nodes back to source coordinates, expanding
/// passthrough placeholders and extending attribute-substituted spans.
pub(crate) fn map_inner_content_locations<'a>(
    content: Vec<InlineNode<'a>>,
    ctx: &LocationMappingContext<'_, 'a>,
    form: Option<&Form>,
) -> Result<Vec<InlineNode<'a>>, Error> {
    map_nodes(content, |node| {
        match node {
            InlineNode::PlainText(plain) => {
                let expanded = map_plain_text_inline_locations(plain, ctx, form)?;
                if expanded.is_none() {
                    plain.location = extend_attribute_location_if_needed(
                        ctx.state,
                        ctx.processed,
                        plain.location.clone(),
                    );
                }
                return Ok(expanded);
            }
            marked_text @ (InlineNode::ItalicText(_)
            | InlineNode::BoldText(_)
            | InlineNode::MonospaceText(_)
            | InlineNode::HighlightText(_)
            | InlineNode::SubscriptText(_)
            | InlineNode::SuperscriptText(_)
            | InlineNode::CurvedQuotationText(_)
            | InlineNode::CurvedApostropheText(_)) => map_marked_text_locations(marked_text, ctx)?,
            InlineNode::Macro(inline_macro) => map_inline_macro(inline_macro, ctx, form)?,
            InlineNode::RawText(_)
            | InlineNode::VerbatimText(_)
            | InlineNode::StandaloneCurvedApostrophe(_)
            | InlineNode::LineBreak(_)
            | InlineNode::InlineAnchor(_)
            | InlineNode::CalloutRef(_) => {}
        }
        Ok(None)
    })
}

/// Keep the original buffer unless a node expands into a replacement sequence.
fn map_nodes<'a>(
    mut content: Vec<InlineNode<'a>>,
    mut map_node: impl FnMut(&mut InlineNode<'a>) -> Result<Option<Vec<InlineNode<'a>>>, Error>,
) -> Result<Vec<InlineNode<'a>>, Error> {
    let mut expansion = None;
    for (index, node) in content.iter_mut().enumerate() {
        if let Some(nodes) = map_node(node)? {
            expansion = Some((index, nodes));
            break;
        }
    }
    let Some((index, nodes)) = expansion else {
        return Ok(content);
    };

    // After the first expansion, append into a new buffer so later expansions
    // do not repeatedly shift the remaining nodes.
    let mut mapped = Vec::with_capacity(content.len() - 1 + nodes.len());
    let mut remaining = content.into_iter();
    mapped.extend(remaining.by_ref().take(index));
    let _ = remaining.next();
    mapped.extend(nodes);
    for mut node in remaining {
        if let Some(nodes) = map_node(&mut node)? {
            mapped.extend(nodes);
        } else {
            mapped.push(node);
        }
    }
    Ok(mapped)
}

fn contains_passthrough_placeholders(content: &str, processed: &ProcessedContent<'_>) -> bool {
    !processed.passthroughs.is_empty()
        && processed.passthroughs.iter().enumerate().any(|(index, _)| {
            let placeholder = format!("���{index}���");
            content.contains(&placeholder)
        })
}

/// Map parsed inline locations to source coordinates and expand passthrough placeholders.
#[tracing::instrument(skip_all, fields(location=?location, processed=?processed, content=?content))]
pub(crate) fn map_inline_locations<'a>(
    state: &ParserState<'a>,
    processed: &ProcessedContent<'a>,
    content: Vec<InlineNode<'a>>,
    location: &Location,
) -> Result<Vec<InlineNode<'a>>, Error> {
    tracing::info!(?location, "mapping inline locations");

    let ctx = LocationMappingContext {
        state,
        processed,
        base_location: location,
    };

    map_nodes(content, |inline| {
        match inline {
            InlineNode::PlainText(plain) => {
                return map_plain_text_inline_locations(plain, &ctx, None);
            }
            marked_text @ (InlineNode::ItalicText(_)
            | InlineNode::BoldText(_)
            | InlineNode::MonospaceText(_)
            | InlineNode::HighlightText(_)
            | InlineNode::SubscriptText(_)
            | InlineNode::SuperscriptText(_)
            | InlineNode::CurvedQuotationText(_)
            | InlineNode::CurvedApostropheText(_)) => map_marked_text_locations(marked_text, &ctx)?,
            InlineNode::StandaloneCurvedApostrophe(standalone) => {
                standalone.location = ctx.map_location(&standalone.location, None)?;
            }
            InlineNode::Macro(inline_macro) => map_inline_macro(inline_macro, &ctx, None)?,
            InlineNode::LineBreak(lb) => lb.location = ctx.map_location(&lb.location, None)?,
            InlineNode::RawText(raw) => raw.location = ctx.map_location(&raw.location, None)?,
            InlineNode::VerbatimText(verbatim) => {
                verbatim.location = ctx.map_location(&verbatim.location, None)?;
            }
            InlineNode::InlineAnchor(anchor) => {
                anchor.location = ctx.map_location(&anchor.location, None)?;
                if let Some(label) = &mut anchor.bibliography_label {
                    label.content =
                        map_inline_locations(state, processed, take(&mut label.content), location)?;
                    // Citation registration uses source lines before passthrough extraction.
                    label.source = anchor
                        .location
                        .absolute_start
                        .checked_sub(1)
                        .and_then(|start| state.input.get(start..anchor.location.absolute_end + 2))
                        .and_then(|source| source.strip_prefix("[[["))
                        .and_then(|source| source.split_once("]]]").map(|(label, _)| label))
                        .and_then(|source| {
                            source.split_once(',').map(|(_, label)| label.trim_start())
                        })
                        .filter(|source| !source.contains(['\n', '\r']));
                }
                anchor.xreflabel = anchor
                    .xreflabel
                    .map(|label| restore_reference_label_passthroughs(label, state, processed));
            }
            InlineNode::CalloutRef(callout) => {
                callout.location = ctx.map_location(&callout.location, None)?;
            }
        }
        Ok(None)
    })
}

fn restore_reference_label_passthroughs<'a>(
    label: &'a str,
    state: &ParserState<'a>,
    processed: &ProcessedContent<'a>,
) -> &'a str {
    let mut restored = None;
    for (index, passthrough) in processed.passthroughs.iter().enumerate() {
        let placeholder = format!("���{index}���");
        let current = restored.as_deref().unwrap_or(label);
        if !current.contains(&placeholder) {
            continue;
        }
        let replacement = match passthrough.kind {
            PassthroughKind::AttributeRef => passthrough.text.unwrap_or_default(),
            PassthroughKind::Single
            | PassthroughKind::Double
            | PassthroughKind::Triple
            | PassthroughKind::Macro => state
                .input
                .get(passthrough.location.absolute_start..passthrough.location.absolute_end)
                .or(passthrough.text)
                .unwrap_or_default(),
        };
        restored = Some(current.replace(&placeholder, replacement));
    }
    match restored {
        Some(restored) => state.intern_str(&restored),
        None => label,
    }
}

fn restore_macro_attributes<'a>(
    inline_macro: &mut InlineMacro<'a>,
    ctx: &LocationMappingContext<'_, 'a>,
) {
    if ctx.processed.passthroughs.is_empty() {
        return;
    }

    let restore = |text: &str| ctx.restore_passthrough_text(text);
    let attributes = match inline_macro {
        InlineMacro::Link(link) => &mut link.attributes,
        InlineMacro::Url(url) => &mut url.attributes,
        InlineMacro::Mailto(mailto) => &mut mailto.attributes,
        InlineMacro::Icon(icon) => &mut icon.attributes,
        InlineMacro::CrossReference(xref) => {
            if let Some(role) = xref.role
                && let Some(restored) = restore(role)
            {
                xref.role = Some(restored);
            }
            return;
        }
        InlineMacro::Image(image) => {
            if image
                .metadata
                .roles
                .iter()
                .any(|role| contains_passthrough_placeholders(role, ctx.processed))
            {
                image.metadata.roles = take(&mut image.metadata.roles)
                    .into_iter()
                    .flat_map(|role| restore(role).unwrap_or(role).split_whitespace())
                    .collect();
            }
            if image
                .metadata
                .options
                .iter()
                .any(|option| contains_passthrough_placeholders(option, ctx.processed))
            {
                image.metadata.options = take(&mut image.metadata.options)
                    .into_iter()
                    .flat_map(|option| restore(option).unwrap_or(option).split(','))
                    .map(str::trim)
                    .filter(|option| !option.is_empty())
                    .collect();
            }
            if let Some(id) = &mut image.metadata.id
                && let Some(restored) = restore(id.id)
            {
                id.id = restored;
            }
            for node in image.title.inlines_mut() {
                if let InlineNode::PlainText(plain) = node
                    && let Some(restored) = restore(plain.content)
                {
                    plain.content = restored;
                }
            }
            &mut image.metadata.attributes
        }
        InlineMacro::Footnote(_)
        | InlineMacro::Keyboard(_)
        | InlineMacro::Button(_)
        | InlineMacro::Menu(_)
        | InlineMacro::Autolink(_)
        | InlineMacro::Pass(_)
        | InlineMacro::Stem(_)
        | InlineMacro::IndexTerm(_) => return,
    };

    // Restore after attribute parsing so protected commas and brackets stay in the value.
    for value in attributes.values_mut() {
        if let AttributeValue::String(text) = value
            && let Some(restored) = restore(text)
        {
            *text = Cow::Borrowed(restored);
        }
    }
}

fn restore_macro_passthroughs<'a>(
    inline_macro: &mut InlineMacro<'a>,
    ctx: &LocationMappingContext<'_, 'a>,
) -> Result<(), Error> {
    if ctx.processed.passthroughs.is_empty() {
        return Ok(());
    }
    restore_macro_attributes(inline_macro, ctx);

    let target = match inline_macro {
        InlineMacro::Link(link) => Some(&mut link.target),
        InlineMacro::Url(url) => Some(&mut url.target),
        InlineMacro::Mailto(mailto) => Some(&mut mailto.target),
        InlineMacro::Autolink(autolink) => Some(&mut autolink.url),
        InlineMacro::Icon(icon) => Some(&mut icon.target),
        InlineMacro::Image(image) => Some(&mut image.source),
        InlineMacro::Footnote(_)
        | InlineMacro::Keyboard(_)
        | InlineMacro::Button(_)
        | InlineMacro::Menu(_)
        | InlineMacro::CrossReference(_)
        | InlineMacro::Pass(_)
        | InlineMacro::Stem(_)
        | InlineMacro::IndexTerm(_) => None,
    };
    if let Some(target) = target
        && let Some(restored) = ctx.restore_passthrough_text(&target.to_string())
    {
        *target = Source::from_str_borrowed(restored)?;
    }

    let text = match inline_macro {
        InlineMacro::Stem(stem) => Some(&mut stem.content),
        InlineMacro::Button(button) => Some(&mut button.label),
        InlineMacro::Menu(menu) => {
            for item in &mut menu.items {
                if let Some(restored) = ctx.restore_passthrough_text(item) {
                    *item = restored;
                }
            }
            Some(&mut menu.target)
        }
        InlineMacro::Footnote(_)
        | InlineMacro::Icon(_)
        | InlineMacro::Image(_)
        | InlineMacro::Keyboard(_)
        | InlineMacro::Url(_)
        | InlineMacro::Link(_)
        | InlineMacro::Mailto(_)
        | InlineMacro::Autolink(_)
        | InlineMacro::CrossReference(_)
        | InlineMacro::Pass(_)
        | InlineMacro::IndexTerm(_) => None,
    };
    if let Some(text) = text
        && let Some(restored) = ctx.restore_passthrough_text(text)
    {
        *text = restored;
    }
    Ok(())
}

fn map_inline_macro<'a>(
    inline_macro: &mut InlineMacro<'a>,
    ctx: &LocationMappingContext<'_, 'a>,
    form: Option<&Form>,
) -> Result<(), Error> {
    restore_macro_passthroughs(inline_macro, ctx)?;
    let LocationMappingContext {
        state,
        processed,
        base_location: location,
    } = *ctx;
    match inline_macro {
        InlineMacro::Footnote(footnote) => {
            footnote.location = ctx.map_location(&footnote.location, form)?;
            footnote.content =
                map_inline_locations(state, processed, take(&mut footnote.content), location)?;
            // The footnote tracker captured this footnote during parsing in
            // preprocessed-local coordinates; propagate the now document-absolute
            // location/content to its `Document.footnotes` entry so the post-parse
            // remap can map it to origin like the in-tree copy.
            state.footnote_tracker.borrow_mut().finalize(footnote);
        }
        InlineMacro::Url(url) => {
            url.location = ctx.map_location(&url.location, form)?;
            url.text = map_inline_locations(state, processed, take(&mut url.text), location)?;
        }
        InlineMacro::Mailto(mailto) => {
            mailto.location = ctx.map_location(&mailto.location, form)?;
            mailto.text = map_inline_locations(state, processed, take(&mut mailto.text), location)?;
        }
        InlineMacro::Link(link) => {
            link.location = ctx.map_location(&link.location, form)?;
            link.text = map_inline_locations(state, processed, take(&mut link.text), location)?;
        }
        InlineMacro::Icon(icon) => icon.location = ctx.map_location(&icon.location, form)?,
        InlineMacro::Button(button) => {
            button.location = ctx.map_location(&button.location, form)?;
        }
        InlineMacro::Image(image) => image.location = ctx.map_location(&image.location, form)?,
        InlineMacro::Menu(menu) => menu.location = ctx.map_location(&menu.location, form)?,
        InlineMacro::Keyboard(keyboard) => {
            keyboard.location = ctx.map_location(&keyboard.location, form)?;
        }
        InlineMacro::CrossReference(xref) => {
            xref.location = ctx.map_location(&xref.location, form)?;
            xref.text = map_inline_locations(state, processed, take(&mut xref.text), location)?;
            if !processed.passthroughs.is_empty() {
                let restored = replace_passthrough_placeholders(xref.target, processed);
                if restored != xref.target {
                    xref.target = state.intern_str(&restored);
                    xref.target_is_local = crate::CrossReference::is_local_target(xref.target);
                    xref.resolve_natural_target = false;
                }
            }
        }
        InlineMacro::Autolink(autolink) => {
            autolink.location = ctx.map_location(&autolink.location, form)?;
        }
        InlineMacro::Stem(stem) => stem.location = ctx.map_location(&stem.location, form)?,
        InlineMacro::Pass(pass) => pass.location = ctx.map_location(&pass.location, form)?,
        InlineMacro::IndexTerm(index_term) => {
            index_term.location = ctx.map_location(&index_term.location, form)?;
            match &mut index_term.kind {
                IndexTermKind::Flow(term) => {
                    *term = map_inline_locations(state, processed, take(term), location)?;
                }
                IndexTermKind::Concealed {
                    term,
                    secondary,
                    tertiary,
                } => {
                    *term = map_inline_locations(state, processed, take(term), location)?;
                    if let Some(secondary) = secondary {
                        *secondary =
                            map_inline_locations(state, processed, take(secondary), location)?;
                    }
                    if let Some(tertiary) = tertiary {
                        *tertiary =
                            map_inline_locations(state, processed, take(tertiary), location)?;
                    }
                }
            }
            if let Some(relationship) = &mut index_term.relationship {
                match relationship {
                    IndexTermRelationship::See { target } => {
                        *target = map_inline_locations(state, processed, take(target), location)?;
                    }
                    IndexTermRelationship::SeeAlso { targets } => {
                        for target in targets {
                            *target =
                                map_inline_locations(state, processed, take(target), location)?;
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn map_plain_text_inline_locations<'a>(
    plain: &mut Plain<'a>,
    ctx: &LocationMappingContext<'_, 'a>,
    form: Option<&Form>,
) -> Result<Option<Vec<InlineNode<'a>>>, Error> {
    plain.location = ctx.map_location(&plain.location, form)?;
    if contains_passthrough_placeholders(plain.content, ctx.processed) {
        // Passthrough locations use the mapped document coordinates as their base.
        return Ok(Some(
            super::passthrough_processing::process_passthrough_placeholders(
                plain.content,
                ctx.processed,
                ctx.state,
                &plain.location,
            ),
        ));
    }
    if plain.content.chars().count() == 1 {
        plain.location.end.column = plain.location.start.column;
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use crate::{Block, InlineMacro, InlineNode, Options, parse};

    #[test]
    fn inline_attribute_passthroughs_restore_image_metadata()
    -> Result<(), Box<dyn std::error::Error>> {
        let parsed = parse(
            include_str!("../../fixtures/tests/inline_attribute_passthroughs.adoc"),
            &Options::default(),
        )?;
        let image = parsed
            .document()
            .blocks
            .iter()
            .filter_map(|block| {
                if let Block::Paragraph(paragraph) = block {
                    Some(&paragraph.content)
                } else {
                    None
                }
            })
            .flatten()
            .find_map(|node| {
                if let InlineNode::Macro(InlineMacro::Image(image)) = node {
                    Some(image)
                } else {
                    None
                }
            })
            .ok_or("expected an inline image")?;

        // Inline image metadata is omitted from the JSON fixture.
        assert_eq!(image.metadata.roles, ["hot", "cool"]);
        assert_eq!(image.metadata.options, ["foo", "bar"]);
        assert_eq!(
            image.metadata.id.as_ref().map(|anchor| anchor.id),
            Some("image-id")
        );
        Ok(())
    }

    fn check_monotonic(input: &str) -> Result<(), String> {
        let parsed = parse(input, &Options::default()).map_err(|e| format!("parse failed: {e}"))?;
        let first_block = parsed
            .document()
            .blocks
            .first()
            .ok_or("expected at least one block")?;
        let Block::Paragraph(para) = first_block else {
            return Err(format!("Expected paragraph, got {first_block:?}"));
        };

        assert!(para.content.len() >= 2, "Expected at least 2 inline nodes");

        let first = para
            .content
            .first()
            .ok_or("missing first inline")?
            .location();
        let second = para
            .content
            .get(1)
            .ok_or("missing second inline")?
            .location();
        assert!(
            second.absolute_start >= first.absolute_end,
            "Non-monotonic: first ends at {}, second starts at {}",
            first.absolute_end,
            second.absolute_start,
        );
        Ok(())
    }

    #[test]
    fn subscript_after_single_char_has_monotonic_positions() -> Result<(), String> {
        check_monotonic("?~sub~")
    }

    #[test]
    fn superscript_after_single_char_has_monotonic_positions() -> Result<(), String> {
        check_monotonic("?^sup^")
    }

    #[test]
    fn short_text_before_subscript_no_constrained_heuristic() -> Result<(), String> {
        check_monotonic("a~sub~")
    }

    #[test]
    fn longer_text_before_subscript_unaffected() -> Result<(), String> {
        check_monotonic("ab~sub~")
    }

    #[test]
    fn subscript_before_single_char_has_monotonic_positions() -> Result<(), String> {
        check_monotonic("~sub~?")
    }

    #[test]
    fn superscript_before_single_char_has_monotonic_positions() -> Result<(), String> {
        check_monotonic("^sup^?")
    }

    #[test]
    fn single_character_after_passthrough_keeps_its_source_range()
    -> Result<(), Box<dyn std::error::Error>> {
        for prefix in ["", "First paragraph.\n\n"] {
            for character in ["a", "é", "😀"] {
                let source = format!("{prefix}+0+\t \\#{character}");
                let parsed = parse(&source, &Options::default())?;
                let Some(Block::Paragraph(paragraph)) = parsed.document().blocks.last() else {
                    return Err("expected a paragraph".into());
                };
                let Some(InlineNode::PlainText(plain)) = paragraph.content.last() else {
                    return Err("expected trailing text".into());
                };
                assert_eq!(plain.content, character);
                let location = &plain.location;
                assert_eq!(location.absolute_start, source.len() - character.len());
                assert_eq!(location.absolute_end, source.len());
                assert_eq!(
                    source.get(location.absolute_start..location.absolute_end),
                    Some(character)
                );
                assert_eq!(location.start.line, if prefix.is_empty() { 1 } else { 3 });
                assert_eq!(location.start.column, 8);
                assert_eq!(location.end, location.start);
            }
        }
        Ok(())
    }
}
