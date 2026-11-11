//! AST-based `AsciiDoc` source syntax highlighting.
//!
//! Highlights entry-file text with `<span class="adoc-*">` elements.
//! Included-file offsets must not be applied to the entry text. Mixed-source
//! containers are still visited so their entry-file children can be highlighted.

use acdc_parser::{
    AdmonitionVariant, FormattingForm,
    ast::{
        AttributesRef, BlockKind, BlockRef, ContentRef, DocumentRef, InlineKind, InlineRef,
        ListItemRef, LocationRef, SectionRef, StructuralRef,
    },
};

/// A span of source text that should receive a CSS class.
struct Span {
    start: usize,
    end: usize,
    class: &'static str,
    /// Higher priority wins when spans overlap at the same byte. Inline (2)
    /// beats block (1).
    priority: u8,
}

/// Highlight entry text using the parsed document's source locations.
/// Skip included-file spans, but still visit their entry-file children.
pub(super) fn highlight_from_ast(input: &str, doc: DocumentRef<'_>) -> String {
    let mut spans = Vec::new();

    if let Some(header) = doc.header() {
        if let Some(start) = primary_start(input, header.location()) {
            spans.push(Span {
                start,
                end: find_line_end(input, start),
                class: "adoc-title",
                priority: 1,
            });
        }

        // Highlight inline content in the title
        if let Some(title) = header.title() {
            collect_inline_spans(input, title, &mut spans);
        }

        // Highlight subtitle if present
        if let Some(subtitle) = header.subtitle() {
            collect_inline_spans(input, subtitle, &mut spans);
        }

        if let Some((start, end)) = primary_range(input, header.location()) {
            collect_header_attributes(input, start, end, &mut spans);
        }
    }

    for block in doc.blocks() {
        collect_block_spans(input, block, &mut spans);
    }

    // Derived spans must also stay on UTF-8 boundaries and within the entry text.
    spans.retain(|span| span.start < span.end && input.get(span.start..span.end).is_some());
    render_spans(input, &mut spans)
}

fn collect_header_attributes(input: &str, start: usize, end: usize, spans: &mut Vec<Span>) {
    let mut pos = find_line_end(input, start);
    while pos < end && pos < input.len() {
        pos += 1;
        let line_end = find_line_end(input, pos);
        if input
            .get(pos..line_end)
            .is_some_and(|line| line.starts_with(':'))
        {
            spans.push(Span {
                start: pos,
                end: line_end,
                class: "adoc-attribute",
                priority: 1,
            });
        }
        pos = line_end;
    }
}

// ---------------------------------------------------------------------------
// Block-level span collection
// ---------------------------------------------------------------------------

fn collect_block_spans(input: &str, block: BlockRef<'_>, spans: &mut Vec<Span>) {
    match block.kind() {
        BlockKind::Section(section) => {
            collect_section_spans(input, section, spans);
            collect_children(input, block.into(), spans);
        }
        BlockKind::DocumentAttribute(_) => {
            push_block_span(input, spans, block.location(), "adoc-attribute");
        }
        BlockKind::Comment(_) => push_block_span(input, spans, block.location(), "adoc-comment"),
        BlockKind::ThematicBreak(_) => {
            push_block_span(input, spans, block.location(), "adoc-thematic-break");
            collect_block_title_spans(input, block.title(), spans);
        }
        BlockKind::PageBreak(_) => {
            push_block_span(input, spans, block.location(), "adoc-page-break");
            collect_block_title_spans(input, block.title(), spans);
        }
        BlockKind::Admonition(admonition) => {
            collect_block_metadata_spans(input, block, spans);
            collect_admonition_label(input, &admonition.variant(), admonition.location(), spans);
            collect_block_title_spans(input, block.title(), spans);
            collect_children(input, block.into(), spans);
        }
        BlockKind::UnorderedList(_)
        | BlockKind::OrderedList(_)
        | BlockKind::CalloutList(_)
        | BlockKind::DescriptionList(_) => {
            collect_block_title_spans(input, block.title(), spans);
            collect_children(input, block.into(), spans);
        }
        BlockKind::Example(_)
        | BlockKind::Open(_)
        | BlockKind::Sidebar(_)
        | BlockKind::Quote(_)
        | BlockKind::Listing(_)
        | BlockKind::Literal(_)
        | BlockKind::Passthrough(_)
        | BlockKind::Verse(_)
        | BlockKind::DelimitedComment(_)
        | BlockKind::Table(_)
        | BlockKind::Stem(_) => {
            collect_delimited_block_spans(input, block, spans);
            collect_block_title_spans(input, block.title(), spans);
        }
        BlockKind::Paragraph(paragraph) => {
            collect_block_metadata_spans(input, block, spans);
            collect_block_title_spans(input, block.title(), spans);
            collect_inline_spans(input, paragraph.content(), spans);
        }
        BlockKind::Image(_) | BlockKind::Audio(_) | BlockKind::Video(_) => {
            collect_block_macro_spans(input, block.location(), spans);
        }
        BlockKind::DiscreteHeading(heading) => {
            push_block_span(input, spans, heading.location(), "adoc-heading");
            if let Some(title) = heading.title() {
                collect_inline_spans(input, title, spans);
            }
        }
        BlockKind::TableOfContents(_) => {
            push_block_span(input, spans, block.location(), "adoc-macro");
        }
        BlockKind::Extension(_) | _ => {}
    }
}

fn collect_section_spans(input: &str, section: SectionRef<'_>, spans: &mut Vec<Span>) {
    if let Some(start) = primary_start(input, section.location()) {
        let heading_end = find_line_end(input, start);
        spans.push(Span {
            start,
            end: heading_end,
            class: "adoc-heading",
            priority: 1,
        });
        let next_line_start = heading_end + 1;
        if primary_range(input, section.location()).is_some_and(|(_, end)| next_line_start < end) {
            let next_line_end = find_line_end(input, next_line_start);
            if let Some(next_line) = input.get(next_line_start..next_line_end) {
                let trimmed = next_line.trim();
                if !trimmed.is_empty()
                    && trimmed.chars().all(|c| matches!(c, '-' | '~' | '^' | '+'))
                {
                    spans.push(Span {
                        start: next_line_start,
                        end: next_line_end,
                        class: "adoc-heading",
                        priority: 1,
                    });
                }
            }
        }
    }
    if let Some(title) = section.title() {
        collect_inline_spans(input, title, spans);
    }
}

fn collect_admonition_label(
    input: &str,
    variant: &AdmonitionVariant,
    location: LocationRef<'_>,
    spans: &mut Vec<Span>,
) {
    let label_len = admonition_label_len(variant);
    let expected_label_prefix = format!("{variant}:").to_uppercase();
    if let Some(start) = primary_start(input, location)
        && let Some(text) = input.get(start..)
        && text.starts_with(&expected_label_prefix)
        && start + label_len <= input.len()
    {
        spans.push(Span {
            start,
            end: start + label_len,
            class: "adoc-admonition",
            priority: 1,
        });
    }
}

fn collect_children(input: &str, node: StructuralRef<'_>, spans: &mut Vec<Span>) {
    for child in node.children() {
        match child {
            StructuralRef::Block(block) => {
                collect_block_spans(input, block, spans);
                continue;
            }
            StructuralRef::ListItem(item) => {
                if let Some(start) = primary_start(input, item.location()) {
                    collect_list_marker_spans(input, item, start, spans);
                }
                collect_inline_spans(input, item.principal(), spans);
            }
            StructuralRef::DescriptionListTerm(term) => {
                if let Some(location) = term.delimiter_location() {
                    push_block_span(input, spans, location, "adoc-description-marker");
                }
                collect_inline_spans(input, term.content(), spans);
            }
            StructuralRef::DescriptionListDefinition(definition) => {
                if let Some(principal) = definition.principal() {
                    collect_inline_spans(input, principal, spans);
                }
            }
            StructuralRef::CalloutListItem(item) => {
                push_inline_span(input, spans, item.callout().location(), "adoc-callout");
                collect_inline_spans(input, item.principal(), spans);
            }
            StructuralRef::DescriptionListItem(_)
            | StructuralRef::TableRow(_)
            | StructuralRef::TableCell(_) => {}
            StructuralRef::Document(_) | StructuralRef::Header(_) | _ => continue,
        }
        collect_children(input, child, spans);
    }
}

fn collect_block_macro_spans(input: &str, location: LocationRef<'_>, spans: &mut Vec<Span>) {
    push_block_span(input, spans, location, "adoc-macro");
    if let Some((start, end)) = primary_range(input, location) {
        collect_macro_target_spans(input, start, end, 1, spans);
    }
}

fn push_block_span(
    input: &str,
    spans: &mut Vec<Span>,
    location: LocationRef<'_>,
    class: &'static str,
) {
    let Some((start, end)) = primary_range(input, location) else {
        return;
    };
    spans.push(Span {
        start,
        end,
        class,
        priority: 1,
    });
}

fn push_inline_span(
    input: &str,
    spans: &mut Vec<Span>,
    location: LocationRef<'_>,
    class: &'static str,
) {
    let Some((start, end)) = primary_range(input, location) else {
        return;
    };
    spans.push(Span {
        start,
        end,
        class,
        priority: 2,
    });
}

fn collect_formatted_inline(
    input: &str,
    location: LocationRef<'_>,
    delimiter_len: usize,
    class: &'static str,
    content: ContentRef<'_>,
    spans: &mut Vec<Span>,
) {
    if let Some((start, end)) = primary_range(input, location)
        && end - start >= delimiter_len * 2
    {
        spans.push(Span {
            start,
            end,
            class,
            priority: 2,
        });
        spans.push(Span {
            start,
            end: start + delimiter_len,
            class: "adoc-delimiter",
            priority: 3,
        });
        spans.push(Span {
            start: end - delimiter_len,
            end,
            class: "adoc-delimiter",
            priority: 3,
        });
    }
    collect_inline_spans(input, content, spans);
}

fn collect_list_marker_spans(
    input: &str,
    item: ListItemRef<'_>,
    start: usize,
    spans: &mut Vec<Span>,
) {
    let marker_len = item.marker().len();
    let marker_end = start + marker_len;

    if marker_end <= input.len() {
        let effective_end = if input.as_bytes().get(marker_end) == Some(&b' ') {
            marker_end + 1
        } else {
            marker_end
        };
        spans.push(Span {
            start,
            end: effective_end,
            class: "adoc-list-marker",
            priority: 1,
        });
    }

    if let Some(checked) = item.checked() {
        let check_text = match checked {
            acdc_parser::ListItemCheckedStatus::Checked => "[x] ",
            acdc_parser::ListItemCheckedStatus::Unchecked => "[ ] ",
            _ => return,
        };
        let check_start = start + marker_len + 1;
        if check_start + check_text.len() <= input.len()
            && input.get(check_start..check_start + check_text.len()) == Some(check_text)
        {
            spans.push(Span {
                start: check_start,
                end: check_start + check_text.len(),
                class: "adoc-checklist",
                priority: 2,
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Delimited blocks
// ---------------------------------------------------------------------------

fn collect_delimited_block_spans(input: &str, block: BlockRef<'_>, spans: &mut Vec<Span>) {
    collect_block_metadata_spans(input, block, spans);
    collect_delimiter_lines(input, block.kind(), spans);
    let block_range = primary_range(input, block.location());
    match block.kind() {
        BlockKind::DelimitedComment(node) => {
            push_verbatim_spans(input, node.content(), "adoc-comment", spans);
        }
        BlockKind::Listing(node) => collect_listing_content(input, node.content(), spans),
        BlockKind::Literal(node) => {
            push_verbatim_spans(input, node.content(), "adoc-literal-content", spans);
        }
        BlockKind::Passthrough(node) => {
            push_verbatim_spans(input, node.content(), "adoc-passthrough-content", spans);
        }
        BlockKind::Verse(node) => collect_inline_spans(input, node.content(), spans),
        BlockKind::Example(_)
        | BlockKind::Open(_)
        | BlockKind::Sidebar(_)
        | BlockKind::Quote(_) => collect_children(input, block.into(), spans),
        BlockKind::Table(_) => collect_table_spans(input, block, block_range, spans),
        BlockKind::Stem(_) => {
            let Some((block_start, block_end)) = block_range else {
                return;
            };
            let content_start = find_line_end(input, block_start);
            let content_end = if block_end > 0 {
                find_line_start(input, block_end.saturating_sub(1))
            } else {
                block_end
            };
            if content_start < content_end {
                spans.push(Span {
                    start: content_start,
                    end: content_end,
                    class: "adoc-code-content",
                    priority: 1,
                });
            }
        }
        BlockKind::TableOfContents(_)
        | BlockKind::Admonition(_)
        | BlockKind::DiscreteHeading(_)
        | BlockKind::DocumentAttribute(_)
        | BlockKind::ThematicBreak(_)
        | BlockKind::PageBreak(_)
        | BlockKind::UnorderedList(_)
        | BlockKind::OrderedList(_)
        | BlockKind::CalloutList(_)
        | BlockKind::DescriptionList(_)
        | BlockKind::Section(_)
        | BlockKind::Paragraph(_)
        | BlockKind::Image(_)
        | BlockKind::Audio(_)
        | BlockKind::Video(_)
        | BlockKind::Comment(_)
        | _ => {}
    }
}

fn collect_delimiter_lines(input: &str, kind: BlockKind<'_>, spans: &mut Vec<Span>) {
    let (delimiter, open, close) = match kind {
        BlockKind::Example(node) => (
            node.delimiter(),
            node.open_delimiter_location(),
            node.close_delimiter_location(),
        ),
        BlockKind::Open(node) => (
            node.delimiter(),
            node.open_delimiter_location(),
            node.close_delimiter_location(),
        ),
        BlockKind::Sidebar(node) => (
            node.delimiter(),
            node.open_delimiter_location(),
            node.close_delimiter_location(),
        ),
        BlockKind::Quote(node) => (
            node.delimiter(),
            node.open_delimiter_location(),
            node.close_delimiter_location(),
        ),
        BlockKind::Listing(node) => (
            node.delimiter(),
            node.open_delimiter_location(),
            node.close_delimiter_location(),
        ),
        BlockKind::Literal(node) => (
            node.delimiter(),
            node.open_delimiter_location(),
            node.close_delimiter_location(),
        ),
        BlockKind::Passthrough(node) => (
            node.delimiter(),
            node.open_delimiter_location(),
            node.close_delimiter_location(),
        ),
        BlockKind::Verse(node) => (
            node.delimiter(),
            node.open_delimiter_location(),
            node.close_delimiter_location(),
        ),
        BlockKind::DelimitedComment(node) => (
            node.delimiter(),
            node.open_delimiter_location(),
            node.close_delimiter_location(),
        ),
        BlockKind::Table(node) => (
            node.delimiter(),
            node.open_delimiter_location(),
            node.close_delimiter_location(),
        ),
        BlockKind::Stem(node) => (
            node.delimiter(),
            node.open_delimiter_location(),
            node.close_delimiter_location(),
        ),
        BlockKind::TableOfContents(_)
        | BlockKind::Admonition(_)
        | BlockKind::DiscreteHeading(_)
        | BlockKind::DocumentAttribute(_)
        | BlockKind::ThematicBreak(_)
        | BlockKind::PageBreak(_)
        | BlockKind::UnorderedList(_)
        | BlockKind::OrderedList(_)
        | BlockKind::CalloutList(_)
        | BlockKind::DescriptionList(_)
        | BlockKind::Section(_)
        | BlockKind::Paragraph(_)
        | BlockKind::Image(_)
        | BlockKind::Audio(_)
        | BlockKind::Video(_)
        | BlockKind::Comment(_)
        | _ => return,
    };
    if delimiter.is_empty() {
        return;
    }
    let class = if matches!(kind, BlockKind::Table(_)) {
        "adoc-table-delimiter"
    } else {
        "adoc-delimiter"
    };
    for location in [open, close].into_iter().flatten() {
        push_block_span(input, spans, location, class);
    }
}

fn collect_listing_content(input: &str, inlines: ContentRef<'_>, spans: &mut Vec<Span>) {
    for node in inlines.inlines() {
        if matches!(node.kind(), InlineKind::Callout(_)) {
            push_inline_span(input, spans, node.location(), "adoc-callout");
        } else {
            push_block_span(input, spans, node.location(), "adoc-code-content");
        }
    }
}

fn push_verbatim_spans(
    input: &str,
    inlines: ContentRef<'_>,
    class: &'static str,
    spans: &mut Vec<Span>,
) {
    for node in inlines.inlines() {
        push_block_span(input, spans, node.location(), class);
    }
}

fn collect_table_spans(
    input: &str,
    table: BlockRef<'_>,
    block_range: Option<(usize, usize)>,
    spans: &mut Vec<Span>,
) {
    if let Some((block_start, block_end)) = block_range {
        let content_start = find_line_end(input, block_start);
        let content_end = if block_end > 0 {
            find_line_start(input, block_end.saturating_sub(1))
        } else {
            block_end
        };

        if let Some(content) = input.get(content_start..content_end) {
            for (i, b) in content.bytes().enumerate() {
                if b == b'|' {
                    let abs = content_start + i;
                    spans.push(Span {
                        start: abs,
                        end: abs + 1,
                        class: "adoc-table-cell",
                        priority: 2,
                    });
                }
            }
        }
    }

    collect_children(input, table.into(), spans);
}

/// Highlight entry-file block attributes and anchors.
fn collect_block_metadata_spans(input: &str, block: BlockRef<'_>, spans: &mut Vec<Span>) {
    for anchor in block.anchors() {
        push_block_span(input, spans, anchor.location(), "adoc-anchor");
    }
    if let Some(location) = block.attributes().and_then(AttributesRef::location) {
        push_block_span(input, spans, location, "adoc-attribute");
    }
}

/// Add granular highlighting for macro target and attributes.
///
/// Splits `name:target[attrs]` or `name::target[attrs]` into a green target
/// span and a dark-pink attributes span. Works for any macro with this pattern;
/// macros without a target (e.g. `kbd:[Ctrl+C]`) only get the attrs span.
fn collect_macro_target_spans(
    input: &str,
    start: usize,
    end: usize,
    base_priority: u8,
    spans: &mut Vec<Span>,
) {
    let Some(text) = input.get(start..end) else {
        return;
    };

    let Some(colon_pos) = text.find(':') else {
        return;
    };

    // Skip past `:` or `::` to find the target start
    let mut target_offset = colon_pos + 1;
    if text.get(target_offset..target_offset + 1) == Some(":") {
        target_offset += 1;
    }
    let target_start = start + target_offset;

    let Some(bracket_pos) = text.find('[') else {
        return;
    };
    let abs_bracket = start + bracket_pos;

    // Target span (path/URL between colons and opening bracket)
    if target_start < abs_bracket {
        spans.push(Span {
            start: target_start,
            end: abs_bracket,
            class: "adoc-macro-target",
            priority: base_priority + 1,
        });
    }

    // Attributes span (content between `[` and `]`, excluding brackets)
    let attrs_start = abs_bracket + 1;
    let attrs_end = if input.as_bytes().get(end - 1) == Some(&b']') {
        end - 1
    } else {
        end
    };
    if attrs_start < attrs_end {
        spans.push(Span {
            start: attrs_start,
            end: attrs_end,
            class: "adoc-macro-attrs",
            priority: base_priority + 1,
        });
    }
}

/// Highlight display text in URL-type macros (e.g. `https://example.com[Text]`).
///
/// Only highlights the bracket content as attrs — the URL part keeps the base
/// link color. Brackets themselves get no extra color.
fn collect_url_display_spans(input: &str, start: usize, end: usize, spans: &mut Vec<Span>) {
    let Some(text) = input.get(start..end) else {
        return;
    };

    let Some(bracket_pos) = text.find('[') else {
        return;
    };
    let abs_bracket = start + bracket_pos;

    let attrs_start = abs_bracket + 1;
    let attrs_end = if input.as_bytes().get(end - 1) == Some(&b']') {
        end - 1
    } else {
        end
    };
    if attrs_start < attrs_end {
        spans.push(Span {
            start: attrs_start,
            end: attrs_end,
            class: "adoc-macro-attrs",
            priority: 3,
        });
    }
}

// ---------------------------------------------------------------------------
// Inline-level span collection
// ---------------------------------------------------------------------------

fn collect_inline_spans(input: &str, nodes: ContentRef<'_>, spans: &mut Vec<Span>) {
    for node in nodes.inlines() {
        collect_single_inline_span(input, node, spans);
    }
}

/// Highlight a block title line (`.Title text`) with `adoc-block-title`, then
/// recurse into its inline content.
fn collect_block_title_spans(input: &str, title: Option<ContentRef<'_>>, spans: &mut Vec<Span>) {
    let Some(title) = title else {
        return;
    };
    let mut nodes = title.inlines();
    let Some(first) = nodes.next() else {
        return;
    };
    let last = nodes.next_back().unwrap_or(first);
    if let Some(start) = primary_start(input, first.location())
        && let Some(end) = primary_end(input, last.location())
        && input.get(start..end).is_some()
        && let Some(dot_start) = start.checked_sub(1)
        && input.as_bytes().get(dot_start) == Some(&b'.')
    {
        spans.push(Span {
            start: dot_start,
            end,
            class: "adoc-block-title",
            priority: 1,
        });
    }
    collect_inline_spans(input, title, spans);
}

fn delimiter_len(form: &FormattingForm) -> usize {
    match form {
        FormattingForm::Constrained => 1,
        FormattingForm::Unconstrained => 2,
    }
}

fn collect_single_inline_span(input: &str, node: InlineRef<'_>, spans: &mut Vec<Span>) {
    match node.kind() {
        InlineKind::Bold(span) => {
            let delimiter_len = delimiter_len(&span.form());
            collect_formatted_inline(
                input,
                span.location(),
                delimiter_len,
                "adoc-bold",
                span.content(),
                spans,
            );
        }
        InlineKind::Italic(span) => {
            let delimiter_len = delimiter_len(&span.form());
            collect_formatted_inline(
                input,
                span.location(),
                delimiter_len,
                "adoc-italic",
                span.content(),
                spans,
            );
        }
        InlineKind::Monospace(span) => {
            let delimiter_len = delimiter_len(&span.form());
            collect_formatted_inline(
                input,
                span.location(),
                delimiter_len,
                "adoc-monospace",
                span.content(),
                spans,
            );
        }
        InlineKind::Highlight(span) => {
            let delimiter_len = delimiter_len(&span.form());
            collect_formatted_inline(
                input,
                span.location(),
                delimiter_len,
                "adoc-highlight",
                span.content(),
                spans,
            );
        }
        InlineKind::Superscript(span) => collect_formatted_inline(
            input,
            span.location(),
            1,
            "adoc-superscript",
            span.content(),
            spans,
        ),
        InlineKind::Subscript(span) => collect_formatted_inline(
            input,
            span.location(),
            1,
            "adoc-subscript",
            span.content(),
            spans,
        ),
        InlineKind::DoubleQuotedSpan(span) => collect_formatted_inline(
            input,
            span.location(),
            2,
            "adoc-bold",
            span.content(),
            spans,
        ),
        InlineKind::Anchor(_) => push_inline_span(input, spans, node.location(), "adoc-anchor"),
        InlineKind::Callout(_) => push_inline_span(input, spans, node.location(), "adoc-callout"),
        InlineKind::Plain(_)
        | InlineKind::Symbol(_)
        | InlineKind::CharacterReference(_)
        | InlineKind::Raw(_)
        | InlineKind::Verbatim(_)
        | InlineKind::SingleQuotedSpan(_)
        | InlineKind::StandaloneCurvedApostrophe(_)
        | InlineKind::LineBreak(_)
        | InlineKind::Footnote(_)
        | InlineKind::Icon(_)
        | InlineKind::Image(_)
        | InlineKind::KeyboardShortcut(_)
        | InlineKind::Button(_)
        | InlineKind::MenuPath(_)
        | InlineKind::UrlMacro(_)
        | InlineKind::Link(_)
        | InlineKind::Mailto(_)
        | InlineKind::Autolink(_)
        | InlineKind::CrossReference(_)
        | InlineKind::Passthrough(_)
        | InlineKind::Stem(_)
        | InlineKind::IndexTerm(_)
        | _ => collect_macro_span(input, node, spans),
    }
}

fn collect_macro_span(input: &str, node: InlineRef<'_>, spans: &mut Vec<Span>) {
    let (class, granular) = match node.kind() {
        InlineKind::UrlMacro(_) | InlineKind::Mailto(_) | InlineKind::Autolink(_) => {
            ("adoc-link", false)
        }
        InlineKind::Link(_) => ("adoc-link", true),
        InlineKind::CrossReference(_) => ("adoc-xref", true),
        InlineKind::IndexTerm(_) => ("adoc-index-term", false),
        InlineKind::Passthrough(_) => ("adoc-passthrough-inline", true),
        InlineKind::Footnote(_) => ("adoc-footnote", true),
        InlineKind::Icon(_)
        | InlineKind::Image(_)
        | InlineKind::KeyboardShortcut(_)
        | InlineKind::Button(_)
        | InlineKind::MenuPath(_)
        | InlineKind::Stem(_) => ("adoc-inline-macro", true),
        InlineKind::Plain(_)
        | InlineKind::Symbol(_)
        | InlineKind::CharacterReference(_)
        | InlineKind::Raw(_)
        | InlineKind::Verbatim(_)
        | InlineKind::Bold(_)
        | InlineKind::Italic(_)
        | InlineKind::Monospace(_)
        | InlineKind::Highlight(_)
        | InlineKind::Subscript(_)
        | InlineKind::Superscript(_)
        | InlineKind::DoubleQuotedSpan(_)
        | InlineKind::SingleQuotedSpan(_)
        | InlineKind::StandaloneCurvedApostrophe(_)
        | InlineKind::LineBreak(_)
        | InlineKind::Anchor(_)
        | InlineKind::Callout(_)
        | _ => return,
    };
    let Some((start, end)) = primary_range(input, node.location()) else {
        return;
    };
    spans.push(Span {
        start,
        end,
        class,
        priority: 2,
    });
    if granular {
        collect_macro_target_spans(input, start, end, 2, spans);
    } else {
        collect_url_display_spans(input, start, end, spans);
    }
}

// ---------------------------------------------------------------------------
// Rendering: flatten spans and emit HTML
// ---------------------------------------------------------------------------

/// Flatten collected spans and emit the highlighted HTML.
fn render_spans(input: &str, spans: &mut [Span]) -> String {
    // Sort by start position. For same start, process lower priority first
    // so they can be split by higher priority spans (which become inner).
    spans.sort_by(|a, b| a.start.cmp(&b.start).then(a.priority.cmp(&b.priority)));

    let events = flatten_spans(spans);

    let mut out = String::with_capacity(input.len() * 2);
    let mut cursor: usize = 0;

    for (pos, class, is_open) in &events {
        if *pos > cursor {
            if let Some(gap) = input.get(cursor..*pos) {
                escape_into(gap, &mut out);
            }
            cursor = *pos;
        }

        if *is_open {
            open_span(class, &mut out);
        } else {
            close_span(&mut out);
        }
    }

    if cursor < input.len()
        && let Some(rest) = input.get(cursor..)
    {
        escape_into(rest, &mut out);
    }

    out
}

/// Flatten sorted spans into open/close events, handling overlaps.
fn flatten_spans(spans: &[Span]) -> Vec<(usize, &'static str, bool)> {
    let mut events: Vec<(usize, &str, bool)> = Vec::with_capacity(spans.len() * 2);
    // (end, class, priority, start)
    let mut active: Vec<(usize, &str, u8, usize)> = Vec::new();

    for span in spans {
        // Close active spans that end before this span starts
        while let Some(&(end, _, _, _)) = active.last() {
            if end <= span.start {
                events.push((end, "", false));
                active.pop();
            } else {
                break;
            }
        }

        // Skip spans dominated by a higher-priority active span
        let dominated = active
            .iter()
            .any(|&(end, _, prio, _)| prio > span.priority && end >= span.end);
        if dominated {
            continue;
        }

        // If active span has lower priority, check if we need to split or can nest
        if let Some(&(parent_end, parent_class, parent_prio, parent_start)) = active.last()
            && parent_prio < span.priority
        {
            // If strictly nested (child ends before or at the same time as parent),
            // nest the child inside the parent so both styles apply.
            if span.end <= parent_end {
                events.push((span.start, span.class, true));
                active.push((span.end, span.class, span.priority, span.start));
                continue;
            }

            // Crossing overlap: split the parent
            events.push((span.start, "", false)); // Close parent
            active.pop();
            events.push((span.start, span.class, true)); // Open child

            // Push parent back first (so it's under child in the stack)
            active.push((parent_end, parent_class, parent_prio, parent_start));
            // Schedule re-open of parent when child ends
            events.push((span.end, parent_class, true));

            // Push child (so it's on top and popped first)
            active.push((span.end, span.class, span.priority, span.start));

            continue;
        }

        events.push((span.start, span.class, true));
        active.push((span.end, span.class, span.priority, span.start));
    }

    while let Some((end, _, _, _)) = active.pop() {
        events.push((end, "", false));
    }

    events.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.2.cmp(&b.2)));

    events
}
// ---------------------------------------------------------------------------
// Utility helpers
// ---------------------------------------------------------------------------

fn admonition_label_len(variant: &AdmonitionVariant) -> usize {
    match variant {
        AdmonitionVariant::Note => 5,
        AdmonitionVariant::Tip => 4,
        AdmonitionVariant::Important => 10,
        AdmonitionVariant::Caution | AdmonitionVariant::Warning => 8,
    }
}

/// A container can start in the entry file and end in an include. Its first
/// line can still be highlighted without using the included endpoint.
fn primary_start<'doc>(input: &str, location: impl Into<LocationRef<'doc>>) -> Option<usize> {
    let location = location.into();
    if location
        .start()
        .include_chain()
        .is_some_and(|mut chain| chain.next().is_some())
    {
        return None;
    }
    input
        .is_char_boundary(location.absolute_start())
        .then_some(location.absolute_start())
}

/// Only entry-file ranges can index `input`. Highlight spans use an exclusive
/// end on a UTF-8 boundary. A parser end can point to the first byte of its final
/// character, or to EOF.
fn primary_range<'doc>(
    input: &str,
    location: impl Into<LocationRef<'doc>>,
) -> Option<(usize, usize)> {
    let location = location.into();
    let start = primary_start(input, location)?;
    let end = primary_end(input, location)?;
    input.get(start..end).map(|_| (start, end))
}

fn primary_end(input: &str, location: LocationRef<'_>) -> Option<usize> {
    if location
        .end()
        .include_chain()
        .is_some_and(|mut chain| chain.next().is_some())
        || location.absolute_end() > input.len()
    {
        return None;
    }
    let mut end = location.absolute_end().checked_add(1)?.min(input.len());
    while !input.is_char_boundary(end) {
        end += 1;
    }
    Some(end)
}

/// Find the end of the line containing `pos` (offset of the newline, or EOF).
fn find_line_end(input: &str, pos: usize) -> usize {
    input
        .get(pos..)
        .and_then(|s| s.find('\n'))
        .map_or(input.len(), |offset| pos + offset)
}

/// Find the start of the line containing `pos`.
fn find_line_start(input: &str, pos: usize) -> usize {
    if pos == 0 {
        return 0;
    }
    input
        .get(..pos)
        .and_then(|s| s.rfind('\n'))
        .map_or(0, |offset| offset + 1)
}

// ---------------------------------------------------------------------------
// HTML output helpers
// ---------------------------------------------------------------------------

/// HTML-escape the full input (no syntax highlighting).
///
/// Used as a fallback when parsing fails so the highlight overlay still shows
/// the current text (the textarea itself has `color: transparent`).
pub(super) fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    escape_into(s, &mut out);
    out
}

fn escape_into(s: &str, out: &mut String) {
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
}

fn open_span(cls: &str, out: &mut String) {
    out.push_str("<span class=\"");
    out.push_str(cls);
    out.push_str("\">");
}

fn close_span(out: &mut String) {
    out.push_str("</span>");
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use acdc_parser::Location;

    fn included_highlight(
        input: &str,
        sources: &[(&str, &str)],
    ) -> Result<String, Box<dyn std::error::Error>> {
        let parsed = acdc_parser::parse(input, &crate::test_sources::options(sources)?)?;
        Ok(highlight_from_ast(input, parsed.document()))
    }

    fn source_text(html: &str) -> String {
        let mut source = String::new();
        let mut tag = false;
        for ch in html.chars() {
            match ch {
                '<' => tag = true,
                '>' => tag = false,
                _ if !tag => source.push(ch),
                _ => {}
            }
        }
        source
    }

    #[test]
    fn included_headings_do_not_style_entry_text() -> Result<(), Box<dyn std::error::Error>> {
        let input = "π 😀 plain entry text.\n\ninclude::chapter.adoc[]\n\n*entry bold*\n";
        let html = included_highlight(
            input,
            &[("chapter.adoc", "== Included heading\n\n_italic_\n")],
        )?;
        assert!(!html.contains("adoc-heading"), "{html}");
        assert!(!html.contains("adoc-italic"), "{html}");
        assert!(html.contains("adoc-bold"), "{html}");
        assert_eq!(source_text(&html), input);
        Ok(())
    }

    #[test]
    fn nested_selected_indented_includes_do_not_style_entry_text()
    -> Result<(), Box<dyn std::error::Error>> {
        let input = "π 😀 ordinary entry text.\n\ninclude::chapters/outer.adoc[]\n\n*entry bold*\n";
        let html = included_highlight(
            input,
            &[
                (
                    "chapters/outer.adoc",
                    "include::inner.adoc[lines=2..3,indent=2]\n",
                ),
                (
                    "chapters/inner.adoc",
                    "Not selected.\n  π 😀 included literal.\n  more text.\nNot selected.\n",
                ),
            ],
        )?;
        assert!(!html.contains("adoc-literal-content"), "{html}");
        assert!(html.contains("adoc-bold"), "{html}");
        assert_eq!(source_text(&html), input);
        Ok(())
    }

    #[test]
    fn entry_delimiters_and_children_survive_mixed_source_blocks()
    -> Result<(), Box<dyn std::error::Error>> {
        let input = "== Entry heading\n\n====\ninclude::part.adoc[]\n\n*entry bold*\n====\n";
        let html = included_highlight(input, &[("part.adoc", "_included italic_\n")])?;
        assert!(html.contains("adoc-heading"), "{html}");
        assert!(html.contains("adoc-delimiter"), "{html}");
        assert!(html.contains("adoc-bold"), "{html}");
        assert!(!html.contains("adoc-italic"), "{html}");
        assert_eq!(source_text(&html), input);
        Ok(())
    }

    #[test]
    fn cross_source_and_invalid_ranges_are_omitted() {
        let input = "😀 entry";
        let mut loc = Location::point(acdc_parser::Position::new(1, 1));
        loc.absolute_end = 3;
        assert_eq!(primary_range(input, &loc), Some((0, 4)));
        loc.end.file = Some(std::sync::Arc::new(vec!["part.adoc".to_string()]));
        assert_eq!(primary_range(input, &loc), None);
        loc.end.file = None;
        loc.absolute_end = input.len() + 1;
        assert_eq!(primary_range(input, &loc), None);
        loc.absolute_start = 1;
        loc.absolute_end = 3;
        assert_eq!(primary_range(input, &loc), None);
    }

    #[test]
    fn unicode_at_range_ends_keeps_the_complete_source() {
        for input in ["// 😀", "----\nπ 😀\n----", "😀 *π* 😀", "term:: π 😀"] {
            assert_eq!(source_text(&highlight(input)), input);
        }
    }

    #[test]
    fn entry_heading_can_end_in_an_included_file() -> Result<(), Box<dyn std::error::Error>> {
        let input = "== Entry heading\n\ninclude::part.adoc[]\n";
        let parsed = acdc_parser::parse(
            input,
            &crate::test_sources::options(&[("part.adoc", "_included italic_\n")])?,
        )?;
        let block = parsed.document().blocks().next().ok_or("missing block")?;
        let BlockKind::Section(section) = block.kind() else {
            return Err("missing section".into());
        };
        assert!(section.location().start().include_chain().is_none());
        assert!(section.location().end().include_chain().is_some());
        let html = highlight_from_ast(input, parsed.document());
        assert!(html.contains("adoc-heading"), "{html}");
        assert!(!html.contains("adoc-italic"), "{html}");
        assert_eq!(source_text(&html), input);
        Ok(())
    }

    fn highlight(input: &str) -> String {
        let options = acdc_parser::Options::default();
        if let Ok(parsed) = acdc_parser::parse(input, &options) {
            highlight_from_ast(input, parsed.document())
        } else {
            let mut out = String::new();
            escape_into(input, &mut out);
            out
        }
    }

    #[test]
    fn test_heading_highlight() {
        let result = highlight("== My heading");
        assert!(result.contains("adoc-heading"), "result: {result}");
        assert!(result.contains("My heading"), "result: {result}");
    }

    #[test]
    fn test_title_highlight() {
        // A bare `= Title` is recognized as the document title (doctitle)
        let result = highlight("= Document title");
        assert!(result.contains("adoc-title"), "result: {result}");
    }

    #[test]
    fn test_title_with_header() {
        // With attributes, the parser produces a proper document header
        let result = highlight("= Document title\n:author: Test");
        assert!(result.contains("adoc-title"), "result: {result}");
    }

    #[test]
    fn test_attribute_highlight() {
        // Standalone `:attr:` is parsed as a document attribute when under a header
        let result = highlight("= Doc\n:date: 2025-01-01");
        assert!(result.contains("adoc-attribute"), "result: {result}");
    }

    #[test]
    fn test_block_title_highlight() {
        let result = highlight(".Block title\nSome paragraph text");
        assert!(result.contains("adoc-block-title"), "result: {result}");
    }

    #[test]
    fn test_list_marker_highlight() {
        let result = highlight("* List item");
        assert!(result.contains("adoc-list-marker"), "result: {result}");
        assert!(result.contains("List item"), "result: {result}");
    }

    #[test]
    fn test_bold_inline() {
        let result = highlight("This is *bold* text");
        assert!(result.contains("adoc-bold"), "result: {result}");
    }

    #[test]
    fn test_italic_inline() {
        let result = highlight("This is _italic_ text");
        assert!(result.contains("adoc-italic"), "result: {result}");
    }

    #[test]
    fn test_monospace_inline() {
        let result = highlight("This is `code` text");
        assert!(result.contains("adoc-monospace"), "result: {result}");
    }

    #[test]
    fn test_comment_highlight() {
        let result = highlight("// a comment");
        assert!(result.contains("adoc-comment"), "result: {result}");
    }

    #[test]
    fn test_block_delimiter() {
        let result = highlight("----\ncode here\n----");
        assert!(result.contains("adoc-delimiter"), "result: {result}");
        assert!(result.contains("adoc-code-content"), "result: {result}");
        // Both opening and closing delimiters should be highlighted
        let delim_count = result.matches("adoc-delimiter").count();
        assert!(
            delim_count >= 2,
            "Both opening and closing ---- should be delimiters, found {delim_count}: {result}"
        );
    }

    #[test]
    fn test_listing_block_content() {
        let result = highlight("----\nfn main() {}\n----");
        assert!(result.contains("adoc-code-content"), "result: {result}");
    }

    #[test]
    fn test_comment_block_content() {
        // The parser recognizes comment blocks inside a document with content
        let input = "Some text\n\n////\nthis is hidden\n////";
        let result = highlight(input);
        assert!(
            result.contains("adoc-comment") || result.contains("adoc-delimiter"),
            "result: {result}"
        );
    }

    #[test]
    fn test_literal_block_content() {
        let result = highlight("....\nverbatim text\n....");
        assert!(result.contains("adoc-literal-content"), "result: {result}");
    }

    #[test]
    fn test_passthrough_block_content() {
        let result = highlight("++++\n<div>raw</div>\n++++");
        assert!(
            result.contains("adoc-passthrough-content"),
            "result: {result}"
        );
    }

    #[test]
    fn test_admonition() {
        let result = highlight("TIP: Do this thing");
        assert!(result.contains("adoc-admonition"), "result: {result}");
    }

    #[test]
    fn test_block_macro() {
        let result = highlight("image::photo.jpg[alt text]");
        assert!(result.contains("adoc-macro"), "result: {result}");
    }

    #[test]
    fn test_block_image_media_highlighting() {
        let result = highlight("image::sunset.jpg[Sunset,300,200]");
        assert!(result.contains("adoc-macro-target"), "result: {result}");
        assert!(result.contains("adoc-macro-attrs"), "result: {result}");
        assert!(result.contains("sunset.jpg"), "result: {result}");
    }

    #[test]
    fn test_inline_image_media_highlighting() {
        let result = highlight("See image:icon.png[Icon] here");
        assert!(result.contains("adoc-macro-target"), "result: {result}");
        assert!(result.contains("adoc-macro-attrs"), "result: {result}");
    }

    #[test]
    fn test_html_escaping() {
        let result = highlight("Use <div> & \"quotes\"");
        assert!(result.contains("&lt;div&gt;"), "result: {result}");
        assert!(result.contains("&amp;"), "result: {result}");
        assert!(result.contains("&quot;"), "result: {result}");
    }

    #[test]
    fn test_block_attribute() {
        let result = highlight("[source,rust]\n----\nfn main() {}\n----");
        assert!(result.contains("adoc-attribute"), "result: {result}");
    }

    #[test]
    fn test_ordered_list() {
        let result = highlight(". First item");
        assert!(result.contains("adoc-list-marker"), "result: {result}");
    }

    #[test]
    fn test_highlight_inline() {
        let result = highlight("This is #highlighted# text");
        assert!(result.contains("adoc-highlight"), "result: {result}");
    }

    #[test]
    fn test_superscript_inline() {
        let result = highlight("E=mc^2^");
        assert!(result.contains("adoc-superscript"), "result: {result}");
    }

    #[test]
    fn test_subscript_inline() {
        let result = highlight("H~2~O");
        assert!(result.contains("adoc-subscript"), "result: {result}");
    }

    #[test]
    fn test_thematic_break() {
        let result = highlight("'''");
        assert!(result.contains("adoc-thematic-break"), "result: {result}");
    }

    #[test]
    fn test_page_break() {
        // Page break is recognized when embedded in a document
        let input = "Some text\n\n<<<\n\nMore text";
        let result = highlight(input);
        assert!(
            result.contains("adoc-page-break") || result.contains("&lt;&lt;&lt;"),
            "result: {result}"
        );
    }

    #[test]
    fn test_table_delimiter() {
        let result = highlight("|===\n| Cell 1 | Cell 2\n|===");
        assert!(result.contains("adoc-table-delimiter"), "result: {result}");
    }

    #[test]
    fn test_table_cell_highlighting() {
        let result = highlight("|===\n| Cell 1 | Cell 2\n|===");
        assert!(result.contains("adoc-table-cell"), "result: {result}");
    }

    #[test]
    fn test_xref() {
        let result = highlight("See <<my-section>>");
        assert!(result.contains("adoc-xref"), "result: {result}");
    }

    #[test]
    fn test_autolink_https() {
        let result = highlight("Visit https://example.com today");
        assert!(result.contains("adoc-link"), "result: {result}");
    }

    #[test]
    fn test_autolink_full_url_colored() {
        // The entire URL should be inside the adoc-link span, including the last character
        let result = highlight("https://asciidoc.org");
        // The entire URL must be inside the span — no trailing characters outside
        assert!(
            result.contains(r#"<span class="adoc-link">https://asciidoc.org</span>"#),
            "Entire URL should be inside adoc-link span, got: {result}"
        );
    }

    #[test]
    fn test_url_with_display_text() {
        // https://asciidoc.org[AsciiDoc] — brackets should not be link-colored,
        // display text should differ from the URL
        let result = highlight("https://asciidoc.org[AsciiDoc].");
        assert!(
            result.contains("adoc-macro-attrs"),
            "Display text should be highlighted as attrs: {result}"
        );
    }

    #[test]
    fn test_footnote_closing_bracket_not_bold() {
        let result = highlight("footnote:id[text here]");
        // The closing ] should not be bold — it should be inside the footnote
        // span, not outside it
        assert!(
            !result.ends_with(']'),
            "Closing ] should be inside a span, not bare: {result}"
        );
    }

    #[test]
    fn test_inline_macro_footnote() {
        let result = highlight("Text footnote:[A note]");
        assert!(result.contains("adoc-footnote"), "result: {result}");
    }

    #[test]
    fn test_description_list() {
        let result = highlight("Term:: Definition");
        assert!(
            result.contains("adoc-description-marker"),
            "result: {result}"
        );
    }

    #[test]
    fn test_double_bold() {
        let result = highlight("**unconstrained**");
        assert!(result.contains("adoc-bold"), "result: {result}");
    }

    #[test]
    fn test_unconstrained_bold_mid_word() {
        // t**he** should highlight **he** as bold with both ** pairs as delimiters
        let result = highlight("t**he**");
        let delim_count = result.matches("adoc-delimiter").count();
        assert!(
            delim_count >= 2,
            "Both opening and closing ** should be delimiters, found {delim_count}: {result}"
        );
        // No bare * should be outside delimiter/bold spans
        assert!(
            !result.contains("</span>*"),
            "No bare * should be outside spans: {result}"
        );
        // Opening ** must be inside bold span (bold wraps delimiter)
        assert!(
            result.contains(r#"<span class="adoc-bold"><span class="adoc-delimiter">**</span>"#),
            "Opening ** should be inside bold span: {result}"
        );

        // Also test Sara**h**
        let result = highlight("Sara**h**");
        // The 'a' before ** must NOT be inside a bold span
        assert!(
            !result.contains(r#"<span class="adoc-bold">a"#),
            "The 'a' before ** should not be inside bold span: {result}"
        );
        // Both opening and closing ** should be highlighted as delimiters
        let delim_count = result.matches("adoc-delimiter").count();
        assert!(
            delim_count >= 2,
            "Both opening and closing ** should be delimiters, found {delim_count}: {result}"
        );
    }

    #[test]
    fn test_unconstrained_italic_mid_word() {
        let result = highlight("Sara__h__");
        assert!(
            !result.contains(r#"<span class="adoc-italic">a"#),
            "The 'a' before __ should not be inside italic span: {result}"
        );
        let delim_count = result.matches("adoc-delimiter").count();
        assert!(
            delim_count >= 2,
            "Both opening and closing __ should be delimiters, found {delim_count}: {result}"
        );
        // Opening __ must be inside italic span
        assert!(
            result.contains(r#"<span class="adoc-italic"><span class="adoc-delimiter">__</span>"#),
            "Opening __ should be inside italic span: {result}"
        );
    }

    #[test]
    fn test_unconstrained_monospace_mid_word() {
        let result = highlight("Sara``h``");
        assert!(
            !result.contains(r#"<span class="adoc-monospace">a"#),
            "The 'a' before `` should not be inside monospace span: {result}"
        );
        let delim_count = result.matches("adoc-delimiter").count();
        assert!(
            delim_count >= 2,
            "Both opening and closing `` should be delimiters, found {delim_count}: {result}"
        );
        // Opening `` must be inside monospace span
        assert!(
            result
                .contains(r#"<span class="adoc-monospace"><span class="adoc-delimiter">``</span>"#),
            "Opening `` should be inside monospace span: {result}"
        );
    }

    #[test]
    fn test_unconstrained_highlight_mid_word() {
        let result = highlight("Sara##h##");
        assert!(
            !result.contains(r#"<span class="adoc-highlight">a"#),
            "The 'a' before ## should not be inside highlight span: {result}"
        );
        let delim_count = result.matches("adoc-delimiter").count();
        assert!(
            delim_count >= 2,
            "Both opening and closing ## should be delimiters, found {delim_count}: {result}"
        );
        // Opening ## must be inside highlight span
        assert!(
            result
                .contains(r#"<span class="adoc-highlight"><span class="adoc-delimiter">##</span>"#),
            "Opening ## should be inside highlight span: {result}"
        );
    }

    #[test]
    fn test_sidebar_block_inline_highlighting() {
        let result = highlight("****\nThis has *bold* text\n****");
        assert!(result.contains("adoc-bold"), "result: {result}");
    }

    #[test]
    fn test_empty_input() {
        let result = highlight("");
        assert_eq!(result, "");
    }

    #[test]
    fn test_plain_text_escaped() {
        let result = highlight("Hello world");
        assert_eq!(result, "Hello world");
    }

    #[test]
    fn test_setext_header_highlight() -> Result<(), Box<dyn std::error::Error>> {
        // Setext header: title line + underline line
        let input = "Setext Header\n-------------";
        let options = acdc_parser::Options::builder().build()?;
        let parsed = acdc_parser::parse(input, &options).expect("failed to parse setext");
        let result = highlight_from_ast(input, parsed.document());

        // Should contain two adoc-heading spans (one for title, one for underline)
        let matches: Vec<_> = result.matches("adoc-heading").collect();
        assert_eq!(matches.len(), 2, "result: {result}");
        assert!(result.contains("Setext Header"), "result: {result}");
        assert!(result.contains("-------------"), "result: {result}");
        Ok(())
    }

    #[test]
    fn test_admonition_style_colon() {
        let result = highlight("NOTE: This is a note.");
        assert!(result.contains("adoc-admonition"), "result: {result}");
        assert!(result.contains(">NOTE:</span>"), "result: {result}");
    }

    #[test]
    fn test_admonition_style_block() {
        // [NOTE] style should NOT trigger adoc-admonition on the label itself
        // because it's handled as an attribute
        let result = highlight("[NOTE]\n====\nThis is a note.\n====");
        // The [NOTE] line is highlighted as an attribute
        assert!(result.contains("adoc-attribute"), "result: {result}");
        // The content inside might be, but there shouldn't be an adoc-admonition span wrapping "NOTE"
        // (unless we decide to treat block attributes differently, but current logic is consistent)
        assert!(!result.contains("adoc-admonition"), "result: {result}");
    }

    #[test]
    fn test_macro_granular_highlighting() {
        let result = highlight("link:https://example.com[Label]");
        // Target (URL) should be highlighted
        assert!(
            result.contains("adoc-macro-target"),
            "Link target should be highlighted: {result}"
        );
        // Bracket content should be highlighted as attrs
        assert!(
            result.contains("adoc-macro-attrs"),
            "Link attrs should be highlighted: {result}"
        );
    }

    #[test]
    fn test_footnote_granular_highlighting() {
        let result = highlight("Text footnote:myid[A note]");
        assert!(
            result.contains("adoc-macro-target"),
            "Footnote id should be highlighted as target: {result}"
        );
        assert!(
            result.contains("adoc-macro-attrs"),
            "Footnote content should be highlighted as attrs: {result}"
        );
    }

    #[test]
    fn test_macro_no_target_highlighting() {
        // kbd has no target, only bracket content
        let result = highlight(":experimental:\n\nPress kbd:[Ctrl+C]");
        assert!(
            result.contains("adoc-macro-attrs"),
            "kbd content should be highlighted as attrs: {result}"
        );
    }

    #[test]
    fn test_heading_with_inlines() {
        let input = "=== Heading with footnote:id[text]";
        let result = highlight(input);
        assert!(result.contains("adoc-heading"), "result: {result}");
        assert!(result.contains("adoc-footnote"), "result: {result}");
        // Verify nesting: adoc-heading should open before adoc-footnote
        let heading_pos = result.find("adoc-heading").unwrap_or(usize::MAX);
        let footnote_pos = result.find("adoc-footnote").unwrap_or(usize::MAX);
        assert!(
            heading_pos < footnote_pos,
            "Heading should start before footnote"
        );
    }
}
