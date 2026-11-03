use std::fmt::Write as _;

#[cfg(feature = "pre-spec-subs")]
use acdc_converters_core::substitutions::effective_subs_flags;
use acdc_converters_core::{
    Doctype, TraversalContext, document_attribute_text,
    icon::IconMode,
    section::{
        appendix_number_prefix, effective_section_level, part_number_prefix, section_number_prefix,
    },
    visitor::{BlockVisitor, DocumentVisitor, InlineVisitor, is_formatting_span},
};
use acdc_parser::{
    AdmonitionVariant, Caption, Image, SectionKind, Source, Video,
    ast::{
        AdmonitionRef, AudioRef, BlockKind, BlockRef, CalloutListRef, ContentRef,
        DescriptionListRef, DiscreteHeaderRef, DocumentRef, HeaderRef, ImageRef, InlineKind,
        InlineRef, OrderedListRef, PageBreakRef, ParagraphRef, SectionRef, TableOfContentsRef,
        ThematicBreakRef, UnorderedListRef, VideoRef,
    },
};

use crate::{
    Error, PdfVisitor, admonition_icon_source, author_name, is_page_breakable_table,
    is_unbreakable_delimited_block, is_unbreakable_paragraph,
    pdf_visitor::{AutomaticPreambleLeadState, ExplicitPageBreakState, attribute_text},
};

fn poster_image<'a>(
    video: VideoRef<'a>,
    poster: &'a str,
    target: &str,
) -> Result<Image<'a>, Error> {
    let video = Video::from(video);
    let mut metadata = video.metadata;
    metadata.caption = Some(Caption::Unnumbered);
    // Video alt is an extension value, not parsed image alternative content.
    metadata.attributes.remove("alt");
    metadata
        .attributes
        .set("link".into(), target.to_owned().into());
    Ok(
        Image::new(Source::from_str_borrowed(poster)?, video.location)
            .with_title(video.title)
            .with_metadata(metadata)?,
    )
}

fn revision_text(attributes: &TraversalContext<'_>) -> Option<String> {
    let revnumber = document_attribute_text((attributes).get("revnumber"));
    let revdate = document_attribute_text((attributes).get("revdate"));
    if revnumber.is_none() && revdate.is_none() {
        return None;
    }

    let mut revision = String::new();
    if let Some(revnumber) = revnumber {
        if let Some(label) = document_attribute_text((attributes).get("version-label"))
            && !label.is_empty()
        {
            revision.push_str(label);
            revision.push(' ');
        }
        revision.push_str(revnumber);
    }
    if let Some(revdate) = revdate {
        if !revision.is_empty() {
            revision.push_str(", ");
        }
        revision.push_str(revdate);
    }
    if let Some(revremark) = document_attribute_text((attributes).get("revremark")) {
        revision.push_str(": ");
        revision.push_str(revremark);
    }
    Some(revision)
}

impl<'a> DocumentVisitor<'a> for PdfVisitor<'a, '_, '_> {
    fn visit_document_start(
        &mut self,
        _traversal: &mut TraversalContext<'a>,
        _doc: DocumentRef<'a>,
    ) -> Result<(), Error> {
        self.page_numbering.write_initial(&mut self.writer);
        Ok(())
    }

    fn visit_body_content_start(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        _doc: DocumentRef<'a>,
    ) -> Result<(), Error> {
        if self.page_numbering.plan.starts_at_body() {
            self.page_numbering.start_arabic(&mut self.writer);
        }
        self.render_toc(traversal, None, "auto")?;
        self.page_numbering.start_arabic(&mut self.writer);
        Ok(())
    }

    fn visit_preamble_start(
        &mut self,
        _traversal: &mut TraversalContext<'a>,
        _doc: DocumentRef<'a>,
    ) -> Result<(), Error> {
        self.automatic_preamble_lead_state = AutomaticPreambleLeadState::Pending;
        Ok(())
    }

    fn visit_preamble_end(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        _doc: DocumentRef<'a>,
    ) -> Result<(), Error> {
        self.automatic_preamble_lead_state = AutomaticPreambleLeadState::Inactive;
        self.render_toc(traversal, None, "preamble")
    }

    fn visit_header(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        header: HeaderRef<'a>,
    ) -> Result<(), Error> {
        let title_page = self.page_numbering.plan.has_title_page();

        if let Some(anchor) = header.primary_anchor() {
            self.write_anchor_target(anchor);
        }

        if self.page_numbering.plan.starts_before_header() {
            self.page_numbering.start_arabic(&mut self.writer);
        }

        if title_page {
            self.writer
                .raw("#page(header: none, footer: none)[\n#v(30%)\n");
        }
        self.writer.raw("#align(center)[\n");
        self.writer.raw("#text(size: 22pt, weight: \"bold\")[");
        self.write_title(traversal, header.title())?;
        self.writer.raw("]\n");
        if let Some(subtitle) = header.subtitle() {
            self.writer.raw("#v(0.2em)\n");
            self.writer
                .raw("#text(size: 14pt, weight: \"bold\", style: \"italic\")[");
            self.write_title(traversal, Some(subtitle))?;
            self.writer.raw("]\n");
        }
        if title_page && header.authors().next().is_some() {
            self.writer.raw("#v(0.4em)\n");
            let authors = header
                .authors()
                .map(author_name)
                .collect::<Vec<_>>()
                .join(", ");
            self.write_text_expr(&authors);
            self.writer.raw("\n");
        }
        if title_page
            && let Some(revision) =
                revision_text(&TraversalContext::new(self.processor.document_attributes()))
        {
            self.writer.raw("#v(0.4em)\n#text(size: 9pt)[");
            self.write_text_expr(&revision);
            self.writer.raw("]\n");
        }
        self.writer.raw("]\n");
        if title_page {
            self.writer.raw("]\n\n");
        } else {
            self.writer.raw("#v(1em)\n\n");
        }
        Ok(())
    }
}

impl<'a> BlockVisitor<'a> for PdfVisitor<'a, '_, '_> {
    type Error = Error;

    fn before_block(
        &mut self,
        _traversal: &mut TraversalContext<'a>,
        block: BlockRef<'a>,
    ) -> Result<(), Error> {
        if self.automatic_preamble_lead_state == AutomaticPreambleLeadState::Pending
            && !matches!(
                block.kind(),
                BlockKind::Paragraph(_) | BlockKind::DocumentAttribute(_) | BlockKind::Comment(_)
            )
        {
            self.automatic_preamble_lead_state = AutomaticPreambleLeadState::Inactive;
        }

        if !matches!(
            block.kind(),
            BlockKind::PageBreak(_) | BlockKind::DocumentAttribute(_) | BlockKind::Comment(_)
        ) {
            self.explicit_page_break_state = ExplicitPageBreakState::Inactive;
        }

        Ok(())
    }

    fn visit_block(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        block: BlockRef<'a>,
    ) -> Result<(), Error> {
        match block.kind() {
            BlockKind::Section(node) => self.visit_section(traversal, node),
            BlockKind::Paragraph(node) => self.visit_paragraph(traversal, node),
            BlockKind::OrderedList(node) => self.visit_ordered_list(traversal, node),
            BlockKind::UnorderedList(node) => self.visit_unordered_list(traversal, node),
            BlockKind::DescriptionList(node) => self.visit_description_list(traversal, node),
            BlockKind::CalloutList(node) => self.visit_callout_list(traversal, node),
            BlockKind::Admonition(node) => self.visit_admonition(traversal, node),
            BlockKind::Image(node) => self.visit_image(traversal, node),
            BlockKind::Video(node) => self.visit_video(traversal, node),
            BlockKind::Audio(node) => self.visit_audio(traversal, node),
            BlockKind::ThematicBreak(node) => {
                self.visit_thematic_break(node);
                Ok(())
            }
            BlockKind::PageBreak(node) => {
                self.visit_page_break(node);
                Ok(())
            }
            BlockKind::TableOfContents(node) => self.visit_table_of_contents(traversal, node),
            BlockKind::DiscreteHeader(node) => self.visit_discrete_header(traversal, node),
            BlockKind::DocumentAttribute(_) | BlockKind::Comment(_) => Ok(()),
            BlockKind::Example(_)
            | BlockKind::Open(_)
            | BlockKind::Sidebar(_)
            | BlockKind::Quote(_)
            | BlockKind::Verse(_)
            | BlockKind::Listing(_)
            | BlockKind::Literal(_)
            | BlockKind::Passthrough(_)
            | BlockKind::Table(_)
            | BlockKind::Stem(_)
            | BlockKind::DelimitedComment(_) => self.visit_delimited_block(traversal, block),
            _ => {
                self.warn_unsupported_parser_variant("block", None);
                Ok(())
            }
        }
    }
}

impl<'a> PdfVisitor<'a, '_, '_> {
    fn visit_section(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        section: SectionRef<'a>,
    ) -> Result<(), Error> {
        let is_index_section = section.kind() == SectionKind::Index;
        let id = section.resolve_id();
        let label = self.anchors.section(section, self.in_asciidoc_table_cell());
        if is_index_section && !self.index_section_is_populated(&id) {
            return Ok(());
        }
        let in_asciidoc_table_cell = self.in_asciidoc_table_cell();
        if !in_asciidoc_table_cell && self.section_break_before(section) {
            self.writer.raw("#pagebreak(weak: true)\n\n");
        }

        let level = effective_section_level(section.level(), section.kind());
        let prefix = section.number().map_or_else(String::new, |number| {
            if section.kind() == SectionKind::Appendix {
                appendix_number_prefix(number, appendix_caption(traversal))
            } else if section.level() == 0 && section.kind() == SectionKind::Normal {
                part_number_prefix(number, part_signifier(traversal))
            } else {
                let signifier = (level == 1 && !in_asciidoc_table_cell)
                    .then(|| {
                        acdc_converters_core::section::book_chapter_signifier(
                            traversal,
                            self.chapter_signifier.as_deref(),
                        )
                    })
                    .flatten();
                section_number_prefix(number, signifier)
            }
        });

        let heading_level = level.max(1);
        if self.doctype == Doctype::Article && section.kind() == SectionKind::Abstract {
            self.write_abstract_title(traversal, section.title())?;
            if !id.is_empty() {
                let _ = write!(self.writer, " <{label}>");
            }
            self.writer.raw("\n#abstract[\n");
            let previous = self.in_article_abstract;
            self.in_article_abstract = true;
            let result = self.write_blocks(traversal, section.blocks());
            self.in_article_abstract = previous;
            self.writer.raw("]\n\n");
            return result;
        }

        let hidden_title = section.options().contains("notitle");
        if is_index_section && hidden_title {
            if !id.is_empty() {
                let _ = writeln!(self.writer, "#metadata(none) <{label}>");
            }
        } else {
            if hidden_title {
                self.writer.raw("#place[#hide[");
            }
            let _ = write!(self.writer, "#heading(level: {heading_level}");
            if self.in_article_abstract || in_asciidoc_table_cell {
                self.writer.raw(", outlined: false, bookmarked: false");
            }
            self.writer.raw(")[");
            if self.in_article_abstract {
                self.writer.raw("#text(style: \"normal\")[");
            }
            if !prefix.is_empty() {
                self.write_text_expr(&prefix);
            }
            self.write_title(traversal, section.title())?;
            if self.in_article_abstract {
                self.writer.raw("]");
            }
            self.writer.raw("]");
            if !id.is_empty() {
                let _ = write!(self.writer, " <{label}>");
            }
            if hidden_title {
                self.writer.raw("]]");
            }
            self.writer.raw("\n");
        }
        self.writer.raw("\n");
        if is_index_section {
            self.write_index_catalog(traversal);
            Ok(())
        } else {
            self.write_blocks(traversal, section.blocks())
        }
    }

    fn visit_paragraph(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        para: ParagraphRef<'_>,
    ) -> Result<(), Error> {
        let automatic_lead = self.automatic_preamble_lead_state
            == AutomaticPreambleLeadState::Pending
            && para.roles().is_empty();
        self.automatic_preamble_lead_state = AutomaticPreambleLeadState::Inactive;
        let unbreakable = is_unbreakable_paragraph(para);
        if unbreakable {
            self.writer.raw("#_acdc_unbreakable[\n");
        }
        self.write_block_anchor(para.primary_anchor());
        let result = self.write_paragraph_content(traversal, para, true, automatic_lead);
        if unbreakable {
            self.writer.raw("]\n\n");
        }
        result
    }

    fn visit_delimited_block(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        block: BlockRef<'a>,
    ) -> Result<(), Error> {
        let unbreakable = is_unbreakable_delimited_block(block);
        if unbreakable {
            self.writer.raw("#_acdc_unbreakable[\n");
        }
        let sticky_anchor = is_page_breakable_table(block) && block.primary_anchor().is_some();
        if sticky_anchor {
            self.writer
                .raw("#block(sticky: true, above: 0pt, below: 0pt)[\n");
        }
        self.write_block_anchor(block.primary_anchor());
        if sticky_anchor {
            self.writer.raw("]\n");
        }

        #[cfg(feature = "pre-spec-subs")]
        let previous_subs = self.processor.current_subs.replace(effective_subs_flags(
            block
                .attributes()
                .and_then(|attributes| match attributes.get("subs") {
                    Some(acdc_parser::ast::AttributeValueRef::Substitutions(spec)) => Some(spec),
                    _ => None,
                }),
            matches!(
                block.kind(),
                BlockKind::Listing(_) | BlockKind::Literal(_) | BlockKind::Passthrough(_)
            ),
        ));

        let result = self.write_delimited_block_content(traversal, block);

        if unbreakable {
            self.writer.raw("]\n\n");
        }

        #[cfg(feature = "pre-spec-subs")]
        self.processor.current_subs.set(previous_subs);
        result
    }

    fn visit_ordered_list(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        list: OrderedListRef<'a>,
    ) -> Result<(), Error> {
        self.write_block_anchor(list.primary_anchor());
        self.write_block_title(traversal, list.title())?;
        self.write_ordered_list_start(Some(list.attributes()), list.marker());
        self.list_depth += 1;
        for item in list.items() {
            self.write_list_item(traversal, "+", item)?;
        }
        self.list_depth -= 1;
        let indent = "  ".repeat(self.list_depth);
        let _ = writeln!(self.writer, "{indent}]");
        self.write_ordered_list_end(Some(list.attributes()));
        self.writer.raw("\n");
        Ok(())
    }

    fn visit_unordered_list(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        list: UnorderedListRef<'a>,
    ) -> Result<(), Error> {
        self.write_block_anchor(list.primary_anchor());
        self.write_block_title(traversal, list.title())?;
        if list.style() == Some("bibliography") {
            return self.write_bibliography_list(traversal, list);
        }
        let styled = self.write_unordered_list_start(Some(list.attributes()));
        self.list_depth += 1;
        self.unordered_list_depth += 1;
        for item in list.items() {
            self.write_list_item(traversal, "-", item)?;
        }
        self.unordered_list_depth -= 1;
        self.list_depth -= 1;
        if styled {
            let indent = "  ".repeat(self.list_depth);
            let _ = writeln!(self.writer, "{indent}]");
            self.write_unordered_list_end();
        }
        self.writer.raw("\n");
        Ok(())
    }

    fn visit_description_list(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        list: DescriptionListRef<'a>,
    ) -> Result<(), Error> {
        self.write_block_anchor(list.primary_anchor());
        if matches!(list.style(), Some("ordered" | "unordered")) {
            return self.write_marker_description_list(traversal, list);
        }
        self.write_block_title(traversal, list.title())?;
        if list.style() == Some("horizontal") {
            return self.write_horizontal_description_list(traversal, list);
        }
        if list.style() == Some("qanda") {
            return self.write_qanda_description_list(traversal, list);
        }
        self.write_description_list(traversal, list)
    }

    fn visit_callout_list(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        list: CalloutListRef<'a>,
    ) -> Result<(), Error> {
        self.write_block_anchor(list.primary_anchor());
        self.write_block_title(traversal, list.title())?;
        self.writer.raw(
            "#grid(columns: (auto, 1fr), column-gutter: 0.5em, row-gutter: 0.5em, align: (x, _) => if x == 0 { right + top } else { left + top },\n",
        );
        for item in list.items() {
            self.writer.raw("[");
            self.write_text_expr(&format!("({})", item.callout().number()));
            self.writer.raw("], [");
            self.write_content(traversal, item.principal())?;
            if !item.principal().is_empty() && item.blocks().next().is_some() {
                self.writer.raw("\n\n");
            }
            self.write_blocks(traversal, item.blocks())?;
            self.writer.raw("],\n");
        }
        self.writer.raw(")\n\n");
        Ok(())
    }

    fn visit_admonition(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        admon: AdmonitionRef<'a>,
    ) -> Result<(), Error> {
        let unbreakable = admon.options().contains("unbreakable");
        if unbreakable {
            self.writer.raw("#_acdc_unbreakable[\n");
        }
        let result = (|| {
            self.write_block_anchor(admon.primary_anchor());
            let kind = match admon.variant() {
                AdmonitionVariant::Note => "note",
                AdmonitionVariant::Tip => "tip",
                AdmonitionVariant::Important => "important",
                AdmonitionVariant::Caution => "caution",
                AdmonitionVariant::Warning => "warning",
            };
            let image_icon = (IconMode::from_attributes(traversal) != IconMode::Text)
                .then(|| attribute_text(admon.attributes(), "icon"))
                .flatten()
                .filter(|icon| !icon.is_empty())
                .map(|icon| admonition_icon_source(traversal, icon))
                .and_then(|source| self.asset_virtual_path(&source));
            if let Some(path) = image_icon {
                self.writer.raw("#_acdc_image_callout(image(");
                self.writer.string_literal(&path);
                self.writer.raw(", width: 36pt, alt: ");
                let alt = admon.caption_content().map(|content| {
                    acdc_converters_core::InlineTextTransform::default().content_to_string(content)
                });
                self.writer.string_literal(alt.as_deref().unwrap_or(kind));
                self.writer.raw("))[\n");
            } else if let Some(content) = admon.caption_content() {
                self.writer.raw("#_acdc_image_callout([");
                self.write_content(traversal, content)?;
                self.writer.raw("])[\n");
            } else {
                self.writer.raw("#callout(");
                self.writer.string_literal(kind);
                self.writer.raw(")[\n");
            }
            if admon.title().is_some() {
                self.writer.raw("#admonitiontitle[");
                self.write_title(traversal, admon.title())?;
                self.writer.raw("]\n");
            }
            if let Some(paragraph) = admon.simple_paragraph() {
                // Keep the paragraph's substitutions without repeating the wrapper's title and anchor.
                self.write_paragraph_content(traversal, paragraph, false, false)?;
            } else {
                self.write_blocks(traversal, admon.blocks())?;
            }
            self.writer.raw("]\n\n");
            Ok(())
        })();
        if unbreakable {
            self.writer.raw("]\n\n");
        }
        result
    }

    fn visit_image(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        img: ImageRef<'_>,
    ) -> Result<(), Error> {
        self.write_block_anchor(img.primary_anchor());
        self.write_block_image(traversal, img)
    }

    fn visit_video(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        video: VideoRef<'_>,
    ) -> Result<(), Error> {
        self.write_block_anchor(video.primary_anchor());
        self.warn_static_media_fallback(&video.location().into());
        let kind = match attribute_text(video.attributes(), "poster") {
            Some("youtube") => "YouTube video",
            Some("vimeo") => "Vimeo video",
            _ => "video",
        };
        let target = acdc_converters_core::video::TryUrl::try_url(&video, false)
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        let sources = [(Self::static_media_target(traversal, &target), kind)];

        let poster = attribute_text(video.attributes(), "poster")
            .filter(|poster| !poster.is_empty() && !matches!(*poster, "youtube" | "vimeo"));
        let poster_target = poster.map(|poster| Self::static_media_target(traversal, poster));
        let poster_rendered = if let (Some(poster), Some(poster_target), Some((target, _))) =
            (poster, poster_target.as_deref(), sources.first())
            && self.has_asset(poster_target)
        {
            let image = poster_image(video, poster, target)?;
            self.write_block_image(traversal, ImageRef::from(&image))?;
            true
        } else {
            false
        };

        for (target, kind) in sources.iter().skip(usize::from(poster_rendered)) {
            self.write_static_media_link(target, kind);
            self.writer.raw("\n");
        }
        if !poster_rendered {
            self.write_static_media_caption(traversal, video.title())?;
        }
        self.writer.raw("\n\n");
        Ok(())
    }

    fn visit_audio(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        audio: AudioRef<'_>,
    ) -> Result<(), Error> {
        self.write_block_anchor(audio.primary_anchor());
        self.warn_static_media_fallback(&audio.location().into());
        let target = Self::static_media_source_target(traversal, audio.source());
        self.write_static_media_link(&target, "audio");
        self.writer.raw("\n");
        self.write_static_media_caption(traversal, audio.title())?;
        self.writer.raw("\n\n");
        Ok(())
    }

    fn visit_thematic_break(&mut self, br: ThematicBreakRef<'_>) {
        if let Some(anchor) = br.anchors().next() {
            self.write_anchor_target(anchor);
        }
        self.writer.raw("#hr()\n\n");
    }

    fn visit_page_break(&mut self, br: PageBreakRef<'_>) {
        self.write_block_anchor(br.primary_anchor());
        if br.options().contains("always") {
            if self.explicit_page_break_state == ExplicitPageBreakState::Weak {
                self.writer.raw("#pagebreak()\n");
            }
            self.writer.raw("#pagebreak()\n\n");
            self.explicit_page_break_state = ExplicitPageBreakState::Inactive;
        } else {
            self.writer.raw("#pagebreak(weak: true)\n\n");
            self.explicit_page_break_state = ExplicitPageBreakState::Weak;
        }
        // Apply the layout after the break so an empty page gets the new geometry.
        self.write_page_layout_change(br);
    }

    fn visit_table_of_contents(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        toc: TableOfContentsRef<'_>,
    ) -> Result<(), Error> {
        self.write_block_anchor(toc.primary_anchor());
        self.render_toc(traversal, Some(toc), "macro")
    }

    fn visit_discrete_header(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        header: DiscreteHeaderRef<'_>,
    ) -> Result<(), Error> {
        self.write_block_anchor(header.primary_anchor());
        let level = header.level().max(1);
        let _ = write!(self.writer, "#heading(level: {level}, outlined: false)[");
        self.write_title(traversal, header.title())?;
        self.writer.raw("]\n\n");
        Ok(())
    }
}

impl<'a> InlineVisitor<'a> for PdfVisitor<'a, '_, '_> {
    type Error = Error;

    fn visit_inline_nodes(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        content: ContentRef<'_>,
    ) -> Result<(), Error> {
        self.write_inline_refs(traversal, content.inlines())
    }

    fn visit_inline_node(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        node: InlineRef<'_>,
    ) -> Result<(), Self::Error> {
        let previous_inline_span = self.in_inline_span;
        let formatting = is_formatting_span(node);
        self.in_inline_span |= formatting;
        let result = if formatting {
            self.write_formatting_span(traversal, node)
        } else {
            self.write_inline_node_content(traversal, node)
        };
        self.in_inline_span = previous_inline_span;
        result
    }
}

impl<'a> PdfVisitor<'a, '_, '_> {
    fn write_formatting_span(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        node: InlineRef<'_>,
    ) -> Result<(), Error> {
        if let InlineKind::Bold(bold) = node.kind() {
            self.write_quoted_span(
                traversal,
                bold.id(),
                bold.role(),
                "#strong[",
                bold.content().inlines(),
                "]",
            )?;
        } else if let InlineKind::Italic(italic) = node.kind() {
            self.write_quoted_span(
                traversal,
                italic.id(),
                italic.role(),
                "#emph[",
                italic.content().inlines(),
                "]",
            )?;
        } else if let InlineKind::Monospace(mono) = node.kind() {
            let state = self.write_inline_span_start(
                mono.id().map(acdc_parser::ast::ElementIdRef::as_str),
                mono.role(),
            );
            self.write_inline_verbatim_nodes(traversal, mono.content())?;
            self.write_inline_span_end(state);
        } else if let InlineKind::Highlight(highlight) = node.kind() {
            let (prefix, suffix) = if highlight.id().is_some() || highlight.role().is_some() {
                ("", "")
            } else {
                ("#highlight[", "]")
            };
            self.write_quoted_span(
                traversal,
                highlight.id(),
                highlight.role(),
                prefix,
                highlight.content().inlines(),
                suffix,
            )?;
        } else if let InlineKind::Subscript(sub) = node.kind() {
            self.write_quoted_span(
                traversal,
                sub.id(),
                sub.role(),
                "#sub[",
                sub.content().inlines(),
                "]",
            )?;
        } else if let InlineKind::Superscript(sup) = node.kind() {
            self.write_quoted_span(
                traversal,
                sup.id(),
                sup.role(),
                "#super[",
                sup.content().inlines(),
                "]",
            )?;
        } else if let InlineKind::CurvedQuotation(quoted) = node.kind() {
            let state = self.write_inline_span_start(
                quoted.id().map(acdc_parser::ast::ElementIdRef::as_str),
                quoted.role(),
            );
            self.write_text_expr("\u{201C}");
            self.write_inline_refs(traversal, quoted.content().inlines())?;
            self.write_text_expr("\u{201D}");
            self.write_inline_span_end(state);
        } else if let InlineKind::CurvedApostrophe(quoted) = node.kind() {
            let state = self.write_inline_span_start(
                quoted.id().map(acdc_parser::ast::ElementIdRef::as_str),
                quoted.role(),
            );
            self.write_text_expr("\u{2018}");
            self.write_inline_refs(traversal, quoted.content().inlines())?;
            self.write_text_expr("\u{2019}");
            self.write_inline_span_end(state);
        } else {
            self.warn_unsupported_parser_variant("formatting span", Some(&node.location().into()));
        }
        Ok(())
    }

    fn write_inline_node_content(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        node: InlineRef<'_>,
    ) -> Result<(), Error> {
        match node.kind() {
            InlineKind::Plain(plain) => self.write_plain(plain.text()),
            InlineKind::Symbol(symbol) => self.write_text_expr(symbol.text()),
            InlineKind::Raw(raw) => self.write_raw(raw),
            InlineKind::Verbatim(verbatim) => self.write_text_expr(verbatim.text()),
            InlineKind::StandaloneCurvedApostrophe(_) => self.write_text_expr("\u{2019}"),
            InlineKind::LineBreak(_) => self.writer.raw("#linebreak()"),
            InlineKind::Anchor(anchor) => self.write_inline_anchor(anchor.id()),
            InlineKind::Callout(callout) => {
                self.write_text_expr(&format!("({})", callout.number()));
            }
            InlineKind::Footnote(_)
            | InlineKind::Icon(_)
            | InlineKind::Image(_)
            | InlineKind::Keyboard(_)
            | InlineKind::Button(_)
            | InlineKind::Menu(_)
            | InlineKind::Url(_)
            | InlineKind::Link(_)
            | InlineKind::Mailto(_)
            | InlineKind::Autolink(_)
            | InlineKind::CrossReference(_)
            | InlineKind::Pass(_)
            | InlineKind::Stem(_)
            | InlineKind::IndexTerm(_) => self.write_inline_macro(traversal, node)?,
            InlineKind::Bold(_)
            | InlineKind::Italic(_)
            | InlineKind::Monospace(_)
            | InlineKind::Highlight(_)
            | InlineKind::Subscript(_)
            | InlineKind::Superscript(_)
            | InlineKind::CurvedQuotation(_)
            | InlineKind::CurvedApostrophe(_)
            | _ => {
                self.warn_unsupported_parser_variant("inline node", Some(&node.location().into()));
            }
        }
        Ok(())
    }
}

fn appendix_caption<'doc>(attributes: &TraversalContext<'doc>) -> Option<&'doc str> {
    match attributes
        .get("appendix-caption")
        .and_then(|value| value.text())
    {
        Some(value) => Some(value),
        None if attributes.is_explicit("appendix-caption") => None,
        None => Some("Appendix"),
    }
}

fn part_signifier<'doc>(attributes: &TraversalContext<'doc>) -> Option<&'doc str> {
    match attributes
        .get("part-signifier")
        .and_then(|value| value.text())
    {
        Some(value) => Some(value),
        None if attributes.is_explicit("part-signifier") => None,
        None => Some("Part"),
    }
}

#[cfg(test)]
mod poster_tests {
    use super::*;
    use acdc_parser::{
        Anchor, BlockMetadata, ContentAttribute, ElementAttributes, Location, Title,
    };

    #[test]
    fn poster_transfer_keeps_compatible_metadata_and_the_original_video()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut attributes = ElementAttributes::default();
        attributes.set("alt".into(), "opaque video extension".into());
        attributes.set("width".into(), "120".into());
        attributes.set("link".into(), "old".into());
        attributes.set("custom".into(), "".into());
        let mut metadata = BlockMetadata::new().with_attributes(attributes)?;
        metadata.caption = Some(Caption::CustomContent(ContentAttribute::default()));
        metadata.id = Some(Anchor::new("clip", Location::default()));
        metadata.roles.push("wide");
        metadata.options.push("loop");
        let video = Video::new(Source::Name("clip"), Location::default())
            .with_title(Title::new(Vec::new()))
            .with_metadata(metadata)?;
        let before = video.clone();
        let image = poster_image(VideoRef::from(&video), "poster.svg", "clip.mp4#t=2")?;
        assert_eq!(video, before);
        assert_eq!(image.title, video.title);
        assert_eq!(image.metadata.id, video.metadata.id);
        assert_eq!(image.metadata.roles, video.metadata.roles);
        assert_eq!(image.metadata.options, video.metadata.options);
        assert_eq!(image.metadata.caption, Some(Caption::Unnumbered));
        assert_eq!(
            image.metadata.attributes.get_string("link").as_deref(),
            Some("clip.mp4#t=2")
        );
        assert_eq!(
            image.metadata.attributes.get_string("width").as_deref(),
            Some("120")
        );
        assert_eq!(
            image.metadata.attributes.get_string("custom").as_deref(),
            Some("")
        );
        assert!(image.alt.is_none());
        assert!(!image.metadata.attributes.contains_key("alt"));
        Ok(())
    }

    #[test]
    fn poster_transfer_reports_conflicting_caller_metadata()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut video = Video::new(Source::Name("clip"), Location::default());
        video
            .metadata
            .attributes
            .set("roles".into(), "shadow".into());
        let before = video.clone();
        let error = poster_image(VideoRef::from(&video), "poster.svg", "clip.mp4")
            .err()
            .ok_or("invalid metadata accepted")?;
        assert!(matches!(error, Error::ReservedAttribute(error) if error.name == "roles"));
        assert_eq!(video, before);
        Ok(())
    }
}
