//! Visitor implementation for Markdown conversion.

use std::{io::Write, rc::Rc};

use acdc_converters_core::{
    Converter, Diagnostics,
    code::detect_language,
    list::OrderedListNumbering,
    media::resolve_target,
    visitor::{Visitor, WritableVisitor},
    xref::{XrefDisplay, resolve_xref},
};
use acdc_parser::{
    Admonition, Audio, Block, CalloutList, CrossReference, DelimitedBlock, DelimitedBlockType,
    DescriptionList, DiscreteHeader, Document, Header, Image, InlineMacro, InlineNode, ListItem,
    OrderedList, PageBreak, Paragraph, Section, Source, Table, TableOfContents, ThematicBreak,
    UnorderedList, Video,
};

use crate::{Error, MarkdownVariant, Processor};

/// Markdown visitor that generates Markdown output from `AsciiDoc` AST.
pub struct MarkdownVisitor<'a, 'd, W: Write> {
    writer: W,
    pub(crate) processor: Processor<'a>,
    /// Per-conversion diagnostics handle.
    pub(crate) diagnostics: Diagnostics<'d>,
    /// Current heading level (for nested sections).
    pub(crate) heading_level: usize,
    list_depth: usize,
    /// Collected footnotes for rendering at document end.
    /// Stored as `(id, pre-rendered markdown content)` so that the visitor
    /// does not need to borrow data from the document being walked.
    pub(crate) footnotes: Vec<(String, String)>,
}

impl<'a, 'd, W: Write> MarkdownVisitor<'a, 'd, W> {
    /// Create a new Markdown visitor.
    pub fn new(writer: W, processor: Processor<'a>, diagnostics: Diagnostics<'d>) -> Self {
        Self {
            writer,
            processor,
            diagnostics,
            heading_level: 0,
            list_depth: 0,
            footnotes: Vec::new(),
        }
    }

    /// Get the Markdown variant being used.
    fn variant(&self) -> MarkdownVariant {
        self.processor.variant()
    }

    fn media_target(&self, source: &Source<'_>) -> String {
        resolve_target(&source.to_string(), self.processor.document_attributes())
    }

    /// Write a warning comment to the output for unsupported features.
    fn write_warning(&mut self, feature: &str, fallback: &str) -> Result<(), Error> {
        self.diagnostics.warn_with_advice(
            format!("{feature} not natively supported in Markdown, {fallback}"),
            "Check whether the selected Markdown variant can represent this construct, or use a backend that preserves it.",
        );
        // Markdown comments are not standard, but HTML comments work in most renderers
        writeln!(
            self.writer,
            "<!-- Warning: {feature} not natively supported in Markdown, {fallback} -->"
        )?;
        Ok(())
    }

    /// Render a collapsible example block as embedded HTML `<details>/<summary>`.
    ///
    /// GitHub, GitLab, and most Markdown renderers accept inline HTML, and
    /// `<details>` is the idiomatic way to express collapsible content.
    fn write_collapsible<F>(
        &mut self,
        title: &acdc_parser::Title<'_>,
        is_open: bool,
        write_body: F,
    ) -> Result<(), Error>
    where
        F: FnOnce(&mut Self) -> Result<bool, Error>,
    {
        if is_open {
            writeln!(self.writer, "<details open>")?;
        } else {
            writeln!(self.writer, "<details>")?;
        }
        write!(self.writer, "<summary>")?;
        if title.is_empty() {
            write!(self.writer, "Details")?;
        } else {
            self.visit_inline_nodes(title.as_ref())?;
        }
        writeln!(self.writer, "</summary>")?;
        // Blank line so inner content is rendered as Markdown inside <details>.
        writeln!(self.writer)?;
        if write_body(self)? {
            writeln!(self.writer)?;
        }
        writeln!(self.writer, "</details>")?;
        Ok(())
    }

    fn block_has_output(&self, block: &Block) -> bool {
        match block {
            Block::DelimitedBlock(block) => match &block.inner {
                DelimitedBlockType::DelimitedOpen(blocks) => {
                    blocks.iter().any(|block| self.block_has_output(block))
                }
                DelimitedBlockType::DelimitedQuote(blocks) => {
                    blocks.iter().any(|block| self.block_has_output(block))
                }
                DelimitedBlockType::DelimitedTable(table) => {
                    self.variant() == MarkdownVariant::CommonMark || !table.rows.is_empty()
                }
                DelimitedBlockType::DelimitedExample(_)
                | DelimitedBlockType::DelimitedListing(_)
                | DelimitedBlockType::DelimitedLiteral(_)
                | DelimitedBlockType::DelimitedSidebar(_)
                | DelimitedBlockType::DelimitedPass(_)
                | DelimitedBlockType::DelimitedVerse(_)
                | DelimitedBlockType::DelimitedStem(_) => true,
                DelimitedBlockType::DelimitedComment(_) | _ => false,
            },
            Block::TableOfContents(_)
            | Block::Admonition(_)
            | Block::DiscreteHeader(_)
            | Block::ThematicBreak(_)
            | Block::PageBreak(_)
            | Block::UnorderedList(_)
            | Block::OrderedList(_)
            | Block::CalloutList(_)
            | Block::DescriptionList(_)
            | Block::Section(_)
            | Block::Paragraph(_)
            | Block::Image(_)
            | Block::Audio(_)
            | Block::Video(_) => true,
            Block::Comment(_) | Block::DocumentAttribute(_) | _ => false,
        }
    }

    fn visit_separated_blocks(
        &mut self,
        blocks: &[Block],
        mut has_output: bool,
    ) -> Result<bool, Error> {
        for block in blocks {
            if !self.block_has_output(block) {
                continue;
            }
            if has_output {
                writeln!(self.writer)?;
            }
            self.visit_block(block)?;
            has_output = true;
        }
        Ok(has_output)
    }

    fn visit_blockquote_blocks(&mut self, blocks: &[Block]) -> Result<(), Error> {
        let mut has_output = false;
        for block in blocks {
            if !self.block_has_output(block) {
                continue;
            }
            if has_output {
                writeln!(self.writer, ">")?;
            }
            write!(self.writer, "> ")?;
            self.visit_block(block)?;
            has_output = true;
        }
        Ok(())
    }
}

impl<W: Write> WritableVisitor for MarkdownVisitor<'_, '_, W> {
    fn writer_mut(&mut self) -> &mut dyn Write {
        &mut self.writer
    }
}

impl<W: Write> Visitor for MarkdownVisitor<'_, '_, W> {
    type Error = Error;

    fn visit_document(&mut self, doc: &Document) -> Result<(), Self::Error> {
        self.visit_document_start(doc)?;

        let mut has_output = false;
        if let Some(header) = &doc.header
            && !header.title.is_empty()
        {
            self.visit_header(header)?;
            has_output = true;
        }

        self.visit_body_content_start(doc)?;

        let first_section = doc
            .blocks
            .iter()
            .position(|block| matches!(block, Block::Section(_)));
        let (preamble, remaining) = first_section.map_or_else(
            || (doc.blocks.as_slice(), &[][..]),
            |index| doc.blocks.split_at(index),
        );
        let emit_preamble = doc.header.is_some()
            && first_section.is_some()
            && preamble
                .iter()
                .any(|block| !matches!(block, Block::Comment(_) | Block::DocumentAttribute(_)));

        if emit_preamble {
            self.visit_preamble_start(doc)?;
        }
        has_output = self.visit_separated_blocks(preamble, has_output)?;
        if emit_preamble {
            self.visit_preamble_end(doc)?;
        }
        has_output = self.visit_separated_blocks(remaining, has_output)?;

        self.visit_document_supplements(doc)?;
        if self.variant() == MarkdownVariant::GitHubFlavored && !self.footnotes.is_empty() {
            if has_output {
                writeln!(self.writer)?;
            }
            let footnotes = std::mem::take(&mut self.footnotes);
            for (id, content) in footnotes {
                writeln!(self.writer, "[^{id}]: {content}")?;
            }
            has_output = true;
        }

        if !has_output {
            writeln!(self.writer)?;
        }
        self.visit_document_end(doc)
    }

    fn visit_document_start(&mut self, _doc: &Document) -> Result<(), Self::Error> {
        // No document-level preamble needed for Markdown
        // Title will be rendered as level-1 heading if present
        Ok(())
    }

    fn visit_header(&mut self, header: &Header) -> Result<(), Self::Error> {
        // Render document title as top-level heading
        if !header.title.is_empty() {
            write!(self.writer, "# ")?;
            self.visit_inline_nodes(header.title.as_ref())?;
            writeln!(self.writer)?;
        }

        // Document attributes and metadata are not directly representable in Markdown
        // Skip author, revision, etc. for now
        Ok(())
    }

    fn visit_section(&mut self, section: &Section) -> Result<(), Self::Error> {
        let level = section.level + 1; // AsciiDoc levels are 0-indexed, Markdown uses 1-6
        let level = level.min(6); // Markdown only supports 6 heading levels

        if section.level >= 6 {
            self.diagnostics.warn_with_advice(
                format!(
                    "section level {} exceeds Markdown maximum 6, capping at level 6",
                    section.level + 1
                ),
                "Markdown only has six heading levels. Reduce the source section depth if the distinction matters.",
            );
        }

        // Write heading
        let hashes = "#".repeat(level as usize);
        write!(self.writer, "{hashes} ")?;
        self.visit_inline_nodes(section.title.as_ref())?;
        writeln!(self.writer)?;

        // Visit section content
        let prev_level = self.heading_level;
        self.heading_level = level as usize;

        self.visit_separated_blocks(&section.content, true)?;

        self.heading_level = prev_level;
        Ok(())
    }

    fn visit_paragraph(&mut self, paragraph: &Paragraph) -> Result<(), Self::Error> {
        if paragraph.metadata.style == Some("example")
            && paragraph.metadata.options.contains(&"collapsible")
        {
            let is_open = paragraph.metadata.options.contains(&"open");
            return self.write_collapsible(&paragraph.title, is_open, |v| {
                v.visit_inline_nodes(&paragraph.content)?;
                writeln!(v.writer)?;
                Ok(true)
            });
        }

        self.visit_inline_nodes(&paragraph.content)?;
        writeln!(self.writer)?;
        Ok(())
    }

    fn visit_unordered_list(&mut self, list: &UnorderedList) -> Result<(), Self::Error> {
        self.visit_list_items(&list.items, "-", 1)
    }

    fn visit_ordered_list(&mut self, list: &OrderedList) -> Result<(), Self::Error> {
        // Markdown (CommonMark/GFM) can only express numeric ordered markers.
        // An explicit numbering style that isn't already numeric (arabic/decimal)
        // is rendered numerically, with a warning.
        if let Some(numbering) = list
            .metadata
            .style
            .and_then(OrderedListNumbering::from_explicit_style)
            && !matches!(
                numbering,
                OrderedListNumbering::Arabic | OrderedListNumbering::Decimal
            )
        {
            self.write_list_indent()?;
            self.write_warning(
                "non-numeric ordered list numbering styles",
                "rendering numerically",
            )?;
        }
        let start = list
            .metadata
            .attributes
            .get_string("start")
            .and_then(|start| start.parse::<usize>().ok())
            .filter(|start| *start > 0)
            .unwrap_or(1);
        self.visit_list_items(&list.items, "1.", start)
    }

    fn visit_list_item(&mut self, _item: &ListItem) -> Result<(), Self::Error> {
        // This is handled by visit_list_items
        Ok(())
    }

    fn visit_thematic_break(&mut self, _br: &ThematicBreak) -> Result<(), Self::Error> {
        writeln!(self.writer, "---")?;
        Ok(())
    }

    fn visit_page_break(&mut self, _pb: &PageBreak) -> Result<(), Self::Error> {
        // Page breaks don't exist in Markdown; use thematic break as fallback
        self.write_warning("page breaks", "using horizontal rule")?;
        writeln!(self.writer, "---")?;
        Ok(())
    }

    fn visit_table_of_contents(&mut self, _toc: &TableOfContents) -> Result<(), Self::Error> {
        // TOC must be manually generated in Markdown
        self.write_warning(
            "automatic table of contents",
            "skipping (must be generated manually)",
        )?;
        Ok(())
    }

    fn visit_delimited_block(&mut self, block: &DelimitedBlock) -> Result<(), Self::Error> {
        match &block.inner {
            DelimitedBlockType::DelimitedListing(content) => {
                // Use fenced code block
                let language = detect_language(&block.metadata).unwrap_or_default();

                writeln!(self.writer, "```{language}")?;
                self.write_code_block_content(content)?;
                writeln!(self.writer, "```")?;
            }
            DelimitedBlockType::DelimitedLiteral(content) => {
                // Use fenced code block without syntax highlighting
                writeln!(self.writer, "```")?;
                self.write_code_block_content(content)?;
                writeln!(self.writer, "```")?;
            }
            DelimitedBlockType::DelimitedQuote(blocks) => {
                self.visit_blockquote_blocks(blocks)?;
            }
            DelimitedBlockType::DelimitedExample(blocks) => {
                if block.metadata.options.contains(&"collapsible") {
                    let is_open = block.metadata.options.contains(&"open");
                    self.write_collapsible(&block.title, is_open, |v| {
                        v.visit_separated_blocks(blocks, false)
                    })?;
                } else {
                    // Examples don't have a direct Markdown equivalent
                    // Use blockquote as fallback
                    self.write_warning("example blocks", "using blockquote")?;
                    self.visit_blockquote_blocks(blocks)?;
                }
            }
            DelimitedBlockType::DelimitedSidebar(blocks) => {
                // Sidebars don't have a direct Markdown equivalent
                self.write_warning("sidebar blocks", "using blockquote")?;
                self.visit_blockquote_blocks(blocks)?;
            }
            DelimitedBlockType::DelimitedOpen(blocks) => {
                // Open blocks are just containers, render contents normally
                self.visit_separated_blocks(blocks, false)?;
            }
            DelimitedBlockType::DelimitedPass(_content) => {
                // Passthrough blocks - skip for now
                self.write_warning("passthrough blocks", "skipping content")?;
            }
            DelimitedBlockType::DelimitedTable(table) => {
                self.visit_table_inner(table)?;
            }
            DelimitedBlockType::DelimitedVerse(content) => {
                // Verse blocks - use blockquote with line breaks preserved
                write!(self.writer, "> ")?;
                for node in content {
                    self.visit_inline_node(node)?;
                }
                writeln!(self.writer)?;
            }
            DelimitedBlockType::DelimitedComment(_) => {
                // Comments don't get rendered
            }
            DelimitedBlockType::DelimitedStem(_stem) => {
                // Math blocks - not supported in standard Markdown
                self.write_warning("STEM/math blocks", "skipping (use LaTeX-enabled renderer)")?;
            }
            _ => {
                self.diagnostics
                    .warn("unsupported delimited block type in Markdown, skipping content");
            }
        }
        Ok(())
    }

    fn visit_admonition(&mut self, admonition: &Admonition) -> Result<(), Self::Error> {
        // GitHub Flavored Markdown supports Alerts syntax (> [!TYPE])
        // CommonMark falls back to blockquote with bold label
        let alert_type = match admonition.variant {
            acdc_parser::AdmonitionVariant::Note => "NOTE",
            acdc_parser::AdmonitionVariant::Tip => "TIP",
            acdc_parser::AdmonitionVariant::Important => "IMPORTANT",
            acdc_parser::AdmonitionVariant::Warning => "WARNING",
            acdc_parser::AdmonitionVariant::Caution => "CAUTION",
        };

        if self.variant() == MarkdownVariant::GitHubFlavored {
            // Use GitHub Alerts syntax (native support, no warning needed)
            writeln!(self.writer, "> [!{alert_type}]")?;
        } else {
            // CommonMark: use blockquote with bold label
            let label = match admonition.variant {
                acdc_parser::AdmonitionVariant::Note => "Note",
                acdc_parser::AdmonitionVariant::Tip => "Tip",
                acdc_parser::AdmonitionVariant::Important => "Important",
                acdc_parser::AdmonitionVariant::Warning => "Warning",
                acdc_parser::AdmonitionVariant::Caution => "Caution",
            };
            self.write_warning(
                &format!("{label} admonitions"),
                "using blockquote with label",
            )?;
            writeln!(self.writer, "> **{label}**")?;
        }

        let mut has_body = false;
        for block in &admonition.blocks {
            if !self.block_has_output(block) {
                continue;
            }
            if has_body {
                writeln!(self.writer, ">")?;
            }
            write!(self.writer, "> ")?;
            self.visit_block(block)?;
            has_body = true;
        }
        Ok(())
    }

    fn visit_discrete_header(&mut self, header: &DiscreteHeader) -> Result<(), Self::Error> {
        // Discrete headers are just headings without section structure
        let level = (header.level + 1).min(6);
        let hashes = "#".repeat(level as usize);
        write!(self.writer, "{hashes} ")?;
        self.visit_inline_nodes(header.title.as_ref())?;
        writeln!(self.writer)?;
        Ok(())
    }

    fn visit_image(&mut self, image: &Image) -> Result<(), Self::Error> {
        let alt = image
            .metadata
            .attributes
            .get_string("alt")
            .unwrap_or(std::borrow::Cow::Borrowed("image"));

        let target = self.media_target(&image.source);

        // Markdown image syntax: ![alt](url "title")
        if let Some(title) = image.metadata.attributes.get_string("title") {
            writeln!(self.writer, r#"![{alt}]({target} "{title}")"#)?;
        } else {
            writeln!(self.writer, "![{alt}]({target})")?;
        }
        Ok(())
    }

    fn visit_video(&mut self, video: &Video) -> Result<(), Self::Error> {
        // Video embedding not supported in standard Markdown
        self.write_warning("video embedding", "providing link")?;
        if let Some(first_source) = video.sources.first() {
            let target = self.media_target(first_source);
            writeln!(self.writer, "[Video: {target}]({target})")?;
        }
        Ok(())
    }

    fn visit_audio(&mut self, audio: &Audio) -> Result<(), Self::Error> {
        // Audio embedding not supported in standard Markdown
        self.write_warning("audio embedding", "providing link")?;
        let target = self.media_target(&audio.source);
        writeln!(self.writer, "[Audio: {target}]({target})")?;
        Ok(())
    }

    fn visit_description_list(&mut self, list: &DescriptionList) -> Result<(), Self::Error> {
        // Description lists (definition lists) not in standard Markdown
        self.write_list_indent()?;
        self.write_warning("description lists", "using regular list")?;
        for item in &list.items {
            // Render term as bold text in a list item
            self.write_list_indent()?;
            write!(self.writer, "- **")?;
            self.visit_inline_nodes(&item.term)?;
            writeln!(self.writer, "**")?;

            // Render principal text (inline content after delimiter) if present
            if !item.principal_text.is_empty() {
                self.write_list_indent()?;
                write!(self.writer, "  ")?;
                self.visit_inline_nodes(&item.principal_text)?;
                writeln!(self.writer)?;
            }

            // Render description blocks indented
            for block in &item.description {
                self.list_depth += 1;
                let result = (|| {
                    if !matches!(
                        block,
                        Block::DescriptionList(_) | Block::OrderedList(_) | Block::UnorderedList(_)
                    ) {
                        self.write_list_indent()?;
                    }
                    self.visit_block(block)
                })();
                self.list_depth -= 1;
                result?;
            }
        }
        Ok(())
    }

    fn visit_callout_list(&mut self, _list: &CalloutList) -> Result<(), Self::Error> {
        // Callout lists not supported in Markdown
        self.write_warning("callout lists", "skipping")?;
        Ok(())
    }

    fn visit_inline_node(&mut self, node: &InlineNode) -> Result<(), Self::Error> {
        match node {
            InlineNode::PlainText(text) => {
                write!(self.writer, "{}", Self::escape_markdown(text.content))?;
            }
            InlineNode::BoldText(text) => {
                write!(self.writer, "**")?;
                self.visit_inline_nodes(&text.content)?;
                write!(self.writer, "**")?;
            }
            InlineNode::ItalicText(text) => {
                write!(self.writer, "*")?;
                self.visit_inline_nodes(&text.content)?;
                write!(self.writer, "*")?;
            }
            InlineNode::MonospaceText(text) => {
                write!(self.writer, "`")?;
                self.visit_inline_nodes(&text.content)?;
                write!(self.writer, "`")?;
            }
            InlineNode::HighlightText(text) => {
                // Highlighting not in standard Markdown
                // Just render as plain text
                self.visit_inline_nodes(&text.content)?;
            }
            InlineNode::SubscriptText(text) => {
                // Subscript not in standard Markdown
                // Render with HTML tags (works in most renderers)
                write!(self.writer, "<sub>")?;
                self.visit_inline_nodes(&text.content)?;
                write!(self.writer, "</sub>")?;
            }
            InlineNode::SuperscriptText(text) => {
                // Superscript not in standard Markdown
                write!(self.writer, "<sup>")?;
                self.visit_inline_nodes(&text.content)?;
                write!(self.writer, "</sup>")?;
            }
            InlineNode::LineBreak(_) => {
                writeln!(self.writer, "  ")?; // Two spaces for line break in Markdown
            }
            InlineNode::RawText(text) => {
                write!(self.writer, "{}", text.content)?;
            }
            InlineNode::VerbatimText(text) => {
                write!(self.writer, "`{}`", text.content)?;
            }
            InlineNode::StandaloneCurvedApostrophe(_) => {
                write!(self.writer, "'")?;
            }
            InlineNode::CurvedQuotationText(text) => {
                // Render with proper quotes
                write!(self.writer, "\"")?;
                self.visit_inline_nodes(&text.content)?;
                write!(self.writer, "\"")?;
            }
            InlineNode::CurvedApostropheText(text) => {
                write!(self.writer, "'")?;
                self.visit_inline_nodes(&text.content)?;
                write!(self.writer, "'")?;
            }
            InlineNode::InlineAnchor(_anchor) => {
                // Anchors are not directly supported in Markdown
                // Could use HTML <a name="..."></a> but skip for now
            }
            InlineNode::Macro(mac) => {
                self.visit_inline_macro_inner(mac)?;
            }
            InlineNode::CalloutRef(_) => {
                // Callout references not supported
                // Skip silently
            }
            _ => {
                self.diagnostics.warn(format!(
                    "unsupported inline node in Markdown, skipping node: {node:?}"
                ));
            }
        }
        Ok(())
    }

    fn visit_text(&mut self, text: &str) -> Result<(), Self::Error> {
        write!(self.writer, "{}", Self::escape_markdown(text))?;
        Ok(())
    }
}

impl<W: Write> MarkdownVisitor<'_, '_, W> {
    /// Write code block content as raw text (no inline formatting).
    fn write_code_block_content(&mut self, content: &[InlineNode]) -> Result<(), Error> {
        for node in content {
            match node {
                InlineNode::VerbatimText(text) => write!(self.writer, "{}", text.content)?,
                InlineNode::RawText(text) => write!(self.writer, "{}", text.content)?,
                InlineNode::PlainText(text) => write!(self.writer, "{}", text.content)?,
                InlineNode::LineBreak(_) => writeln!(self.writer)?,
                InlineNode::BoldText(_)
                | InlineNode::ItalicText(_)
                | InlineNode::MonospaceText(_)
                | InlineNode::HighlightText(_)
                | InlineNode::SubscriptText(_)
                | InlineNode::SuperscriptText(_)
                | InlineNode::CurvedQuotationText(_)
                | InlineNode::CurvedApostropheText(_)
                | InlineNode::StandaloneCurvedApostrophe(_)
                | InlineNode::InlineAnchor(_)
                | InlineNode::Macro(_)
                | InlineNode::CalloutRef(_)
                | _ => {}
            }
        }
        writeln!(self.writer)?;
        Ok(())
    }

    /// Handle inline macros.
    fn visit_inline_macro_inner(&mut self, mac: &InlineMacro) -> Result<(), Error> {
        match mac {
            InlineMacro::Link(link) => {
                let target = link.target.to_string();
                if link.text.is_empty() {
                    write!(self.writer, "[{target}]({target})")?;
                } else {
                    write!(self.writer, "[")?;
                    for node in &link.text {
                        self.visit_inline_node(node)?;
                    }
                    write!(self.writer, "]({target})")?;
                }
            }
            InlineMacro::Image(image) => {
                // Inline image macro
                let target = self.media_target(&image.source);
                // Use the image alt text or default
                let alt = "image"; // Inline images don't have attributes field
                write!(self.writer, "![{alt}]({target})")?;
            }
            InlineMacro::Icon(_icon) => {
                // Icons not supported in Markdown - skip silently
            }
            InlineMacro::Keyboard(_kbd) => {
                // Keyboard shortcuts - skip for now
            }
            InlineMacro::Button(_btn) => {
                // Button formatting - skip for now
            }
            InlineMacro::Menu(_menu) => {
                // Menu navigation - skip for now
            }
            InlineMacro::Footnote(footnote) => {
                if self.variant() == MarkdownVariant::GitHubFlavored {
                    // GFM supports footnotes
                    let id: String = footnote
                        .id
                        .as_ref()
                        .map_or_else(|| footnote.number.to_string(), |c| (*c).to_string());

                    // Store footnote for later rendering (only if not already stored)
                    if !footnote.content.is_empty()
                        && !self
                            .footnotes
                            .iter()
                            .any(|(existing_id, _)| existing_id == &id)
                    {
                        // Pre-render the footnote content into a markdown string
                        // using a temporary visitor so we don't hold borrows from
                        // the document being walked.
                        let mut buffer: Vec<u8> = Vec::new();
                        {
                            let mut tmp = MarkdownVisitor {
                                writer: &mut buffer,
                                processor: self.processor.clone(),
                                diagnostics: self.diagnostics.reborrow(),
                                heading_level: self.heading_level,
                                list_depth: self.list_depth,
                                footnotes: Vec::new(),
                            };
                            for node in &footnote.content {
                                tmp.visit_inline_node(node)?;
                            }
                        }
                        let rendered = String::from_utf8(buffer).unwrap_or_default();
                        self.footnotes.push((id.clone(), rendered));
                    }

                    // Render inline reference
                    write!(self.writer, "[^{id}]")?;
                } else {
                    // CommonMark: render footnote inline with superscript number
                    write!(self.writer, "<sup>{}</sup>", footnote.number)?;
                }
            }
            InlineMacro::Url(url) => {
                // URL macro - text is Vec<InlineNode>
                let target = url.target.to_string();
                if url.text.is_empty() {
                    write!(self.writer, "[{target}]({target})")?;
                } else {
                    write!(self.writer, "[")?;
                    for node in &url.text {
                        self.visit_inline_node(node)?;
                    }
                    write!(self.writer, "]({target})")?;
                }
            }
            InlineMacro::Mailto(mailto) => {
                // Email link - text is Vec<InlineNode>
                let target = mailto.target.to_string();
                if mailto.text.is_empty() {
                    write!(self.writer, "[{target}](mailto:{target})")?;
                } else {
                    write!(self.writer, "[")?;
                    for node in &mailto.text {
                        self.visit_inline_node(node)?;
                    }
                    write!(self.writer, "](mailto:{target})")?;
                }
            }
            InlineMacro::Autolink(autolink) => {
                // Auto-detected link
                let target = autolink.url.to_string();
                write!(self.writer, "{target}")?;
            }
            InlineMacro::CrossReference(xref) => self.visit_cross_reference(xref)?,
            InlineMacro::IndexTerm(term) if term.is_visible() => {
                self.visit_inline_nodes(term.term())?;
            }
            InlineMacro::IndexTerm(_) => {}
            InlineMacro::Pass(_) | InlineMacro::Stem(_) | _ => {
                self.diagnostics.warn(format!(
                    "unsupported inline macro in Markdown, skipping macro: {mac:?}"
                ));
            }
        }
        Ok(())
    }

    /// Render a cross-reference as a link to the target's anchor.
    ///
    /// Markdown has no cross-reference syntax, so this is a plain link to the
    /// `#id` fragment. Its text is the reference's own text when it has one,
    /// otherwise the target's reference text (an explicit label or its title),
    /// falling back to `[id]` as asciidoctor does.
    fn visit_cross_reference(&mut self, xref: &CrossReference<'_>) -> Result<(), Error> {
        if !xref.text.is_empty() {
            return self.write_anchor_link(xref.target, |visitor| {
                for node in &xref.text {
                    visitor.visit_inline_node(node)?;
                }
                Ok(())
            });
        }

        // Clone the handles so the borrowed reference text and the resolution
        // guard both outlive the `&mut self` render calls.
        let references = Rc::clone(&self.processor.references);
        let guard = self.processor.xref_guard.clone();
        match resolve_xref(references.get(xref.target), xref.target, &guard) {
            XrefDisplay::Title(inlines, _scope) | XrefDisplay::Label(inlines, _scope) => self
                .write_anchor_link(xref.target, |visitor| {
                    for node in inlines {
                        visitor.visit_inline_node(node)?;
                    }
                    Ok(())
                }),
            XrefDisplay::Fallback(text) | XrefDisplay::Unresolved(text) => {
                self.write_anchor_link(xref.target, |visitor| {
                    write!(visitor.writer, "{text}")?;
                    Ok(())
                })
            }
            XrefDisplay::External(target) => self.write_anchor_link(&target, |visitor| {
                write!(visitor.writer, "[{target}]")?;
                Ok(())
            }),
            // Markdown links do not nest, so a reference inside another one's
            // text is text alone.
            XrefDisplay::Nested(text) => {
                write!(self.writer, "{text}")?;
                Ok(())
            }
        }
    }

    /// Write `text` as a link to an `#id` fragment.
    fn write_anchor_link(
        &mut self,
        target: &str,
        text: impl FnOnce(&mut Self) -> Result<(), Error>,
    ) -> Result<(), Error> {
        write!(self.writer, "[")?;
        text(self)?;
        write!(self.writer, "](#{target})")?;
        Ok(())
    }

    fn write_list_indent(&mut self) -> Result<(), Error> {
        for _ in 0..self.list_depth {
            write!(self.writer, "    ")?;
        }
        Ok(())
    }

    /// Render list items with the given marker (for both ordered and unordered lists).
    fn visit_list_items(
        &mut self,
        items: &[ListItem],
        marker: &str,
        start: usize,
    ) -> Result<(), Error> {
        for (i, item) in items.iter().enumerate() {
            self.write_list_indent()?;
            // For ordered lists, use the actual number
            let item_marker = if marker.ends_with('.') {
                format!("{}.", start.saturating_add(i))
            } else {
                marker.to_string()
            };

            // Check for task list items (GFM extension)
            let is_task = item.checked.is_some();
            let is_checked = matches!(
                item.checked,
                Some(acdc_parser::ListItemCheckedStatus::Checked)
            );

            if is_task && self.variant() == MarkdownVariant::GitHubFlavored {
                let checkbox = if is_checked { "[x]" } else { "[ ]" };
                write!(self.writer, "{item_marker} {checkbox} ")?;
            } else {
                write!(self.writer, "{item_marker} ")?;
            }

            // Render item content
            self.visit_inline_nodes(&item.principal)?;
            writeln!(self.writer)?;

            // Render nested blocks (indented)
            for block in &item.blocks {
                if matches!(block, Block::OrderedList(_) | Block::UnorderedList(_)) {
                    self.list_depth += 1;
                    let result = self.visit_block(block);
                    self.list_depth -= 1;
                    result?;
                } else {
                    write!(self.writer, "    ")?;
                    self.visit_block(block)?;
                }
            }
        }
        Ok(())
    }

    /// Render a table (handles both GFM and fallback).
    fn visit_table_inner(&mut self, table: &Table) -> Result<(), Error> {
        if self.variant() == MarkdownVariant::CommonMark {
            self.write_warning("tables", "not supported in CommonMark, skipping")?;
            return Ok(());
        }

        // GFM tables
        self.render_gfm_table(table)?;
        Ok(())
    }

    /// Render a GFM table.
    fn render_gfm_table(&mut self, table: &Table) -> Result<(), Error> {
        // Note: GFM tables don't support cell spanning, but we render what we can

        // GFM tables: | Header 1 | Header 2 |
        //             |----------|----------|
        //             | Cell 1   | Cell 2   |

        let rows = &table.rows;
        if rows.is_empty() {
            return Ok(());
        }

        // Check if table has a header
        let has_header = table.header.is_some();

        // Render header row if present
        if let Some(ref header) = table.header {
            write!(self.writer, "|")?;
            for column in &header.columns {
                write!(self.writer, " ")?;
                for block in &column.content {
                    // Tables cells can only contain inline content in Markdown
                    if let Block::Paragraph(para) = block {
                        self.visit_inline_nodes(&para.content)?;
                    }
                }
                write!(self.writer, " |")?;
            }
            writeln!(self.writer)?;

            // Add delimiter row
            write!(self.writer, "|")?;
            for _ in &header.columns {
                write!(self.writer, " --- |")?;
            }
            writeln!(self.writer)?;
        } else if let Some(first_row) = rows.first() {
            // No explicit header, use first row as header
            write!(self.writer, "|")?;
            for column in &first_row.columns {
                write!(self.writer, " ")?;
                for block in &column.content {
                    if let Block::Paragraph(para) = block {
                        self.visit_inline_nodes(&para.content)?;
                    }
                }
                write!(self.writer, " |")?;
            }
            writeln!(self.writer)?;

            // Add delimiter row
            write!(self.writer, "|")?;
            for _ in &first_row.columns {
                write!(self.writer, " --- |")?;
            }
            writeln!(self.writer)?;
        }

        // Render body rows (skip first if it was used as header)
        let start_idx = usize::from(!has_header);
        for row in rows.iter().skip(start_idx) {
            write!(self.writer, "|")?;
            for column in &row.columns {
                write!(self.writer, " ")?;
                for block in &column.content {
                    if let Block::Paragraph(para) = block {
                        self.visit_inline_nodes(&para.content)?;
                    }
                }
                write!(self.writer, " |")?;
            }
            writeln!(self.writer)?;
        }
        Ok(())
    }

    /// Escape special Markdown characters.
    ///
    /// Only escapes characters that actually need escaping in prose context.
    /// Most special characters only need escaping in specific positions.
    fn escape_markdown(text: &str) -> String {
        // Characters that ALWAYS need escaping: \ ` * _ [ ] |
        // Characters that only need escaping in specific contexts are not escaped
        let mut result = String::with_capacity(text.len());
        for ch in text.chars() {
            match ch {
                '\\' | '`' | '*' | '_' | '[' | ']' | '|' => {
                    result.push('\\');
                    result.push(ch);
                }
                _ => result.push(ch),
            }
        }
        result
    }
}
