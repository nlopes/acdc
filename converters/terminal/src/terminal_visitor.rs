//! Visitor implementation for terminal output.

use std::io::Write;

use acdc_converters_core::{
    Diagnostics,
    visitor::{Visitor, WritableVisitor},
};
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
pub struct TerminalVisitor<'a, 'd, W: Write> {
    writer: W,
    pub(crate) processor: Processor<'a>,
    /// Per-conversion diagnostics handle.
    pub(crate) diagnostics: Diagnostics<'d>,
    /// Whether we are inside an inline formatting span (bold, italic, etc.).
    /// When true, em-dash boundary replacement at string start/end is suppressed.
    pub(crate) in_inline_span: bool,
}

impl<'a, 'd, W: Write> TerminalVisitor<'a, 'd, W> {
    pub fn new(writer: W, processor: Processor<'a>, diagnostics: Diagnostics<'d>) -> Self {
        Self {
            writer,
            processor,
            diagnostics,
            in_inline_span: false,
        }
    }

    /// Consume the visitor and return the writer
    pub fn into_writer(self) -> W {
        self.writer
    }
}

impl<W: Write> Visitor for TerminalVisitor<'_, '_, W> {
    type Error = crate::Error;

    fn visit_header(&mut self, header: &Header) -> Result<(), Self::Error> {
        // In embedded mode, skip header output (title, authors, revision info)
        if self.processor.options.embedded() {
            return Ok(());
        }
        self.render_header(header)
    }

    fn visit_body_content_start(&mut self, _doc: &Document) -> Result<(), Self::Error> {
        self.render_toc(None, "auto")?;
        Ok(())
    }

    fn visit_preamble_end(&mut self, _doc: &Document) -> Result<(), Self::Error> {
        self.render_toc(None, "preamble")?;
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
            .is_some_and(|s| *s == "index");

        // Index sections are only rendered if they're the last section
        if is_index_section && !self.processor.has_valid_index_section() {
            return Ok(());
        }

        self.render_section(section)?;

        if is_index_section {
            // Render the collected index catalog instead of normal content
            let processor = self.processor.clone();
            crate::index::render(self, &processor)?;
        } else {
            // Walk nested blocks within the section
            for nested_block in &section.content.clone() {
                self.visit_block(nested_block)?;
            }
        }

        Ok(())
    }

    fn visit_paragraph(&mut self, para: &Paragraph) -> Result<(), Self::Error> {
        self.render_paragraph(para)
    }

    fn visit_delimited_block(&mut self, block: &DelimitedBlock) -> Result<(), Self::Error> {
        self.render_delimited_block(block)
    }

    fn visit_ordered_list(&mut self, list: &OrderedList) -> Result<(), Self::Error> {
        self.render_ordered_list(list)
    }

    fn visit_unordered_list(&mut self, list: &UnorderedList) -> Result<(), Self::Error> {
        self.render_unordered_list(list)
    }

    fn visit_description_list(&mut self, list: &DescriptionList) -> Result<(), Self::Error> {
        self.render_description_list(list)
    }

    fn visit_callout_list(&mut self, list: &CalloutList) -> Result<(), Self::Error> {
        self.render_callout_list(list)
    }

    fn visit_list_item(&mut self, _item: &ListItem) -> Result<(), Self::Error> {
        // List items are handled by their parent list visitors
        Ok(())
    }

    fn visit_admonition(&mut self, admon: &Admonition) -> Result<(), Self::Error> {
        self.render_admonition(admon)
    }

    fn visit_image(&mut self, img: &Image) -> Result<(), Self::Error> {
        self.render_image(img)
    }

    fn visit_video(&mut self, video: &Video) -> Result<(), Self::Error> {
        self.render_video(video)
    }

    fn visit_audio(&mut self, audio: &Audio) -> Result<(), Self::Error> {
        self.render_audio(audio)
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
        self.render_toc(Some(toc), "macro")
    }

    fn visit_discrete_header(&mut self, header: &DiscreteHeader) -> Result<(), Self::Error> {
        self.render_discrete_header(header)
    }

    fn visit_inline_node(&mut self, node: &InlineNode) -> Result<(), Self::Error> {
        let saved = self.in_inline_span;
        if acdc_converters_core::visitor::is_formatting_span(node) {
            self.in_inline_span = true;
        }

        let in_span = self.in_inline_span;
        let result = self.render_inline_node(node, in_span);

        self.in_inline_span = saved;
        result
    }

    fn visit_text(&mut self, text: &str) -> Result<(), Self::Error> {
        write!(self.writer, "{text}")?;
        Ok(())
    }
}

impl<W: Write> WritableVisitor for TerminalVisitor<'_, '_, W> {
    fn writer_mut(&mut self) -> &mut dyn Write {
        &mut self.writer
    }
}
