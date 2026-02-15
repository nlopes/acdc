//! Visitor implementation for manpage (roff/troff) conversion.

use std::io::Write;

use acdc_converters_core::visitor::{Visitor, WritableVisitor};
use acdc_parser::{
    Admonition, Audio, CalloutList, DelimitedBlock, DescriptionList, DiscreteHeader, Document,
    Header, Image, InlineMacro, InlineNode, ListItem, OrderedList, PageBreak, Paragraph, Section,
    TableOfContents, ThematicBreak, UnorderedList, Video,
};

use crate::escape::{EscapeMode, manify};

use crate::{Error, Processor};

/// Manpage visitor that generates roff/troff output from `AsciiDoc` AST.
pub struct ManpageVisitor<W: Write> {
    writer: W,
    pub(crate) processor: Processor,
    /// Current nesting depth for lists (used for .RS/.RE indentation).
    pub(crate) list_depth: usize,
    /// Whether we're currently in the NAME section (which shouldn't have .sp before content).
    pub(crate) in_name_section: bool,
    /// Whether the next text node should have leading whitespace stripped.
    /// Set after `.URL`/`.MTO` macros which end with a newline.
    pub(crate) strip_next_leading_space: bool,
    /// Title of the first level-1 section (for NAME validation).
    first_section_title: Option<String>,
    /// Title of the second level-1 section (for SYNOPSIS validation).
    second_section_title: Option<String>,
}

impl<W: Write> ManpageVisitor<W> {
    /// Create a new manpage visitor.
    pub fn new(writer: W, processor: Processor) -> Self {
        Self {
            writer,
            processor,
            list_depth: 0,
            in_name_section: false,
            strip_next_leading_space: false,
            first_section_title: None,
            second_section_title: None,
        }
    }

    /// Record a level-1 section title for validation.
    pub(crate) fn record_section_title(&mut self, title: &str) {
        if self.first_section_title.is_none() {
            self.first_section_title = Some(title.to_string());
        } else if self.second_section_title.is_none() {
            self.second_section_title = Some(title.to_string());
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

    /// Collect trailing content for a mailto autolink.
    ///
    /// Collects and renders all inline nodes following the mailto until whitespace
    /// is encountered (in `PlainText`). Returns:
    /// - The rendered trailing string
    /// - The number of fully consumed nodes to skip
    /// - The number of bytes consumed from a partially consumed `PlainText` (0 if none)
    fn collect_trailing_for_mailto(
        &self,
        nodes: &[InlineNode],
    ) -> Result<(String, usize, usize), Error> {
        let mut buf = Vec::new();
        let processor = self.processor.clone();
        let mut trailing_visitor = ManpageVisitor::new(&mut buf, processor);
        let mut skip_count = 0;
        let mut partial_bytes = 0;

        for next_node in nodes {
            match next_node {
                InlineNode::PlainText(text) => {
                    // Stop if text starts with whitespace
                    if text.content.starts_with(char::is_whitespace) {
                        break;
                    }
                    // If text contains whitespace, render only up to it and stop
                    if let Some(ws_pos) = text.content.find(char::is_whitespace) {
                        let partial = &text.content[..ws_pos];
                        // For trailing content inside quotes, only escape hyphens
                        // (don't escape leading periods - they won't be interpreted as macros)
                        let escaped = partial.replace('-', "\\-");
                        write!(trailing_visitor.writer, "{escaped}")?;
                        // Record how many bytes we consumed from this node
                        partial_bytes = ws_pos;
                        break;
                    }
                    // Render entire PlainText - only escape hyphens for trailing content
                    let escaped = text.content.replace('-', "\\-");
                    write!(trailing_visitor.writer, "{escaped}")?;
                    skip_count += 1;
                }
                // Render formatted text nodes
                InlineNode::BoldText(_)
                | InlineNode::ItalicText(_)
                | InlineNode::MonospaceText(_)
                | InlineNode::HighlightText(_)
                | InlineNode::SubscriptText(_)
                | InlineNode::SuperscriptText(_)
                | InlineNode::CurvedQuotationText(_)
                | InlineNode::CurvedApostropheText(_) => {
                    trailing_visitor.visit_inline_node(next_node)?;
                    skip_count += 1;
                }
                // Stop on these node types
                InlineNode::RawText(_)
                | InlineNode::VerbatimText(_)
                | InlineNode::StandaloneCurvedApostrophe(_)
                | InlineNode::LineBreak(_)
                | InlineNode::InlineAnchor(_)
                | InlineNode::Macro(_)
                | _ => break,
            }
        }

        let trailing = String::from_utf8_lossy(&buf).to_string();
        Ok((trailing, skip_count, partial_bytes))
    }
}

impl<W: Write> Visitor for ManpageVisitor<W> {
    type Error = Error;

    fn visit_document_start(&mut self, doc: &Document) -> Result<(), Self::Error> {
        crate::document::visit_document_start(doc, self)
    }

    fn visit_document_supplements(&mut self, doc: &Document) -> Result<(), Self::Error> {
        // Render footnotes as NOTES section (matching asciidoctor)
        if !doc.footnotes.is_empty() {
            let w = self.writer_mut();
            writeln!(w, ".SH \"NOTES\"")?;

            for footnote in &doc.footnotes {
                let w = self.writer_mut();
                writeln!(w, ".IP [{}] 4", footnote.number)?;
                self.visit_inline_nodes(&footnote.content)?;
                let w = self.writer_mut();
                writeln!(w)?;
            }
        }

        // Render AUTHOR(S) section if document has authors
        if let Some(header) = &doc.header
            && !header.authors.is_empty()
        {
            let w = self.writer_mut();
            if header.authors.len() == 1 {
                writeln!(w, ".SH \"AUTHOR\"")?;
            } else {
                writeln!(w, ".SH \"AUTHORS\"")?;
            }
            for author in &header.authors {
                let w = self.writer_mut();
                writeln!(w, ".sp")?;
                let name = crate::document::format_author_name(author);
                write!(w, "\\fB{}\\fP", manify(&name, EscapeMode::Normalize))?;
                if let Some(email) = &author.email {
                    let escaped_email = email.replace('@', "\\(at");
                    writeln!(w, " \\c\n.MTO \"{escaped_email}\" \"\" \"\"")?;
                } else {
                    writeln!(w)?;
                }
            }
        }

        Ok(())
    }

    fn visit_document_end(&mut self, _doc: &Document) -> Result<(), Self::Error> {
        // Validate manpage section order conventions
        if let Some(ref first) = self.first_section_title
            && !first.eq_ignore_ascii_case("NAME")
        {
            tracing::warn!(
                first_section = %first,
                "manpage convention: NAME should be the first section"
            );
        }
        if let Some(ref second) = self.second_section_title
            && !second.eq_ignore_ascii_case("SYNOPSIS")
        {
            tracing::warn!(
                second_section = %second,
                "manpage convention: SYNOPSIS should be the second section"
            );
        }
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
        let mut i = 0;
        while i < nodes.len() {
            let Some(node) = nodes.get(i) else {
                break;
            };

            // Check if this is a mailto autolink - collect all trailing non-whitespace
            if let InlineNode::Macro(InlineMacro::Autolink(al)) = node
                && al.url.to_string().starts_with("mailto:")
            {
                let (trailing, skip_count, partial_bytes) =
                    self.collect_trailing_for_mailto(nodes.get(i + 1..).unwrap_or_default())?;

                crate::inlines::write_autolink_with_trailing(self, al, &trailing)?;
                i += 1 + skip_count;

                // If a PlainText node was partially consumed, render only the remainder
                if partial_bytes > 0
                    && let Some(InlineNode::PlainText(text)) = nodes.get(i)
                {
                    let remaining = &text.content[partial_bytes..];
                    let content = if self.strip_next_leading_space {
                        self.strip_next_leading_space = false;
                        remaining.trim_start()
                    } else {
                        remaining
                    };
                    if !content.is_empty() {
                        let escaped =
                            crate::escape::manify(content, crate::escape::EscapeMode::Normalize);
                        write!(self.writer_mut(), "{escaped}")?;
                    }
                    i += 1;
                }
                continue;
            }

            self.visit_inline_node(node)?;
            i += 1;
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
