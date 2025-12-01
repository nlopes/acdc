//! Visitor implementation for manpage (roff/troff) conversion.

use std::io::Write;

use acdc_converters_common::visitor::{Visitor, WritableVisitor};
use acdc_parser::{
    Admonition, Audio, CalloutList, DelimitedBlock, DescriptionList, DiscreteHeader, Document,
    Header, Image, InlineNode, ListItem, OrderedList, PageBreak, Paragraph, Section,
    TableOfContents, ThematicBreak, UnorderedList, Video,
};

use crate::{Error, Processor};

/// Manpage visitor that generates roff/troff output from `AsciiDoc` AST.
pub struct ManpageVisitor<W: Write> {
    writer: W,
    pub(crate) processor: Processor,
    /// Current nesting depth for lists (used for .RS/.RE indentation).
    pub(crate) list_depth: usize,
}

impl<W: Write> ManpageVisitor<W> {
    /// Create a new manpage visitor.
    pub fn new(writer: W, processor: Processor) -> Self {
        Self {
            writer,
            processor,
            list_depth: 0,
        }
    }

    /// Consume the visitor and return the writer.
    #[must_use]
    pub fn into_writer(self) -> W {
        self.writer
    }

    /// Write a blank line for spacing.
    pub(crate) fn write_sp(&mut self) -> Result<(), Error> {
        writeln!(self.writer, ".sp")?;
        Ok(())
    }
}

impl<W: Write> Visitor for ManpageVisitor<W> {
    type Error = Error;

    fn visit_document_start(&mut self, doc: &Document) -> Result<(), Self::Error> {
        crate::document::visit_document_start(doc, self)
    }

    fn visit_document_end(&mut self, _doc: &Document) -> Result<(), Self::Error> {
        // No special cleanup needed for manpages
        Ok(())
    }

    fn visit_header(&mut self, _header: &Header) -> Result<(), Self::Error> {
        // Header is handled in visit_document_start for manpages
        // The .TH macro contains all header information
        Ok(())
    }

    fn visit_section(&mut self, section: &Section) -> Result<(), Self::Error> {
        crate::section::visit_section(section, self)
    }

    fn visit_paragraph(&mut self, para: &Paragraph) -> Result<(), Self::Error> {
        crate::paragraph::visit_paragraph(para, self)
    }

    fn visit_delimited_block(&mut self, block: &DelimitedBlock) -> Result<(), Self::Error> {
        crate::delimited::visit_delimited_block(block, self)
    }

    fn visit_ordered_list(&mut self, list: &OrderedList) -> Result<(), Self::Error> {
        crate::list::visit_ordered_list(list, self)
    }

    fn visit_unordered_list(&mut self, list: &UnorderedList) -> Result<(), Self::Error> {
        crate::list::visit_unordered_list(list, self)
    }

    fn visit_description_list(&mut self, list: &DescriptionList) -> Result<(), Self::Error> {
        crate::list::visit_description_list(list, self)
    }

    fn visit_callout_list(&mut self, list: &CalloutList) -> Result<(), Self::Error> {
        crate::list::visit_callout_list(list, self)
    }

    fn visit_list_item(&mut self, _item: &ListItem) -> Result<(), Self::Error> {
        // List items are handled by their parent list visitors
        Ok(())
    }

    fn visit_admonition(&mut self, admon: &Admonition) -> Result<(), Self::Error> {
        crate::admonition::visit_admonition(admon, self)
    }

    fn visit_image(&mut self, img: &Image) -> Result<(), Self::Error> {
        // Images cannot be embedded in man pages - show title as alt text
        self.write_sp()?;
        if img.title.is_empty() {
            writeln!(self.writer, "[IMAGE]")?;
        } else {
            write!(self.writer, "[")?;
            self.visit_inline_nodes(&img.title)?;
            writeln!(self.writer, "]")?;
        }
        Ok(())
    }

    fn visit_video(&mut self, video: &Video) -> Result<(), Self::Error> {
        // Videos cannot be embedded in man pages - show placeholder
        // Video has multiple sources, use the first one or empty
        self.write_sp()?;
        if let Some(first_source) = video.sources.first() {
            writeln!(self.writer, "[VIDEO: {first_source}]")?;
        } else {
            writeln!(self.writer, "[VIDEO]")?;
        }
        Ok(())
    }

    fn visit_audio(&mut self, audio: &Audio) -> Result<(), Self::Error> {
        // Audio cannot be embedded in man pages - show placeholder
        self.write_sp()?;
        writeln!(self.writer, "[AUDIO: {}]", audio.source)?;
        Ok(())
    }

    fn visit_thematic_break(&mut self, _br: &ThematicBreak) -> Result<(), Self::Error> {
        // Thematic break as a centered line of dashes
        self.write_sp()?;
        writeln!(self.writer, ".ce")?;
        writeln!(self.writer, "* * *")?;
        writeln!(self.writer, ".ce 0")?;
        Ok(())
    }

    fn visit_page_break(&mut self, _br: &PageBreak) -> Result<(), Self::Error> {
        // Page break in roff
        writeln!(self.writer, ".bp")?;
        Ok(())
    }

    fn visit_table_of_contents(&mut self, _toc: &TableOfContents) -> Result<(), Self::Error> {
        // TOC is not typically included in man pages
        // Could optionally generate a list of sections, but skip for now
        Ok(())
    }

    fn visit_discrete_header(&mut self, header: &DiscreteHeader) -> Result<(), Self::Error> {
        // Discrete headers are rendered as bold text, not as sections
        self.write_sp()?;
        write!(self.writer, "\\fB")?;
        self.visit_inline_nodes(&header.title)?;
        writeln!(self.writer, "\\fP")?;
        Ok(())
    }

    fn visit_inline_nodes(&mut self, nodes: &[InlineNode]) -> Result<(), Self::Error> {
        for node in nodes {
            self.visit_inline_node(node)?;
        }
        Ok(())
    }

    fn visit_inline_node(&mut self, node: &InlineNode) -> Result<(), Self::Error> {
        crate::inlines::visit_inline_node(node, self)
    }

    fn visit_text(&mut self, text: &str) -> Result<(), Self::Error> {
        let escaped = crate::escape::manify(text, crate::escape::EscapeMode::Normalize);
        write!(self.writer, "{escaped}")?;
        Ok(())
    }
}

impl<W: Write> WritableVisitor for ManpageVisitor<W> {
    fn writer_mut(&mut self) -> &mut dyn Write {
        &mut self.writer
    }
}
