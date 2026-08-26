use std::{borrow::Cow, fmt::Write as _};

#[cfg(feature = "pre-spec-subs")]
use acdc_converters_core::substitutions::effective_subs_flags;
use acdc_converters_core::{
    Doctype,
    icon::IconMode,
    inlines_to_string,
    section::{
        appendix_number_prefix, effective_section_level, part_number_prefix, section_number_prefix,
    },
    visitor::Visitor,
};
use acdc_parser::{
    Admonition, AdmonitionVariant, AttributeValue, Audio, Block, CalloutList, Caption,
    DelimitedBlock, DescriptionList, DiscreteHeader, Header, Image, InlineNode, ListItem,
    OrderedList, PageBreak, Paragraph, Section, Source, TableOfContents, ThematicBreak,
    UnorderedList, Video,
};

use crate::{
    Error, PdfVisitor, admonition_icon_source, author_name, encode_label, is_page_breakable_table,
    is_unbreakable_delimited_block, is_unbreakable_paragraph,
    pdf_visitor::{AutomaticPreambleLeadState, ExplicitPageBreakState},
};

fn revision_text(attributes: &acdc_parser::DocumentAttributes<'_>) -> Option<String> {
    let revnumber = attributes.get_string("revnumber");
    let revdate = attributes.get_string("revdate");
    if revnumber.is_none() && revdate.is_none() {
        return None;
    }

    let mut revision = String::new();
    if let Some(revnumber) = revnumber {
        if let Some(label) = attributes.get_string("version-label")
            && !label.is_empty()
        {
            revision.push_str(&label);
            revision.push(' ');
        }
        revision.push_str(&revnumber);
    }
    if let Some(revdate) = revdate {
        if !revision.is_empty() {
            revision.push_str(", ");
        }
        revision.push_str(&revdate);
    }
    if let Some(revremark) = attributes.get_string("revremark") {
        revision.push_str(": ");
        revision.push_str(&revremark);
    }
    Some(revision)
}

impl Visitor for PdfVisitor<'_, '_, '_> {
    type Error = Error;

    fn visit_document_start(
        &mut self,
        _doc: &acdc_parser::Document<'_>,
    ) -> Result<(), Self::Error> {
        self.page_numbering.write_initial(&mut self.writer);
        Ok(())
    }

    fn visit_block(&mut self, block: &Block<'_>) -> Result<(), Self::Error> {
        if self.automatic_preamble_lead_state == AutomaticPreambleLeadState::Pending
            && !matches!(
                block,
                Block::Paragraph(_) | Block::DocumentAttribute(_) | Block::Comment(_)
            )
        {
            self.automatic_preamble_lead_state = AutomaticPreambleLeadState::Inactive;
        }

        if !matches!(
            block,
            Block::PageBreak(_) | Block::DocumentAttribute(_) | Block::Comment(_)
        ) {
            self.explicit_page_break_state = ExplicitPageBreakState::Inactive;
        }

        match block {
            Block::Section(section) => self.visit_section(section),
            Block::Paragraph(para) => self.visit_paragraph(para),
            Block::DelimitedBlock(delimited) => self.visit_delimited_block(delimited),
            Block::OrderedList(list) => self.visit_ordered_list(list),
            Block::UnorderedList(list) => self.visit_unordered_list(list),
            Block::DescriptionList(list) => self.visit_description_list(list),
            Block::CalloutList(list) => self.visit_callout_list(list),
            Block::Admonition(admon) => self.visit_admonition(admon),
            Block::Image(img) => self.visit_image(img),
            Block::Video(video) => self.visit_video(video),
            Block::Audio(audio) => self.visit_audio(audio),
            Block::ThematicBreak(br) => self.visit_thematic_break(br),
            Block::PageBreak(br) => self.visit_page_break(br),
            Block::TableOfContents(toc) => self.visit_table_of_contents(toc),
            Block::DiscreteHeader(header) => self.visit_discrete_header(header),
            Block::DocumentAttribute(_) | Block::Comment(_) => Ok(()),
            _ => self.visit_unhandled_block(block),
        }
    }

    fn visit_unhandled_block(&mut self, _block: &Block<'_>) -> Result<(), Self::Error> {
        self.warn_unsupported_parser_variant("block", None);
        Ok(())
    }

    fn visit_body_content_start(
        &mut self,
        _doc: &acdc_parser::Document<'_>,
    ) -> Result<(), Self::Error> {
        if self.page_numbering.plan.starts_at_body() {
            self.page_numbering.start_arabic(&mut self.writer);
        }
        self.render_toc(None, "auto")?;
        self.page_numbering.start_arabic(&mut self.writer);
        Ok(())
    }

    fn visit_preamble_start(
        &mut self,
        _doc: &acdc_parser::Document<'_>,
    ) -> Result<(), Self::Error> {
        self.automatic_preamble_lead_state = AutomaticPreambleLeadState::Pending;
        Ok(())
    }

    fn visit_preamble_end(&mut self, _doc: &acdc_parser::Document<'_>) -> Result<(), Self::Error> {
        self.automatic_preamble_lead_state = AutomaticPreambleLeadState::Inactive;
        self.render_toc(None, "preamble")
    }

    fn visit_header(&mut self, header: &Header<'_>) -> Result<(), Self::Error> {
        let title_page = self.page_numbering.plan.has_title_page();

        if self.page_numbering.plan.starts_before_header() {
            self.page_numbering.start_arabic(&mut self.writer);
        }

        if title_page {
            self.writer
                .raw("#page(header: none, footer: none)[\n#v(30%)\n");
        }
        self.writer.raw("#align(center)[\n");
        self.writer.raw("#text(size: 22pt, weight: \"bold\")[");
        self.write_title(&header.title)?;
        self.writer.raw("]\n");
        if let Some(subtitle) = &header.subtitle {
            self.writer.raw("#v(0.2em)\n");
            self.writer
                .raw("#text(size: 14pt, weight: \"bold\", style: \"italic\")[");
            self.write_title(subtitle)?;
            self.writer.raw("]\n");
        }
        if title_page && !header.authors.is_empty() {
            self.writer.raw("#v(0.4em)\n");
            let authors = header
                .authors
                .iter()
                .map(author_name)
                .collect::<Vec<_>>()
                .join(", ");
            self.write_text_expr(&authors);
            self.writer.raw("\n");
        }
        if title_page && let Some(revision) = revision_text(self.processor.document_attributes()) {
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

    fn visit_section(&mut self, section: &Section<'_>) -> Result<(), Self::Error> {
        let is_index_section = section.kind == acdc_parser::SectionKind::Index;
        let id =
            acdc_parser::Section::generate_id_string(&section.metadata, section.title.as_ref());
        if is_index_section && !self.index_section_is_populated(&id) {
            return Ok(());
        }
        let in_asciidoc_table_cell = self.in_asciidoc_table_cell();
        if !in_asciidoc_table_cell && self.section_break_before(section) {
            self.writer.raw("#pagebreak(weak: true)\n\n");
        }

        let level = effective_section_level(section.level, section.kind);
        let prefix = section.number().map_or_else(String::new, |number| {
            if section.kind == acdc_parser::SectionKind::Appendix {
                appendix_number_prefix(
                    number,
                    appendix_caption(self.processor.document_attributes()),
                )
            } else if section.level == 0 && section.kind == acdc_parser::SectionKind::Normal {
                part_number_prefix(number, part_signifier(self.processor.document_attributes()))
            } else {
                let signifier = (level == 1 && !in_asciidoc_table_cell)
                    .then_some(self.chapter_signifier.as_deref())
                    .flatten();
                section_number_prefix(number, signifier)
            }
        });

        let heading_level = level.max(1);
        if self.doctype == Doctype::Article && section.kind == acdc_parser::SectionKind::Abstract {
            self.write_abstract_title(&section.title)?;
            if !id.is_empty() {
                let _ = write!(self.writer, " <{}>", encode_label(&id));
            }
            self.writer.raw("\n#abstract[\n");
            let previous = self.in_article_abstract;
            self.in_article_abstract = true;
            let result = self.write_blocks(&section.content);
            self.in_article_abstract = previous;
            self.writer.raw("]\n\n");
            return result;
        }

        let hidden_title = section.metadata.options.contains(&"notitle");
        if is_index_section && hidden_title {
            if !id.is_empty() {
                let _ = writeln!(self.writer, "#metadata(none) <{}>", encode_label(&id));
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
            self.write_title(&section.title)?;
            if self.in_article_abstract {
                self.writer.raw("]");
            }
            self.writer.raw("]");
            if !id.is_empty() {
                let _ = write!(self.writer, " <{}>", encode_label(&id));
            }
            if hidden_title {
                self.writer.raw("]]");
            }
            self.writer.raw("\n");
        }
        self.writer.raw("\n");
        if is_index_section {
            self.write_index_catalog();
            Ok(())
        } else {
            self.write_blocks(&section.content)
        }
    }

    fn visit_paragraph(&mut self, para: &Paragraph<'_>) -> Result<(), Self::Error> {
        let automatic_lead = self.automatic_preamble_lead_state
            == AutomaticPreambleLeadState::Pending
            && para.metadata.roles.is_empty();
        self.automatic_preamble_lead_state = AutomaticPreambleLeadState::Inactive;
        let unbreakable = is_unbreakable_paragraph(para);
        if unbreakable {
            self.writer.raw("#_acdc_unbreakable[\n");
        }
        self.write_block_anchor(&para.metadata);
        let result = self.write_paragraph_content(para, true, automatic_lead);
        if unbreakable {
            self.writer.raw("]\n\n");
        }
        result
    }

    fn visit_delimited_block(&mut self, block: &DelimitedBlock<'_>) -> Result<(), Self::Error> {
        let unbreakable = is_unbreakable_delimited_block(block);
        if unbreakable {
            self.writer.raw("#_acdc_unbreakable[\n");
        }
        let sticky_anchor = is_page_breakable_table(block)
            && (block.metadata.id.is_some() || !block.metadata.anchors.is_empty());
        if sticky_anchor {
            self.writer
                .raw("#block(sticky: true, above: 0pt, below: 0pt)[\n");
        }
        self.write_block_anchor(&block.metadata);
        if sticky_anchor {
            self.writer.raw("]\n");
        }

        #[cfg(feature = "pre-spec-subs")]
        let previous_subs = self.processor.current_subs.replace(effective_subs_flags(
            block.metadata.substitutions.as_ref(),
            matches!(
                block.inner,
                acdc_parser::DelimitedBlockType::DelimitedListing(_)
                    | acdc_parser::DelimitedBlockType::DelimitedLiteral(_)
                    | acdc_parser::DelimitedBlockType::DelimitedPass(_)
                    | acdc_parser::DelimitedBlockType::DelimitedVerse(_)
            ),
        ));

        let result = self.write_delimited_block_content(block);

        if unbreakable {
            self.writer.raw("]\n\n");
        }

        #[cfg(feature = "pre-spec-subs")]
        self.processor.current_subs.set(previous_subs);
        result
    }

    fn visit_ordered_list(&mut self, list: &OrderedList<'_>) -> Result<(), Self::Error> {
        self.write_block_anchor(&list.metadata);
        self.write_block_title(&list.title)?;
        self.write_ordered_list_start(&list.metadata, list.marker);
        self.list_depth += 1;
        for item in &list.items {
            self.write_list_item("+", item)?;
        }
        self.list_depth -= 1;
        let indent = "  ".repeat(self.list_depth);
        let _ = writeln!(self.writer, "{indent}]");
        self.write_ordered_list_end(&list.metadata);
        self.writer.raw("\n");
        Ok(())
    }

    fn visit_unordered_list(&mut self, list: &UnorderedList<'_>) -> Result<(), Self::Error> {
        self.write_block_anchor(&list.metadata);
        self.write_block_title(&list.title)?;
        if list.metadata.style == Some("bibliography") {
            return self.write_bibliography_list(list);
        }
        let styled = self.write_unordered_list_start(&list.metadata);
        self.list_depth += 1;
        self.unordered_list_depth += 1;
        for item in &list.items {
            self.write_list_item("-", item)?;
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

    fn visit_description_list(&mut self, list: &DescriptionList<'_>) -> Result<(), Self::Error> {
        self.write_block_anchor(&list.metadata);
        if matches!(list.metadata.style, Some("ordered" | "unordered")) {
            return self.write_marker_description_list(list);
        }
        self.write_block_title(&list.title)?;
        if list.metadata.style == Some("horizontal") {
            return self.write_horizontal_description_list(list);
        }
        if list.metadata.style == Some("qanda") {
            return self.write_qanda_description_list(list);
        }
        self.write_description_list(list)
    }

    fn visit_callout_list(&mut self, list: &CalloutList<'_>) -> Result<(), Self::Error> {
        self.write_block_anchor(&list.metadata);
        self.write_block_title(&list.title)?;
        self.writer.raw(
            "#grid(columns: (auto, 1fr), column-gutter: 0.5em, row-gutter: 0.5em, align: (x, _) => if x == 0 { right + top } else { left + top },\n",
        );
        for item in &list.items {
            self.writer.raw("[");
            self.write_text_expr(&format!("({})", item.callout.number));
            self.writer.raw("], [");
            self.write_inlines(&item.principal)?;
            if !item.principal.is_empty() && !item.blocks.is_empty() {
                self.writer.raw("\n\n");
            }
            self.write_blocks(&item.blocks)?;
            self.writer.raw("],\n");
        }
        self.writer.raw(")\n\n");
        Ok(())
    }

    fn visit_list_item(&mut self, _item: &ListItem<'_>) -> Result<(), Self::Error> {
        Ok(())
    }

    fn visit_admonition(&mut self, admon: &Admonition<'_>) -> Result<(), Self::Error> {
        let unbreakable = admon.metadata.options.contains(&"unbreakable");
        if unbreakable {
            self.writer.raw("#_acdc_unbreakable[\n");
        }
        let result = (|| {
            self.write_block_anchor(&admon.metadata);
            let kind = match admon.variant {
                AdmonitionVariant::Note => "note",
                AdmonitionVariant::Tip => "tip",
                AdmonitionVariant::Important => "important",
                AdmonitionVariant::Caution => "caution",
                AdmonitionVariant::Warning => "warning",
            };
            let image_icon = (IconMode::from(self.processor.document_attributes())
                != IconMode::Text)
                .then(|| admon.metadata.attributes.get_string("icon"))
                .flatten()
                .filter(|icon| !icon.is_empty())
                .map(|icon| admonition_icon_source(self.processor.document_attributes(), &icon))
                .and_then(|source| self.asset_virtual_path(&source));
            if let Some(path) = image_icon {
                self.writer.raw("#_acdc_image_callout(image(");
                self.writer.string_literal(&path);
                self.writer.raw(", width: 36pt, alt: ");
                self.writer.string_literal(kind);
                self.writer.raw("))[\n");
            } else {
                self.writer.raw("#callout(");
                self.writer.string_literal(kind);
                self.writer.raw(")[\n");
            }
            if !admon.title.is_empty() {
                self.writer.raw("#admonitiontitle[");
                self.write_title(&admon.title)?;
                self.writer.raw("]\n");
            }
            match admon.blocks.as_slice() {
                [Block::Paragraph(para)] if para.metadata == admon.metadata => {
                    // The parser copies a simple admonition's metadata to its
                    // synthetic paragraph. Render that paragraph without its anchor
                    // and title, which the admonition wrapper already wrote, but
                    // through the same content path so `[subs=…]` still applies.
                    self.write_paragraph_content(para, false, false)?;
                }
                blocks => self.write_blocks(blocks)?,
            }
            self.writer.raw("]\n\n");
            Ok(())
        })();
        if unbreakable {
            self.writer.raw("]\n\n");
        }
        result
    }

    fn visit_image(&mut self, img: &Image<'_>) -> Result<(), Self::Error> {
        self.write_block_anchor(&img.metadata);
        self.write_block_image(img)
    }

    fn visit_video(&mut self, video: &Video<'_>) -> Result<(), Self::Error> {
        self.write_block_anchor(&video.metadata);
        self.warn_static_media_fallback(&video.location);
        let youtube = matches!(
            video.metadata.attributes.get("youtube"),
            Some(AttributeValue::Bool(true))
        );
        let is_vimeo = matches!(
            video.metadata.attributes.get("vimeo"),
            Some(AttributeValue::Bool(true))
        );
        let sources = video
            .sources
            .iter()
            .map(|source| {
                if youtube {
                    (
                        format!("https://www.youtube.com/watch?v={source}"),
                        "YouTube video",
                    )
                } else if is_vimeo {
                    (format!("https://vimeo.com/{source}"), "Vimeo video")
                } else {
                    (self.static_media_source_target(source), "video")
                }
            })
            .collect::<Vec<_>>();

        let poster = video
            .metadata
            .attributes
            .get_string("poster")
            .filter(|poster| !poster.is_empty());
        let poster_target = poster
            .as_deref()
            .map(|poster| self.static_media_target(poster));
        let poster_rendered = if let (Some(poster), Some(poster_target), Some((target, _))) =
            (poster.as_deref(), poster_target.as_deref(), sources.first())
            && self.has_asset(poster_target)
        {
            let mut metadata = video.metadata.clone();
            metadata.caption = Some(Caption::Unnumbered);
            metadata.attributes.set(
                Cow::Borrowed("link"),
                AttributeValue::String(Cow::Owned(target.clone())),
            );
            let image = Image::new(Source::from_str_borrowed(poster)?, video.location.clone())
                .with_title(video.title.clone())
                .with_metadata(metadata);
            self.write_block_image(&image)?;
            true
        } else {
            false
        };

        for (target, kind) in sources.iter().skip(usize::from(poster_rendered)) {
            self.write_static_media_link(target, kind);
            self.writer.raw("\n");
        }
        if !poster_rendered {
            self.write_static_media_caption(&video.title)?;
        }
        self.writer.raw("\n\n");
        Ok(())
    }

    fn visit_audio(&mut self, audio: &Audio<'_>) -> Result<(), Self::Error> {
        self.write_block_anchor(&audio.metadata);
        self.warn_static_media_fallback(&audio.location);
        let target = self.static_media_source_target(&audio.source);
        self.write_static_media_link(&target, "audio");
        self.writer.raw("\n");
        self.write_static_media_caption(&audio.title)?;
        self.writer.raw("\n\n");
        Ok(())
    }

    fn visit_thematic_break(&mut self, br: &ThematicBreak<'_>) -> Result<(), Self::Error> {
        if let Some(anchor) = br.anchors.first() {
            self.write_anchor_target(anchor);
        }
        self.writer.raw("#hr()\n\n");
        Ok(())
    }

    fn visit_page_break(&mut self, br: &PageBreak<'_>) -> Result<(), Self::Error> {
        self.write_block_anchor(&br.metadata);
        if br.metadata.attributes.get_string("page-layout").is_some()
            || br
                .metadata
                .roles
                .iter()
                .any(|role| matches!(*role, "landscape" | "portrait"))
        {
            self.warn_unsupported_once(
                "page-layout-change",
                "page-break layout changes are not supported by the PDF backend; keeping the document page layout",
                "Use Asciidoctor PDF when a document must switch between portrait and landscape pages.",
                br.metadata.location.as_ref().or(Some(&br.location)),
            );
        }
        if br.metadata.options.contains(&"always") {
            if self.explicit_page_break_state == ExplicitPageBreakState::Weak {
                self.writer.raw("#pagebreak()\n");
            }
            self.writer.raw("#pagebreak()\n\n");
            self.explicit_page_break_state = ExplicitPageBreakState::Inactive;
        } else {
            self.writer.raw("#pagebreak(weak: true)\n\n");
            self.explicit_page_break_state = ExplicitPageBreakState::Weak;
        }
        Ok(())
    }

    fn visit_table_of_contents(&mut self, toc: &TableOfContents<'_>) -> Result<(), Self::Error> {
        self.write_block_anchor(&toc.metadata);
        self.render_toc(Some(toc), "macro")
    }

    fn visit_discrete_header(&mut self, header: &DiscreteHeader<'_>) -> Result<(), Self::Error> {
        self.write_block_anchor(&header.metadata);
        let level = header.level.max(1);
        let _ = write!(self.writer, "#heading(level: {level}, outlined: false)[");
        self.write_title(&header.title)?;
        self.writer.raw("]\n\n");
        Ok(())
    }

    fn visit_inline_node(&mut self, node: &InlineNode<'_>) -> Result<(), Self::Error> {
        let previous_inline_span = self.in_inline_span;
        if acdc_converters_core::visitor::is_formatting_span(node) {
            self.in_inline_span = true;
        }

        let result = (|| {
            match node {
                InlineNode::PlainText(plain) => self.write_plain(plain.content),
                InlineNode::RawText(raw) => self.write_raw(raw),
                InlineNode::VerbatimText(verbatim) => self.write_text_expr(verbatim.content),
                InlineNode::BoldText(bold) => {
                    self.write_quoted_span(bold.id, bold.role, "#strong[", &bold.content, "]")?;
                }
                InlineNode::ItalicText(italic) => {
                    self.write_quoted_span(italic.id, italic.role, "#emph[", &italic.content, "]")?;
                }
                InlineNode::MonospaceText(mono) => {
                    let state = self.write_inline_span_start(mono.id, mono.role);
                    let text = inlines_to_string(&mono.content);
                    let text = self.normalize_prose_whitespace(&text);
                    self.write_inline_verbatim(&text);
                    self.write_inline_span_end(state);
                }
                InlineNode::HighlightText(highlight) => {
                    let (prefix, suffix) = if highlight.id.is_some() || highlight.role.is_some() {
                        ("", "")
                    } else {
                        ("#highlight[", "]")
                    };
                    self.write_quoted_span(
                        highlight.id,
                        highlight.role,
                        prefix,
                        &highlight.content,
                        suffix,
                    )?;
                }
                InlineNode::SubscriptText(sub) => {
                    self.write_quoted_span(sub.id, sub.role, "#sub[", &sub.content, "]")?;
                }
                InlineNode::SuperscriptText(sup) => {
                    self.write_quoted_span(sup.id, sup.role, "#super[", &sup.content, "]")?;
                }
                InlineNode::CurvedQuotationText(quoted) => {
                    let state = self.write_inline_span_start(quoted.id, quoted.role);
                    self.write_text_expr("\u{201C}");
                    self.write_inlines(&quoted.content)?;
                    self.write_text_expr("\u{201D}");
                    self.write_inline_span_end(state);
                }
                InlineNode::CurvedApostropheText(quoted) => {
                    let state = self.write_inline_span_start(quoted.id, quoted.role);
                    self.write_text_expr("\u{2018}");
                    self.write_inlines(&quoted.content)?;
                    self.write_text_expr("\u{2019}");
                    self.write_inline_span_end(state);
                }
                InlineNode::StandaloneCurvedApostrophe(_) => self.write_text_expr("\u{2019}"),
                InlineNode::LineBreak(_) => self.writer.raw("#linebreak()"),
                InlineNode::InlineAnchor(anchor) => {
                    let _ = write!(self.writer, "#metadata(none) <{}>", encode_label(anchor.id));
                }
                InlineNode::Macro(inline_macro) => self.write_inline_macro(inline_macro)?,
                InlineNode::CalloutRef(callout) => {
                    self.write_text_expr(&format!("({})", callout.number));
                }
                _ => self.warn_unsupported_parser_variant("inline node", Some(node.location())),
            }
            Ok(())
        })();

        self.in_inline_span = previous_inline_span;
        result
    }

    fn visit_text(&mut self, text: &str) -> Result<(), Self::Error> {
        self.write_plain(text);
        Ok(())
    }
}

fn appendix_caption<'a>(attributes: &'a acdc_parser::DocumentAttributes<'_>) -> Option<&'a str> {
    match attributes.get("appendix-caption") {
        Some(AttributeValue::String(value)) => Some(value.as_ref()),
        Some(_) => None,
        None => Some("Appendix"),
    }
}

fn part_signifier<'a>(attributes: &'a acdc_parser::DocumentAttributes<'_>) -> Option<&'a str> {
    match attributes.get("part-signifier") {
        Some(AttributeValue::String(value)) => Some(value.as_ref()),
        Some(_) => None,
        None => Some("Part"),
    }
}
