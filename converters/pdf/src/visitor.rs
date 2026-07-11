use std::fmt::Write as _;

#[cfg(feature = "pre-spec-subs")]
use acdc_converters_core::substitutions::effective_subs_flags;
use acdc_converters_core::{decode_numeric_char_refs, inlines_to_string, visitor::Visitor};
use acdc_parser::{
    Admonition, AdmonitionVariant, Audio, CalloutList, DelimitedBlock, DelimitedBlockType,
    DescriptionList, DiscreteHeader, Header, Image, InlineNode, ListItem, OrderedList, PageBreak,
    Paragraph, Section, TableOfContents, ThematicBreak, UnorderedList, Video,
};

use crate::{Error, PdfVisitor, encode_label};

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
        self.writer.raw("#align(center)[\n");
        self.writer.raw("#text(size: 22pt, weight: \"bold\")[");
        self.write_title(&header.title)?;
        self.writer.raw("]\n");
        if !header.authors.is_empty() {
            self.writer.raw("#v(0.4em)\n");
            let authors = header
                .authors
                .iter()
                .map(|author| {
                    let middle = author
                        .middle_name
                        .map_or_else(String::new, |middle| format!(" {middle}"));
                    format!("{}{} {}", author.first_name, middle, author.last_name)
                })
                .collect::<Vec<_>>()
                .join(", ");
            self.write_text_expr(&authors);
            self.writer.raw("\n");
        }
        self.writer.raw("]\n#v(1em)\n\n");
        Ok(())
    }

    fn visit_section(&mut self, section: &Section<'_>) -> Result<(), Self::Error> {
        let participates = self
            .special_section_tracker
            .enter(section.level, section.kind);
        let mut prefix = String::new();
        if section.kind == acdc_parser::SectionKind::Appendix {
            prefix.push_str(&self.appendix_tracker.enter_appendix());
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
        #[cfg(feature = "pre-spec-subs")]
        let previous_subs = self.processor.current_subs.replace(effective_subs_flags(
            para.metadata.substitutions.as_ref(),
            matches!(
                para.metadata.style,
                Some("verse" | "literal" | "listing" | "source")
            ),
        ));

        let result = (|| {
            self.write_block_title(&para.title)?;
            self.write_inlines(&para.content)?;
            self.writer.raw("\n\n");
            Ok(())
        })();

        #[cfg(feature = "pre-spec-subs")]
        self.processor.current_subs.set(previous_subs);
        result
    }

    fn visit_delimited_block(&mut self, block: &DelimitedBlock<'_>) -> Result<(), Self::Error> {
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
            self.write_block_title(&block.title)?;
            match &block.inner {
                DelimitedBlockType::DelimitedExample(blocks)
                | DelimitedBlockType::DelimitedOpen(blocks)
                | DelimitedBlockType::DelimitedSidebar(blocks) => {
                    self.write_framed_blocks(None, blocks)
                }
                DelimitedBlockType::DelimitedQuote(blocks) => {
                    self.writer.raw("#blockquote[\n");
                    self.write_blocks(blocks)?;
                    self.writer.raw("]\n\n");
                    Ok(())
                }
                DelimitedBlockType::DelimitedListing(nodes)
                | DelimitedBlockType::DelimitedLiteral(nodes)
                | DelimitedBlockType::DelimitedPass(nodes)
                | DelimitedBlockType::DelimitedVerse(nodes) => {
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
        self.write_block_title(&list.title)?;
        for item in &list.items {
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
        let kind = match admon.variant {
            AdmonitionVariant::Note => "note",
            AdmonitionVariant::Tip => "tip",
            AdmonitionVariant::Important => "important",
            AdmonitionVariant::Caution => "caution",
            AdmonitionVariant::Warning => "warning",
        };
        self.write_block_title(&admon.title)?;
        self.write_callout(kind, &admon.blocks)
    }

    fn visit_image(&mut self, img: &Image<'_>) -> Result<(), Self::Error> {
        self.write_block_image(img)
    }

    fn visit_video(&mut self, video: &Video<'_>) -> Result<(), Self::Error> {
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
        self.warn_unsupported("audio blocks", "rendering the audio target as text");
        self.write_block_title(&audio.title)?;
        self.write_text_expr(&format!("[audio: {}]", audio.source));
        self.writer.raw("\n\n");
        Ok(())
    }

    fn visit_thematic_break(&mut self, _br: &ThematicBreak<'_>) -> Result<(), Self::Error> {
        self.writer.raw("#hr()\n\n");
        Ok(())
    }

    fn visit_page_break(&mut self, _br: &PageBreak<'_>) -> Result<(), Self::Error> {
        self.writer.raw("#pagebreak()\n\n");
        Ok(())
    }

    fn visit_table_of_contents(&mut self, toc: &TableOfContents<'_>) -> Result<(), Self::Error> {
        self.render_toc(Some(toc), "macro");
        Ok(())
    }

    fn visit_discrete_header(&mut self, header: &DiscreteHeader<'_>) -> Result<(), Self::Error> {
        let level = header.level.max(1);
        let _ = write!(self.writer, "#heading(level: {level}, outlined: false)[");
        self.write_title(&header.title)?;
        self.writer.raw("]\n\n");
        Ok(())
    }

    fn visit_inline_node(&mut self, node: &InlineNode<'_>) -> Result<(), Self::Error> {
        match node {
            InlineNode::PlainText(plain) => self.write_plain(plain.content),
            InlineNode::RawText(raw) => {
                self.write_text_expr(&decode_numeric_char_refs(raw.content));
            }
            InlineNode::VerbatimText(verbatim) => self.write_text_expr(verbatim.content),
            InlineNode::BoldText(bold) => {
                self.write_quoted_span("#strong[", &bold.content, "]")?;
            }
            InlineNode::ItalicText(italic) => {
                self.write_quoted_span("#emph[", &italic.content, "]")?;
            }
            InlineNode::MonospaceText(mono) => {
                let text = inlines_to_string(&mono.content);
                self.writer.raw("#raw(");
                self.writer.string_literal(&text);
                self.writer.raw(")");
            }
            InlineNode::HighlightText(highlight) => {
                self.write_quoted_span("#highlight[", &highlight.content, "]")?;
            }
            InlineNode::SubscriptText(sub) => {
                self.write_quoted_span("#sub[", &sub.content, "]")?;
            }
            InlineNode::SuperscriptText(sup) => {
                self.write_quoted_span("#super[", &sup.content, "]")?;
            }
            InlineNode::CurvedQuotationText(quoted) => {
                self.write_text_expr("\u{201C}");
                self.write_inlines(&quoted.content)?;
                self.write_text_expr("\u{201D}");
            }
            InlineNode::CurvedApostropheText(quoted) => {
                self.write_text_expr("\u{2018}");
                self.write_inlines(&quoted.content)?;
                self.write_text_expr("\u{2019}");
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
    }

    fn visit_text(&mut self, text: &str) -> Result<(), Self::Error> {
        self.write_plain(text);
        Ok(())
    }
}
