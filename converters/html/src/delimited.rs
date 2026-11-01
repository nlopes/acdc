use std::{borrow::Cow, io::Write};

use acdc_converters_core::{TraversalContext, code::source_language};

#[cfg(not(feature = "pre-spec-subs"))]
use acdc_converters_core::substitutions::baseline_subs;
#[cfg(feature = "pre-spec-subs")]
use acdc_converters_core::substitutions::{effective_subs, substitute_attributes};

use acdc_parser::{
    Block, BlockMetadata, CaptionKind, DelimitedBlock, DelimitedBlockType, InlineNode,
    StemNotation,
    ast::{
        AttributeValueRef, AttributesRef, BlockKind, BlockRef, ContentRef, ExampleRef, ListingRef,
        LiteralRef, StemBlockRef, TokensRef, VerseRef,
    },
};

#[cfg(feature = "pre-spec-subs")]
use acdc_parser::{
    Location, Plain, Substitution,
    ast::{InlineKind, SubstitutionSpecRef},
};

#[cfg(feature = "terminal")]
use crate::terminal::{is_terminal_session, render_listing, render_session};
use crate::{
    Error, HtmlVariant, HtmlVisitor, build_class, image_helpers::attribute_text,
    inlines::escape_pcdata, write_attribution, write_element_id, write_id,
    write_semantic_attribution,
};

fn block_class(base: &str, attributes: Option<AttributesRef<'_>>) -> String {
    let roles = match attributes.and_then(|attributes| attributes.get("role")) {
        Some(AttributeValueRef::Tokens(roles)) => Some(roles),
        _ => None,
    };
    build_class(base, roles.into_iter().flat_map(TokensRef::iter))
}

/// Write the opening `<div>` tag with optional ID and class attributes.
fn write_block_div_open<W: Write>(
    w: &mut W,
    attributes: Option<AttributesRef<'_>>,
    base_class: &str,
) -> Result<(), Error> {
    write!(w, "<div")?;
    write_id(
        w,
        attributes.and_then(|attributes| attribute_text(attributes, "id")),
    )?;
    let class = block_class(base_class, attributes);
    writeln!(w, " class=\"{class}\">")?;
    Ok(())
}

/// Write an opening tag with optional ID and class for semantic mode.
fn write_semantic_tag_open<W: Write>(
    w: &mut W,
    tag: &str,
    attributes: Option<AttributesRef<'_>>,
    base_class: &str,
) -> Result<(), Error> {
    write!(w, "<{tag}")?;
    let class = block_class(base_class, attributes);
    write!(w, " class=\"{class}\"")?;
    write_id(
        w,
        attributes.and_then(|attributes| attribute_text(attributes, "id")),
    )?;
    writeln!(w, ">")?;
    Ok(())
}

impl<'a, W: Write> HtmlVisitor<'a, '_, W> {
    fn write_example_block(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        block: ExampleRef<'_>,
        blocks: &'a [Block<'a>],
    ) -> Result<(), Error> {
        let is_collapsible = block.options().contains("collapsible");

        if is_collapsible {
            return self.write_example_block_collapsible(traversal, block, blocks);
        }

        write_block_div_open(&mut self.writer, Some(block.attributes()), "exampleblock")?;

        if block.title().is_some() {
            self.render_captioned_title_with_wrapper(
                traversal,
                block.title(),
                Some(block.attributes()),
                Some(CaptionKind::Example),
                "<div class=\"title\">",
                "</div>\n",
            )?;
        }

        writeln!(self.writer, "<div class=\"content\">")?;
        for nested_block in blocks {
            traversal.visit_block(self, nested_block)?;
        }
        writeln!(self.writer, "</div>")?;
        writeln!(self.writer, "</div>")?;
        Ok(())
    }

    fn write_example_block_collapsible(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        block: ExampleRef<'_>,
        blocks: &'a [Block<'a>],
    ) -> Result<(), Error> {
        let is_open = block.options().contains("open");

        write!(self.writer, "<details")?;
        write_element_id(&mut self.writer, block.id())?;
        if !block.roles().is_empty() {
            write!(
                self.writer,
                " class=\"{}\"",
                build_class("", block.roles().iter())
            )?;
        }
        if is_open {
            writeln!(self.writer, " open>")?;
        } else {
            writeln!(self.writer, ">")?;
        }

        if block.title().is_some() {
            self.render_title_content(
                traversal,
                block.title(),
                "<summary class=\"title\">",
                "</summary>\n",
            )?;
        } else {
            writeln!(self.writer, "<summary class=\"title\">Details</summary>")?;
        }

        writeln!(self.writer, "<div class=\"content\">")?;
        for nested_block in blocks {
            traversal.visit_block(self, nested_block)?;
        }
        writeln!(self.writer, "</div>")?;
        writeln!(self.writer, "</details>")?;
        Ok(())
    }

    fn write_example_block_semantic(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        block: ExampleRef<'_>,
        blocks: &'a [Block<'a>],
    ) -> Result<(), Error> {
        let is_collapsible = block.options().contains("collapsible");
        let is_open = block.options().contains("open");

        if is_collapsible {
            // Collapsible: <details> with no class (unless id/roles)
            write!(self.writer, "<details")?;
            if !block.roles().is_empty() {
                write!(
                    self.writer,
                    " class=\"{}\"",
                    build_class("", block.roles().iter())
                )?;
            }
            if let Some(id) = block.id() {
                write!(self.writer, " id=\"{}\"", id.as_str())?;
            }
            if is_open {
                writeln!(self.writer, " open>")?;
            } else {
                writeln!(self.writer, ">")?;
            }
            if block.title().is_some() {
                self.render_captioned_title_with_wrapper(
                    traversal,
                    block.title(),
                    Some(block.attributes()),
                    Some(CaptionKind::Example),
                    "<summary>",
                    "</summary>\n",
                )?;
            }
            // Collapsible content wrapper
            writeln!(self.writer, "<div class=\"content\">")?;
            for nested_block in blocks {
                traversal.visit_block(self, nested_block)?;
            }
            writeln!(self.writer, "</div>")?;
            writeln!(self.writer, "</details>")?;
        } else if block.title().is_some() {
            // Titled: use figure/figcaption with inner div.example
            write_semantic_tag_open(
                &mut self.writer,
                "figure",
                Some(block.attributes()),
                "example-block",
            )?;
            self.render_captioned_title_with_wrapper(
                traversal,
                block.title(),
                Some(block.attributes()),
                Some(CaptionKind::Example),
                "<figcaption>",
                "</figcaption>\n",
            )?;
            writeln!(self.writer, "<div class=\"example\">")?;
            for nested_block in blocks {
                traversal.visit_block(self, nested_block)?;
            }
            writeln!(self.writer, "</div>")?;
            writeln!(self.writer, "</figure>")?;
        } else {
            // Untitled: use div with inner div.example
            write_semantic_tag_open(
                &mut self.writer,
                "div",
                Some(block.attributes()),
                "example-block",
            )?;
            writeln!(self.writer, "<div class=\"example\">")?;
            for nested_block in blocks {
                traversal.visit_block(self, nested_block)?;
            }
            writeln!(self.writer, "</div>")?;
            writeln!(self.writer, "</div>")?;
        }
        Ok(())
    }
}

impl<'a, W: Write> HtmlVisitor<'a, '_, W> {
    /// Render a delimited block to HTML.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn render_delimited_block(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        block: &'a DelimitedBlock<'a>,
    ) -> Result<(), Error> {
        let processor = self.processor.clone();
        let data = BlockRef::from(block);
        let caption_kind = CaptionKind::for_block(data);
        match (data.kind(), &block.inner) {
            (BlockKind::Quote(data), DelimitedBlockType::DelimitedQuote(blocks)) => {
                if processor.variant() == HtmlVariant::Semantic {
                    let has_title = data.title().is_some();
                    if has_title {
                        write_semantic_tag_open(
                            &mut self.writer,
                            "section",
                            Some(data.attributes()),
                            "quote-block",
                        )?;
                        self.render_title_content(
                            traversal,
                            data.title(),
                            "<h6 class=\"block-title\">",
                            "</h6>\n",
                        )?;
                    } else {
                        write_semantic_tag_open(
                            &mut self.writer,
                            "div",
                            Some(data.attributes()),
                            "quote-block",
                        )?;
                    }
                    writeln!(self.writer, "<blockquote>")?;
                    for nested_block in blocks {
                        traversal.visit_block(self, nested_block)?;
                    }
                    // Attribution goes inside blockquote as <footer>
                    write_semantic_attribution(traversal, self, Some(data.attributes()))?;
                    writeln!(self.writer, "</blockquote>")?;
                    if has_title {
                        writeln!(self.writer, "</section>")?;
                    } else {
                        writeln!(self.writer, "</div>")?;
                    }
                } else {
                    let base_class = if let Some(style) = data.style() {
                        format!("{style}block")
                    } else {
                        "quoteblock".to_string()
                    };
                    write_block_div_open(&mut self.writer, Some(data.attributes()), &base_class)?;
                    self.render_title_content(
                        traversal,
                        data.title(),
                        "<div class=\"title\">",
                        "</div>\n",
                    )?;
                    writeln!(self.writer, "<blockquote>")?;
                    for nested_block in blocks {
                        traversal.visit_block(self, nested_block)?;
                    }
                    writeln!(self.writer, "</blockquote>")?;
                    write_attribution(traversal, self, Some(data.attributes()))?;
                    writeln!(self.writer, "</div>")?;
                }
            }
            (BlockKind::Open(data), DelimitedBlockType::DelimitedOpen(blocks)) => {
                if processor.variant() == HtmlVariant::Semantic {
                    let is_abstract = data.style() == Some("abstract");
                    if is_abstract {
                        // Abstract style: render as quote-block abstract
                        let has_title = data.title().is_some();
                        if has_title {
                            let base = build_class("quote-block abstract", data.roles().iter());
                            write!(self.writer, "<section class=\"{base}\"")?;
                            if let Some(id) = data.id() {
                                write!(self.writer, " id=\"{}\"", id.as_str())?;
                            }
                            writeln!(self.writer, ">")?;
                            self.render_captioned_title_with_wrapper(
                                traversal,
                                data.title(),
                                Some(data.attributes()),
                                caption_kind,
                                "<h6 class=\"block-title\">",
                                "</h6>\n",
                            )?;
                        } else {
                            write_semantic_tag_open(
                                &mut self.writer,
                                "div",
                                Some(data.attributes()),
                                "quote-block abstract",
                            )?;
                        }
                        writeln!(self.writer, "<blockquote>")?;
                        for nested_block in blocks {
                            traversal.visit_block(self, nested_block)?;
                        }
                        writeln!(self.writer, "</blockquote>")?;
                        if has_title {
                            writeln!(self.writer, "</section>")?;
                        } else {
                            writeln!(self.writer, "</div>")?;
                        }
                    } else {
                        // Regular open block in semantic mode
                        let has_title = data.title().is_some();
                        if has_title {
                            write_semantic_tag_open(
                                &mut self.writer,
                                "section",
                                Some(data.attributes()),
                                "open-block",
                            )?;
                            self.render_captioned_title_with_wrapper(
                                traversal,
                                data.title(),
                                Some(data.attributes()),
                                caption_kind,
                                "<h6 class=\"block-title\">",
                                "</h6>\n",
                            )?;
                        } else {
                            write_semantic_tag_open(
                                &mut self.writer,
                                "div",
                                Some(data.attributes()),
                                "open-block",
                            )?;
                        }
                        writeln!(self.writer, "<div class=\"content\">")?;
                        for nested_block in blocks {
                            traversal.visit_block(self, nested_block)?;
                        }
                        writeln!(self.writer, "</div>")?;
                        if has_title {
                            writeln!(self.writer, "</section>")?;
                        } else {
                            writeln!(self.writer, "</div>")?;
                        }
                    }
                } else {
                    write_block_div_open(&mut self.writer, Some(data.attributes()), "openblock")?;
                    self.render_captioned_title_with_wrapper(
                        traversal,
                        data.title(),
                        Some(data.attributes()),
                        caption_kind,
                        "<div class=\"title\">",
                        "</div>\n",
                    )?;
                    writeln!(self.writer, "<div class=\"content\">")?;
                    for nested_block in blocks {
                        traversal.visit_block(self, nested_block)?;
                    }
                    writeln!(self.writer, "</div>")?;
                    writeln!(self.writer, "</div>")?;
                }
            }
            (BlockKind::Example(data), DelimitedBlockType::DelimitedExample(blocks)) => {
                if processor.variant() == HtmlVariant::Semantic {
                    self.write_example_block_semantic(traversal, data, blocks)?;
                } else {
                    self.write_example_block(traversal, data, blocks)?;
                }
            }
            (BlockKind::Sidebar(data), DelimitedBlockType::DelimitedSidebar(blocks)) => {
                if processor.variant() == HtmlVariant::Semantic {
                    write_semantic_tag_open(
                        &mut self.writer,
                        "aside",
                        Some(data.attributes()),
                        "sidebar",
                    )?;
                    self.render_title_content(
                        traversal,
                        data.title(),
                        "<h6 class=\"block-title\">",
                        "</h6>\n",
                    )?;
                    for nested_block in blocks {
                        traversal.visit_block(self, nested_block)?;
                    }
                    writeln!(self.writer, "</aside>")?;
                } else {
                    write_block_div_open(
                        &mut self.writer,
                        Some(data.attributes()),
                        "sidebarblock",
                    )?;
                    writeln!(self.writer, "<div class=\"content\">")?;
                    self.render_title_content(
                        traversal,
                        data.title(),
                        "<div class=\"title\">",
                        "</div>\n",
                    )?;
                    for nested_block in blocks {
                        traversal.visit_block(self, nested_block)?;
                    }
                    writeln!(self.writer, "</div>")?;
                    writeln!(self.writer, "</div>")?;
                }
            }
            // Handle tables
            (BlockKind::Table(data), DelimitedBlockType::DelimitedTable(rows)) => {
                let processor = self.processor.clone();
                crate::table::render_table(traversal, data, rows, self, &processor)?;
            }
            // Verse, literal, and stem blocks need semantic handling
            (
                _,
                DelimitedBlockType::DelimitedVerse(_)
                | DelimitedBlockType::DelimitedLiteral(_)
                | DelimitedBlockType::DelimitedStem(_),
            ) if processor.variant() == HtmlVariant::Semantic => {
                self.render_delimited_block_inner_semantic(traversal, block)?;
            }
            // For other block types, use the regular rendering
            _ => {
                self.render_delimited_block_inner(traversal, block)?;
            }
        }
        Ok(())
    }
}

impl<'a, W: Write> HtmlVisitor<'a, '_, W> {
    fn render_listing_code(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        content: ContentRef<'_>,
        attributes: Option<AttributesRef<'_>>,
    ) -> Result<(), Error> {
        let language = attributes
            .and_then(source_language)
            .map(str::to_owned)
            .or_else(|| {
                matches!(
                    attributes.and_then(|attributes| attributes.get("style")),
                    Some(AttributeValueRef::Text("source"))
                )
                .then(|| {
                    traversal
                        .get("source-language")
                        .and_then(|value| value.text())
                })
                .flatten()
                .map(str::to_owned)
            });
        let source_indent =
            crate::source_indent::resolve(attributes, traversal, &mut self.diagnostics);
        #[cfg(feature = "pre-spec-subs")]
        let subs = effective_subs(
            match attributes.and_then(|attributes| attributes.get("subs")) {
                Some(AttributeValueRef::Substitutions(spec)) => Some(spec),
                _ => None,
            },
            true,
        );
        #[cfg(not(feature = "pre-spec-subs"))]
        let subs = baseline_subs(true);

        crate::render_pre_code(
            traversal,
            content,
            attributes,
            language.as_deref(),
            self,
            &subs,
            source_indent,
        )
    }

    fn render_listing_block(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        inlines: &[InlineNode],
        data: ListingRef<'_>,
        metadata: &BlockMetadata,
        caption_kind: Option<CaptionKind>,
    ) -> Result<(), Error> {
        let content = data.content();
        let title = data.title();
        let attributes = Some(data.attributes());
        #[cfg(not(feature = "terminal"))]
        let _ = metadata;

        let processor = self.processor.clone();
        if processor.variant() == HtmlVariant::Semantic {
            return self.render_listing_block_semantic(
                traversal,
                inlines,
                data,
                metadata,
                caption_kind,
            );
        }

        #[cfg(feature = "terminal")]
        if is_terminal_session(metadata) && self.terminal_emulator_allowed() {
            return self.render_terminal_session_block(
                traversal,
                inlines,
                title,
                metadata,
                caption_kind,
                attributes,
            );
        }

        #[cfg(feature = "terminal")]
        if crate::terminal::is_terminal_listing(processor.document_attributes(), metadata)
            && self.terminal_emulator_allowed()
        {
            return self.render_terminal_listing_block(
                traversal,
                inlines,
                title,
                metadata,
                caption_kind,
                attributes,
            );
        }

        write_block_div_open(&mut self.writer, attributes, "listingblock")?;

        self.render_captioned_title_with_wrapper(
            traversal,
            title,
            attributes,
            caption_kind,
            "<div class=\"title\">",
            "</div>\n",
        )?;

        writeln!(self.writer, "<div class=\"content\">")?;
        self.render_listing_code(traversal, content, attributes)?;
        writeln!(self.writer, "</div>")?;
        writeln!(self.writer, "</div>")?;
        Ok(())
    }

    fn render_listing_block_semantic(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        inlines: &[InlineNode],
        data: ListingRef<'_>,
        metadata: &BlockMetadata,
        caption_kind: Option<CaptionKind>,
    ) -> Result<(), Error> {
        let content = data.content();
        let title = data.title();
        let attributes = Some(data.attributes());
        #[cfg(not(feature = "terminal"))]
        let _ = (inlines, metadata);

        #[cfg(feature = "terminal")]
        if is_terminal_session(metadata) && self.terminal_emulator_allowed() {
            return self.render_terminal_session_block_semantic(
                traversal,
                inlines,
                title,
                metadata,
                caption_kind,
                attributes,
            );
        }

        #[cfg(feature = "terminal")]
        if crate::terminal::is_terminal_listing(self.processor.document_attributes(), metadata)
            && self.terminal_emulator_allowed()
        {
            return self.render_terminal_listing_block_semantic(
                traversal,
                inlines,
                title,
                metadata,
                caption_kind,
                attributes,
            );
        }

        if title.is_some() {
            // Titled: use figure/figcaption
            write_semantic_tag_open(&mut self.writer, "figure", attributes, "listing-block")?;
            self.render_captioned_title_with_wrapper(
                traversal,
                title,
                attributes,
                caption_kind,
                "<figcaption>",
                "</figcaption>\n",
            )?;
            self.render_listing_code(traversal, content, attributes)?;
            writeln!(self.writer, "</figure>")?;
        } else {
            // Untitled: use div
            write_semantic_tag_open(&mut self.writer, "div", attributes, "listing-block")?;
            self.render_listing_code(traversal, content, attributes)?;
            writeln!(self.writer, "</div>")?;
        }
        Ok(())
    }

    /// Whether the terminal emulator may render the current block. Terminal
    /// rendering feeds document-controlled bytes through a native terminal
    /// emulator (`libghostty-vt`), so it only runs for trusted documents. At
    /// `SafeMode::Server` and above the block degrades to a plain listing,
    /// mirroring how Asciidoctor renders an unrecognized block style, and a
    /// warning records why the preview is absent.
    #[cfg(feature = "terminal")]
    fn terminal_emulator_allowed(&mut self) -> bool {
        let safe_mode = self.processor.options.safe_mode();
        if safe_mode < acdc_parser::SafeMode::Server {
            return true;
        }
        self.diagnostics.warn_with_advice(
            format!(
                "terminal rendering is disabled at safe mode `{safe_mode:?}`; rendering the block as a plain listing"
            ),
            "Use terminal previews only for trusted documents below Server safe mode, or use a plain listing block.",
        );
        false
    }

    #[cfg(feature = "terminal")]
    fn render_terminal_session_block(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        inlines: &[InlineNode],
        title: Option<ContentRef<'_>>,
        metadata: &BlockMetadata,
        caption_kind: Option<CaptionKind>,
        attributes: Option<AttributesRef<'_>>,
    ) -> Result<(), Error> {
        let attrs = self.processor.document_attributes().clone();
        let options = self.processor.options.clone();
        write_block_div_open(&mut self.writer, attributes, "terminalblock terminal-block")?;

        self.render_captioned_title_with_wrapper(
            traversal,
            title,
            attributes,
            caption_kind,
            "<div class=\"title\">",
            "</div>\n",
        )?;

        writeln!(self.writer, "<div class=\"content\">")?;
        // Direct field access so the writer and diagnostics borrows stay
        // disjoint; `writer_mut()` would borrow all of `self`.
        render_session(
            &mut self.writer,
            inlines,
            metadata,
            options,
            &attrs,
            &mut self.diagnostics,
        )?;
        writeln!(self.writer, "</div>")?;
        writeln!(self.writer, "</div>")?;
        Ok(())
    }

    #[cfg(feature = "terminal")]
    fn render_terminal_session_block_semantic(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        inlines: &[InlineNode],
        title: Option<ContentRef<'_>>,
        metadata: &BlockMetadata,
        caption_kind: Option<CaptionKind>,
        attributes: Option<AttributesRef<'_>>,
    ) -> Result<(), Error> {
        let attrs = self.processor.document_attributes().clone();
        let options = self.processor.options.clone();
        // Direct field access so the writer and diagnostics borrows stay
        // disjoint; `writer_mut()` would borrow all of `self`.
        if title.is_some() {
            write_semantic_tag_open(&mut self.writer, "figure", attributes, "terminal-block")?;
            self.render_captioned_title_with_wrapper(
                traversal,
                title,
                attributes,
                caption_kind,
                "<figcaption>",
                "</figcaption>\n",
            )?;
            render_session(
                &mut self.writer,
                inlines,
                metadata,
                options,
                &attrs,
                &mut self.diagnostics,
            )?;
            writeln!(self.writer, "</figure>")?;
        } else {
            write_semantic_tag_open(&mut self.writer, "div", attributes, "terminal-block")?;
            render_session(
                &mut self.writer,
                inlines,
                metadata,
                options,
                &attrs,
                &mut self.diagnostics,
            )?;
            writeln!(self.writer, "</div>")?;
        }
        Ok(())
    }

    #[cfg(feature = "terminal")]
    fn render_terminal_listing_block(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        inlines: &[InlineNode],
        title: Option<ContentRef<'_>>,
        metadata: &BlockMetadata,
        caption_kind: Option<CaptionKind>,
        attributes: Option<AttributesRef<'_>>,
    ) -> Result<(), Error> {
        let attrs = self.processor.document_attributes().clone();
        let options = self.processor.options.clone();
        write_block_div_open(&mut self.writer, attributes, "listingblock terminal-block")?;

        self.render_captioned_title_with_wrapper(
            traversal,
            title,
            attributes,
            caption_kind,
            "<div class=\"title\">",
            "</div>\n",
        )?;

        writeln!(self.writer, "<div class=\"content\">")?;
        render_listing(&mut self.writer, inlines, metadata, options, &attrs)?;
        writeln!(self.writer, "</div>")?;
        writeln!(self.writer, "</div>")?;
        Ok(())
    }

    #[cfg(feature = "terminal")]
    fn render_terminal_listing_block_semantic(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        inlines: &[InlineNode],
        title: Option<ContentRef<'_>>,
        metadata: &BlockMetadata,
        caption_kind: Option<CaptionKind>,
        attributes: Option<AttributesRef<'_>>,
    ) -> Result<(), Error> {
        let attrs = self.processor.document_attributes().clone();
        let options = self.processor.options.clone();
        if title.is_some() {
            write_semantic_tag_open(
                &mut self.writer,
                "figure",
                attributes,
                "listing-block terminal-block",
            )?;
            self.render_captioned_title_with_wrapper(
                traversal,
                title,
                attributes,
                caption_kind,
                "<figcaption>",
                "</figcaption>\n",
            )?;
            render_listing(&mut self.writer, inlines, metadata, options, &attrs)?;
            writeln!(self.writer, "</figure>")?;
        } else {
            write_semantic_tag_open(
                &mut self.writer,
                "div",
                attributes,
                "listing-block terminal-block",
            )?;
            render_listing(&mut self.writer, inlines, metadata, options, &attrs)?;
            writeln!(self.writer, "</div>")?;
        }
        Ok(())
    }

    /// Render a passthrough block with `subs=` override applied.
    ///
    /// Passthrough blocks default to no substitutions, emitting raw content.
    /// When `subs=` is specified, the raw content is processed through the
    /// substitution pipeline (attributes, quotes, specialchars, replacements).
    #[cfg(feature = "pre-spec-subs")]
    fn render_pass_block_with_subs(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        body: ContentRef<'_>,
        spec: SubstitutionSpecRef<'_>,
    ) -> Result<(), Error> {
        // Passthrough blocks default to no subs, so the baseline is empty.
        let effective = spec.resolve(&[]);

        let mut content = String::new();
        for node in body.inlines() {
            if let InlineKind::Raw(raw) = node.kind() {
                content.push_str(raw.text());
            }
        }

        // Apply attribute substitution if enabled (not done by parser for
        // passthrough blocks)
        if effective.contains(&Substitution::Attributes) {
            content = substitute_attributes(&content, traversal).into_owned();
        }

        // If quotes substitution is enabled, parse the content for inline
        // formatting (bold, italic, etc.) and render each node with the full
        // effective subs. This mirrors VerbatimText rendering which passes full
        // subs to avoid PlainText's no_quotes_subs optimization that would
        // prevent Bold/Italic nodes from rendering as HTML.
        if effective.contains(&Substitution::Quotes) {
            let parsed = acdc_parser::parse_text_for_quotes(&content);
            let options = self.render_options.clone();
            for node in parsed.inlines() {
                self.render_inline_node(traversal, node.into(), &options, &effective)?;
            }
        } else {
            let plain = InlineNode::PlainText(Plain {
                content: &content,
                location: Location::default(),
                escaped: false,
            });
            let options = self.render_options.clone();
            self.render_inline_node(traversal, (&plain).into(), &options, &effective)?;
        }
        Ok(())
    }

    fn render_literal_block(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        inlines: &[InlineNode<'_>],
        data: LiteralRef<'_>,
        metadata: &BlockMetadata<'_>,
        caption_kind: Option<CaptionKind>,
    ) -> Result<(), Error> {
        let content = data.content();
        let title = data.title();
        let attributes = Some(data.attributes());
        #[cfg(not(feature = "terminal"))]
        let _ = (inlines, metadata);

        #[cfg(feature = "terminal")]
        if is_terminal_session(metadata) && self.terminal_emulator_allowed() {
            if self.processor.variant() == HtmlVariant::Semantic {
                self.render_terminal_session_block_semantic(
                    traversal,
                    inlines,
                    title,
                    metadata,
                    caption_kind,
                    attributes,
                )?;
            } else {
                self.render_terminal_session_block(
                    traversal,
                    inlines,
                    title,
                    metadata,
                    caption_kind,
                    attributes,
                )?;
            }
            return Ok(());
        }

        let base_class = if let Some(style) =
            attributes.and_then(|attributes| attribute_text(attributes, "style"))
            && style != "source"
        {
            format!("{style}block")
        } else {
            "literalblock".to_string()
        };
        write_block_div_open(&mut self.writer, attributes, &base_class)?;
        self.render_captioned_title_with_wrapper(
            traversal,
            title,
            attributes,
            caption_kind,
            "<div class=\"title\">",
            "</div>\n",
        )?;
        writeln!(self.writer, "<div class=\"content\">")?;
        write!(self.writer, "<pre>")?;
        self.visit_indented_inlines(traversal, content, attributes)?;
        writeln!(self.writer, "</pre>")?;
        writeln!(self.writer, "</div>")?;
        writeln!(self.writer, "</div>")?;
        Ok(())
    }

    fn render_delimited_block_inner(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        block: &DelimitedBlock,
    ) -> Result<(), Error> {
        let data = BlockRef::from(block);
        let attributes = data.attributes();
        let caption_kind = CaptionKind::for_block(data);
        match (data.kind(), &block.inner) {
            (BlockKind::Passthrough(data), _) => {
                #[cfg(feature = "pre-spec-subs")]
                if let Some(AttributeValueRef::Substitutions(spec)) = data.attributes().get("subs")
                {
                    self.render_pass_block_with_subs(traversal, data.content(), spec)?;
                } else {
                    self.visit_content(traversal, data.content())?;
                }
                #[cfg(not(feature = "pre-spec-subs"))]
                self.visit_content(traversal, data.content())?;
            }
            (BlockKind::Listing(data), DelimitedBlockType::DelimitedListing(inlines)) => {
                self.render_listing_block(traversal, inlines, data, &block.metadata, caption_kind)?;
            }
            (BlockKind::Literal(data), DelimitedBlockType::DelimitedLiteral(inlines)) => {
                self.render_literal_block(traversal, inlines, data, &block.metadata, caption_kind)?;
            }
            (BlockKind::Stem(data), _) => {
                write_block_div_open(&mut self.writer, attributes, "stemblock")?;
                self.render_title_content(
                    traversal,
                    data.title(),
                    "<div class=\"title\">",
                    "</div>\n",
                )?;
                let processor = self.processor.clone();
                render_stem_content(
                    data,
                    &mut self.writer,
                    processor.variant() == HtmlVariant::Semantic,
                    traversal,
                )?;
                writeln!(self.writer, "</div>")?;
            }
            (BlockKind::DelimitedComment(_), _) => {}
            (BlockKind::Verse(data), _) => {
                write_block_div_open(&mut self.writer, attributes, "verseblock")?;
                self.render_title_content(
                    traversal,
                    data.title(),
                    "<div class=\"title\">",
                    "</div>\n",
                )?;
                write!(self.writer, "<pre class=\"content\">")?;
                self.visit_content(traversal, data.content())?;
                writeln!(self.writer, "</pre>")?;
                write_attribution(traversal, self, attributes)?;
                writeln!(self.writer, "</div>")?;
            }
            _ => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Unsupported,
                    format!("Unsupported delimited block type: {:?}", block.inner),
                )
                .into());
            }
        }
        Ok(())
    }
}

fn render_stem_content<W: Write + ?Sized>(
    stem: StemBlockRef<'_>,
    w: &mut W,
    semantic: bool,
    attributes: &TraversalContext<'_>,
) -> Result<(), Error> {
    let forced = if semantic {
        attributes
            .get("html5s-force-stem-type")
            .and_then(|value| value.text())
            .and_then(|value| value.parse::<StemNotation>().ok())
    } else {
        None
    };
    let notation = forced.unwrap_or_else(|| stem.notation());
    let content = stem_html(stem);
    writeln!(w, "<div class=\"content\">")?;
    match notation {
        StemNotation::Latexmath => {
            write!(w, "\\[{content}\\]")?;
        }
        StemNotation::Asciimath => {
            write!(w, "\\${content}\\$")?;
        }
    }
    writeln!(w, "</div>")?;
    Ok(())
}

/// Render stem content in semantic mode with `<div class="math" data-lang="...">`.
fn render_stem_content_semantic<W: Write + ?Sized>(
    stem: StemBlockRef<'_>,
    w: &mut W,
    attributes: &TraversalContext<'_>,
) -> Result<(), Error> {
    let forced = attributes
        .get("html5s-force-stem-type")
        .and_then(|value| value.text())
        .and_then(|value| value.parse::<StemNotation>().ok());
    let notation = forced.unwrap_or_else(|| stem.notation());
    let content = stem_html(stem);
    let data_lang = match notation {
        StemNotation::Latexmath => "tex",
        StemNotation::Asciimath => "asciimath",
    };
    write!(w, "<div class=\"math\" data-lang=\"{data_lang}\">")?;
    match notation {
        StemNotation::Latexmath => {
            write!(w, "\\[{content}\\]")?;
        }
        StemNotation::Asciimath => {
            write!(w, "\\${content}\\$")?;
        }
    }
    writeln!(w, "</div>")?;
    Ok(())
}

fn stem_html(stem: StemBlockRef<'_>) -> Cow<'_, str> {
    #[cfg(feature = "pre-spec-subs")]
    let special_chars = match stem.attributes().get("subs") {
        Some(AttributeValueRef::Substitutions(spec)) => spec
            .resolve(&[Substitution::SpecialChars])
            .contains(&Substitution::SpecialChars),
        _ => true,
    };
    #[cfg(not(feature = "pre-spec-subs"))]
    let special_chars = true;
    if special_chars {
        Cow::Owned(escape_pcdata(stem.text()))
    } else {
        Cow::Borrowed(stem.text())
    }
}

impl<'a, W: Write> HtmlVisitor<'a, '_, W> {
    /// Render verse, literal, and stem blocks in semantic HTML5 mode.
    fn render_delimited_block_inner_semantic(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        block: &DelimitedBlock,
    ) -> Result<(), Error> {
        let data = BlockRef::from(block);
        let attributes = data.attributes();
        let caption_kind = CaptionKind::for_block(data);
        match (data.kind(), &block.inner) {
            (BlockKind::Verse(data), _) => {
                self.render_verse_block_semantic(traversal, data)?;
            }
            (BlockKind::Literal(data), _) => {
                let has_title = data.title().is_some();
                if has_title {
                    write_semantic_tag_open(
                        &mut self.writer,
                        "section",
                        attributes,
                        "literal-block",
                    )?;
                    self.render_captioned_title_with_wrapper(
                        traversal,
                        data.title(),
                        attributes,
                        caption_kind,
                        "<h6 class=\"block-title\">",
                        "</h6>\n",
                    )?;
                } else {
                    write_semantic_tag_open(&mut self.writer, "div", attributes, "literal-block")?;
                }
                write!(self.writer, "<pre>")?;
                self.visit_indented_inlines(traversal, data.content(), attributes)?;
                writeln!(self.writer, "</pre>")?;
                if has_title {
                    writeln!(self.writer, "</section>")?;
                } else {
                    writeln!(self.writer, "</div>")?;
                }
            }
            (BlockKind::Stem(data), _) => {
                let has_title = data.title().is_some();
                if has_title {
                    write_semantic_tag_open(&mut self.writer, "figure", attributes, "stem-block")?;
                    self.render_title_content(
                        traversal,
                        data.title(),
                        "<figcaption>",
                        "</figcaption>\n",
                    )?;
                } else {
                    write_semantic_tag_open(&mut self.writer, "div", attributes, "stem-block")?;
                }
                render_stem_content_semantic(data, &mut self.writer, traversal)?;
                if has_title {
                    writeln!(self.writer, "</figure>")?;
                } else {
                    writeln!(self.writer, "</div>")?;
                }
            }
            _ => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Unsupported,
                    format!(
                        "Unsupported delimited block type for semantic rendering: {:?}",
                        block.inner
                    ),
                )
                .into());
            }
        }
        Ok(())
    }

    fn render_verse_block_semantic(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        block: VerseRef<'_>,
    ) -> Result<(), Error> {
        let title = block.title();
        let attributes = Some(block.attributes());
        let has_title = title.is_some();
        let has_attribution = block.attribution().is_some() || block.citetitle().is_some();

        if has_title {
            write_semantic_tag_open(&mut self.writer, "section", attributes, "verse-block")?;
            self.render_title_content(traversal, title, "<h6 class=\"block-title\">", "</h6>\n")?;
        } else {
            write_semantic_tag_open(&mut self.writer, "div", attributes, "verse-block")?;
        }

        if has_attribution {
            writeln!(self.writer, "<blockquote class=\"verse\">")?;
            write!(self.writer, "<pre class=\"verse\">")?;
            self.visit_content(traversal, block.content())?;
            writeln!(self.writer, "</pre>")?;
            write_semantic_attribution(traversal, self, attributes)?;
            writeln!(self.writer, "</blockquote>")?;
        } else {
            write!(self.writer, "<pre class=\"verse\">")?;
            self.visit_content(traversal, block.content())?;
            writeln!(self.writer, "</pre>")?;
        }

        if has_title {
            writeln!(self.writer, "</section>")?;
        } else {
            writeln!(self.writer, "</div>")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Processor, RenderOptions};

    use std::{cell::Cell, rc::Rc};

    use acdc_converters_core::{
        Converter, Diagnostics, Doctype, Options, WarningSource, visitor::Visitor,
    };
    use acdc_parser::{
        BlockMetadata, DocumentAttributes, InlineNode, Location, Plain, SafeMode, Title,
    };

    fn create_test_inlines(content: &str) -> Vec<InlineNode<'_>> {
        vec![InlineNode::PlainText(Plain {
            content,
            location: Location::default(),
            escaped: false,
        })]
    }

    fn create_test_processor_raw() -> Processor<'static> {
        let options = Options::builder()
            .doctype(Doctype::Article)
            .safe_mode(SafeMode::Unsafe)
            .build();
        let document_attributes = DocumentAttributes::default();
        Processor {
            options,
            parser_options: acdc_parser::Options::default()
                .with_document_attributes(document_attributes),
            toc_entries: Vec::new(),
            references: std::collections::HashMap::new(),
            xref_guard: acdc_converters_core::xref::XrefGuard::default(),
            example_counter: Rc::new(Cell::new(0)),
            table_counter: Rc::new(Cell::new(0)),
            figure_counter: Rc::new(Cell::new(0)),
            listing_counter: Rc::new(Cell::new(0)),
            index_term_counter: Rc::new(Cell::new(0)),
            index_catalog_counter: Rc::new(Cell::new(0)),
            index_entries: Rc::new(std::cell::RefCell::new(Vec::new())),
            generate_index: false,
            variant: crate::HtmlVariant::Standard,
        }
    }

    fn create_test_processor() -> Rc<Processor<'static>> {
        Rc::new(create_test_processor_raw())
    }

    #[test]
    fn test_listing_block_renders_as_listingblock() -> Result<(), Error> {
        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedListing(create_test_inlines("code here")),
            "----",
            Location::default(),
        );

        let output = Vec::new();
        let processor = create_test_processor();
        let options = RenderOptions::default();
        let mut warnings = Vec::new();
        let source = WarningSource::new("html");
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let attribute_header = Converter::document_attributes(processor.as_ref()).clone();
        let mut traversal = TraversalContext::new(&attribute_header);
        let mut visitor = HtmlVisitor::new(output, processor, options, diagnostics.reborrow());

        visitor.visit_delimited_block(&mut traversal, &block)?;
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
            "....",
            Location::default(),
        );

        let output = Vec::new();
        let processor = create_test_processor();
        let options = RenderOptions::default();
        let mut warnings = Vec::new();
        let source = WarningSource::new("html");
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let attribute_header = Converter::document_attributes(processor.as_ref()).clone();
        let mut traversal = TraversalContext::new(&attribute_header);
        let mut visitor = HtmlVisitor::new(output, processor, options, diagnostics.reborrow());

        visitor.visit_delimited_block(&mut traversal, &block)?;
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
        attributes.insert("language".into(), AttributeValue::String("bash".into()));

        let metadata = BlockMetadata::new()
            .with_style(Some("source"))
            .with_attributes(attributes);

        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedListing(create_test_inlines("code here")),
            "----",
            Location::default(),
        )
        .with_metadata(metadata);

        let output = Vec::new();
        let processor = create_test_processor();
        let options = RenderOptions::default();
        let mut warnings = Vec::new();
        let source = WarningSource::new("html");
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let attribute_header = Converter::document_attributes(processor.as_ref()).clone();
        let mut traversal = TraversalContext::new(&attribute_header);
        let mut visitor = HtmlVisitor::new(output, processor, options, diagnostics.reborrow());

        visitor.visit_delimited_block(&mut traversal, &block)?;
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
        let metadata = BlockMetadata::new().with_style(Some("verse"));

        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedLiteral(create_test_inlines("literal text")),
            "....",
            Location::default(),
        )
        .with_metadata(metadata);

        let output = Vec::new();
        let processor = create_test_processor();
        let options = RenderOptions::default();
        let mut warnings = Vec::new();
        let source = WarningSource::new("html");
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let attribute_header = Converter::document_attributes(processor.as_ref()).clone();
        let mut traversal = TraversalContext::new(&attribute_header);
        let mut visitor = HtmlVisitor::new(output, processor, options, diagnostics.reborrow());

        visitor.visit_delimited_block(&mut traversal, &block)?;
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
            content: "My Code Example",
            location: Location::default(),
            escaped: false,
        })]);

        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedListing(create_test_inlines("code here")),
            "----",
            Location::default(),
        )
        .with_title(title);

        let output = Vec::new();
        let processor = create_test_processor();
        let options = RenderOptions::default();
        let mut warnings = Vec::new();
        let source = WarningSource::new("html");
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let attribute_header = Converter::document_attributes(processor.as_ref()).clone();
        let mut traversal = TraversalContext::new(&attribute_header);
        let mut visitor =
            HtmlVisitor::new(output, processor.clone(), options, diagnostics.reborrow());

        visitor.visit_delimited_block(&mut traversal, &block)?;
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
        let title1 = Title::new(vec![InlineNode::PlainText(Plain {
            content: "First Example",
            location: Location::default(),
            escaped: false,
        })]);

        let title2 = Title::new(vec![InlineNode::PlainText(Plain {
            content: "Second Example",
            location: Location::default(),
            escaped: false,
        })]);

        let block1 = DelimitedBlock::new(
            DelimitedBlockType::DelimitedListing(create_test_inlines("code 1")),
            "----",
            Location::default(),
        )
        .with_title(title1);

        let block2 = DelimitedBlock::new(
            DelimitedBlockType::DelimitedListing(create_test_inlines("code 2")),
            "----",
            Location::default(),
        )
        .with_title(title2);

        let output = Vec::new();
        let processor = {
            let mut p = create_test_processor_raw();
            p.parser_options = p
                .parser_options
                .into_builder()
                .with_attribute("listing-caption", "Listing")
                .build()?;
            Rc::new(p)
        };

        let options = RenderOptions::default();
        let mut warnings = Vec::new();
        let source = WarningSource::new("html");
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let attribute_header = Converter::document_attributes(processor.as_ref()).clone();
        let mut traversal = TraversalContext::new(&attribute_header);
        let mut visitor = HtmlVisitor::new(
            output,
            processor.clone(),
            options.clone(),
            diagnostics.reborrow(),
        );

        // Render first block
        visitor.visit_delimited_block(&mut traversal, &block1)?;
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
        let mut warnings = Vec::new();
        let source = WarningSource::new("html");
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let attribute_header = Converter::document_attributes(processor.as_ref()).clone();
        let mut traversal = TraversalContext::new(&attribute_header);
        let mut visitor2 =
            HtmlVisitor::new(output2, processor.clone(), options, diagnostics.reborrow());
        visitor2.visit_delimited_block(&mut traversal, &block2)?;
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
            .with_id(Some(Anchor::new("my-listing-id", Location::default())))
            .with_roles(vec!["highlight", "special"]);

        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedListing(create_test_inlines("code here")),
            "----",
            Location::default(),
        )
        .with_metadata(metadata);

        let output = Vec::new();
        let processor = create_test_processor();
        let options = RenderOptions::default();
        let mut warnings = Vec::new();
        let source = WarningSource::new("html");
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let attribute_header = Converter::document_attributes(processor.as_ref()).clone();
        let mut traversal = TraversalContext::new(&attribute_header);
        let mut visitor = HtmlVisitor::new(output, processor, options, diagnostics.reborrow());

        visitor.visit_delimited_block(&mut traversal, &block)?;
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
            .with_id(Some(Anchor::new("example-id", Location::default())))
            .with_roles(vec!["special"]);

        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedExample(vec![]),
            "====",
            Location::default(),
        )
        .with_metadata(metadata);

        let output = Vec::new();
        let processor = create_test_processor();
        let options = RenderOptions::default();
        let mut warnings = Vec::new();
        let source = WarningSource::new("html");
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let attribute_header = Converter::document_attributes(processor.as_ref()).clone();
        let mut traversal = TraversalContext::new(&attribute_header);
        let mut visitor = HtmlVisitor::new(output, processor, options, diagnostics.reborrow());

        visitor.visit_delimited_block(&mut traversal, &block)?;
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
            .with_id(Some(Anchor::new("quote-id", Location::default())))
            .with_roles(vec!["highlight"]);

        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedQuote(vec![]),
            "____",
            Location::default(),
        )
        .with_metadata(metadata);

        let output = Vec::new();
        let processor = create_test_processor();
        let options = RenderOptions::default();
        let mut warnings = Vec::new();
        let source = WarningSource::new("html");
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let attribute_header = Converter::document_attributes(processor.as_ref()).clone();
        let mut traversal = TraversalContext::new(&attribute_header);
        let mut visitor = HtmlVisitor::new(output, processor, options, diagnostics.reborrow());

        visitor.visit_delimited_block(&mut traversal, &block)?;
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
            .with_id(Some(Anchor::new("sidebar-id", Location::default())))
            .with_roles(vec!["sidebar-role"]);

        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedSidebar(vec![]),
            "****",
            Location::default(),
        )
        .with_metadata(metadata);

        let output = Vec::new();
        let processor = create_test_processor();
        let options = RenderOptions::default();
        let mut warnings = Vec::new();
        let source = WarningSource::new("html");
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let attribute_header = Converter::document_attributes(processor.as_ref()).clone();
        let mut traversal = TraversalContext::new(&attribute_header);
        let mut visitor = HtmlVisitor::new(output, processor, options, diagnostics.reborrow());

        visitor.visit_delimited_block(&mut traversal, &block)?;
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
            .with_id(Some(Anchor::new("open-id", Location::default())))
            .with_roles(vec!["open-role"]);

        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedOpen(vec![]),
            "--",
            Location::default(),
        )
        .with_metadata(metadata);

        let output = Vec::new();
        let processor = create_test_processor();
        let options = RenderOptions::default();
        let mut warnings = Vec::new();
        let source = WarningSource::new("html");
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let attribute_header = Converter::document_attributes(processor.as_ref()).clone();
        let mut traversal = TraversalContext::new(&attribute_header);
        let mut visitor = HtmlVisitor::new(output, processor, options, diagnostics.reborrow());

        visitor.visit_delimited_block(&mut traversal, &block)?;
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
            content: "Click to expand",
            location: Location::default(),
            escaped: false,
        })]);

        let metadata = BlockMetadata::new().with_options(vec!["collapsible"]);

        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedExample(vec![]),
            "====",
            Location::default(),
        )
        .with_metadata(metadata)
        .with_title(title);

        let output = Vec::new();
        let processor = create_test_processor();
        let options = RenderOptions::default();
        let mut warnings = Vec::new();
        let source = WarningSource::new("html");
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let attribute_header = Converter::document_attributes(processor.as_ref()).clone();
        let mut traversal = TraversalContext::new(&attribute_header);
        let mut visitor = HtmlVisitor::new(output, processor, options, diagnostics.reborrow());

        visitor.visit_delimited_block(&mut traversal, &block)?;
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
            content: "Initially open",
            location: Location::default(),
            escaped: false,
        })]);

        let metadata = BlockMetadata::new().with_options(vec!["collapsible", "open"]);

        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedExample(vec![]),
            "====",
            Location::default(),
        )
        .with_metadata(metadata)
        .with_title(title);

        let output = Vec::new();
        let processor = create_test_processor();
        let options = RenderOptions::default();
        let mut warnings = Vec::new();
        let source = WarningSource::new("html");
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let attribute_header = Converter::document_attributes(processor.as_ref()).clone();
        let mut traversal = TraversalContext::new(&attribute_header);
        let mut visitor = HtmlVisitor::new(output, processor, options, diagnostics.reborrow());

        visitor.visit_delimited_block(&mut traversal, &block)?;
        let html = String::from_utf8(visitor.into_writer())?;

        assert!(
            html.contains("<details open>"),
            "Collapsible open block should have open attribute, got: {html}",
        );
        Ok(())
    }

    #[test]
    fn test_example_block_collapsible_without_title() -> Result<(), Error> {
        let metadata = BlockMetadata::new().with_options(vec!["collapsible"]);

        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedExample(vec![]),
            "====",
            Location::default(),
        )
        .with_metadata(metadata);

        let output = Vec::new();
        let processor = create_test_processor();
        let options = RenderOptions::default();
        let mut warnings = Vec::new();
        let source = WarningSource::new("html");
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let attribute_header = Converter::document_attributes(processor.as_ref()).clone();
        let mut traversal = TraversalContext::new(&attribute_header);
        let mut visitor = HtmlVisitor::new(output, processor, options, diagnostics.reborrow());

        visitor.visit_delimited_block(&mut traversal, &block)?;
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
        let mut metadata = BlockMetadata::new().with_roles(vec!["my-role"]);
        metadata.anchors = vec![Anchor::new("anchor-fallback", Location::default())];

        let block = DelimitedBlock::new(
            DelimitedBlockType::DelimitedListing(create_test_inlines("code")),
            "----",
            Location::default(),
        )
        .with_metadata(metadata);

        let output = Vec::new();
        let processor = create_test_processor();
        let options = RenderOptions::default();
        let mut warnings = Vec::new();
        let source = WarningSource::new("html");
        let mut diagnostics = Diagnostics::new(&source, &mut warnings);
        let attribute_header = Converter::document_attributes(processor.as_ref()).clone();
        let mut traversal = TraversalContext::new(&attribute_header);
        let mut visitor = HtmlVisitor::new(output, processor, options, diagnostics.reborrow());

        visitor.visit_delimited_block(&mut traversal, &block)?;
        let html = String::from_utf8(visitor.into_writer())?;

        assert!(
            html.contains("<div id=\"anchor-fallback\" class=\"listingblock my-role\">"),
            "Expected listing block with anchor fallback ID, got: {html}",
        );
        Ok(())
    }
}
