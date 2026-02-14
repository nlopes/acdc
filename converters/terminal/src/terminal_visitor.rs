//! Visitor implementation for terminal output.

use std::io::Write;

use acdc_converters_core::visitor::{Visitor, WritableVisitor};
use acdc_parser::{
    Admonition, Audio, CalloutList, DelimitedBlock, DescriptionList, DiscreteHeader, Document,
    Header, Image, InlineNode, ListItem, OrderedList, PageBreak, Paragraph, Section,
    TableOfContents, ThematicBreak, UnorderedList, Video,
};
use crossterm::{
    QueueableCommand,
    style::{PrintStyledContent, Stylize},
};

use crate::Processor;

/// Terminal visitor that generates terminal output from `AsciiDoc` AST
pub struct TerminalVisitor<W: Write> {
    writer: W,
    pub(crate) processor: Processor,
}

impl<W: Write> TerminalVisitor<W> {
    pub fn new(writer: W, processor: Processor) -> Self {
        Self { writer, processor }
    }

    /// Consume the visitor and return the writer
    pub fn into_writer(self) -> W {
        self.writer
    }
}

impl<W: Write> Visitor for TerminalVisitor<W> {
    type Error = crate::Error;

    fn visit_header(&mut self, header: &Header) -> Result<(), Self::Error> {
        // In embedded mode, skip header output (title, authors, revision info)
        if self.processor.options.embedded() {
            return Ok(());
        }
        let processor = self.processor.clone();
        crate::document::visit_header(header, self, &processor)
    }

    fn visit_body_content_start(&mut self, _doc: &Document) -> Result<(), Self::Error> {
        let processor = self.processor.clone();
        crate::toc::render(None, self, "auto", &processor)?;
        Ok(())
    }

    fn visit_preamble_end(&mut self, _doc: &Document) -> Result<(), Self::Error> {
        let processor = self.processor.clone();
        crate::toc::render(None, self, "preamble", &processor)?;
        Ok(())
    }

    fn visit_document_supplements(&mut self, doc: &Document) -> Result<(), Self::Error> {
        // Render footnotes at the end of the document if any exist
        if !doc.footnotes.is_empty() {
            writeln!(self.writer)?;
            writeln!(self.writer, "─────")?; // Simple separator
            for footnote in &doc.footnotes {
                self.writer.queue(PrintStyledContent(
                    format!("[{}]", footnote.number)
                        .with(self.processor.appearance.colors.footnote)
                        .bold(),
                ))?;
                write!(self.writer, " ")?;

                // Render the footnote content
                self.visit_inline_nodes(&footnote.content)?;
                writeln!(self.writer)?;
            }
        }
        Ok(())
    }

    fn visit_section(&mut self, section: &Section) -> Result<(), Self::Error> {
        let is_index_section = section
            .metadata
            .style
            .as_ref()
            .is_some_and(|s| s == "index");

        // Index sections are only rendered if they're the last section
        if is_index_section && !self.processor.has_valid_index_section() {
            return Ok(());
        }

        let processor = self.processor.clone();
        crate::section::visit_section(section, self, &processor)?;

        if is_index_section {
            // Render the collected index catalog instead of normal content
            let processor = self.processor.clone();
            crate::index::render(self, &processor)?;
        } else {
            // Walk nested blocks within the section
            for nested_block in &section.content {
                self.visit_block(nested_block)?;
            }
        }

        Ok(())
    }

    fn visit_paragraph(&mut self, para: &Paragraph) -> Result<(), Self::Error> {
        let processor = self.processor.clone();
        crate::paragraph::visit_paragraph(para, self, &processor)
    }

    fn visit_delimited_block(&mut self, block: &DelimitedBlock) -> Result<(), Self::Error> {
        let processor = self.processor.clone();
        crate::delimited::visit_delimited_block(self, block, &processor)
    }

    fn visit_ordered_list(&mut self, list: &OrderedList) -> Result<(), Self::Error> {
        let processor = self.processor.clone();
        crate::list::visit_ordered_list(list, self, &processor)
    }

    fn visit_unordered_list(&mut self, list: &UnorderedList) -> Result<(), Self::Error> {
        let processor = self.processor.clone();
        crate::list::visit_unordered_list(list, self, &processor)
    }

    fn visit_description_list(&mut self, list: &DescriptionList) -> Result<(), Self::Error> {
        let processor = self.processor.clone();
        crate::list::visit_description_list(list, self, &processor)
    }

    fn visit_callout_list(&mut self, list: &CalloutList) -> Result<(), Self::Error> {
        let processor = self.processor.clone();
        crate::list::visit_callout_list(list, self, &processor)
    }

    fn visit_list_item(&mut self, _item: &ListItem) -> Result<(), Self::Error> {
        // List items are handled by their parent list visitors
        Ok(())
    }

    fn visit_admonition(&mut self, admon: &Admonition) -> Result<(), Self::Error> {
        let processor = self.processor.clone();
        crate::admonition::visit_admonition(self, admon, &processor)
    }

    fn visit_image(&mut self, img: &Image) -> Result<(), Self::Error> {
        crate::image::visit_image(img, self)
    }

    fn visit_video(&mut self, video: &Video) -> Result<(), Self::Error> {
        crate::video::visit_video(video, self)
    }

    fn visit_audio(&mut self, audio: &Audio) -> Result<(), Self::Error> {
        crate::audio::visit_audio(audio, self)
    }

    fn visit_thematic_break(&mut self, _br: &ThematicBreak) -> Result<(), Self::Error> {
        let width = self.processor.terminal_width;
        writeln!(self.writer, "{}", "─".repeat(width))?;
        Ok(())
    }

    fn visit_page_break(&mut self, _br: &PageBreak) -> Result<(), Self::Error> {
        let width = self.processor.terminal_width;
        writeln!(self.writer, "\n{}\n", "═".repeat(width))?;
        Ok(())
    }

    fn visit_table_of_contents(&mut self, toc: &TableOfContents) -> Result<(), Self::Error> {
        let processor = self.processor.clone();
        crate::toc::render(Some(toc), self, "macro", &processor)
    }

    fn visit_discrete_header(&mut self, header: &DiscreteHeader) -> Result<(), Self::Error> {
        let processor = self.processor.clone();
        crate::section::visit_discrete_header(header, self, &processor)
    }

    fn visit_inline_nodes(&mut self, nodes: &[InlineNode]) -> Result<(), Self::Error> {
        for inline in nodes {
            self.visit_inline_node(inline)?;
        }
        Ok(())
    }

    fn visit_inline_node(&mut self, node: &InlineNode) -> Result<(), Self::Error> {
        let processor = self.processor.clone();
        crate::inlines::visit_inline_node(node, self, &processor)
    }

    fn visit_text(&mut self, text: &str) -> Result<(), Self::Error> {
        write!(self.writer, "{text}")?;
        Ok(())
    }
}

impl<W: Write> WritableVisitor for TerminalVisitor<W> {
    fn writer_mut(&mut self) -> &mut dyn Write {
        &mut self.writer
    }
}
