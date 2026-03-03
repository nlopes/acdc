use std::io::Write;

use acdc_converters_core::{
    code::{default_line_comment, detect_language},
    visitor::{WritableVisitor, WritableVisitorExt},
};
use acdc_parser::{
    AttributeValue, Block, BlockMetadata, DelimitedBlock, DelimitedBlockType, InlineNode,
    StemContent, StemNotation,
};

use crate::{
    Error, HtmlVariant, Processor, RenderOptions, build_class, write_attribution,
    write_semantic_attribution,
};

/// Write the opening `<div>` tag with optional ID and class attributes.
/// Follows the pattern used in lists: metadata.id takes precedence, fallback to anchors.
fn write_block_div_open<W: Write>(
    w: &mut W,
    metadata: &BlockMetadata,
    base_class: &str,
) -> Result<(), Error> {
    write!(w, "<div")?;
    if let Some(id) = &metadata.id {
        write!(w, " id=\"{}\"", id.id)?;
    } else if let Some(anchor) = metadata.anchors.first() {
        write!(w, " id=\"{}\"", anchor.id)?;
    }
    let class = build_class(base_class, &metadata.roles);
    writeln!(w, " class=\"{class}\">")?;
    Ok(())
}

/// Write an opening tag with optional ID, class (base + roles) for semantic mode.
fn write_semantic_tag_open<W: Write>(
    w: &mut W,
    tag: &str,
    metadata: &BlockMetadata,
    base_class: &str,
) -> Result<(), Error> {
    write!(w, "<{tag}")?;
    let class = build_class(base_class, &metadata.roles);
    write!(w, " class=\"{class}\"")?;
    if let Some(id) = &metadata.id {
        write!(w, " id=\"{}\"", id.id)?;
    } else if let Some(anchor) = metadata.anchors.first() {
        write!(w, " id=\"{}\"", anchor.id)?;
    }
    writeln!(w, ">")?;
    Ok(())
}

fn write_example_block<V: WritableVisitor<Error = Error>>(
    visitor: &mut V,
    block: &DelimitedBlock,
    processor: &Processor,
    blocks: &[Block],
) -> Result<(), Error> {
    let is_collapsible = block.metadata.options.contains(&"collapsible".to_string());

    if is_collapsible {
        return write_example_block_collapsible(visitor, block, blocks);
    }

    let mut writer = visitor.writer_mut();
    write_block_div_open(&mut writer, &block.metadata, "exampleblock")?;
    let _ = writer;

    // Render title with caption prefix if title exists
    // Caption can be disabled with :example-caption!:
    if !block.title.is_empty() {
        let prefix =
            processor.caption_prefix("example-caption", &processor.example_counter, "Example");
        visitor.render_title_with_wrapper(
            &block.title,
            &format!("<div class=\"title\">{prefix}"),
            "</div>\n",
        )?;
    }

    writer = visitor.writer_mut();
    writeln!(writer, "<div class=\"content\">")?;
    let _ = writer;
    for nested_block in blocks {
        visitor.visit_block(nested_block)?;
    }
    writer = visitor.writer_mut();
    writeln!(writer, "</div>")?;
    writeln!(writer, "</div>")?;
    Ok(())
}

fn write_example_block_collapsible<V: WritableVisitor<Error = Error>>(
    visitor: &mut V,
    block: &DelimitedBlock,
    blocks: &[Block],
) -> Result<(), Error> {
    let is_open = block.metadata.options.contains(&"open".to_string());

    let mut writer = visitor.writer_mut();
    write!(writer, "<details")?;
    if let Some(id) = &block.metadata.id {
        write!(writer, " id=\"{}\"", id.id)?;
    } else if let Some(anchor) = block.metadata.anchors.first() {
        write!(writer, " id=\"{}\"", anchor.id)?;
    }
    if is_open {
        writeln!(writer, " open>")?;
    } else {
        writeln!(writer, ">")?;
    }
    let _ = writer;

    if block.title.is_empty() {
        let writer = visitor.writer_mut();
        writeln!(writer, "<summary class=\"title\">Details</summary>")?;
    } else {
        visitor.render_title_with_wrapper(
            &block.title,
            "<summary class=\"title\">",
            "</summary>\n",
        )?;
    }

    writer = visitor.writer_mut();
    writeln!(writer, "<div class=\"content\">")?;
    let _ = writer;
    for nested_block in blocks {
        visitor.visit_block(nested_block)?;
    }
    writer = visitor.writer_mut();
    writeln!(writer, "</div>")?;
    writeln!(writer, "</details>")?;
    Ok(())
}

fn write_example_block_semantic<V: WritableVisitor<Error = Error>>(
    visitor: &mut V,
    block: &DelimitedBlock,
    processor: &Processor,
    blocks: &[Block],
) -> Result<(), Error> {
    let is_collapsible = block.metadata.options.contains(&"collapsible".to_string());
    let is_open = block.metadata.options.contains(&"open".to_string());

    let mut writer = visitor.writer_mut();
    if is_collapsible {
        // Collapsible: <details> with no class (unless id/roles)
        write!(writer, "<details")?;
        if !block.metadata.roles.is_empty() {
            write!(writer, " class=\"{}\"", block.metadata.roles.join(" "))?;
        }
        if let Some(id) = &block.metadata.id {
            write!(writer, " id=\"{}\"", id.id)?;
        } else if let Some(anchor) = block.metadata.anchors.first() {
            write!(writer, " id=\"{}\"", anchor.id)?;
        }
        if is_open {
            writeln!(writer, " open>")?;
        } else {
            writeln!(writer, ">")?;
        }
        let _ = writer;
        if !block.title.is_empty() {
            let prefix =
                processor.caption_prefix("example-caption", &processor.example_counter, "Example");
            visitor.render_title_with_wrapper(
                &block.title,
                &format!("<summary>{prefix}"),
                "</summary>\n",
            )?;
        }
        // Collapsible content wrapper
        writer = visitor.writer_mut();
        writeln!(writer, "<div class=\"content\">")?;
        let _ = writer;
        for nested_block in blocks {
            visitor.visit_block(nested_block)?;
        }
        writer = visitor.writer_mut();
        writeln!(writer, "</div>")?;
        writeln!(writer, "</details>")?;
    } else if !block.title.is_empty() {
        // Titled: use figure/figcaption with inner div.example
        write_semantic_tag_open(&mut writer, "figure", &block.metadata, "example-block")?;
        let _ = writer;
        let prefix =
            processor.caption_prefix("example-caption", &processor.example_counter, "Example");
        visitor.render_title_with_wrapper(
            &block.title,
            &format!("<figcaption>{prefix}"),
            "</figcaption>\n",
        )?;
        writer = visitor.writer_mut();
        writeln!(writer, "<div class=\"example\">")?;
        let _ = writer;
        for nested_block in blocks {
            visitor.visit_block(nested_block)?;
        }
        writer = visitor.writer_mut();
        writeln!(writer, "</div>")?;
        writeln!(writer, "</figure>")?;
    } else {
        // Untitled: use div with inner div.example
        write_semantic_tag_open(&mut writer, "div", &block.metadata, "example-block")?;
        writeln!(writer, "<div class=\"example\">")?;
        let _ = writer;
        for nested_block in blocks {
            visitor.visit_block(nested_block)?;
        }
        writer = visitor.writer_mut();
        writeln!(writer, "</div>")?;
        writeln!(writer, "</div>")?;
    }
    Ok(())
}

/// Visit a delimited block using the visitor pattern with ability to walk nested blocks
#[allow(clippy::too_many_lines)]
pub(crate) fn visit_delimited_block<V: WritableVisitor<Error = Error>>(
    visitor: &mut V,
    block: &DelimitedBlock,
    processor: &Processor,
    options: &RenderOptions,
) -> Result<(), Error> {
    match &block.inner {
        DelimitedBlockType::DelimitedQuote(blocks) => {
            if processor.variant() == HtmlVariant::Semantic {
                let has_title = !block.title.is_empty();
                let mut writer = visitor.writer_mut();
                if has_title {
                    write_semantic_tag_open(
                        &mut writer,
                        "section",
                        &block.metadata,
                        "quote-block",
                    )?;
                    let _ = writer;
                    visitor.render_title_with_wrapper(
                        &block.title,
                        "<h6 class=\"block-title\">",
                        "</h6>\n",
                    )?;
                    writer = visitor.writer_mut();
                } else {
                    write_semantic_tag_open(&mut writer, "div", &block.metadata, "quote-block")?;
                }
                writeln!(writer, "<blockquote>")?;
                let _ = writer;
                for nested_block in blocks {
                    visitor.visit_block(nested_block)?;
                }
                let _ = visitor.writer_mut();
                // Attribution goes inside blockquote as <footer>
                write_semantic_attribution(visitor, &block.metadata)?;
                let writer = visitor.writer_mut();
                writeln!(writer, "</blockquote>")?;
                if has_title {
                    writeln!(writer, "</section>")?;
                } else {
                    writeln!(writer, "</div>")?;
                }
            } else {
                let mut writer = visitor.writer_mut();
                let base_class = if let Some(style) = &block.metadata.style {
                    format!("{style}block")
                } else {
                    "quoteblock".to_string()
                };
                write_block_div_open(&mut writer, &block.metadata, &base_class)?;
                writeln!(writer, "<blockquote>")?;
                let _ = writer;
                for nested_block in blocks {
                    visitor.visit_block(nested_block)?;
                }
                let writer = visitor.writer_mut();
                writeln!(writer, "</blockquote>")?;
                let _ = writer;
                write_attribution(visitor, &block.metadata)?;
                let writer = visitor.writer_mut();
                writeln!(writer, "</div>")?;
            }
        }
        DelimitedBlockType::DelimitedOpen(blocks) => {
            if processor.variant() == HtmlVariant::Semantic {
                let is_abstract = block.metadata.style.as_deref() == Some("abstract");
                if is_abstract {
                    // Abstract style: render as quote-block abstract
                    let has_title = !block.title.is_empty();
                    let mut writer = visitor.writer_mut();
                    if has_title {
                        let base = build_class("quote-block abstract", &block.metadata.roles);
                        write!(writer, "<section class=\"{base}\"")?;
                        if let Some(id) = &block.metadata.id {
                            write!(writer, " id=\"{}\"", id.id)?;
                        } else if let Some(anchor) = block.metadata.anchors.first() {
                            write!(writer, " id=\"{}\"", anchor.id)?;
                        }
                        writeln!(writer, ">")?;
                        let _ = writer;
                        visitor.render_title_with_wrapper(
                            &block.title,
                            "<h6 class=\"block-title\">",
                            "</h6>\n",
                        )?;
                        writer = visitor.writer_mut();
                    } else {
                        write_semantic_tag_open(
                            &mut writer,
                            "div",
                            &block.metadata,
                            "quote-block abstract",
                        )?;
                    }
                    writeln!(writer, "<blockquote>")?;
                    let _ = writer;
                    for nested_block in blocks {
                        visitor.visit_block(nested_block)?;
                    }
                    writer = visitor.writer_mut();
                    writeln!(writer, "</blockquote>")?;
                    if has_title {
                        writeln!(writer, "</section>")?;
                    } else {
                        writeln!(writer, "</div>")?;
                    }
                } else {
                    // Regular open block in semantic mode
                    let has_title = !block.title.is_empty();
                    let mut writer = visitor.writer_mut();
                    if has_title {
                        write_semantic_tag_open(
                            &mut writer,
                            "section",
                            &block.metadata,
                            "open-block",
                        )?;
                        let _ = writer;
                        visitor.render_title_with_wrapper(
                            &block.title,
                            "<h6 class=\"block-title\">",
                            "</h6>\n",
                        )?;
                    } else {
                        write_semantic_tag_open(&mut writer, "div", &block.metadata, "open-block")?;
                    }
                    writer = visitor.writer_mut();
                    writeln!(writer, "<div class=\"content\">")?;
                    let _ = writer;
                    for nested_block in blocks {
                        visitor.visit_block(nested_block)?;
                    }
                    writer = visitor.writer_mut();
                    writeln!(writer, "</div>")?;
                    if has_title {
                        writeln!(writer, "</section>")?;
                    } else {
                        writeln!(writer, "</div>")?;
                    }
                }
            } else {
                let mut writer = visitor.writer_mut();
                write_block_div_open(&mut writer, &block.metadata, "openblock")?;
                let _ = writer;
                visitor.render_title_with_wrapper(
                    &block.title,
                    "<div class=\"title\">",
                    "</div>\n",
                )?;
                writer = visitor.writer_mut();
                writeln!(writer, "<div class=\"content\">")?;
                let _ = writer;
                for nested_block in blocks {
                    visitor.visit_block(nested_block)?;
                }
                writer = visitor.writer_mut();
                writeln!(writer, "</div>")?;
                writeln!(writer, "</div>")?;
            }
        }
        DelimitedBlockType::DelimitedExample(blocks) => {
            if processor.variant() == HtmlVariant::Semantic {
                write_example_block_semantic(visitor, block, processor, blocks)?;
            } else {
                write_example_block(visitor, block, processor, blocks)?;
            }
        }
        DelimitedBlockType::DelimitedSidebar(blocks) => {
            if processor.variant() == HtmlVariant::Semantic {
                let mut writer = visitor.writer_mut();
                write_semantic_tag_open(&mut writer, "aside", &block.metadata, "sidebar")?;
                let _ = writer;
                visitor.render_title_with_wrapper(
                    &block.title,
                    "<h6 class=\"block-title\">",
                    "</h6>\n",
                )?;
                for nested_block in blocks {
                    visitor.visit_block(nested_block)?;
                }
                let writer = visitor.writer_mut();
                writeln!(writer, "</aside>")?;
            } else {
                let mut writer = visitor.writer_mut();
                write_block_div_open(&mut writer, &block.metadata, "sidebarblock")?;
                writeln!(writer, "<div class=\"content\">")?;
                let _ = writer;
                visitor.render_title_with_wrapper(
                    &block.title,
                    "<div class=\"title\">",
                    "</div>\n",
                )?;
                writer = visitor.writer_mut();
                let _ = writer;
                for nested_block in blocks {
                    visitor.visit_block(nested_block)?;
                }
                writer = visitor.writer_mut();
                writeln!(writer, "</div>")?;
                writeln!(writer, "</div>")?;
            }
        }
        // Handle tables
        DelimitedBlockType::DelimitedTable(t) => {
            crate::table::render_table(
                t,
                visitor,
                processor,
                options,
                &block.metadata,
                &block.title,
            )?;
        }
        // Verse, literal, and stem blocks need semantic handling
        DelimitedBlockType::DelimitedVerse(_)
        | DelimitedBlockType::DelimitedLiteral(_)
        | DelimitedBlockType::DelimitedStem(_)
            if processor.variant() == HtmlVariant::Semantic =>
        {
            render_delimited_block_inner_semantic(
                &block.inner,
                &block.title,
                &block.metadata,
                visitor,
                processor,
            )?;
        }
        // For other block types, use the regular rendering
        DelimitedBlockType::DelimitedPass(_)
        | DelimitedBlockType::DelimitedListing(_)
        | DelimitedBlockType::DelimitedLiteral(_)
        | DelimitedBlockType::DelimitedStem(_)
        | DelimitedBlockType::DelimitedComment(_)
        | DelimitedBlockType::DelimitedVerse(_)
        | _ => {
            render_delimited_block_inner(
                &block.inner,
                &block.title,
                &block.metadata,
                visitor,
                processor,
                options,
            )?;
        }
    }
    Ok(())
}

/// Strip comment guard prefix from a `VerbatimText` node's content.
/// The guard is the comment prefix that appears before a callout marker.
fn strip_callout_comment_guard(text: &str, comment_prefix: Option<&str>) -> String {
    let Some(prefix) = comment_prefix else {
        return text.to_string();
    };

    // The comment guard appears at the end of the line, just before the callout
    // e.g., "let x = 1; // " -> "let x = 1; "
    let trimmed = text.trim_end();
    if let Some(stripped) = trimmed.strip_suffix(prefix) {
        // Return the text without the comment prefix
        stripped.trim_end().to_string() + " "
    } else {
        text.to_string()
    }
}

/// Process inlines to strip comment guards from `VerbatimText` nodes that precede `CalloutRef` nodes.
fn process_callout_guards(inlines: &[InlineNode], comment_prefix: Option<&str>) -> Vec<InlineNode> {
    let mut result = Vec::with_capacity(inlines.len());

    for (i, node) in inlines.iter().enumerate() {
        // Check if this VerbatimText is followed by a CalloutRef
        let next_is_callout = inlines
            .get(i + 1)
            .is_some_and(|n| matches!(n, InlineNode::CalloutRef(_)));

        if let InlineNode::VerbatimText(v) = node {
            if next_is_callout {
                // Strip the comment guard from this VerbatimText
                let stripped_content = strip_callout_comment_guard(&v.content, comment_prefix);
                result.push(InlineNode::VerbatimText(acdc_parser::Verbatim {
                    content: stripped_content,
                    location: v.location.clone(),
                }));
            } else {
                result.push(node.clone());
            }
        } else {
            result.push(node.clone());
        }
    }

    result
}

fn render_listing_code<V: WritableVisitor<Error = Error>>(
    inlines: &[InlineNode],
    metadata: &BlockMetadata,
    visitor: &mut V,
    processor: &Processor,
) -> Result<(), Error> {
    let language = detect_language(metadata);

    #[cfg(feature = "highlighting")]
    let highlighting_enabled = processor
        .document_attributes
        .get("source-highlighter")
        .is_some_and(|v| !matches!(v, AttributeValue::Bool(false)));

    let comment_prefix = default_line_comment(language);
    let processed_inlines = process_callout_guards(inlines, comment_prefix);

    let mut w = visitor.writer_mut();

    #[cfg(feature = "highlighting")]
    if let Some(lang) = language {
        if highlighting_enabled {
            write!(
                w,
                "<pre class=\"highlight\"><code class=\"language-{lang}\" data-lang=\"{lang}\">"
            )?;
            let _ = w;
            let (theme_name, mode) = crate::resolve_highlight_settings(processor);
            crate::syntax::highlight_code(
                visitor.writer_mut(),
                &processed_inlines,
                lang,
                &theme_name,
                mode,
            )?;
            w = visitor.writer_mut();
            writeln!(w, "</code></pre>")?;
        } else {
            write!(
                w,
                "<pre class=\"highlight\"><code class=\"language-{lang}\" data-lang=\"{lang}\">"
            )?;
            let _ = w;
            visitor.visit_inline_nodes(&processed_inlines)?;
            w = visitor.writer_mut();
            writeln!(w, "</code></pre>")?;
        }
    } else {
        write!(w, "<pre>")?;
        let _ = w;
        visitor.visit_inline_nodes(&processed_inlines)?;
        w = visitor.writer_mut();
        writeln!(w, "</pre>")?;
    }

    #[cfg(not(feature = "highlighting"))]
    {
        let _ = processor;
        if let Some(lang) = language {
            write!(
                w,
                "<pre class=\"highlight\"><code class=\"language-{lang}\" data-lang=\"{lang}\">"
            )?;
            let _ = w;
            visitor.visit_inline_nodes(&processed_inlines)?;
            w = visitor.writer_mut();
            writeln!(w, "</code></pre>")?;
        } else {
            write!(w, "<pre>")?;
            let _ = w;
            visitor.visit_inline_nodes(&processed_inlines)?;
            w = visitor.writer_mut();
            writeln!(w, "</pre>")?;
        }
    }

    Ok(())
}

fn render_listing_block<V: WritableVisitor<Error = Error>>(
    inlines: &[InlineNode],
    title: &[InlineNode],
    metadata: &BlockMetadata,
    visitor: &mut V,
    processor: &Processor,
) -> Result<(), Error> {
    if processor.variant() == HtmlVariant::Semantic {
        return render_listing_block_semantic(inlines, title, metadata, visitor, processor);
    }

    let mut w = visitor.writer_mut();
    write_block_div_open(&mut w, metadata, "listingblock")?;
    let _ = w;

    // Check if listing-caption is set and block has a title
    if !title.is_empty() {
        if let Some(AttributeValue::String(caption)) =
            processor.document_attributes.get("listing-caption")
        {
            let count = processor.listing_counter.get() + 1;
            processor.listing_counter.set(count);
            visitor.render_title_with_wrapper(
                title,
                &format!("<div class=\"title\">{caption} {count}. "),
                "</div>\n",
            )?;
        } else {
            // No listing-caption, render title without numbering
            visitor.render_title_with_wrapper(title, "<div class=\"title\">", "</div>\n")?;
        }
    }

    let w = visitor.writer_mut();
    writeln!(w, "<div class=\"content\">")?;
    let _ = w;

    render_listing_code(inlines, metadata, visitor, processor)?;

    let w = visitor.writer_mut();
    writeln!(w, "</div>")?;
    writeln!(w, "</div>")?;
    Ok(())
}

fn render_listing_block_semantic<V: WritableVisitor<Error = Error>>(
    inlines: &[InlineNode],
    title: &[InlineNode],
    metadata: &BlockMetadata,
    visitor: &mut V,
    processor: &Processor,
) -> Result<(), Error> {
    let mut w = visitor.writer_mut();
    if title.is_empty() {
        // Untitled: use div
        write_semantic_tag_open(&mut w, "div", metadata, "listing-block")?;
        let _ = w;
        render_listing_code(inlines, metadata, visitor, processor)?;
        let w = visitor.writer_mut();
        writeln!(w, "</div>")?;
    } else {
        // Titled: use figure/figcaption
        write_semantic_tag_open(&mut w, "figure", metadata, "listing-block")?;
        let _ = w;
        visitor.render_title_with_wrapper(title, "<figcaption>", "</figcaption>\n")?;
        render_listing_code(inlines, metadata, visitor, processor)?;
        let w = visitor.writer_mut();
        writeln!(w, "</figure>")?;
    }
    Ok(())
}

fn render_delimited_block_inner<V: WritableVisitor<Error = Error>>(
    inner: &DelimitedBlockType,
    title: &[InlineNode],
    metadata: &BlockMetadata,
    visitor: &mut V,
    processor: &Processor,
    _options: &RenderOptions,
) -> Result<(), Error> {
    match inner {
        DelimitedBlockType::DelimitedPass(inlines) => {
            visitor.visit_inline_nodes(inlines)?;
        }
        DelimitedBlockType::DelimitedListing(inlines) => {
            render_listing_block(inlines, title, metadata, visitor, processor)?;
        }
        DelimitedBlockType::DelimitedLiteral(inlines) => {
            // Check for custom style other than "source" - I've done this because
            // `asciidoctor` seems to always use "literalblock" for source blocks or
            // so I think!
            let mut w = visitor.writer_mut();
            let base_class = if let Some(style) = &metadata.style
                && style != "source"
            {
                format!("{style}block")
            } else {
                "literalblock".to_string()
            };
            write_block_div_open(&mut w, metadata, &base_class)?;
            let _ = w;
            visitor.render_title_with_wrapper(title, "<div class=\"title\">", "</div>\n")?;
            w = visitor.writer_mut();
            writeln!(w, "<div class=\"content\">")?;
            write!(w, "<pre>")?;
            let _ = w;
            visitor.visit_inline_nodes(inlines)?;
            w = visitor.writer_mut();
            writeln!(w, "</pre>")?;
            writeln!(w, "</div>")?;
            writeln!(w, "</div>")?;
        }
        DelimitedBlockType::DelimitedStem(stem) => {
            let mut w = visitor.writer_mut();
            write_block_div_open(&mut w, metadata, "stemblock")?;
            let _ = w;
            visitor.render_title_with_wrapper(title, "<div class=\"title\">", "</div>\n")?;
            w = visitor.writer_mut();
            render_stem_content(stem, w, processor)?;
            writeln!(w, "</div>")?;
        }
        DelimitedBlockType::DelimitedComment(_) => {
            // Comment blocks produce no output
        }
        DelimitedBlockType::DelimitedVerse(inlines) => {
            let mut w = visitor.writer_mut();
            write_block_div_open(&mut w, metadata, "verseblock")?;
            let _ = w;
            visitor.render_title_with_wrapper(title, "<div class=\"title\">", "</div>\n")?;
            w = visitor.writer_mut();
            write!(w, "<pre class=\"content\">")?;
            let _ = w;
            visitor.visit_inline_nodes(inlines)?;
            w = visitor.writer_mut();
            writeln!(w, "</pre>")?;
            let _ = w;
            write_attribution(visitor, metadata)?;
            w = visitor.writer_mut();
            writeln!(w, "</div>")?;
        }
        DelimitedBlockType::DelimitedQuote(_)
        | DelimitedBlockType::DelimitedOpen(_)
        | DelimitedBlockType::DelimitedExample(_)
        | DelimitedBlockType::DelimitedSidebar(_)
        | DelimitedBlockType::DelimitedTable(_)
        | _ => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                format!("Unsupported delimited block type: {inner:?}"),
            )
            .into());
        }
    }
    Ok(())
}

fn render_stem_content<W: Write + ?Sized>(
    stem: &StemContent,
    w: &mut W,
    processor: &Processor,
) -> Result<(), Error> {
    let forced = if processor.variant() == HtmlVariant::Semantic {
        processor
            .document_attributes()
            .get("html5s-force-stem-type")
            .and_then(|v| v.to_string().parse::<StemNotation>().ok())
    } else {
        None
    };
    let notation = forced.as_ref().unwrap_or(&stem.notation);
    writeln!(w, "<div class=\"content\">")?;
    match notation {
        StemNotation::Latexmath => {
            write!(w, "\\[{}\\]", stem.content)?;
        }
        StemNotation::Asciimath => {
            write!(w, "\\${}\\$", stem.content)?;
        }
    }
    writeln!(w, "</div>")?;
    Ok(())
}

/// Render stem content in semantic mode with `<div class="math" data-lang="...">`.
fn render_stem_content_semantic<W: Write + ?Sized>(
    stem: &StemContent,
    w: &mut W,
    processor: &Processor,
) -> Result<(), Error> {
    let forced = processor
        .document_attributes()
        .get("html5s-force-stem-type")
        .and_then(|v| v.to_string().parse::<StemNotation>().ok());
    let notation = forced.as_ref().unwrap_or(&stem.notation);
    let data_lang = match notation {
        StemNotation::Latexmath => "tex",
        StemNotation::Asciimath => "asciimath",
    };
    write!(w, "<div class=\"math\" data-lang=\"{data_lang}\">")?;
    match notation {
        StemNotation::Latexmath => {
            write!(w, "\\[{}\\]", stem.content)?;
        }
        StemNotation::Asciimath => {
            write!(w, "\\${}\\$", stem.content)?;
        }
    }
    writeln!(w, "</div>")?;
    Ok(())
}

/// Render verse, literal, and stem blocks in semantic HTML5 mode.
fn render_delimited_block_inner_semantic<V: WritableVisitor<Error = Error>>(
    inner: &DelimitedBlockType,
    title: &[InlineNode],
    metadata: &BlockMetadata,
    visitor: &mut V,
    processor: &Processor,
) -> Result<(), Error> {
    match inner {
        DelimitedBlockType::DelimitedVerse(inlines) => {
            render_verse_block_semantic(inlines, title, metadata, visitor)?;
        }
        DelimitedBlockType::DelimitedLiteral(inlines) => {
            let has_title = !title.is_empty();
            let mut w = visitor.writer_mut();
            if has_title {
                write_semantic_tag_open(&mut w, "section", metadata, "literal-block")?;
                let _ = w;
                visitor.render_title_with_wrapper(
                    title,
                    "<h6 class=\"block-title\">",
                    "</h6>\n",
                )?;
                w = visitor.writer_mut();
            } else {
                write_semantic_tag_open(&mut w, "div", metadata, "literal-block")?;
            }
            write!(w, "<pre>")?;
            let _ = w;
            visitor.visit_inline_nodes(inlines)?;
            w = visitor.writer_mut();
            writeln!(w, "</pre>")?;
            if has_title {
                writeln!(w, "</section>")?;
            } else {
                writeln!(w, "</div>")?;
            }
        }
        DelimitedBlockType::DelimitedStem(stem) => {
            let has_title = !title.is_empty();
            let mut w = visitor.writer_mut();
            if has_title {
                write_semantic_tag_open(&mut w, "figure", metadata, "stem-block")?;
                let _ = w;
                visitor.render_title_with_wrapper(title, "<figcaption>", "</figcaption>\n")?;
                w = visitor.writer_mut();
            } else {
                write_semantic_tag_open(&mut w, "div", metadata, "stem-block")?;
            }
            render_stem_content_semantic(stem, w, processor)?;
            w = visitor.writer_mut();
            if has_title {
                writeln!(w, "</figure>")?;
            } else {
                writeln!(w, "</div>")?;
            }
        }
        DelimitedBlockType::DelimitedComment(_)
        | DelimitedBlockType::DelimitedExample(_)
        | DelimitedBlockType::DelimitedListing(_)
        | DelimitedBlockType::DelimitedOpen(_)
        | DelimitedBlockType::DelimitedSidebar(_)
        | DelimitedBlockType::DelimitedTable(_)
        | DelimitedBlockType::DelimitedPass(_)
        | DelimitedBlockType::DelimitedQuote(_)
        | _ => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                format!("Unsupported delimited block type for semantic rendering: {inner:?}"),
            )
            .into());
        }
    }
    Ok(())
}

fn render_verse_block_semantic<V: WritableVisitor<Error = Error>>(
    inlines: &[InlineNode],
    title: &[InlineNode],
    metadata: &BlockMetadata,
    visitor: &mut V,
) -> Result<(), Error> {
    let has_title = !title.is_empty();
    let has_attribution = metadata.attribution.as_ref().is_some_and(|a| !a.is_empty())
        || metadata.citetitle.as_ref().is_some_and(|c| !c.is_empty());

    let mut w = visitor.writer_mut();
    if has_title {
        write_semantic_tag_open(&mut w, "section", metadata, "verse-block")?;
        let _ = w;
        visitor.render_title_with_wrapper(title, "<h6 class=\"block-title\">", "</h6>\n")?;
        w = visitor.writer_mut();
    } else {
        write_semantic_tag_open(&mut w, "div", metadata, "verse-block")?;
    }

    if has_attribution {
        writeln!(w, "<blockquote class=\"verse\">")?;
        write!(w, "<pre class=\"verse\">")?;
        let _ = w;
        visitor.visit_inline_nodes(inlines)?;
        w = visitor.writer_mut();
        writeln!(w, "</pre>")?;
        let _ = w;
        write_semantic_attribution(visitor, metadata)?;
        w = visitor.writer_mut();
        writeln!(w, "</blockquote>")?;
    } else {
        write!(w, "<pre class=\"verse\">")?;
        let _ = w;
        visitor.visit_inline_nodes(inlines)?;
        w = visitor.writer_mut();
        writeln!(w, "</pre>")?;
    }

    if has_title {
        writeln!(w, "</section>")?;
    } else {
        writeln!(w, "</div>")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::{cell::Cell, rc::Rc};

    use acdc_converters_core::{Doctype, Options, visitor::Visitor};
    use acdc_parser::{
        BlockMetadata, DocumentAttributes, InlineNode, Location, Plain, SafeMode, Title,
    };

    use crate::{AppendixTracker, PartNumberTracker, SectionNumberTracker};

    fn create_test_inlines(content: &str) -> Vec<InlineNode> {
        vec![InlineNode::PlainText(Plain {
            content: content.to_string(),
            location: Location::default(),
            escaped: false,
        })]
    }

    fn create_test_processor() -> Processor {
        let options = Options::builder()
            .doctype(Doctype::Article)
            .safe_mode(SafeMode::Unsafe)
            .build();
        let document_attributes = DocumentAttributes::default();
        let section_number_tracker = SectionNumberTracker::new(&document_attributes);
        let part_number_tracker =
            PartNumberTracker::new(&document_attributes, section_number_tracker.clone());
        let appendix_tracker =
            AppendixTracker::new(&document_attributes, section_number_tracker.clone());
        Processor {
            options,
            document_attributes,
            toc_entries: Vec::new(),
            example_counter: Rc::new(Cell::new(0)),
            table_counter: Rc::new(Cell::new(0)),
            figure_counter: Rc::new(Cell::new(0)),
            listing_counter: Rc::new(Cell::new(0)),
            index_term_counter: Rc::new(Cell::new(0)),
            index_entries: Rc::new(std::cell::RefCell::new(Vec::new())),
            has_valid_index_section: false,
            section_number_tracker,
            part_number_tracker,
            appendix_tracker,
            variant: crate::HtmlVariant::Standard,
        }
    }

    #[test]
    fn test_listing_block_renders_as_listingblock() -> Result<(), Error> {
        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedListing(create_test_inlines("code here")),
            "----".to_string(),
            Location::default(),
        );

        let output = Vec::new();
        let processor = create_test_processor();
        let options = RenderOptions::default();
        let mut visitor = crate::HtmlVisitor::new(output, processor, options);

        visitor.visit_delimited_block(&block)?;
        let html = String::from_utf8(visitor.into_writer())?;

        assert!(
            html.contains("<div class=\"listingblock\">"),
            "Expected listing block to render with 'listingblock' class, got: {html}",
        );
        Ok(())
    }

    #[test]
    fn test_literal_block_renders_as_literalblock() -> Result<(), Error> {
        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedLiteral(create_test_inlines("literal text")),
            "....".to_string(),
            Location::default(),
        );

        let output = Vec::new();
        let processor = create_test_processor();
        let options = RenderOptions::default();
        let mut visitor = crate::HtmlVisitor::new(output, processor, options);

        visitor.visit_delimited_block(&block)?;
        let html = String::from_utf8(visitor.into_writer())?;

        assert!(
            html.contains("<div class=\"literalblock\">"),
            "Expected literal block to render with 'literalblock' class, got: {html}",
        );
        Ok(())
    }

    #[test]
    fn test_listing_block_with_source_style_and_language() -> Result<(), Error> {
        use acdc_parser::{AttributeValue, ElementAttributes};

        let mut attributes = ElementAttributes::default();
        attributes.insert("bash".to_string(), AttributeValue::None);

        let metadata = BlockMetadata::new()
            .with_style(Some("source".to_string()))
            .with_attributes(attributes);

        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedListing(create_test_inlines("code here")),
            "----".to_string(),
            Location::default(),
        )
        .with_metadata(metadata);

        let output = Vec::new();
        let processor = create_test_processor();
        let options = RenderOptions::default();
        let mut visitor = crate::HtmlVisitor::new(output, processor, options);

        visitor.visit_delimited_block(&block)?;
        let html = String::from_utf8(visitor.into_writer())?;

        assert!(
            html.contains("<div class=\"listingblock\">"),
            "Expected listing block to always use 'listingblock' class, got: {html}",
        );
        assert!(
            html.contains("<pre class=\"highlight\">"),
            "Expected source block to have 'highlight' class on pre tag, got: {html}",
        );
        assert!(
            html.contains("<code class=\"language-bash\" data-lang=\"bash\">"),
            "Expected source block to have language attributes, got: {html}",
        );
        Ok(())
    }

    #[test]
    fn test_literal_block_with_style_attribute() -> Result<(), Error> {
        let metadata = BlockMetadata::new().with_style(Some("verse".to_string()));

        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedLiteral(create_test_inlines("literal text")),
            "....".to_string(),
            Location::default(),
        )
        .with_metadata(metadata);

        let output = Vec::new();
        let processor = create_test_processor();
        let options = RenderOptions::default();
        let mut visitor = crate::HtmlVisitor::new(output, processor, options);

        visitor.visit_delimited_block(&block)?;
        let html = String::from_utf8(visitor.into_writer())?;

        assert!(
            html.contains("<div class=\"verseblock\">"),
            "Expected literal block with style to render with custom class, got: {html}",
        );
        assert!(
            !html.contains("<div class=\"literalblock\">"),
            "Literal block with style should not have default 'literalblock' class"
        );
        Ok(())
    }

    #[test]
    fn test_listing_block_without_listing_caption_renders_title_without_number() -> Result<(), Error>
    {
        let title = Title::new(vec![InlineNode::PlainText(Plain {
            content: "My Code Example".to_string(),
            location: Location::default(),
            escaped: false,
        })]);

        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedListing(create_test_inlines("code here")),
            "----".to_string(),
            Location::default(),
        )
        .with_title(title);

        let output = Vec::new();
        let processor = create_test_processor();
        let options = RenderOptions::default();
        let mut visitor = crate::HtmlVisitor::new(output, processor.clone(), options);

        visitor.visit_delimited_block(&block)?;
        let html = String::from_utf8(visitor.into_writer())?;

        assert!(
            html.contains("<div class=\"title\">My Code Example</div>"),
            "Expected title without numbering when listing-caption is not set, got: {html}",
        );
        assert!(
            !html.contains("Listing 1"),
            "Should not contain numbering when listing-caption is not set, got: {html}",
        );
        assert_eq!(
            processor.listing_counter.get(),
            0,
            "Counter should not be incremented when listing-caption is not set"
        );
        Ok(())
    }

    #[test]
    fn test_listing_block_with_listing_caption_renders_title_with_number() -> Result<(), Error> {
        use acdc_parser::AttributeValue;

        let title1 = Title::new(vec![InlineNode::PlainText(Plain {
            content: "First Example".to_string(),
            location: Location::default(),
            escaped: false,
        })]);

        let title2 = Title::new(vec![InlineNode::PlainText(Plain {
            content: "Second Example".to_string(),
            location: Location::default(),
            escaped: false,
        })]);

        let block1 = DelimitedBlock::new(
            DelimitedBlockType::DelimitedListing(create_test_inlines("code 1")),
            "----".to_string(),
            Location::default(),
        )
        .with_title(title1);

        let block2 = DelimitedBlock::new(
            DelimitedBlockType::DelimitedListing(create_test_inlines("code 2")),
            "----".to_string(),
            Location::default(),
        )
        .with_title(title2);

        let output = Vec::new();
        let mut processor = create_test_processor();
        // Set listing-caption attribute
        processor.document_attributes.set(
            "listing-caption".to_string(),
            AttributeValue::String("Listing".to_string()),
        );

        let options = RenderOptions::default();
        let mut visitor = crate::HtmlVisitor::new(output, processor.clone(), options.clone());

        // Render first block
        visitor.visit_delimited_block(&block1)?;
        let html1 = String::from_utf8(visitor.into_writer())?;

        assert!(
            html1.contains("<div class=\"title\">Listing 1. First Example</div>"),
            "Expected numbered title for first listing block, got: {html1}",
        );
        assert_eq!(
            processor.listing_counter.get(),
            1,
            "Counter should be incremented to 1"
        );

        // Render second block
        let output2 = Vec::new();
        let mut visitor2 = crate::HtmlVisitor::new(output2, processor.clone(), options);
        visitor2.visit_delimited_block(&block2)?;
        let html2 = String::from_utf8(visitor2.into_writer())?;

        assert!(
            html2.contains("<div class=\"title\">Listing 2. Second Example</div>"),
            "Expected numbered title for second listing block, got: {html2}",
        );
        assert_eq!(
            processor.listing_counter.get(),
            2,
            "Counter should be incremented to 2"
        );
        Ok(())
    }

    #[test]
    fn test_listing_block_with_id_and_role() -> Result<(), Error> {
        use acdc_parser::Anchor;

        let metadata = BlockMetadata::new()
            .with_id(Some(Anchor::new(
                "my-listing-id".to_string(),
                Location::default(),
            )))
            .with_roles(vec!["highlight".to_string(), "special".to_string()]);

        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedListing(create_test_inlines("code here")),
            "----".to_string(),
            Location::default(),
        )
        .with_metadata(metadata);

        let output = Vec::new();
        let processor = create_test_processor();
        let options = RenderOptions::default();
        let mut visitor = crate::HtmlVisitor::new(output, processor, options);

        visitor.visit_delimited_block(&block)?;
        let html = String::from_utf8(visitor.into_writer())?;

        assert!(
            html.contains("<div id=\"my-listing-id\" class=\"listingblock highlight special\">"),
            "Expected listing block with ID and roles, got: {html}",
        );
        Ok(())
    }

    #[test]
    fn test_example_block_with_id_and_role() -> Result<(), Error> {
        use acdc_parser::Anchor;

        let metadata = BlockMetadata::new()
            .with_id(Some(Anchor::new(
                "example-id".to_string(),
                Location::default(),
            )))
            .with_roles(vec!["special".to_string()]);

        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedExample(vec![]),
            "====".to_string(),
            Location::default(),
        )
        .with_metadata(metadata);

        let output = Vec::new();
        let processor = create_test_processor();
        let options = RenderOptions::default();
        let mut visitor = crate::HtmlVisitor::new(output, processor, options);

        visitor.visit_delimited_block(&block)?;
        let html = String::from_utf8(visitor.into_writer())?;

        assert!(
            html.contains("<div id=\"example-id\" class=\"exampleblock special\">"),
            "Expected example block with ID and role, got: {html}",
        );
        Ok(())
    }

    #[test]
    fn test_quote_block_with_id_and_role() -> Result<(), Error> {
        use acdc_parser::Anchor;

        let metadata = BlockMetadata::new()
            .with_id(Some(Anchor::new(
                "quote-id".to_string(),
                Location::default(),
            )))
            .with_roles(vec!["highlight".to_string()]);

        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedQuote(vec![]),
            "____".to_string(),
            Location::default(),
        )
        .with_metadata(metadata);

        let output = Vec::new();
        let processor = create_test_processor();
        let options = RenderOptions::default();
        let mut visitor = crate::HtmlVisitor::new(output, processor, options);

        visitor.visit_delimited_block(&block)?;
        let html = String::from_utf8(visitor.into_writer())?;

        assert!(
            html.contains("<div id=\"quote-id\" class=\"quoteblock highlight\">"),
            "Expected quote block with ID and role, got: {html}",
        );
        Ok(())
    }

    #[test]
    fn test_sidebar_block_with_id_and_role() -> Result<(), Error> {
        use acdc_parser::Anchor;

        let metadata = BlockMetadata::new()
            .with_id(Some(Anchor::new(
                "sidebar-id".to_string(),
                Location::default(),
            )))
            .with_roles(vec!["sidebar-role".to_string()]);

        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedSidebar(vec![]),
            "****".to_string(),
            Location::default(),
        )
        .with_metadata(metadata);

        let output = Vec::new();
        let processor = create_test_processor();
        let options = RenderOptions::default();
        let mut visitor = crate::HtmlVisitor::new(output, processor, options);

        visitor.visit_delimited_block(&block)?;
        let html = String::from_utf8(visitor.into_writer())?;

        assert!(
            html.contains("<div id=\"sidebar-id\" class=\"sidebarblock sidebar-role\">"),
            "Expected sidebar block with ID and role, got: {html}",
        );
        Ok(())
    }

    #[test]
    fn test_open_block_with_id_and_role() -> Result<(), Error> {
        use acdc_parser::Anchor;

        let metadata = BlockMetadata::new()
            .with_id(Some(Anchor::new(
                "open-id".to_string(),
                Location::default(),
            )))
            .with_roles(vec!["open-role".to_string()]);

        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedOpen(vec![]),
            "--".to_string(),
            Location::default(),
        )
        .with_metadata(metadata);

        let output = Vec::new();
        let processor = create_test_processor();
        let options = RenderOptions::default();
        let mut visitor = crate::HtmlVisitor::new(output, processor, options);

        visitor.visit_delimited_block(&block)?;
        let html = String::from_utf8(visitor.into_writer())?;

        assert!(
            html.contains("<div id=\"open-id\" class=\"openblock open-role\">"),
            "Expected open block with ID and role, got: {html}",
        );
        Ok(())
    }

    #[test]
    fn test_example_block_collapsible() -> Result<(), Error> {
        let title = Title::new(vec![InlineNode::PlainText(Plain {
            content: "Click to expand".to_string(),
            location: Location::default(),
            escaped: false,
        })]);

        let metadata = BlockMetadata::new().with_options(vec!["collapsible".to_string()]);

        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedExample(vec![]),
            "====".to_string(),
            Location::default(),
        )
        .with_metadata(metadata)
        .with_title(title);

        let output = Vec::new();
        let processor = create_test_processor();
        let options = RenderOptions::default();
        let mut visitor = crate::HtmlVisitor::new(output, processor, options);

        visitor.visit_delimited_block(&block)?;
        let html = String::from_utf8(visitor.into_writer())?;

        assert!(
            html.contains("<details>"),
            "Collapsible block should render as <details>, got: {html}",
        );
        assert!(
            html.contains("<summary class=\"title\">"),
            "Collapsible block should have <summary class=\"title\">, got: {html}",
        );
        assert!(
            !html.contains("<div class=\"exampleblock\">"),
            "Collapsible block should not have exampleblock div, got: {html}",
        );
        assert!(
            html.contains("</details>"),
            "Collapsible block should close with </details>, got: {html}",
        );
        Ok(())
    }

    #[test]
    fn test_example_block_collapsible_open() -> Result<(), Error> {
        let title = Title::new(vec![InlineNode::PlainText(Plain {
            content: "Initially open".to_string(),
            location: Location::default(),
            escaped: false,
        })]);

        let metadata =
            BlockMetadata::new().with_options(vec!["collapsible".to_string(), "open".to_string()]);

        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedExample(vec![]),
            "====".to_string(),
            Location::default(),
        )
        .with_metadata(metadata)
        .with_title(title);

        let output = Vec::new();
        let processor = create_test_processor();
        let options = RenderOptions::default();
        let mut visitor = crate::HtmlVisitor::new(output, processor, options);

        visitor.visit_delimited_block(&block)?;
        let html = String::from_utf8(visitor.into_writer())?;

        assert!(
            html.contains("<details open>"),
            "Collapsible open block should have open attribute, got: {html}",
        );
        Ok(())
    }

    #[test]
    fn test_example_block_collapsible_without_title() -> Result<(), Error> {
        let metadata = BlockMetadata::new().with_options(vec!["collapsible".to_string()]);

        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedExample(vec![]),
            "====".to_string(),
            Location::default(),
        )
        .with_metadata(metadata);

        let output = Vec::new();
        let processor = create_test_processor();
        let options = RenderOptions::default();
        let mut visitor = crate::HtmlVisitor::new(output, processor, options);

        visitor.visit_delimited_block(&block)?;
        let html = String::from_utf8(visitor.into_writer())?;

        assert!(
            html.contains("<summary class=\"title\">Details</summary>"),
            "Collapsible block without title should use 'Details' as default summary, got: {html}",
        );
        Ok(())
    }

    #[test]
    fn test_block_with_anchor_fallback() -> Result<(), Error> {
        use acdc_parser::Anchor;

        // Test that anchors are used as fallback when id is None
        let mut metadata = BlockMetadata::new().with_roles(vec!["my-role".to_string()]);
        metadata.anchors = vec![Anchor::new(
            "anchor-fallback".to_string(),
            Location::default(),
        )];

        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedListing(create_test_inlines("code")),
            "----".to_string(),
            Location::default(),
        )
        .with_metadata(metadata);

        let output = Vec::new();
        let processor = create_test_processor();
        let options = RenderOptions::default();
        let mut visitor = crate::HtmlVisitor::new(output, processor, options);

        visitor.visit_delimited_block(&block)?;
        let html = String::from_utf8(visitor.into_writer())?;

        assert!(
            html.contains("<div id=\"anchor-fallback\" class=\"listingblock my-role\">"),
            "Expected listing block with anchor fallback ID, got: {html}",
        );
        Ok(())
    }
}
