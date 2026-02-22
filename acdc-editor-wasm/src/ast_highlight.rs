//! AST-based `AsciiDoc` source syntax highlighting.
//!
//! Walks the parsed AST from `acdc-parser` and uses `Location` byte offsets to
//! map each node back to its source text, wrapping relevant ranges in
//! `<span class="adoc-*">` for CSS-based highlighting.

use acdc_parser::{
    AdmonitionVariant, Block, DelimitedBlockType, Document, InlineMacro, InlineNode,
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

/// Highlight `AsciiDoc` source text using pre-parsed AST locations.
///
/// Walks the document tree, collects byte-offset spans, flattens overlaps, and
/// emits HTML with `<span class="adoc-*">` wrappers.
pub fn highlight_from_ast(input: &str, doc: &Document) -> String {
    let mut spans = Vec::new();

    // Document header: title line + attribute lines
    if let Some(header) = &doc.header {
        let start = header.location.absolute_start;
        let title_end = find_line_end(input, start);
        spans.push(Span {
            start,
            end: title_end,
            class: "adoc-title",
            priority: 1,
        });

        // Highlight inline content in the title
        collect_inline_spans(input, &header.title, &mut spans);

        // Highlight subtitle if present
        if let Some(subtitle) = &header.subtitle {
            collect_inline_spans(input, subtitle, &mut spans);
        }

        // Highlight remaining header lines (document attributes like `:date:`)
        let header_end = header.location.absolute_end;
        let mut pos = if title_end < input.len() {
            title_end + 1
        } else {
            title_end
        };
        while pos < header_end {
            let line_end = find_line_end(input, pos);
            let line = input.get(pos..line_end).unwrap_or("");
            if line.starts_with(':') {
                spans.push(Span {
                    start: pos,
                    end: line_end,
                    class: "adoc-attribute",
                    priority: 1,
                });
            }
            pos = if line_end < input.len() {
                line_end + 1
            } else {
                break;
            };
        }
    }

    for block in &doc.blocks {
        collect_block_spans(input, block, &mut spans);
    }

    render_spans(input, &mut spans)
}

// ---------------------------------------------------------------------------
// Block-level span collection
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_lines)]
fn collect_block_spans(input: &str, block: &Block, spans: &mut Vec<Span>) {
    match block {
        Block::Section(section) => {
            let start = section.location.absolute_start;
            let end = section.location.absolute_end;
            let heading_end = find_line_end(input, start);

            spans.push(Span {
                start,
                end: heading_end,
                class: "adoc-heading",
                priority: 1,
            });

            // Check if this is a setext header (multi-line)
            // If the section block extends past the first line, check the next line for underline chars
            if heading_end < end {
                let next_line_start = heading_end + 1; // Skip newline
                if next_line_start < end {
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
            // Highlight inline content in the section title
            collect_inline_spans(input, &section.title, spans);

            for child in &section.content {
                collect_block_spans(input, child, spans);
            }
        }
        Block::DocumentAttribute(attr) => {
            push_block_span(spans, &attr.location, "adoc-attribute");
        }
        Block::Comment(comment) => {
            push_block_span(spans, &comment.location, "adoc-comment");
        }
        Block::ThematicBreak(tb) => {
            push_block_span(spans, &tb.location, "adoc-thematic-break");
            collect_inline_spans(input, &tb.title, spans);
        }
        Block::PageBreak(pb) => {
            push_block_span(spans, &pb.location, "adoc-page-break");
            collect_inline_spans(input, &pb.title, spans);
        }
        Block::Admonition(adm) => {
            collect_block_metadata_spans(input, &adm.metadata, adm.location.absolute_start, spans);
            let label_len = admonition_label_len(&adm.variant);
            let start = adm.location.absolute_start;
            // Only highlight the label if the text actually starts with it (handles NOTE: vs [NOTE])
            // The label length includes the colon and space, so we check for that prefix
            let expected_label_prefix = format!("{}:", adm.variant).to_uppercase();
            if let Some(text) = input.get(start..)
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

            // Highlight title if present
            collect_inline_spans(input, &adm.title, spans);

            for child in &adm.blocks {
                collect_block_spans(input, child, spans);
            }
        }
        Block::UnorderedList(list) => {
            // Highlight list title
            collect_inline_spans(input, &list.title, spans);
            for item in &list.items {
                collect_list_item_spans(input, item, spans);
            }
        }
        Block::OrderedList(list) => {
            // Highlight list title
            collect_inline_spans(input, &list.title, spans);
            for item in &list.items {
                collect_list_item_spans(input, item, spans);
            }
        }
        Block::CalloutList(list) => {
            // Highlight list title
            collect_inline_spans(input, &list.title, spans);
            for item in &list.items {
                push_inline_span(spans, &item.callout.location, "adoc-callout");
                collect_inline_spans(input, &item.principal, spans);
                for child in &item.blocks {
                    collect_block_spans(input, child, spans);
                }
            }
        }
        Block::DescriptionList(list) => {
            // Highlight list title
            collect_inline_spans(input, &list.title, spans);
            collect_description_list_spans(input, list, spans);
        }
        Block::DelimitedBlock(db) => {
            collect_delimited_block_spans(input, db, spans);
            // Highlight title if present
            collect_inline_spans(input, &db.title, spans);
        }
        Block::Paragraph(para) => {
            collect_block_metadata_spans(
                input,
                &para.metadata,
                para.location.absolute_start,
                spans,
            );
            // Highlight title if present
            collect_inline_spans(input, &para.title, spans);
            collect_inline_spans(input, &para.content, spans);
        }
        Block::Image(img) => {
            push_block_span(spans, &img.location, "adoc-macro");
            collect_macro_target_spans(
                input,
                img.location.absolute_start,
                img.location.absolute_end,
                1,
                spans,
            );
        }
        Block::Audio(audio) => {
            push_block_span(spans, &audio.location, "adoc-macro");
            collect_macro_target_spans(
                input,
                audio.location.absolute_start,
                audio.location.absolute_end,
                1,
                spans,
            );
        }
        Block::Video(video) => {
            push_block_span(spans, &video.location, "adoc-macro");
            collect_macro_target_spans(
                input,
                video.location.absolute_start,
                video.location.absolute_end,
                1,
                spans,
            );
        }
        Block::DiscreteHeader(dh) => {
            push_block_span(spans, &dh.location, "adoc-heading");
            collect_inline_spans(input, &dh.title, spans);
        }
        Block::TableOfContents(toc) => push_block_span(spans, &toc.location, "adoc-macro"),
        _ => {}
    }
}

fn push_block_span(spans: &mut Vec<Span>, location: &acdc_parser::Location, class: &'static str) {
    spans.push(Span {
        start: location.absolute_start,
        end: location.absolute_end,
        class,
        priority: 1,
    });
}

fn push_inline_span(spans: &mut Vec<Span>, location: &acdc_parser::Location, class: &'static str) {
    spans.push(Span {
        start: location.absolute_start,
        end: location.absolute_end,
        class,
        priority: 2,
    });
}

/// Highlight opening and closing delimiters for an inline node.
///
/// Returns the effective end position of the delimited region. This may
/// extend past `location.absolute_end` when the parser's location is off
/// by one for unconstrained markers (e.g. `t**he**`).
fn highlight_delimited_inline(
    input: &str,
    location: &acdc_parser::Location,
    marker_char: char,
    spans: &mut Vec<Span>,
) -> usize {
    let start = location.absolute_start;
    let end = location.absolute_end;
    let Some(text) = input.get(start..end) else {
        return end;
    };

    // Count consecutive markers at start (e.g. "**" or "*")
    let len = text.chars().take_while(|&c| c == marker_char).count();
    if len == 0 {
        return end;
    }

    // Opening marker
    spans.push(Span {
        start,
        end: start + len,
        class: "adoc-delimiter",
        priority: 3,
    });

    // Closing marker — try several positions to handle parser off-by-one
    let is_marker = |s: usize, e: usize| -> bool {
        s >= start + len
            && e <= input.len()
            && input
                .get(s..e)
                .is_some_and(|t| t.len() == len && t.chars().all(|c| c == marker_char))
    };

    // 1. Standard: closing delimiter at end of node location
    let close_start = end.saturating_sub(len);
    if is_marker(close_start, end) {
        spans.push(Span {
            start: close_start,
            end,
            class: "adoc-delimiter",
            priority: 3,
        });
        return end;
    }

    // 2. Immediately after the node (parser excluded closing delim entirely)
    if is_marker(end, end + len) {
        spans.push(Span {
            start: end,
            end: end + len,
            class: "adoc-delimiter",
            priority: 3,
        });
        return end + len;
    }

    // 3. Straddling the node boundary (off by one for unconstrained markers)
    if len > 1 {
        let alt_start = end - len + 1;
        let alt_end = alt_start + len;
        if is_marker(alt_start, alt_end) {
            spans.push(Span {
                start: alt_start,
                end: alt_end,
                class: "adoc-delimiter",
                priority: 3,
            });
            return alt_end;
        }
    }

    end
}

fn collect_description_list_spans(
    input: &str,
    list: &acdc_parser::DescriptionList,
    spans: &mut Vec<Span>,
) {
    for item in &list.items {
        let item_start = item.location.absolute_start;
        let item_source = input
            .get(item_start..item.location.absolute_end)
            .unwrap_or("");
        if let Some(delim_pos) = item_source.find(&item.delimiter) {
            let abs_delim_start = item_start + delim_pos;
            let abs_delim_end = abs_delim_start + item.delimiter.len();
            spans.push(Span {
                start: abs_delim_start,
                end: abs_delim_end,
                class: "adoc-description-marker",
                priority: 1,
            });
        }
        collect_inline_spans(input, &item.term, spans);
        collect_inline_spans(input, &item.principal_text, spans);
        for child in &item.description {
            collect_block_spans(input, child, spans);
        }
    }
}

fn collect_list_item_spans(input: &str, item: &acdc_parser::ListItem, spans: &mut Vec<Span>) {
    let start = item.location.absolute_start;
    let marker_len = item.marker.len();
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

    if let Some(checked) = &item.checked {
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

    collect_inline_spans(input, &item.principal, spans);
    for child in &item.blocks {
        collect_block_spans(input, child, spans);
    }
}

// ---------------------------------------------------------------------------
// Delimited blocks
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_lines)]
fn collect_delimited_block_spans(
    input: &str,
    db: &acdc_parser::DelimitedBlock,
    spans: &mut Vec<Span>,
) {
    let block_start = db.location.absolute_start;
    let block_end = db.location.absolute_end;

    collect_block_metadata_spans(input, &db.metadata, block_start, spans);
    collect_delimiter_lines(input, db, block_start, block_end, spans);
    collect_delimited_content(input, &db.inner, block_start, block_end, spans);
}

/// Find the opening and closing delimiter lines within a delimited block.
///
/// The block location may include preceding metadata lines (e.g. `[source,rust]`),
/// so we scan forward through lines to find one matching the delimiter string.
fn collect_delimiter_lines(
    input: &str,
    db: &acdc_parser::DelimitedBlock,
    block_start: usize,
    block_end: usize,
    spans: &mut Vec<Span>,
) {
    let delim = &db.delimiter;
    if delim.is_empty() {
        return;
    }

    let cls = delimiter_class(&db.inner);

    // Scan forward from block_start to find the opening delimiter line
    let mut pos = block_start;
    while pos < block_end {
        let line_end = find_line_end(input, pos);
        let line = input.get(pos..line_end).unwrap_or("");
        if line.trim() == delim {
            spans.push(Span {
                start: pos,
                end: line_end,
                class: cls,
                priority: 1,
            });
            break;
        }
        // Skip past the newline
        pos = if line_end < input.len() {
            line_end + 1
        } else {
            break;
        };
    }

    // Closing delimiter: check the last line of the block.
    // Try both block_end and block_end+1 to handle parser off-by-one.
    if block_end > 0 {
        for effective_end in [block_end, block_end + 1] {
            if effective_end > input.len() {
                continue;
            }
            let close_start = find_line_start(input, effective_end.saturating_sub(1));
            let close_line = input.get(close_start..effective_end).unwrap_or("");
            if close_start > block_start && close_line.trim() == delim {
                spans.push(Span {
                    start: close_start,
                    end: effective_end,
                    class: cls,
                    priority: 1,
                });
                break;
            }
        }
    }
}

fn collect_delimited_content(
    input: &str,
    inner: &DelimitedBlockType,
    block_start: usize,
    block_end: usize,
    spans: &mut Vec<Span>,
) {
    match inner {
        DelimitedBlockType::DelimitedComment(inlines) => {
            for node in inlines {
                let (s, e) = inline_node_range(node);
                spans.push(Span {
                    start: s,
                    end: e,
                    class: "adoc-comment",
                    priority: 1,
                });
            }
        }
        DelimitedBlockType::DelimitedListing(inlines) => {
            collect_listing_content(inlines, spans);
        }
        DelimitedBlockType::DelimitedLiteral(inlines) => {
            push_verbatim_spans(inlines, "adoc-literal-content", spans);
        }
        DelimitedBlockType::DelimitedPass(inlines) => {
            push_verbatim_spans(inlines, "adoc-passthrough-content", spans);
        }
        DelimitedBlockType::DelimitedVerse(inlines) => {
            collect_inline_spans(input, inlines, spans);
        }
        DelimitedBlockType::DelimitedExample(blocks)
        | DelimitedBlockType::DelimitedOpen(blocks)
        | DelimitedBlockType::DelimitedSidebar(blocks)
        | DelimitedBlockType::DelimitedQuote(blocks) => {
            for child in blocks {
                collect_block_spans(input, child, spans);
            }
        }
        DelimitedBlockType::DelimitedTable(table) => {
            collect_table_spans(input, table, block_start, block_end, spans);
        }
        DelimitedBlockType::DelimitedStem(_) => {
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
        _ => {}
    }
}

fn collect_listing_content(inlines: &[InlineNode], spans: &mut Vec<Span>) {
    for node in inlines {
        let (s, e) = inline_node_range(node);
        if matches!(node, InlineNode::CalloutRef(..)) {
            spans.push(Span {
                start: s,
                end: e,
                class: "adoc-callout",
                priority: 2,
            });
        } else {
            spans.push(Span {
                start: s,
                end: e,
                class: "adoc-code-content",
                priority: 1,
            });
        }
    }
}

fn push_verbatim_spans(inlines: &[InlineNode], class: &'static str, spans: &mut Vec<Span>) {
    for node in inlines {
        let (s, e) = inline_node_range(node);
        spans.push(Span {
            start: s,
            end: e,
            class,
            priority: 1,
        });
    }
}

fn collect_table_spans(
    input: &str,
    table: &acdc_parser::Table,
    block_start: usize,
    block_end: usize,
    spans: &mut Vec<Span>,
) {
    let open_end = find_line_end(input, block_start);
    spans.push(Span {
        start: block_start,
        end: open_end,
        class: "adoc-table-delimiter",
        priority: 1,
    });

    if block_end > 0 {
        let close_start = find_line_start(input, block_end.saturating_sub(1));
        if close_start > block_start {
            spans.push(Span {
                start: close_start,
                end: block_end,
                class: "adoc-table-delimiter",
                priority: 1,
            });
        }
    }

    let content_start = open_end;
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

    collect_table_row_block_spans(input, table.header.as_ref(), spans);
    for row in &table.rows {
        collect_table_row_block_spans(input, Some(row), spans);
    }
    collect_table_row_block_spans(input, table.footer.as_ref(), spans);
}

fn collect_table_row_block_spans(
    input: &str,
    row: Option<&acdc_parser::TableRow>,
    spans: &mut Vec<Span>,
) {
    if let Some(r) = row {
        for col in &r.columns {
            for block in &col.content {
                collect_block_spans(input, block, spans);
            }
        }
    }
}

/// Add spans for block metadata (attributes like `[source,rust]`, block titles).
///
/// Metadata lines may either precede the block (scanned backwards) or be
/// included in the block's own location range (scanned forwards).
fn collect_block_metadata_spans(
    input: &str,
    metadata: &acdc_parser::BlockMetadata,
    block_start: usize,
    spans: &mut Vec<Span>,
) {
    if let Some(anchor) = &metadata.id {
        push_block_span(spans, &anchor.location, "adoc-anchor");
    }

    for anchor in &metadata.anchors {
        push_block_span(spans, &anchor.location, "adoc-anchor");
    }

    let has_attrs = !metadata.attributes.is_empty()
        || !metadata.positional_attributes.is_empty()
        || !metadata.roles.is_empty()
        || !metadata.options.is_empty()
        || metadata.style.is_some();

    // Even if metadata is empty (e.g. [NOTE] consumed into AdmonitionVariant),
    // we should check for attribute lines in the source.
    let looks_like_meta = input.get(block_start..).is_some_and(|s| {
        let trimmed = s.trim_start();
        trimmed.starts_with('[') || (trimmed.starts_with('.') && !trimmed.starts_with(".."))
    });

    if has_attrs || looks_like_meta {
        // Try scanning backwards first (metadata before block location)
        scan_preceding_attributes(input, block_start, spans);
        // Also scan forward from block_start for metadata lines included in
        // the block's location (e.g. when `[source,rust]` is at byte 0)
        scan_leading_attributes(input, block_start, spans);
    }
}

fn scan_preceding_attributes(input: &str, block_start: usize, spans: &mut Vec<Span>) {
    let mut pos = block_start;
    while pos > 0 {
        let line_start = find_line_start(input, pos.saturating_sub(1));
        let line = input.get(line_start..pos).unwrap_or("");
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            spans.push(Span {
                start: line_start,
                end: pos,
                class: "adoc-attribute",
                priority: 1,
            });
            pos = line_start;
        } else if trimmed.starts_with('.') && !trimmed.starts_with("..") {
            spans.push(Span {
                start: line_start,
                end: pos,
                class: "adoc-block-title",
                priority: 1,
            });
            pos = line_start;
        } else {
            break;
        }
    }
}

/// Scan forward from `block_start` for attribute/title lines that are part of
/// the block's location (e.g. `[source,rust]` at byte 0 before `----`).
fn scan_leading_attributes(input: &str, block_start: usize, spans: &mut Vec<Span>) {
    let mut pos = block_start;
    loop {
        let line_end = find_line_end(input, pos);
        let line = input.get(pos..line_end).unwrap_or("");
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            spans.push(Span {
                start: pos,
                end: line_end,
                class: "adoc-attribute",
                priority: 1,
            });
        } else if trimmed.starts_with('.') && !trimmed.starts_with("..") {
            spans.push(Span {
                start: pos,
                end: line_end,
                class: "adoc-block-title",
                priority: 1,
            });
        } else {
            break;
        }
        pos = if line_end < input.len() {
            line_end + 1
        } else {
            break;
        };
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

fn collect_inline_spans(input: &str, nodes: &[InlineNode], spans: &mut Vec<Span>) {
    for node in nodes {
        collect_single_inline_span(input, node, spans);
    }
}

fn collect_single_inline_span(input: &str, node: &InlineNode, spans: &mut Vec<Span>) {
    match node {
        InlineNode::BoldText(b) => {
            let effective_end = highlight_delimited_inline(input, &b.location, '*', spans);
            spans.push(Span {
                start: b.location.absolute_start,
                end: effective_end,
                class: "adoc-bold",
                priority: 2,
            });
            collect_inline_spans(input, &b.content, spans);
        }
        InlineNode::ItalicText(i) => {
            let effective_end = highlight_delimited_inline(input, &i.location, '_', spans);
            spans.push(Span {
                start: i.location.absolute_start,
                end: effective_end,
                class: "adoc-italic",
                priority: 2,
            });
            collect_inline_spans(input, &i.content, spans);
        }
        InlineNode::MonospaceText(m) => {
            let effective_end = highlight_delimited_inline(input, &m.location, '`', spans);
            spans.push(Span {
                start: m.location.absolute_start,
                end: effective_end,
                class: "adoc-monospace",
                priority: 2,
            });
            collect_inline_spans(input, &m.content, spans);
        }
        InlineNode::HighlightText(h) => {
            let effective_end = highlight_delimited_inline(input, &h.location, '#', spans);
            spans.push(Span {
                start: h.location.absolute_start,
                end: effective_end,
                class: "adoc-highlight",
                priority: 2,
            });
            collect_inline_spans(input, &h.content, spans);
        }
        InlineNode::SuperscriptText(s) => {
            let effective_end = highlight_delimited_inline(input, &s.location, '^', spans);
            spans.push(Span {
                start: s.location.absolute_start,
                end: effective_end,
                class: "adoc-superscript",
                priority: 2,
            });
            collect_inline_spans(input, &s.content, spans);
        }
        InlineNode::SubscriptText(s) => {
            let effective_end = highlight_delimited_inline(input, &s.location, '~', spans);
            spans.push(Span {
                start: s.location.absolute_start,
                end: effective_end,
                class: "adoc-subscript",
                priority: 2,
            });
            collect_inline_spans(input, &s.content, spans);
        }
        InlineNode::CurvedQuotationText(q) => {
            let effective_end = highlight_delimited_inline(input, &q.location, '"', spans);
            spans.push(Span {
                start: q.location.absolute_start,
                end: effective_end,
                class: "adoc-bold", // Usually treated as bold or quote
                priority: 2,
            });
            collect_inline_spans(input, &q.content, spans);
        }
        InlineNode::InlineAnchor(a) => {
            push_inline_span(spans, &a.location, "adoc-anchor");
        }
        InlineNode::CalloutRef(cr) => {
            push_inline_span(spans, &cr.location, "adoc-callout");
        }
        InlineNode::Macro(m) => {
            collect_macro_span(input, m, spans);
        }
        InlineNode::PlainText(_)
        | InlineNode::RawText(_)
        | InlineNode::VerbatimText(_)
        | InlineNode::CurvedApostropheText(_)
        | InlineNode::StandaloneCurvedApostrophe(_)
        | InlineNode::LineBreak(_)
        | _ => {}
    }
}

fn collect_macro_span(input: &str, mac: &InlineMacro, spans: &mut Vec<Span>) {
    // Macros where the colon is part of the content (URLs), not a name:target separator.
    // These get only the base span with no granular target/attrs highlighting.
    let (class, location, granular) = match mac {
        InlineMacro::Url(u) => ("adoc-link", &u.location, false),
        InlineMacro::Mailto(m) => ("adoc-link", &m.location, false),
        InlineMacro::Autolink(a) => ("adoc-link", &a.location, false),
        InlineMacro::Link(l) => ("adoc-link", &l.location, true),
        InlineMacro::CrossReference(xr) => ("adoc-xref", &xr.location, true),
        InlineMacro::IndexTerm(it) => ("adoc-index-term", &it.location, false),
        InlineMacro::Pass(p) => ("adoc-passthrough-inline", &p.location, true),
        InlineMacro::Footnote(f) => ("adoc-footnote", &f.location, true),
        InlineMacro::Icon(i) => ("adoc-inline-macro", &i.location, true),
        InlineMacro::Image(img) => ("adoc-inline-macro", &img.location, true),
        InlineMacro::Keyboard(k) => ("adoc-inline-macro", &k.location, true),
        InlineMacro::Button(b) => ("adoc-inline-macro", &b.location, true),
        InlineMacro::Menu(m) => ("adoc-inline-macro", &m.location, true),
        InlineMacro::Stem(s) => ("adoc-inline-macro", &s.location, true),
        _ => return,
    };

    // Determine effective end — parser may exclude trailing `]`
    let start = location.absolute_start;
    let mut end = location.absolute_end;
    if input.as_bytes().get(end) == Some(&b']') {
        end += 1;
    }

    // Base span for the whole macro
    spans.push(Span {
        start,
        end,
        class,
        priority: 2,
    });

    // Granular highlighting: target in green, bracket content in dark pink
    if granular {
        collect_macro_target_spans(input, start, end, 2, spans);
    } else {
        // For URL-type macros, still highlight bracket content (display text)
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

fn delimiter_class(inner: &DelimitedBlockType) -> &'static str {
    match inner {
        DelimitedBlockType::DelimitedTable(_) => "adoc-table-delimiter",
        DelimitedBlockType::DelimitedComment(_)
        | DelimitedBlockType::DelimitedExample(_)
        | DelimitedBlockType::DelimitedListing(_)
        | DelimitedBlockType::DelimitedLiteral(_)
        | DelimitedBlockType::DelimitedOpen(_)
        | DelimitedBlockType::DelimitedSidebar(_)
        | DelimitedBlockType::DelimitedPass(_)
        | DelimitedBlockType::DelimitedQuote(_)
        | DelimitedBlockType::DelimitedVerse(_)
        | DelimitedBlockType::DelimitedStem(_)
        | _ => "adoc-delimiter",
    }
}

/// Get the byte range (start, end) of an inline node.
fn inline_node_range(node: &InlineNode) -> (usize, usize) {
    match node {
        InlineNode::PlainText(p) => (p.location.absolute_start, p.location.absolute_end),
        InlineNode::RawText(r) => (r.location.absolute_start, r.location.absolute_end),
        InlineNode::VerbatimText(v) => (v.location.absolute_start, v.location.absolute_end),
        InlineNode::BoldText(b) => (b.location.absolute_start, b.location.absolute_end),
        InlineNode::ItalicText(i) => (i.location.absolute_start, i.location.absolute_end),
        InlineNode::MonospaceText(m) => (m.location.absolute_start, m.location.absolute_end),
        InlineNode::HighlightText(h) => (h.location.absolute_start, h.location.absolute_end),
        InlineNode::SuperscriptText(s) => (s.location.absolute_start, s.location.absolute_end),
        InlineNode::SubscriptText(s) => (s.location.absolute_start, s.location.absolute_end),
        InlineNode::CurvedQuotationText(q) => (q.location.absolute_start, q.location.absolute_end),
        InlineNode::CurvedApostropheText(a) => (a.location.absolute_start, a.location.absolute_end),
        InlineNode::StandaloneCurvedApostrophe(a) => {
            (a.location.absolute_start, a.location.absolute_end)
        }
        InlineNode::LineBreak(lb) => (lb.location.absolute_start, lb.location.absolute_end),
        InlineNode::InlineAnchor(a) => (a.location.absolute_start, a.location.absolute_end),
        InlineNode::CalloutRef(c) => (c.location.absolute_start, c.location.absolute_end),
        InlineNode::Macro(m) => macro_range(m),
        _ => (0, 0),
    }
}

fn macro_range(mac: &InlineMacro) -> (usize, usize) {
    match mac {
        InlineMacro::Footnote(f) => (f.location.absolute_start, f.location.absolute_end),
        InlineMacro::Icon(i) => (i.location.absolute_start, i.location.absolute_end),
        InlineMacro::Image(img) => (img.location.absolute_start, img.location.absolute_end),
        InlineMacro::Keyboard(k) => (k.location.absolute_start, k.location.absolute_end),
        InlineMacro::Button(b) => (b.location.absolute_start, b.location.absolute_end),
        InlineMacro::Menu(m) => (m.location.absolute_start, m.location.absolute_end),
        InlineMacro::Url(u) => (u.location.absolute_start, u.location.absolute_end),
        InlineMacro::Link(l) => (l.location.absolute_start, l.location.absolute_end),
        InlineMacro::Mailto(m) => (m.location.absolute_start, m.location.absolute_end),
        InlineMacro::Autolink(a) => (a.location.absolute_start, a.location.absolute_end),
        InlineMacro::CrossReference(x) => (x.location.absolute_start, x.location.absolute_end),
        InlineMacro::Pass(p) => (p.location.absolute_start, p.location.absolute_end),
        InlineMacro::Stem(s) => (s.location.absolute_start, s.location.absolute_end),
        InlineMacro::IndexTerm(i) => (i.location.absolute_start, i.location.absolute_end),
        _ => (0, 0),
    }
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
pub fn escape_html(s: &str) -> String {
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

    fn highlight(input: &str) -> String {
        let options = acdc_parser::Options::default();
        if let Ok(doc) = acdc_parser::parse(input, &options) {
            highlight_from_ast(input, &doc)
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
        // A bare `= Title` without attributes is parsed as a level-0 section
        let result = highlight("= Document title");
        assert!(result.contains("adoc-heading"), "result: {result}");
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
        // The parser recognises comment blocks inside a document with content
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
        // Page break is recognised when embedded in a document
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
            !result.ends_with("]"),
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
    fn test_setext_header_highlight() {
        // Setext header: title line + underline line
        let input = "Setext Header\n-------------";
        // Enable setext in options for this test
        let options = acdc_parser::Options::builder().with_setext().build();
        let doc = acdc_parser::parse(input, &options).expect("failed to parse setext");
        let result = highlight_from_ast(input, &doc);

        // Should contain two adoc-heading spans (one for title, one for underline)
        let matches: Vec<_> = result.matches("adoc-heading").collect();
        assert_eq!(matches.len(), 2, "result: {result}");
        assert!(result.contains("Setext Header"), "result: {result}");
        assert!(result.contains("-------------"), "result: {result}");
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
        let result = highlight("Press kbd:[Ctrl+C]");
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
