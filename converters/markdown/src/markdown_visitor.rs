//! Visitor implementation for Markdown conversion.

use std::io::Write;

use acdc_converters_core::{
    Warning, WarningSource,
    code::detect_language,
    visitor::{Visitor, WritableVisitor},
};
use acdc_parser::{
    Admonition, Audio, Block, CalloutList, DelimitedBlock, DelimitedBlockType, DescriptionList,
    DiscreteHeader, Document, Header, Image, InlineMacro, InlineNode, ListItem, OrderedList,
    PageBreak, Paragraph, Section, Table, TableOfContents, ThematicBreak, UnorderedList, Video,
};

use crate::{Error, MarkdownVariant, Processor};

/// Markdown visitor that generates Markdown output from `AsciiDoc` AST.
pub struct MarkdownVisitor<'a, W: Write> {
    writer: W,
    pub(crate) processor: Processor<'a>,
    /// Current heading level (for nested sections).
    pub(crate) heading_level: usize,
    /// Collected footnotes for rendering at document end.
    /// Stored as `(id, pre-rendered markdown content)` so that the visitor
    /// does not need to borrow data from the document being walked.
    pub(crate) footnotes: Vec<(String, String)>,
}

impl<'a, W: Write> MarkdownVisitor<'a, W> {
    /// Create a new Markdown visitor.
    pub fn new(writer: W, processor: Processor<'a>) -> Self {
        Self {
            writer,
            processor,
            heading_level: 0,
            footnotes: Vec::new(),
        }
    }

    /// Get the Markdown variant being used.
    fn variant(&self) -> MarkdownVariant {
        self.processor.variant()
    }

    /// Write a warning comment to the output for unsupported features.
    fn write_warning(&mut self, feature: &str, fallback: &str) -> Result<(), Error> {
        self.processor.warnings.emit(
            Warning::new(
                WarningSource::new("markdown")
                    .with_variant(self.processor.variant().to_string()),
                format!("{feature} not natively supported in Markdown, {fallback}"),
                None,
            )
            .with_advice("Check whether the selected Markdown variant can represent this construct, or use a backend that preserves it."),
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
        F: FnOnce(&mut Self) -> Result<(), Error>,
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
        write_body(self)?;
        writeln!(self.writer, "</details>")?;
        writeln!(self.writer)?;
        Ok(())
    }
}

impl<W: Write> WritableVisitor for MarkdownVisitor<'_, W> {
    fn writer_mut(&mut self) -> &mut dyn Write {
        &mut self.writer
    }
}

impl<W: Write> Visitor for MarkdownVisitor<'_, W> {
    type Error = Error;

    fn visit_document_start(&mut self, _doc: &Document) -> Result<(), Self::Error> {
        // No document-level preamble needed for Markdown
        // Title will be rendered as level-1 heading if present
        Ok(())
    }

    fn visit_document_end(&mut self, _doc: &Document) -> Result<(), Self::Error> {
        // Render collected footnotes (GFM only)
        if self.variant() == MarkdownVariant::GitHubFlavored && !self.footnotes.is_empty() {
            writeln!(self.writer)?;
            // Footnotes are already pre-rendered as markdown strings.
            let footnotes = std::mem::take(&mut self.footnotes);
            for (id, content) in footnotes {
                writeln!(self.writer, "[^{id}]: {content}")?;
            }
        }

        // Ensure final newline
        writeln!(self.writer)?;
        Ok(())
    }

    fn visit_header(&mut self, header: &Header) -> Result<(), Self::Error> {
        // Render document title as top-level heading
        if !header.title.is_empty() {
            write!(self.writer, "# ")?;
            self.visit_inline_nodes(header.title.as_ref())?;
            writeln!(self.writer)?;
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
            self.processor.warnings.emit(
                Warning::new(
                    WarningSource::new("markdown")
                        .with_variant(self.processor.variant().to_string()),
                    format!(
                        "section level {} exceeds Markdown maximum 6, capping at level 6",
                        section.level + 1
                    ),
                    None,
                )
                .with_advice("Markdown only has six heading levels. Reduce the source section depth if the distinction matters."),
            );
        }

        // Write heading
        let hashes = "#".repeat(level as usize);
        write!(self.writer, "{hashes} ")?;
        self.visit_inline_nodes(section.title.as_ref())?;
        writeln!(self.writer)?;
        writeln!(self.writer)?;

        // Visit section content
        let prev_level = self.heading_level;
        self.heading_level = level as usize;

        for block in &section.content {
            self.visit_block(block)?;
        }

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
                writeln!(v.writer)?;
                Ok(())
            });
        }

        self.visit_inline_nodes(&paragraph.content)?;
        writeln!(self.writer)?;
        writeln!(self.writer)?;
        Ok(())
    }

    fn visit_unordered_list(&mut self, list: &UnorderedList) -> Result<(), Self::Error> {
        self.visit_list_items(&list.items, "-")
    }

    fn visit_ordered_list(&mut self, list: &OrderedList) -> Result<(), Self::Error> {
        self.visit_list_items(&list.items, "1.")
    }

    fn visit_list_item(&mut self, _item: &ListItem) -> Result<(), Self::Error> {
        // This is handled by visit_list_items
        Ok(())
    }

    fn visit_thematic_break(&mut self, _br: &ThematicBreak) -> Result<(), Self::Error> {
        writeln!(self.writer, "---")?;
        writeln!(self.writer)?;
        Ok(())
    }

    fn visit_page_break(&mut self, _pb: &PageBreak) -> Result<(), Self::Error> {
        // Page breaks don't exist in Markdown; use thematic break as fallback
        self.write_warning("page breaks", "using horizontal rule")?;
        writeln!(self.writer, "---")?;
        writeln!(self.writer)?;
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
                writeln!(self.writer)?;
            }
            DelimitedBlockType::DelimitedLiteral(content) => {
                // Use fenced code block without syntax highlighting
                writeln!(self.writer, "```")?;
                self.write_code_block_content(content)?;
                writeln!(self.writer, "```")?;
                writeln!(self.writer)?;
            }
            DelimitedBlockType::DelimitedQuote(blocks) => {
                // Markdown blockquotes
                for block_item in blocks {
                    write!(self.writer, "> ")?;
                    // Visit each block in the quote
                    self.visit_block(block_item)?;
                }
                writeln!(self.writer)?;
            }
            DelimitedBlockType::DelimitedExample(blocks) => {
                if block.metadata.options.contains(&"collapsible") {
                    let is_open = block.metadata.options.contains(&"open");
                    self.write_collapsible(&block.title, is_open, |v| {
                        for block_item in blocks {
                            v.visit_block(block_item)?;
                        }
                        Ok(())
                    })?;
                } else {
                    // Examples don't have a direct Markdown equivalent
                    // Use blockquote as fallback
                    self.write_warning("example blocks", "using blockquote")?;
                    for block_item in blocks {
                        write!(self.writer, "> ")?;
                        self.visit_block(block_item)?;
                    }
                    writeln!(self.writer)?;
                }
            }
            DelimitedBlockType::DelimitedSidebar(blocks) => {
                // Sidebars don't have a direct Markdown equivalent
                self.write_warning("sidebar blocks", "using blockquote")?;
                for block_item in blocks {
                    write!(self.writer, "> ")?;
                    self.visit_block(block_item)?;
                }
                writeln!(self.writer)?;
            }
            DelimitedBlockType::DelimitedOpen(blocks) => {
                // Open blocks are just containers, render contents normally
                for block_item in blocks {
                    self.visit_block(block_item)?;
                }
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
                self.processor.warnings.emit(Warning::new(
                    WarningSource::new("markdown")
                        .with_variant(self.processor.variant().to_string()),
                    "unsupported delimited block type in Markdown, skipping content",
                    None,
                ));
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

        for block in &admonition.blocks {
            write!(self.writer, "> ")?;
            self.visit_block(block)?;
        }
        writeln!(self.writer)?;
        Ok(())
    }

    fn visit_discrete_header(&mut self, header: &DiscreteHeader) -> Result<(), Self::Error> {
        // Discrete headers are just headings without section structure
        let level = (header.level + 1).min(6);
        let hashes = "#".repeat(level as usize);
        write!(self.writer, "{hashes} ")?;
        self.visit_inline_nodes(header.title.as_ref())?;
        writeln!(self.writer)?;
        writeln!(self.writer)?;
        Ok(())
    }

    fn visit_image(&mut self, image: &Image) -> Result<(), Self::Error> {
        let alt = image
            .metadata
            .attributes
            .get_string("alt")
            .unwrap_or(std::borrow::Cow::Borrowed("image"));

        let target = image.source.to_string();

        // Markdown image syntax: ![alt](url "title")
        if let Some(title) = image.metadata.attributes.get_string("title") {
            writeln!(self.writer, r#"![{alt}]({target} "{title}")"#)?;
        } else {
            writeln!(self.writer, "![{alt}]({target})")?;
        }
        writeln!(self.writer)?;
        Ok(())
    }

    fn visit_video(&mut self, video: &Video) -> Result<(), Self::Error> {
        // Video embedding not supported in standard Markdown
        self.write_warning("video embedding", "providing link")?;
        if let Some(first_source) = video.sources.first() {
            let target = first_source.to_string();
            writeln!(self.writer, "[Video: {target}]({target})")?;
        }
        writeln!(self.writer)?;
        Ok(())
    }

    fn visit_audio(&mut self, audio: &Audio) -> Result<(), Self::Error> {
        // Audio embedding not supported in standard Markdown
        self.write_warning("audio embedding", "providing link")?;
        let target = audio.source.to_string();
        writeln!(self.writer, "[Audio: {target}]({target})")?;
        writeln!(self.writer)?;
        Ok(())
    }

    fn visit_description_list(&mut self, list: &DescriptionList) -> Result<(), Self::Error> {
        // Description lists (definition lists) not in standard Markdown
        self.write_warning("description lists", "using regular list")?;
        for item in &list.items {
            // Render term as bold text in a list item
            write!(self.writer, "- **")?;
            self.visit_inline_nodes(&item.term)?;
            writeln!(self.writer, "**")?;

            // Render principal text (inline content after delimiter) if present
            if !item.principal_text.is_empty() {
                write!(self.writer, "  ")?;
                self.visit_inline_nodes(&item.principal_text)?;
                writeln!(self.writer)?;
            }

            // Render description blocks indented
            for block in &item.description {
                write!(self.writer, "  ")?;
                self.visit_block(block)?;
            }
        }
        writeln!(self.writer)?;
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
                self.processor.warnings.emit(Warning::new(
                    WarningSource::new("markdown")
                        .with_variant(self.processor.variant().to_string()),
                    format!("unsupported inline node in Markdown, skipping node: {node:?}"),
                    None,
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

impl<W: Write> MarkdownVisitor<'_, W> {
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
                let target = image.source.to_string();
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
                                heading_level: self.heading_level,
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
            InlineMacro::CrossReference(_)
            | InlineMacro::Pass(_)
            | InlineMacro::Stem(_)
            | InlineMacro::IndexTerm(_)
            | _ => {
                self.processor.warnings.emit(Warning::new(
                    WarningSource::new("markdown")
                        .with_variant(self.processor.variant().to_string()),
                    format!("unsupported inline macro in Markdown, skipping macro: {mac:?}"),
                    None,
                ));
            }
        }
        Ok(())
    }

    /// Render list items with the given marker (for both ordered and unordered lists).
    fn visit_list_items(&mut self, items: &[ListItem], marker: &str) -> Result<(), Error> {
        for (i, item) in items.iter().enumerate() {
            // For ordered lists, use the actual number
            let item_marker = if marker.ends_with('.') {
                format!("{}.", i + 1)
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
                // Indent nested content
                write!(self.writer, "    ")?;
                self.visit_block(block)?;
            }
        }
        writeln!(self.writer)?;
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
        writeln!(self.writer)?;

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
