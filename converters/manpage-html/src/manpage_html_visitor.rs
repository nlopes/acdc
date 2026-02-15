use std::io::Write;

use acdc_converters_core::visitor::{Visitor, WritableVisitor};
use acdc_parser::{
    Admonition, Audio, CalloutList, DelimitedBlock, DescriptionList, DiscreteHeader, Document,
    Header, Image, InlineNode, ListItem, OrderedList, PageBreak, Paragraph, Section,
    TableOfContents, ThematicBreak, UnorderedList, Video,
};

use crate::{Error, Processor};

pub struct ManpageHtmlVisitor<W: Write> {
    writer: W,
    pub(crate) processor: Processor,
    pub(crate) list_depth: usize,
    pub(crate) in_name_section: bool,
    pub(crate) first_section_title: Option<String>,
    pub(crate) second_section_title: Option<String>,
}

impl<W: Write> ManpageHtmlVisitor<W> {
    pub fn new(writer: W, processor: Processor) -> Self {
        Self {
            writer,
            processor,
            list_depth: 0,
            in_name_section: false,
            first_section_title: None,
            second_section_title: None,
        }
    }

    pub(crate) fn record_section_title(&mut self, title: &str) {
        if self.first_section_title.is_none() {
            self.first_section_title = Some(title.to_string());
        } else if self.second_section_title.is_none() {
            self.second_section_title = Some(title.to_string());
        }
    }
}

impl<W: Write> Visitor for ManpageHtmlVisitor<W> {
    type Error = Error;

    fn visit_document_start(&mut self, doc: &Document) -> Result<(), Self::Error> {
        crate::document::visit_document_start(doc, self)
    }

    fn visit_document_supplements(&mut self, doc: &Document) -> Result<(), Self::Error> {
        crate::document::visit_document_supplements(doc, self)
    }

    fn visit_document_end(&mut self, doc: &Document) -> Result<(), Self::Error> {
        crate::document::visit_document_end(doc, self)
    }

    fn visit_header(&mut self, _header: &Header) -> Result<(), Self::Error> {
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
        Ok(())
    }

    fn visit_admonition(&mut self, admon: &Admonition) -> Result<(), Self::Error> {
        crate::admonition::visit_admonition(admon, self)
    }

    fn visit_image(&mut self, img: &Image) -> Result<(), Self::Error> {
        let w = self.writer_mut();
        if img.title.is_empty() {
            write!(w, "<p class=\"Pp\">[IMAGE]</p>")?;
        } else {
            write!(w, "<p class=\"Pp\">[")?;
            self.visit_inline_nodes(&img.title)?;
            write!(self.writer_mut(), "]</p>")?;
        }
        Ok(())
    }

    fn visit_video(&mut self, video: &Video) -> Result<(), Self::Error> {
        let w = self.writer_mut();
        if let Some(first_source) = video.sources.first() {
            write!(w, "<p class=\"Pp\">[VIDEO: {first_source}]</p>")?;
        } else {
            write!(w, "<p class=\"Pp\">[VIDEO]</p>")?;
        }
        Ok(())
    }

    fn visit_audio(&mut self, audio: &Audio) -> Result<(), Self::Error> {
        let w = self.writer_mut();
        write!(w, "<p class=\"Pp\">[AUDIO: {}]</p>", audio.source)?;
        Ok(())
    }

    fn visit_thematic_break(&mut self, _br: &ThematicBreak) -> Result<(), Self::Error> {
        write!(self.writer_mut(), "<hr>")?;
        Ok(())
    }

    fn visit_page_break(&mut self, _br: &PageBreak) -> Result<(), Self::Error> {
        write!(self.writer_mut(), "<hr class=\"page-break\">")?;
        Ok(())
    }

    fn visit_table_of_contents(&mut self, _toc: &TableOfContents) -> Result<(), Self::Error> {
        Ok(())
    }

    fn visit_discrete_header(&mut self, header: &DiscreteHeader) -> Result<(), Self::Error> {
        let tag = match header.level {
            1 => "h1",
            2 => "h2",
            3 => "h3",
            4 => "h4",
            _ => "h5",
        };
        write!(self.writer_mut(), "<{tag} class=\"discrete\">")?;
        self.visit_inline_nodes(&header.title)?;
        write!(self.writer_mut(), "</{tag}>")?;
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
        write!(self.writer, "{}", crate::escape::escape_html(text))?;
        Ok(())
    }
}

impl<W: Write> WritableVisitor for ManpageHtmlVisitor<W> {
    fn writer_mut(&mut self) -> &mut dyn Write {
        &mut self.writer
    }
}
