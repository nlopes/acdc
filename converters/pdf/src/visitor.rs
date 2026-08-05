use std::fmt::Write as _;

#[cfg(feature = "pre-spec-subs")]
use acdc_converters_core::substitutions::effective_subs_flags;
use acdc_converters_core::{
    decode_numeric_char_refs, inlines_to_string, shows_block_title, visitor::Visitor,
};
use acdc_parser::{
    Admonition, AdmonitionVariant, AttributeValue, Audio, Block, CalloutList, DelimitedBlock,
    DelimitedBlockType, DescriptionList, DiscreteHeader, Header, Image, InlineNode, ListItem,
    OrderedList, PageBreak, Paragraph, Section, TableOfContents, ThematicBreak, UnorderedList,
    Video,
};

use crate::{
    Error, PdfVisitor, author_name, encode_label, pdf_visitor::collapse_source_whitespace,
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

    fn visit_body_content_start(
        &mut self,
        _doc: &acdc_parser::Document<'_>,
    ) -> Result<(), Self::Error> {
        self.render_toc(None, "auto");
        Ok(())
    }

    fn visit_preamble_end(&mut self, _doc: &acdc_parser::Document<'_>) -> Result<(), Self::Error> {
        self.render_toc(None, "preamble");
        Ok(())
    }

    fn visit_header(&mut self, header: &Header<'_>) -> Result<(), Self::Error> {
        let title_page = self
            .processor
            .document_attributes()
            .get("title-page")
            .is_some_and(|value| {
                !matches!(value, AttributeValue::Bool(false) | AttributeValue::None)
            })
            || self
                .processor
                .document_attributes()
                .get_string("doctype")
                .as_deref()
                == Some("book");

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
            self.writer.raw("#counter(page).update(0)\n]\n\n");
        } else {
            self.writer.raw("#v(1em)\n\n");
        }
        Ok(())
    }

    fn visit_section(&mut self, section: &Section<'_>) -> Result<(), Self::Error> {
        if self.section_break_before(section) {
            self.writer.raw("#pagebreak(weak: true)\n\n");
        }

        let participates = self
            .special_section_tracker
            .enter(section.level, section.kind);
        let mut prefix = String::new();
        if section.kind == acdc_parser::SectionKind::Appendix {
            prefix.push_str(&self.appendix_tracker.enter_appendix());
        } else if section.level == 0 && participates {
            if let Some(number) = self.part_number_tracker.enter_part() {
                prefix.push_str(&number);
            }
        } else if participates
            && let Some(number) = self.section_number_tracker.enter_section(section.level)
        {
            prefix.push_str(&number);
        }

        let level = section.level.max(1);
        let _ = write!(self.writer, "#heading(level: {level})[");
        if !prefix.is_empty() {
            self.write_text_expr(&prefix);
        }
        self.write_title(&section.title)?;
        self.writer.raw("]");
        let id =
            acdc_parser::Section::generate_id_string(&section.metadata, section.title.as_ref());
        if !id.is_empty() {
            let _ = write!(self.writer, " <{}>", encode_label(&id));
        }
        self.writer.raw("\n\n");
        self.write_blocks(&section.content)
    }

    fn visit_paragraph(&mut self, para: &Paragraph<'_>) -> Result<(), Self::Error> {
        self.write_block_anchor(&para.metadata);
        self.write_paragraph_content(para, true)
    }

    fn visit_delimited_block(&mut self, block: &DelimitedBlock<'_>) -> Result<(), Self::Error> {
        self.write_block_anchor(&block.metadata);

        #[cfg(feature = "pre-spec-subs")]
        let previous_subs = self.processor.current_subs.replace(effective_subs_flags(
            block.metadata.substitutions.as_ref(),
            matches!(
                block.inner,
                DelimitedBlockType::DelimitedListing(_)
                    | DelimitedBlockType::DelimitedLiteral(_)
                    | DelimitedBlockType::DelimitedPass(_)
                    | DelimitedBlockType::DelimitedVerse(_)
            ),
        ));

        let result = (|| {
            // A sidebar sets its own title inside its box and an example sets
            // its own numbered caption; every other block that shows a title
            // takes it above.
            let writes_own_title = matches!(
                block.inner,
                DelimitedBlockType::DelimitedSidebar(_) | DelimitedBlockType::DelimitedExample(_)
            );
            if shows_block_title(&block.inner) && !writes_own_title {
                self.write_block_title(&block.title)?;
            }
            match &block.inner {
                // Each container reads differently in print: an example takes a
                // light frame, a sidebar a shaded box, and an open block is a
                // transparent container that takes neither.
                DelimitedBlockType::DelimitedExample(blocks) => {
                    self.write_example(&block.title, blocks)
                }
                DelimitedBlockType::DelimitedSidebar(blocks) => {
                    self.write_sidebar(&block.title, blocks)
                }
                // An `[abstract]` open block reads as a quote, matching the HTML
                // backend. Asciidoctor PDF drops it; acdc keeps the content.
                DelimitedBlockType::DelimitedOpen(blocks)
                    if block.metadata.style == Some("abstract") =>
                {
                    self.write_quote_block(&block.metadata, |visitor| visitor.write_blocks(blocks))
                }
                DelimitedBlockType::DelimitedOpen(blocks) => {
                    self.write_framed_blocks(None, None, blocks)
                }
                DelimitedBlockType::DelimitedQuote(blocks) => {
                    self.write_quote_block(&block.metadata, |visitor| visitor.write_blocks(blocks))
                }
                DelimitedBlockType::DelimitedVerse(nodes) => {
                    self.write_verse_block(nodes, &block.metadata)
                }
                DelimitedBlockType::DelimitedListing(nodes)
                | DelimitedBlockType::DelimitedLiteral(nodes)
                | DelimitedBlockType::DelimitedPass(nodes) => {
                    self.write_verbatim_block(nodes);
                    Ok(())
                }
                DelimitedBlockType::DelimitedTable(table) => self.write_table(table),
                DelimitedBlockType::DelimitedStem(stem) => {
                    self.write_stem_fallback(stem.content, true);
                    Ok(())
                }
                DelimitedBlockType::DelimitedComment(_) | _ => Ok(()),
            }
        })();

        #[cfg(feature = "pre-spec-subs")]
        self.processor.current_subs.set(previous_subs);
        result
    }

    fn visit_ordered_list(&mut self, list: &OrderedList<'_>) -> Result<(), Self::Error> {
        self.write_block_anchor(&list.metadata);
        self.write_block_title(&list.title)?;
        self.list_depth += 1;
        for item in &list.items {
            self.write_list_item("+", item)?;
        }
        self.list_depth -= 1;
        self.writer.raw("\n");
        Ok(())
    }

    fn visit_unordered_list(&mut self, list: &UnorderedList<'_>) -> Result<(), Self::Error> {
        self.write_block_anchor(&list.metadata);
        self.write_block_title(&list.title)?;
        self.list_depth += 1;
        for item in &list.items {
            self.write_list_item("-", item)?;
        }
        self.list_depth -= 1;
        self.writer.raw("\n");
        Ok(())
    }

    fn visit_description_list(&mut self, list: &DescriptionList<'_>) -> Result<(), Self::Error> {
        self.write_block_anchor(&list.metadata);
        self.write_block_title(&list.title)?;
        for item in &list.items {
            for anchor in &item.anchors {
                self.write_anchor_target(anchor);
            }
            self.writer.raw("#text(weight: \"bold\")[");
            self.write_inlines(&item.term)?;
            self.writer.raw("]\n");
            if !item.principal_text.is_empty() {
                self.write_inlines(&item.principal_text)?;
                self.writer.raw("\n");
            }
            self.write_blocks(&item.description)?;
        }
        self.writer.raw("\n");
        Ok(())
    }

    fn visit_callout_list(&mut self, list: &CalloutList<'_>) -> Result<(), Self::Error> {
        self.write_block_anchor(&list.metadata);
        self.write_block_title(&list.title)?;
        for item in &list.items {
            self.writer.raw("- ");
            self.write_text_expr(&format!("({}) ", item.callout.number));
            self.write_inlines(&item.principal)?;
            self.writer.raw("\n");
            self.write_blocks(&item.blocks)?;
        }
        self.writer.raw("\n");
        Ok(())
    }

    fn visit_list_item(&mut self, _item: &ListItem<'_>) -> Result<(), Self::Error> {
        Ok(())
    }

    fn visit_admonition(&mut self, admon: &Admonition<'_>) -> Result<(), Self::Error> {
        self.write_block_anchor(&admon.metadata);
        let kind = match admon.variant {
            AdmonitionVariant::Note => "note",
            AdmonitionVariant::Tip => "tip",
            AdmonitionVariant::Important => "important",
            AdmonitionVariant::Caution => "caution",
            AdmonitionVariant::Warning => "warning",
        };
        self.write_block_title(&admon.title)?;
        self.writer.raw("#callout(");
        self.writer.string_literal(kind);
        self.writer.raw(")[\n");
        match admon.blocks.as_slice() {
            [Block::Paragraph(para)] if para.metadata == admon.metadata => {
                // The parser copies a simple admonition's metadata to its
                // synthetic paragraph. Render that paragraph without its anchor
                // and title, which the admonition wrapper already wrote, but
                // through the same content path so `[subs=…]` still applies.
                self.write_paragraph_content(para, false)?;
            }
            blocks => self.write_blocks(blocks)?,
        }
        self.writer.raw("]\n\n");
        Ok(())
    }

    fn visit_image(&mut self, img: &Image<'_>) -> Result<(), Self::Error> {
        self.write_block_anchor(&img.metadata);
        self.write_block_image(img)
    }

    fn visit_video(&mut self, video: &Video<'_>) -> Result<(), Self::Error> {
        self.write_block_anchor(&video.metadata);
        self.warn_unsupported("video blocks", "rendering the video target as text");
        self.write_block_title(&video.title)?;
        let sources = video
            .sources
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        self.write_text_expr(&format!("[video: {sources}]"));
        self.writer.raw("\n\n");
        Ok(())
    }

    fn visit_audio(&mut self, audio: &Audio<'_>) -> Result<(), Self::Error> {
        self.write_block_anchor(&audio.metadata);
        self.warn_unsupported("audio blocks", "rendering the audio target as text");
        self.write_block_title(&audio.title)?;
        self.write_text_expr(&format!("[audio: {}]", audio.source));
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
        self.writer.raw("#pagebreak()\n\n");
        Ok(())
    }

    fn visit_table_of_contents(&mut self, toc: &TableOfContents<'_>) -> Result<(), Self::Error> {
        self.write_block_anchor(&toc.metadata);
        self.render_toc(Some(toc), "macro");
        Ok(())
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
                InlineNode::RawText(raw) => {
                    self.write_text_expr(&decode_numeric_char_refs(raw.content));
                }
                InlineNode::VerbatimText(verbatim) => self.write_text_expr(verbatim.content),
                InlineNode::BoldText(bold) => {
                    self.write_quoted_span(bold.id, bold.role, "#strong[", &bold.content, "]")?;
                }
                InlineNode::ItalicText(italic) => {
                    self.write_quoted_span(italic.id, italic.role, "#emph[", &italic.content, "]")?;
                }
                InlineNode::MonospaceText(mono) => {
                    let wrappers = self.write_inline_span_start(mono.id, mono.role);
                    let text = inlines_to_string(&mono.content);
                    let text = collapse_source_whitespace(&text);
                    self.writer.raw("#raw(");
                    self.writer.string_literal(&text);
                    self.writer.raw(")");
                    self.write_inline_span_end(wrappers);
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
                    let wrappers = self.write_inline_span_start(quoted.id, quoted.role);
                    self.write_text_expr("\u{201C}");
                    self.write_inlines(&quoted.content)?;
                    self.write_text_expr("\u{201D}");
                    self.write_inline_span_end(wrappers);
                }
                InlineNode::CurvedApostropheText(quoted) => {
                    let wrappers = self.write_inline_span_start(quoted.id, quoted.role);
                    self.write_text_expr("\u{2018}");
                    self.write_inlines(&quoted.content)?;
                    self.write_text_expr("\u{2019}");
                    self.write_inline_span_end(wrappers);
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
                _ => {}
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
