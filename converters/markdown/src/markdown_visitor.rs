//! Visitor implementation for Markdown conversion.

use std::io::Write;

use acdc_converters_core::visitor::{Visitor, WritableVisitor};
use acdc_parser::{
    Admonition, Audio, Block, CalloutList, DelimitedBlock, DelimitedBlockType, DescriptionList,
    DiscreteHeader, Document, Header, Image, InlineMacro, InlineNode, ListItem, OrderedList,
    PageBreak, Paragraph, Section, Table, TableOfContents, ThematicBreak, UnorderedList, Video,
};

use crate::{Error, MarkdownVariant, Processor};

/// Markdown visitor that generates Markdown output from `AsciiDoc` AST.
pub struct MarkdownVisitor<W: Write> {
    writer: W,
    pub(crate) processor: Processor,
    /// Current heading level (for nested sections).
    pub(crate) heading_level: usize,
    /// Collected footnotes for rendering at document end.
    pub(crate) footnotes: Vec<(String, Vec<InlineNode>)>,
}

impl<W: Write> MarkdownVisitor<W> {
    /// Create a new Markdown visitor.
    pub fn new(writer: W, processor: Processor) -> Self {
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
        tracing::warn!("Markdown does not support {feature}, using {fallback}");
        // Markdown comments are not standard, but HTML comments work in most renderers
        writeln!(
            self.writer,
            "<!-- Warning: {feature} not natively supported in Markdown, {fallback} -->"
        )?;
        Ok(())
    }
}

impl<W: Write> WritableVisitor for MarkdownVisitor<W> {
    fn writer_mut(&mut self) -> &mut dyn Write {
        &mut self.writer
    }
}

impl<W: Write> Visitor for MarkdownVisitor<W> {
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
            // Clone footnotes to avoid borrow checker issues
            let footnotes = self.footnotes.clone();
            for (id, content) in footnotes {
                write!(self.writer, "[^{id}]: ")?;
                for node in &content {
                    self.visit_inline_node(node)?;
                }
                writeln!(self.writer)?;
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
            tracing::warn!(
                "Section level {} exceeds Markdown maximum (6), capping at level 6",
                section.level + 1
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
                let language = block
                    .metadata
                    .attributes
                    .get_string("language")
                    .unwrap_or_default();

                writeln!(self.writer, "```{language}")?;
                // Content is Vec<InlineNode>, need to render it
                for node in content {
                    self.visit_inline_node(node)?;
                }
                writeln!(self.writer, "```")?;
                writeln!(self.writer)?;
            }
            DelimitedBlockType::DelimitedLiteral(content) => {
                // Use fenced code block without syntax highlighting
                writeln!(self.writer, "```")?;
                for node in content {
                    self.visit_inline_node(node)?;
                }
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
                // Examples don't have a direct Markdown equivalent
                // Use blockquote as fallback
                self.write_warning("example blocks", "using blockquote")?;
                for block_item in blocks {
                    write!(self.writer, "> ")?;
                    self.visit_block(block_item)?;
                }
                writeln!(self.writer)?;
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
                tracing::warn!("Unsupported delimited block type");
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
            .unwrap_or_else(|| "image".to_string());

        let target = Self::source_to_string(&image.source);

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
            let target = Self::source_to_string(first_source);
            writeln!(self.writer, "[Video: {target}]({target})")?;
        }
        writeln!(self.writer)?;
        Ok(())
    }

    fn visit_audio(&mut self, audio: &Audio) -> Result<(), Self::Error> {
        // Audio embedding not supported in standard Markdown
        self.write_warning("audio embedding", "providing link")?;
        let target = Self::source_to_string(&audio.source);
        writeln!(self.writer, "[Audio: {target}]({target})")?;
        writeln!(self.writer)?;
        Ok(())
    }

    fn visit_description_list(&mut self, _list: &DescriptionList) -> Result<(), Self::Error> {
        // Description lists (definition lists) not in standard Markdown
        self.write_warning("description lists", "using regular list")?;
        // TODO: Implement fallback rendering
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
                write!(self.writer, "{}", Self::escape_markdown(&text.content))?;
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
                tracing::warn!(?node, "Unsupported inline node type");
            }
        }
        Ok(())
    }

    fn visit_inline_nodes(&mut self, nodes: &[InlineNode]) -> Result<(), Self::Error> {
        for node in nodes {
            self.visit_inline_node(node)?;
        }
        Ok(())
    }

    fn visit_text(&mut self, text: &str) -> Result<(), Self::Error> {
        write!(self.writer, "{}", Self::escape_markdown(text))?;
        Ok(())
    }
}

impl<W: Write> MarkdownVisitor<W> {
    /// Convert a Source to a string for use in Markdown links/images.
    fn source_to_string(source: &acdc_parser::Source) -> String {
        match source {
            acdc_parser::Source::Path(path) => path.display().to_string(),
            acdc_parser::Source::Url(url) => url.to_string(),
            acdc_parser::Source::Name(name) => name.clone(),
        }
    }

    /// Handle inline macros.
    fn visit_inline_macro_inner(&mut self, mac: &InlineMacro) -> Result<(), Error> {
        match mac {
            InlineMacro::Link(link) => {
                let target = Self::source_to_string(&link.target);
                let text = link.text.as_deref().unwrap_or(&target);
                write!(self.writer, "[{text}]({target})")?;
            }
            InlineMacro::Image(image) => {
                // Inline image macro
                let target = Self::source_to_string(&image.source);
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
                    let id = footnote
                        .id
                        .clone()
                        .unwrap_or_else(|| footnote.number.to_string());

                    // Store footnote for later rendering (only if not already stored)
                    if !footnote.content.is_empty()
                        && !self
                            .footnotes
                            .iter()
                            .any(|(existing_id, _)| existing_id == &id)
                    {
                        self.footnotes.push((id.clone(), footnote.content.clone()));
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
                let target = Self::source_to_string(&url.target);
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
                let target = Self::source_to_string(&mailto.target);
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
                let target = Self::source_to_string(&autolink.url);
                write!(self.writer, "{target}")?;
            }
            InlineMacro::CrossReference(_)
            | InlineMacro::Pass(_)
            | InlineMacro::Stem(_)
            | InlineMacro::IndexTerm(_)
            | _ => {
                tracing::warn!("Unsupported inline macro type");
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
