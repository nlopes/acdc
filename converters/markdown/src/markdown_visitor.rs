//! Visitor implementation for Markdown conversion.

use std::{borrow::Cow, collections::HashSet, io::Write, rc::Rc};

use acdc_converters_core::{
    Converter, Diagnostics,
    code::detect_language,
    list::OrderedListNumbering,
    media::resolve_target,
    section::{
        appendix_number_prefix, effective_section_level, part_number_prefix, section_number_prefix,
    },
    toc::{Config as TocConfig, NumberingConfig, effective_level, has_real_parts, section_numbers},
    visitor::{Visitor, WritableVisitor},
    xref::{XrefDisplay, interdocument_xref, resolve_xref},
};
use acdc_parser::{
    Admonition, AttributeValue, Audio, Block, BlockMetadata, CalloutList, CrossReference,
    DelimitedBlock, DelimitedBlockType, DescriptionList, DiscreteHeader, Document, Header, Image,
    InlineMacro, InlineNode, ListItem, OrderedList, PageBreak, Paragraph, Section, SectionKind,
    Source, Table, TableOfContents, ThematicBreak, TocEntry, UnorderedList, Video,
};

use crate::{BACKEND_TRAITS, Error, MarkdownVariant, Processor};

struct TocRenderConfig<'a> {
    max_level: u8,
    section_numbers: &'a [Option<String>],
    has_real_parts: bool,
}

#[derive(Clone, Copy)]
struct TocRenderPosition {
    current_level: u8,
    base_index: usize,
    parts_at_current_level: bool,
    indent: usize,
}

/// Markdown visitor that generates Markdown output from `AsciiDoc` AST.
pub struct MarkdownVisitor<'a, 'd, W: Write> {
    writer: W,
    pub(crate) processor: Processor<'a>,
    /// Per-conversion diagnostics handle.
    pub(crate) diagnostics: Diagnostics<'d>,
    /// Current heading level (for nested sections).
    pub(crate) heading_level: usize,
    list_depth: usize,
    emitted_anchors: HashSet<String>,
    in_link_text: bool,
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
            emitted_anchors: HashSet::new(),
            in_link_text: false,
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

    fn section_prefix(&self, section: &Section<'_>) -> String {
        section.number().map_or_else(String::new, |number| {
            if section.kind == SectionKind::Appendix {
                let caption = match self.processor.document_attributes().get("appendix-caption") {
                    Some(AttributeValue::String(caption)) => Some(caption.as_ref()),
                    Some(_) | None => None,
                };
                appendix_number_prefix(number, caption)
            } else if section.level == 0 && section.kind == SectionKind::Normal {
                let signifier = match self.processor.document_attributes().get("part-signifier") {
                    Some(AttributeValue::String(signifier)) => Some(signifier.as_ref()),
                    Some(_) | None => None,
                };
                part_number_prefix(number, signifier)
            } else {
                section_number_prefix(number, None)
            }
        })
    }

    fn write_anchor(&mut self, id: &str) -> Result<bool, Error> {
        if !self.emitted_anchors.insert(id.to_owned()) {
            return Ok(false);
        }
        write!(self.writer, "<a id=\"")?;
        for character in id.chars() {
            match character {
                '&' => write!(self.writer, "&amp;")?,
                '"' => write!(self.writer, "&quot;")?,
                '<' => write!(self.writer, "&lt;")?,
                '>' => write!(self.writer, "&gt;")?,
                _ => write!(self.writer, "{character}")?,
            }
        }
        write!(self.writer, "\"></a>")?;
        Ok(true)
    }

    fn write_block_anchor(&mut self, id: &str) -> Result<(), Error> {
        if self.write_anchor(id)? {
            writeln!(self.writer)?;
        }
        Ok(())
    }

    fn write_inline_anchor(&mut self, id: Option<&str>) -> Result<(), Error> {
        if !self.in_link_text
            && let Some(id) = id
        {
            self.write_anchor(id)?;
        }
        Ok(())
    }

    fn write_metadata_anchor(&mut self, metadata: &BlockMetadata<'_>) -> Result<(), Error> {
        if let Some(anchor) = metadata.id.as_ref().or_else(|| metadata.anchors.first()) {
            self.write_block_anchor(anchor.id)?;
        }
        Ok(())
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

    fn toc_will_render(&self, toc_macro: Option<&TableOfContents<'_>>, placement: &str) -> bool {
        if self.processor.toc_entries.is_empty() {
            return false;
        }
        let config = TocConfig::from_attributes(toc_macro, self.processor.document_attributes());
        match placement {
            "auto" => matches!(
                config.placement(),
                "auto" | "left" | "right" | "top" | "bottom"
            ),
            other => config.placement() == other,
        }
    }

    fn render_toc_entries(
        &mut self,
        entries: &[TocEntry<'_>],
        config: &TocRenderConfig<'_>,
        position: TocRenderPosition,
    ) -> Result<(), Error> {
        if position.current_level > config.max_level {
            return Ok(());
        }

        let first_real_part = if position.parts_at_current_level {
            entries
                .iter()
                .position(|entry| entry.level == 0 && entry.kind == SectionKind::Normal)
        } else {
            None
        };
        let current_entries = entries
            .iter()
            .enumerate()
            .filter(|(index, entry)| {
                let level = effective_level(entry, config.has_real_parts);
                if level == position.current_level {
                    first_real_part
                        .is_none_or(|part| *index < part || entry.level != position.current_level)
                } else {
                    position.parts_at_current_level && entry.level == 0 && level == 0
                }
            })
            .collect::<Vec<_>>();

        for (entry_position, (entry_index, entry)) in current_entries.iter().enumerate() {
            write!(self.writer, "{:indent$}- ", "", indent = position.indent)?;
            self.write_anchor_link(entry.id, |visitor| {
                if let Some(Some(number)) = config
                    .section_numbers
                    .get(position.base_index + entry_index)
                {
                    write!(visitor.writer, "{number}")?;
                }
                visitor.visit_inline_nodes(&entry.title)
            })?;
            writeln!(self.writer)?;

            let child_start = entry_index + 1;
            let child_end = current_entries
                .get(entry_position + 1)
                .map_or(entries.len(), |next| next.0);
            let child_level = if entry.level == 0 && entry.kind.is_special() {
                2
            } else {
                effective_level(entry, config.has_real_parts) + 1
            };
            if let Some(children) = entries.get(child_start..child_end)
                && child_level <= config.max_level
                && children.iter().any(|child| child.level == child_level)
            {
                self.render_toc_entries(
                    children,
                    config,
                    TocRenderPosition {
                        current_level: child_level,
                        base_index: position.base_index + child_start,
                        parts_at_current_level: false,
                        indent: position.indent + 2,
                    },
                )?;
            }
        }
        Ok(())
    }

    fn render_toc(
        &mut self,
        toc_macro: Option<&TableOfContents<'_>>,
        placement: &str,
    ) -> Result<(), Error> {
        if !self.toc_will_render(toc_macro, placement) {
            return Ok(());
        }

        let processor = self.processor.clone();
        let config = TocConfig::from_attributes(toc_macro, processor.document_attributes());
        let title = config.title().or_else(|| {
            (!processor.document_attributes().contains_key("toc-title"))
                .then_some("Table of Contents")
        });
        if let Some(title) = title.filter(|title| !title.is_empty()) {
            writeln!(self.writer, "## {}", Self::escape_markdown(title))?;
            writeln!(self.writer)?;
        }

        let part_signifier = match processor.document_attributes().get("part-signifier") {
            Some(AttributeValue::String(signifier)) => Some(signifier.as_ref()),
            Some(_) | None => None,
        };
        let numbers = section_numbers(
            &processor.toc_entries,
            &NumberingConfig::new(processor.document_attributes(), part_signifier),
        );
        let real_parts = has_real_parts(&processor.toc_entries);
        let first_level = processor
            .toc_entries
            .first()
            .map_or(1, |entry| effective_level(entry, real_parts));
        let parts_at_current_level = first_level > 0 && real_parts;
        let start_level = if parts_at_current_level {
            1
        } else {
            first_level
        };
        self.render_toc_entries(
            &processor.toc_entries,
            &TocRenderConfig {
                max_level: config.levels(),
                section_numbers: &numbers,
                has_real_parts: real_parts,
            },
            TocRenderPosition {
                current_level: start_level,
                base_index: 0,
                parts_at_current_level,
                indent: 0,
            },
        )?;
        Ok(())
    }

    fn render_placed_toc(
        &mut self,
        toc_macro: Option<&TableOfContents<'_>>,
        placement: &str,
        has_output: bool,
    ) -> Result<bool, Error> {
        if !self.toc_will_render(toc_macro, placement) {
            return Ok(has_output);
        }
        if has_output {
            writeln!(self.writer)?;
        }
        self.render_toc(toc_macro, placement)?;
        Ok(true)
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
            Block::TableOfContents(toc) => {
                toc.metadata.id.is_some()
                    || !toc.metadata.anchors.is_empty()
                    || self.toc_will_render(Some(toc), "macro")
            }
            Block::Admonition(_)
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
        has_output = self.render_placed_toc(None, "auto", has_output)?;

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
            has_output = self.render_placed_toc(None, "preamble", has_output)?;
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
        self.write_block_anchor(&Section::generate_id_string(
            &section.metadata,
            &section.title,
        ))?;

        let effective_level = effective_section_level(section.level, section.kind);
        let level = effective_level + 1; // AsciiDoc levels are 0-indexed, Markdown uses 1-6
        let level = level.min(6); // Markdown only supports 6 heading levels

        if effective_level >= 6 {
            self.diagnostics.warn_with_advice(
                format!(
                    "section level {} exceeds Markdown maximum 6, capping at level 6",
                    effective_level + 1
                ),
                "Markdown only has six heading levels. Reduce the source section depth if the distinction matters.",
            );
        }

        if !section.metadata.options.contains(&"notitle") {
            let hashes = "#".repeat(level as usize);
            write!(self.writer, "{hashes} {}", self.section_prefix(section))?;
            self.visit_inline_nodes(section.title.as_ref())?;
            writeln!(self.writer)?;
        }

        // Visit section content
        let prev_level = self.heading_level;
        self.heading_level = level as usize;

        self.visit_separated_blocks(&section.content, true)?;

        self.heading_level = prev_level;
        Ok(())
    }

    fn visit_paragraph(&mut self, paragraph: &Paragraph) -> Result<(), Self::Error> {
        self.write_metadata_anchor(&paragraph.metadata)?;

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
        self.write_metadata_anchor(&list.metadata)?;
        self.visit_list_items(&list.items, "-", 1)
    }

    fn visit_ordered_list(&mut self, list: &OrderedList) -> Result<(), Self::Error> {
        self.write_metadata_anchor(&list.metadata)?;

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

    fn visit_thematic_break(&mut self, br: &ThematicBreak) -> Result<(), Self::Error> {
        if let Some(anchor) = br.anchors.first() {
            self.write_block_anchor(anchor.id)?;
        }
        writeln!(self.writer, "---")?;
        Ok(())
    }

    fn visit_page_break(&mut self, page_break: &PageBreak) -> Result<(), Self::Error> {
        self.write_metadata_anchor(&page_break.metadata)?;
        // Page breaks don't exist in Markdown; use thematic break as fallback
        self.write_warning("page breaks", "using horizontal rule")?;
        writeln!(self.writer, "---")?;
        Ok(())
    }

    fn visit_table_of_contents(&mut self, toc: &TableOfContents) -> Result<(), Self::Error> {
        self.write_metadata_anchor(&toc.metadata)?;
        self.render_toc(Some(toc), "macro")?;
        Ok(())
    }

    fn visit_delimited_block(&mut self, block: &DelimitedBlock) -> Result<(), Self::Error> {
        self.write_metadata_anchor(&block.metadata)?;

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
        self.write_metadata_anchor(&admonition.metadata)?;

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
        self.write_block_anchor(&Section::generate_id_string(
            &header.metadata,
            &header.title,
        ))?;

        // Discrete headers are just headings without section structure
        let level = (header.level + 1).min(6);
        let hashes = "#".repeat(level as usize);
        write!(self.writer, "{hashes} ")?;
        self.visit_inline_nodes(header.title.as_ref())?;
        writeln!(self.writer)?;
        Ok(())
    }

    fn visit_image(&mut self, image: &Image) -> Result<(), Self::Error> {
        self.write_metadata_anchor(&image.metadata)?;

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
        self.write_metadata_anchor(&video.metadata)?;
        // Video embedding not supported in standard Markdown
        self.write_warning("video embedding", "providing link")?;
        if let Some(first_source) = video.sources.first() {
            let target = self.media_target(first_source);
            writeln!(self.writer, "[Video: {target}]({target})")?;
        }
        Ok(())
    }

    fn visit_audio(&mut self, audio: &Audio) -> Result<(), Self::Error> {
        self.write_metadata_anchor(&audio.metadata)?;
        // Audio embedding not supported in standard Markdown
        self.write_warning("audio embedding", "providing link")?;
        let target = self.media_target(&audio.source);
        writeln!(self.writer, "[Audio: {target}]({target})")?;
        Ok(())
    }

    fn visit_description_list(&mut self, list: &DescriptionList) -> Result<(), Self::Error> {
        self.write_metadata_anchor(&list.metadata)?;
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

    fn visit_callout_list(&mut self, list: &CalloutList) -> Result<(), Self::Error> {
        self.write_metadata_anchor(&list.metadata)?;
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
                self.write_inline_anchor(text.id)?;
                write!(self.writer, "**")?;
                self.visit_inline_nodes(&text.content)?;
                write!(self.writer, "**")?;
            }
            InlineNode::ItalicText(text) => {
                self.write_inline_anchor(text.id)?;
                write!(self.writer, "*")?;
                self.visit_inline_nodes(&text.content)?;
                write!(self.writer, "*")?;
            }
            InlineNode::MonospaceText(text) => {
                self.write_inline_anchor(text.id)?;
                write!(self.writer, "`")?;
                self.visit_inline_nodes(&text.content)?;
                write!(self.writer, "`")?;
            }
            InlineNode::HighlightText(text) => {
                self.write_inline_anchor(text.id)?;
                // Highlighting not in standard Markdown
                // Just render as plain text
                self.visit_inline_nodes(&text.content)?;
            }
            InlineNode::SubscriptText(text) => {
                self.write_inline_anchor(text.id)?;
                // Subscript not in standard Markdown
                // Render with HTML tags (works in most renderers)
                write!(self.writer, "<sub>")?;
                self.visit_inline_nodes(&text.content)?;
                write!(self.writer, "</sub>")?;
            }
            InlineNode::SuperscriptText(text) => {
                self.write_inline_anchor(text.id)?;
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
                self.write_inline_anchor(text.id)?;
                // Render with proper quotes
                write!(self.writer, "\"")?;
                self.visit_inline_nodes(&text.content)?;
                write!(self.writer, "\"")?;
            }
            InlineNode::CurvedApostropheText(text) => {
                self.write_inline_anchor(text.id)?;
                write!(self.writer, "'")?;
                self.visit_inline_nodes(&text.content)?;
                write!(self.writer, "'")?;
            }
            InlineNode::InlineAnchor(anchor) => {
                self.write_inline_anchor(Some(anchor.id))?;
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

    fn write_macro_link(
        &mut self,
        destination: &str,
        default_label: &str,
        label: &[InlineNode<'_>],
    ) -> Result<(), Error> {
        self.write_link(destination, |visitor| {
            if label.is_empty() {
                write!(visitor.writer, "{default_label}")?;
            } else {
                for node in label {
                    visitor.visit_inline_node(node)?;
                }
            }
            Ok(())
        })
    }

    /// Handle inline macros.
    fn visit_inline_macro_inner(&mut self, mac: &InlineMacro) -> Result<(), Error> {
        match mac {
            InlineMacro::Link(link) => {
                let target = link.target.to_string();
                self.write_macro_link(&target, &target, &link.text)?;
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
                if self.in_link_text {
                    write!(self.writer, "[{}]", footnote.number)?;
                } else if self.variant() == MarkdownVariant::GitHubFlavored {
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
                            let mut tmp = MarkdownVisitor::new(
                                &mut buffer,
                                self.processor.clone(),
                                self.diagnostics.reborrow(),
                            );
                            tmp.heading_level = self.heading_level;
                            tmp.list_depth = self.list_depth;
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
                self.write_macro_link(&target, &target, &url.text)?;
            }
            InlineMacro::Mailto(mailto) => {
                // Email link - text is Vec<InlineNode>
                let target = mailto.target.to_string();
                self.write_macro_link(&format!("mailto:{target}"), &target, &mailto.text)?;
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

    /// Render a cross-reference as a local anchor or interdocument link.
    ///
    /// Local references link to the `#id` fragment. Interdocument references
    /// link to the corresponding Markdown output. The text is the reference's
    /// own text when it has one, otherwise the target's reference text.
    fn visit_cross_reference(&mut self, xref: &CrossReference<'_>) -> Result<(), Error> {
        let target = xref.target;
        if !xref.text.is_empty() {
            if let Some((destination, _)) = self.interdocument_xref(target) {
                return self.write_link(&destination, |visitor| {
                    for node in &xref.text {
                        visitor.visit_inline_node(node)?;
                    }
                    Ok(())
                });
            }
            return self.write_anchor_link(target, |visitor| {
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
        match resolve_xref(references.get(target), xref, &guard) {
            XrefDisplay::Title(inlines, _scope) | XrefDisplay::Label(inlines, _scope) => self
                .write_anchor_link(target, |visitor| {
                    for node in inlines {
                        visitor.visit_inline_node(node)?;
                    }
                    Ok(())
                }),
            XrefDisplay::ShortCaption(prefix) => self.write_anchor_link(target, |visitor| {
                write!(visitor.writer, "{prefix}")?;
                Ok(())
            }),
            XrefDisplay::FullCaption(prefix, inlines, _scope) => {
                self.write_anchor_link(target, |visitor| {
                    write!(visitor.writer, "{prefix}, “")?;
                    for node in inlines {
                        visitor.visit_inline_node(node)?;
                    }
                    write!(visitor.writer, "”")?;
                    Ok(())
                })
            }
            XrefDisplay::Fallback(text) | XrefDisplay::Unresolved(text) => {
                self.write_anchor_link(target, |visitor| {
                    write!(visitor.writer, "{text}")?;
                    Ok(())
                })
            }
            XrefDisplay::External(target) => {
                if let Some((destination, text)) = self.interdocument_xref(&target) {
                    self.write_link(&destination, |visitor| {
                        write!(visitor.writer, "{text}")?;
                        Ok(())
                    })
                } else {
                    write!(self.writer, "{target}")?;
                    Ok(())
                }
            }
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
        self.write_link(&format!("#{target}"), text)
    }

    fn write_link(
        &mut self,
        destination: &str,
        text: impl FnOnce(&mut Self) -> Result<(), Error>,
    ) -> Result<(), Error> {
        if self.in_link_text {
            return text(self);
        }
        write!(self.writer, "[")?;
        let previous = std::mem::replace(&mut self.in_link_text, true);
        let result = text(self);
        self.in_link_text = previous;
        result?;
        write!(self.writer, "]({destination})")?;
        Ok(())
    }

    fn interdocument_xref(&self, target: &str) -> Option<(String, String)> {
        let attributes = self.processor.document_attributes();
        let extension = attributes
            .get_string("relfilesuffix")
            .or_else(|| attributes.get_string("outfilesuffix"))
            .unwrap_or_else(|| Cow::Borrowed(BACKEND_TRAITS.outfilesuffix()));
        interdocument_xref(target, extension.strip_prefix('.').unwrap_or(&extension))
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
