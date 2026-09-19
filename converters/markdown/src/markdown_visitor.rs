//! Visitor implementation for Markdown conversion.

use std::{borrow::Cow, collections::HashSet, fmt::Write as _, io::Write, path::Path, rc::Rc};

use acdc_converters_core::{
    Converter, Diagnostics, Warning,
    code::{SourceLineOptions, default_line_comment, detect_language},
    icon,
    inline_text::InlineTextTransform,
    link::{autolink_fallback, link_fallback, mailto_fallback},
    list::OrderedListNumbering,
    media::resolve_target,
    section::{
        appendix_number_prefix, book_chapter_signifier, effective_section_level,
        part_number_prefix, section_number_prefix,
    },
    shows_block_title,
    substitutions::{Replacements, TextBoundaries, strip_backslash_escapes},
    table::{CellKind, GridRow, build_grid, determine_column_count, table_has_spans},
    toc::{Config as TocConfig, NumberingConfig, effective_level, has_real_parts, section_numbers},
    visitor::{Visitor, WritableVisitor},
    xref::{XrefDisplay, interdocument_xref, resolve_xref},
};
use acdc_parser::{
    Admonition, Anchor, AttributeValue, Audio, Author, Block, BlockMetadata, CalloutList,
    CaptionKind, ColumnStyle, ColumnWidth, CrossReference, DelimitedBlock, DelimitedBlockType,
    DescriptionList, DiscreteHeader, Document, Footnote, Header, HorizontalAlignment, Image,
    IndexTerm, IndexTermRelationship, InlineMacro, InlineNode, Link, ListItem,
    ListItemCheckedStatus, Location, OrderedList, PageBreak, Paragraph, Section, SectionKind,
    Source, SourceLocation, Substitution, Table, TableColumn, TableOfContents, ThematicBreak,
    Title, TocEntry, UnorderedList, VerticalAlignment, Video,
};

use crate::{
    Error, IndexCatalogRelationship, IndexTermEntry, IndexTermLabel, MARKDOWN_BACKEND,
    MarkdownVariant, Processor,
};

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

#[derive(Clone, Copy)]
enum RoleClose {
    MarkdownStrike,
    Html(&'static str),
}

fn escape_html_text(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            _ => output.push(character),
        }
    }
    output
}

fn escape_html_attribute(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&#39;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            _ => output.push(character),
        }
    }
    output
}

fn escape_link_destination(destination: &str) -> String {
    let mut output = String::with_capacity(destination.len());
    for character in destination.chars() {
        match character {
            '\\' => output.push_str("%5C"),
            '(' | ')' => {
                output.push('\\');
                output.push(character);
            }
            '<' => output.push_str("%3C"),
            '>' => output.push_str("%3E"),
            character if character.is_ascii_whitespace() || character.is_ascii_control() => {
                let _ = write!(output, "%{:02X}", u32::from(character));
            }
            _ => output.push(character),
        }
    }
    output
}

fn escape_link_title(title: &str) -> String {
    title.replace('\\', "\\\\").replace('"', "\\\"")
}

fn passthrough_content(text: &str, substitutions: &[Substitution]) -> String {
    let mut output = text.to_owned();
    for substitution in substitutions {
        match substitution {
            Substitution::SpecialChars => output = escape_html_text(&output),
            Substitution::Replacements => {
                output = Replacements::unicode().transform(&output, TextBoundaries::BOTH);
            }
            Substitution::Attributes
            | Substitution::Macros
            | Substitution::PostReplacements
            | Substitution::Normal
            | Substitution::Verbatim
            | Substitution::Quotes
            | Substitution::Callouts
            | _ => {}
        }
    }
    output
}

fn max_backtick_run(text: &str) -> usize {
    text.split(|character| character != '`')
        .map(str::len)
        .max()
        .unwrap_or(0)
}

fn suppresses_list_marker(style: Option<&str>, ordered: bool) -> bool {
    matches!(style, Some("none" | "no-bullet" | "unstyled"))
        || (ordered && style == Some("unnumbered"))
}

fn list_sequence(metadata: &BlockMetadata<'_>, item_count: usize) -> (usize, bool) {
    let reversed = metadata.options.contains(&"reversed");
    let start = metadata
        .attributes
        .get_string("start")
        .and_then(|start| start.parse::<usize>().ok())
        .filter(|start| *start > 0)
        .unwrap_or(if reversed { item_count } else { 1 });
    (start, reversed)
}

fn table_cells<'table, 'source>(
    table: &'table Table<'source>,
) -> impl Iterator<Item = &'table TableColumn<'source>> {
    table
        .header
        .iter()
        .chain(table.rows.iter())
        .chain(table.footer.iter())
        .flat_map(|row| row.columns.iter())
}

fn escape_unescaped_table_pipes(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut preceding_backslashes = 0;
    for character in text.chars() {
        if character == '|' {
            if preceding_backslashes % 2 == 0 {
                output.push('\\');
            }
            output.push(character);
            preceding_backslashes = 0;
        } else {
            output.push(character);
            if character == '\\' {
                preceding_backslashes += 1;
            } else {
                preceding_backslashes = 0;
            }
        }
    }
    output
}

fn flatten_table_cell(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .map(escape_unescaped_table_pipes)
        .collect::<Vec<_>>()
        .join("<br>")
}

fn image_filename_alt(source: &Source<'_>) -> String {
    source
        .get_filename()
        .and_then(|filename| Path::new(filename).file_stem())
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or_default()
        .replace(['-', '_'], " ")
}

fn block_image_alt(image: &Image<'_>) -> String {
    image
        .metadata
        .attributes
        .get_string("alt")
        .map_or_else(|| image_filename_alt(&image.source), Cow::into_owned)
}

fn inline_image_alt(image: &Image<'_>) -> String {
    if image.title.is_empty() {
        block_image_alt(image)
    } else {
        InlineTextTransform::default().to_string(image.title.as_ref())
    }
}

fn is_linked_image(node: &InlineNode<'_>) -> bool {
    matches!(
        node,
        InlineNode::Macro(InlineMacro::Image(image))
            if image.metadata.attributes.get_string("link").is_some()
    )
}

fn block_location<'block>(block: &'block Block<'_>) -> Option<&'block Location> {
    match block {
        Block::TableOfContents(block) => Some(&block.location),
        Block::Admonition(block) => Some(&block.location),
        Block::DiscreteHeader(block) => Some(&block.location),
        Block::DocumentAttribute(block) => Some(&block.location),
        Block::ThematicBreak(block) => Some(&block.location),
        Block::PageBreak(block) => Some(&block.location),
        Block::UnorderedList(block) => Some(&block.location),
        Block::OrderedList(block) => Some(&block.location),
        Block::CalloutList(block) => Some(&block.location),
        Block::DescriptionList(block) => Some(&block.location),
        Block::Section(block) => Some(&block.location),
        Block::DelimitedBlock(block) => Some(&block.location),
        Block::Paragraph(block) => Some(&block.location),
        Block::Image(block) => Some(&block.location),
        Block::Audio(block) => Some(&block.location),
        Block::Video(block) => Some(&block.location),
        Block::Comment(block) => Some(&block.location),
        _ => None,
    }
}

fn raw_content<'nodes, 'source>(
    nodes: &'nodes [InlineNode<'source>],
) -> (String, Option<&'nodes InlineNode<'source>>) {
    let mut output = String::new();
    let mut unknown = None;
    for node in nodes {
        match node {
            InlineNode::VerbatimText(text) => output.push_str(text.content),
            InlineNode::RawText(text) => output.push_str(text.content),
            InlineNode::PlainText(text) => output.push_str(text.content),
            InlineNode::LineBreak(_) => output.push('\n'),
            InlineNode::CalloutRef(callout) => {
                let _ = write!(output, "({})", callout.number);
            }
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
            | InlineNode::Macro(_) => {
                let _ = InlineTextTransform::default()
                    .line_break("\n")
                    .write(&mut output, std::slice::from_ref(node));
            }
            _ => {
                unknown.get_or_insert(node);
            }
        }
    }
    (output, unknown)
}

fn source_content<'nodes, 'source>(
    nodes: &'nodes [InlineNode<'source>],
    language: Option<&str>,
) -> (String, Option<&'nodes InlineNode<'source>>) {
    let mut output = String::new();
    let mut unknown = None;
    let comment_prefix = default_line_comment(language);
    for (index, node) in nodes.iter().enumerate() {
        match node {
            InlineNode::VerbatimText(text) => {
                let mut content = text.content.to_owned();
                if index.checked_sub(1).is_some_and(|previous| {
                    matches!(nodes.get(previous), Some(InlineNode::CalloutRef(_)))
                }) {
                    let stripped = content.strip_prefix("-->").unwrap_or(&content).to_owned();
                    content = stripped;
                }
                if index
                    .checked_add(1)
                    .is_some_and(|next| matches!(nodes.get(next), Some(InlineNode::CalloutRef(_))))
                {
                    let stripped = if index
                        .checked_add(1)
                        .is_some_and(|next| is_xml_callout(nodes, next))
                    {
                        content.strip_suffix("<!--").unwrap_or(&content).to_owned()
                    } else {
                        strip_callout_guard(&content, comment_prefix).into_owned()
                    };
                    content = stripped;
                }
                output.push_str(&content);
            }
            InlineNode::RawText(text) => output.push_str(text.content),
            InlineNode::PlainText(text) => output.push_str(text.content),
            InlineNode::LineBreak(_) => output.push('\n'),
            InlineNode::CalloutRef(callout) => {
                let _ = write!(output, "({})", callout.number);
            }
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
            | InlineNode::Macro(_) => {
                let _ = InlineTextTransform::default()
                    .line_break("\n")
                    .write(&mut output, std::slice::from_ref(node));
            }
            _ => {
                unknown.get_or_insert(node);
            }
        }
    }
    (output, unknown)
}

fn is_xml_callout(nodes: &[InlineNode<'_>], index: usize) -> bool {
    matches!(nodes.get(index), Some(InlineNode::CalloutRef(_)))
        && index.checked_sub(1).is_some_and(|previous| {
            matches!(
                nodes.get(previous),
                Some(InlineNode::VerbatimText(text)) if text.content.ends_with("<!--")
            )
        })
        && index.checked_add(1).is_some_and(|next| {
            matches!(
                nodes.get(next),
                Some(InlineNode::VerbatimText(text)) if text.content.starts_with("-->")
            )
        })
}

fn strip_callout_guard<'a>(text: &'a str, language_prefix: Option<&str>) -> Cow<'a, str> {
    let trimmed = text.trim_end();
    for prefix in [
        language_prefix,
        Some("//"),
        Some("#"),
        Some("--"),
        Some(";;"),
    ]
    .into_iter()
    .flatten()
    {
        if let Some(content) = trimmed.strip_suffix(prefix) {
            return Cow::Owned(format!("{} ", content.trim_end()));
        }
    }
    Cow::Borrowed(text)
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
    collect_index_terms: bool,
    current_section_title: Option<String>,
    /// Footnotes collected for document-end output as `(id, number, rendered content)`.
    pub(crate) footnotes: Vec<(String, u32, String)>,
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
            collect_index_terms: true,
            current_section_title: None,
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
                let signifier = (section.level == 1 && section.kind == SectionKind::Normal)
                    .then(|| book_chapter_signifier(self.processor.document_attributes(), None))
                    .flatten();
                section_number_prefix(number, signifier)
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

    fn write_inline_anchor_node(&mut self, anchor: &Anchor<'_>) -> Result<(), Error> {
        self.write_inline_anchor(Some(anchor.id))?;
        if !anchor.is_bibliography() {
            return Ok(());
        }

        let label = self
            .processor
            .references
            .get(anchor.id)
            .and_then(|reference| reference.xreflabel.as_deref())
            .map(<[_]>::to_vec);
        if let Some(label) = label {
            self.visit_inline_nodes(&label)?;
        } else {
            write!(self.writer, "\\[{}\\]", Self::escape_markdown(anchor.id))?;
        }
        Ok(())
    }

    fn write_metadata_anchor(&mut self, metadata: &BlockMetadata<'_>) -> Result<(), Error> {
        if let Some(anchor) = metadata.id.as_ref().or_else(|| metadata.anchors.first()) {
            self.write_block_anchor(anchor.id)?;
        }
        Ok(())
    }

    fn write_block_title_line(
        &mut self,
        title: &Title<'_>,
        metadata: &BlockMetadata<'_>,
        fallback: Option<CaptionKind>,
    ) -> Result<(), Error> {
        if title.is_empty() {
            return Ok(());
        }
        let caption = self.processor.caption_prefix(metadata, fallback);
        write!(self.writer, "<strong>")?;
        if let Some(caption) = caption {
            write!(self.writer, "{}", Self::escape_markdown(&caption))?;
        }
        self.visit_inline_nodes(title.as_ref())?;
        writeln!(self.writer, "</strong>")?;
        Ok(())
    }

    fn write_block_title(
        &mut self,
        title: &Title<'_>,
        metadata: &BlockMetadata<'_>,
        fallback: Option<CaptionKind>,
    ) -> Result<(), Error> {
        if title.is_empty() {
            return Ok(());
        }
        self.write_block_title_line(title, metadata, fallback)?;
        writeln!(self.writer)?;
        Ok(())
    }

    fn write_attribution(
        &mut self,
        metadata: &BlockMetadata<'_>,
        line_prefix: &str,
    ) -> Result<(), Error> {
        let attribution = metadata
            .attribution
            .as_ref()
            .filter(|value| !value.is_empty());
        let citation = metadata
            .citetitle
            .as_ref()
            .filter(|value| !value.is_empty());
        if attribution.is_none() && citation.is_none() {
            return Ok(());
        }

        write!(self.writer, "{line_prefix}")?;
        if let Some(attribution) = attribution {
            write!(self.writer, "— ")?;
            self.visit_inline_nodes(attribution)?;
        }
        if let Some(citation) = citation {
            if attribution.is_some() {
                write!(self.writer, ", ")?;
            }
            write!(self.writer, "<cite>")?;
            self.visit_inline_nodes(citation)?;
            write!(self.writer, "</cite>")?;
        }
        writeln!(self.writer)?;
        Ok(())
    }

    fn write_blockquote_inlines(&mut self, content: &[InlineNode<'_>]) -> Result<(), Error> {
        write!(self.writer, "> ")?;
        for node in content {
            match node {
                InlineNode::PlainText(text) => {
                    self.write_blockquote_text(text.content, true)?;
                }
                InlineNode::RawText(text) => {
                    self.write_blockquote_text(text.content, false)?;
                }
                InlineNode::VerbatimText(text) => {
                    self.write_blockquote_text(text.content, false)?;
                }
                InlineNode::LineBreak(_) => self.write_blockquote_break()?,
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
                | _ => self.visit_inline_node(node)?,
            }
        }
        writeln!(self.writer)?;
        Ok(())
    }

    fn write_blockquote_text(&mut self, text: &str, escape: bool) -> Result<(), Error> {
        let mut lines = text.split('\n').peekable();
        while let Some(line) = lines.next() {
            if escape {
                write!(
                    self.writer,
                    "{}",
                    Self::escape_markdown(&strip_backslash_escapes(line))
                )?;
            } else {
                write!(self.writer, "{line}")?;
            }
            if lines.peek().is_some() {
                self.write_blockquote_break()?;
            }
        }
        Ok(())
    }

    fn write_blockquote_break(&mut self) -> Result<(), Error> {
        writeln!(self.writer, "\\")?;
        write!(self.writer, "> ")?;
        Ok(())
    }

    fn write_fenced_code_block(
        &mut self,
        metadata: &BlockMetadata<'_>,
        content: &[InlineNode<'_>],
    ) -> Result<(), Error> {
        let language = detect_language(metadata);
        let (content, unknown) = source_content(content, language);
        if let Some(node) = unknown {
            self.warn_unknown_inline_node(node);
        }
        self.warn_source_options(metadata, &content, language);

        self.write_raw_fenced_code_block(language, &content)
    }

    fn write_raw_fenced_code_block(
        &mut self,
        language: Option<&str>,
        content: &str,
    ) -> Result<(), Error> {
        let fence = "`".repeat(max_backtick_run(content).saturating_add(1).max(3));
        writeln!(self.writer, "{fence}{}", language.unwrap_or_default())?;
        write!(self.writer, "{content}")?;
        if !content.ends_with('\n') {
            writeln!(self.writer)?;
        }
        writeln!(self.writer, "{fence}")?;
        Ok(())
    }

    fn warn_source_options(
        &mut self,
        metadata: &BlockMetadata<'_>,
        content: &str,
        language: Option<&str>,
    ) {
        let options = SourceLineOptions::resolve(metadata, content);
        if options.line_number_start.is_some() {
            self.warn_once(
                "source-line-numbers",
                "source line numbering is not supported by the Markdown backend; preserving the source without line numbers",
                "Use a Markdown renderer extension for line numbers, or use a backend that supports this source presentation option.",
            );
        }
        if !options.highlighted_lines.is_empty() {
            self.warn_once(
                "source-highlighted-lines",
                "selected source-line highlighting is not supported by the Markdown backend; preserving the source without highlighted lines",
                "Use a Markdown renderer extension for selected-line highlighting, or use a backend that supports this source presentation option.",
            );
        }
        if metadata.style == Some("source")
            && language == Some("php")
            && metadata.options.contains(&"mixed")
        {
            self.warn_once(
                "php-mixed-highlighting",
                "PHP source block mixed-mode highlighting is not supported by the Markdown backend; rendering a normal PHP code fence",
                "Use the `html+php` source language when it gives acceptable highlighting, or use Asciidoctor HTML for explicit `%mixed` highlighting.",
            );
        }
    }

    fn warn_once(&mut self, key: &'static str, message: &'static str, advice: &'static str) {
        if self.processor.mark_fallback(key) {
            self.diagnostics.warn_with_advice(message, advice);
        }
    }

    fn warn_once_at(
        &mut self,
        key: &'static str,
        message: impl Into<Cow<'static, str>>,
        advice: &'static str,
        location: Option<&Location>,
    ) {
        if self.processor.mark_fallback(key) {
            let mut warning =
                Warning::new(self.diagnostics.source().clone(), message).with_advice(advice);
            if let Some(location) = location {
                warning = warning.at(SourceLocation::at_location(None, location.clone()));
            }
            self.diagnostics.emit(warning);
        }
    }

    fn warn_unknown_inline_node(&mut self, node: &InlineNode<'_>) {
        self.warn_once_at(
            "unknown-inline-node",
            format!(
                "unknown inline feature is not supported by the Markdown backend; skipping content: {node:?}"
            ),
            "Use a backend that supports this inline feature, or replace it with portable inline content.",
            Some(node.location()),
        );
    }

    fn with_index_collection<T>(
        &mut self,
        enabled: bool,
        render: impl FnOnce(&mut Self) -> Result<T, Error>,
    ) -> Result<T, Error> {
        let previous = std::mem::replace(&mut self.collect_index_terms, enabled);
        let result = render(self);
        self.collect_index_terms = previous;
        result
    }

    fn write_code_span(&mut self, content: &str) -> Result<(), Error> {
        if content.contains('\n')
            || content.starts_with(char::is_whitespace)
            || content.ends_with(char::is_whitespace)
        {
            write!(self.writer, "<code>{}</code>", escape_html_text(content))?;
            return Ok(());
        }

        let fence = "`".repeat(max_backtick_run(content).saturating_add(1));
        let padded = content.starts_with('`') || content.ends_with('`');
        write!(self.writer, "{fence}")?;
        if padded {
            write!(self.writer, " ")?;
        }
        write!(self.writer, "{content}")?;
        if padded {
            write!(self.writer, " ")?;
        }
        write!(self.writer, "{fence}")?;
        Ok(())
    }

    fn write_escaped_text(&mut self, text: &str) -> Result<(), Error> {
        write!(
            self.writer,
            "{}",
            Self::escape_markdown(&strip_backslash_escapes(text))
        )?;
        Ok(())
    }

    fn write_passthrough(
        &mut self,
        text: &str,
        substitutions: &[Substitution],
    ) -> Result<(), Error> {
        write!(self.writer, "{}", passthrough_content(text, substitutions))?;
        Ok(())
    }

    fn write_role_start(&mut self, role: Option<&str>) -> Result<Option<RoleClose>, Error> {
        let Some(role) = role.filter(|role| !role.trim().is_empty()) else {
            return Ok(None);
        };
        let only_line_through = role.split_whitespace().eq(["line-through"]);
        if only_line_through && self.variant() == MarkdownVariant::GitHubFlavored {
            write!(self.writer, "~~")?;
            return Ok(Some(RoleClose::MarkdownStrike));
        }

        let contains = |name| role.split_whitespace().any(|candidate| candidate == name);
        let tag = if contains("line-through") {
            "s"
        } else if contains("underline") {
            "u"
        } else if contains("highlight") {
            "mark"
        } else if contains("small") {
            "small"
        } else {
            "span"
        };
        write!(
            self.writer,
            "<{tag} class=\"{}\"",
            escape_html_attribute(role)
        )?;
        let style = if contains("pre-wrap") {
            Some("white-space: pre-wrap")
        } else if contains("big") {
            Some("font-size: larger")
        } else if contains("subtitle") {
            Some("font-size: smaller")
        } else {
            None
        };
        if let Some(style) = style {
            write!(self.writer, " style=\"{style}\"")?;
        }
        write!(self.writer, ">")?;
        Ok(Some(RoleClose::Html(tag)))
    }

    fn write_role_end(&mut self, close: Option<RoleClose>) -> Result<(), Error> {
        match close {
            Some(RoleClose::MarkdownStrike) => write!(self.writer, "~~")?,
            Some(RoleClose::Html(tag)) => write!(self.writer, "</{tag}>")?,
            None => {}
        }
        Ok(())
    }

    fn record_footnote(&mut self, footnote: &Footnote<'_>) -> Result<String, Error> {
        let id = footnote
            .id
            .map_or_else(|| footnote.number.to_string(), str::to_owned);
        if footnote.content.is_empty()
            || self
                .footnotes
                .iter()
                .any(|(existing_id, _, _)| existing_id == &id)
        {
            return Ok(id);
        }

        let mut buffer = Vec::new();
        {
            let mut visitor = MarkdownVisitor::new(
                &mut buffer,
                self.processor.clone(),
                self.diagnostics.reborrow(),
            );
            visitor.heading_level = self.heading_level;
            visitor.list_depth = self.list_depth;
            visitor.collect_index_terms = self.collect_index_terms;
            visitor
                .current_section_title
                .clone_from(&self.current_section_title);
            for node in &footnote.content {
                visitor.visit_inline_node(node)?;
            }
        }
        let rendered = String::from_utf8(buffer).unwrap_or_default();
        self.footnotes.push((id.clone(), footnote.number, rendered));
        Ok(id)
    }

    fn render_index_term_label(
        &mut self,
        inlines: &[InlineNode<'_>],
    ) -> Result<IndexTermLabel, Error> {
        let plain = InlineTextTransform::default()
            .references(&self.processor.references)
            .to_string(inlines);
        let mut output = Vec::new();
        {
            let mut visitor = MarkdownVisitor::new(
                &mut output,
                self.processor.clone(),
                self.diagnostics.reborrow(),
            );
            visitor.collect_index_terms = false;
            visitor.in_link_text = true;
            visitor
                .current_section_title
                .clone_from(&self.current_section_title);
            visitor.visit_inline_nodes(inlines)?;
        }
        Ok(IndexTermLabel {
            plain,
            rendered: String::from_utf8(output)?,
        })
    }

    fn visit_index_term(&mut self, term: &IndexTerm<'_>) -> Result<(), Error> {
        if self.collect_index_terms && self.processor.generate_index() {
            let primary = self.render_index_term_label(term.term())?;
            let secondary = term
                .secondary()
                .map(|inlines| self.render_index_term_label(inlines))
                .transpose()?;
            let tertiary = term
                .tertiary()
                .map(|inlines| self.render_index_term_label(inlines))
                .transpose()?;
            let relationship = match term.relationship.as_ref() {
                Some(IndexTermRelationship::See { target }) => {
                    IndexCatalogRelationship::See(self.render_index_term_label(target)?)
                }
                Some(IndexTermRelationship::SeeAlso { targets }) => {
                    IndexCatalogRelationship::SeeAlso(
                        targets
                            .iter()
                            .map(|target| self.render_index_term_label(target))
                            .collect::<Result<_, _>>()?,
                    )
                }
                None | Some(_) => IndexCatalogRelationship::None,
            };
            let anchor_id = self.processor.add_index_entry(IndexTermEntry {
                primary,
                secondary,
                tertiary,
                relationship,
                anchor_id: String::new(),
                section_title: self.current_section_title.clone(),
            });
            self.write_anchor(&anchor_id)?;
        }

        if term.is_visible() {
            self.visit_inline_nodes(term.term())?;
        }
        Ok(())
    }

    fn write_raw_block_content(&mut self, content: &[InlineNode<'_>]) -> Result<(), Error> {
        let (content, unknown) = raw_content(content);
        if let Some(node) = unknown {
            self.warn_unknown_inline_node(node);
        }
        write!(self.writer, "{content}")?;
        if !content.ends_with('\n') {
            writeln!(self.writer)?;
        }
        Ok(())
    }

    fn write_author(&mut self, author: &Author<'_>) -> Result<(), Error> {
        write!(self.writer, "{} ", Self::escape_markdown(author.first_name))?;
        if let Some(middle_name) = author.middle_name {
            write!(self.writer, "{} ", Self::escape_markdown(middle_name))?;
        }
        write!(self.writer, "{}", Self::escape_markdown(author.last_name))?;
        if let Some(email) = author.email {
            write!(self.writer, " <{}>", Self::escape_markdown(email))?;
        }
        Ok(())
    }

    fn write_revision(&mut self) -> Result<(), Error> {
        let attributes = self.processor.document_attributes();
        let number = attributes.get_string("revnumber").map(Cow::into_owned);
        let date = attributes.get_string("revdate").map(Cow::into_owned);
        let remark = attributes.get_string("revremark").map(Cow::into_owned);
        let label = attributes
            .get_string("version-label")
            .filter(|label| !label.is_empty())
            .map(Cow::into_owned);
        if number.is_none() && date.is_none() && remark.is_none() {
            return Ok(());
        }

        let has_number = number.is_some();
        let has_date = date.is_some();
        if let Some(number) = number {
            if let Some(label) = label {
                write!(self.writer, "{} ", Self::escape_markdown(&label))?;
            }
            write!(self.writer, "{}", Self::escape_markdown(&number))?;
            if has_date {
                write!(self.writer, ", ")?;
            }
        }
        if let Some(date) = date {
            write!(self.writer, "{}", Self::escape_markdown(&date))?;
        }
        if has_number || has_date {
            writeln!(self.writer)?;
        }
        if let Some(remark) = remark {
            writeln!(self.writer, "{}", Self::escape_markdown(&remark))?;
        }
        Ok(())
    }

    /// Write a fallback marker and record its structured warning once per document.
    fn write_warning(
        &mut self,
        key: &'static str,
        feature: &str,
        fallback: &str,
    ) -> Result<(), Error> {
        if self.processor.mark_fallback(key) {
            self.diagnostics.warn_with_advice(
                format!("{feature} not natively supported in Markdown, {fallback}"),
                "Check whether the selected Markdown variant can represent this construct, or use a backend that preserves it.",
            );
        }
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
        let chapter_signifier = book_chapter_signifier(processor.document_attributes(), None);
        let numbering_config = NumberingConfig::new(
            processor.document_attributes(),
            part_signifier,
            chapter_signifier,
        );
        let numbers = section_numbers(&processor.toc_entries, &numbering_config);
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
        self.with_index_collection(false, |visitor| visitor.render_toc(toc_macro, placement))?;
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
                    self.variant() == MarkdownVariant::CommonMark
                        || table.header.is_some()
                        || !table.rows.is_empty()
                        || table.footer.is_some()
                }
                DelimitedBlockType::DelimitedComment(_) => false,
                DelimitedBlockType::DelimitedExample(_)
                | DelimitedBlockType::DelimitedListing(_)
                | DelimitedBlockType::DelimitedLiteral(_)
                | DelimitedBlockType::DelimitedSidebar(_)
                | DelimitedBlockType::DelimitedPass(_)
                | DelimitedBlockType::DelimitedVerse(_)
                | DelimitedBlockType::DelimitedStem(_)
                | _ => true,
            },
            Block::TableOfContents(toc) => {
                toc.metadata.id.is_some()
                    || !toc.metadata.anchors.is_empty()
                    || self.toc_will_render(Some(toc), "macro")
            }
            Block::Comment(_) | Block::DocumentAttribute(_) => false,
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
            | Block::Video(_)
            | _ => true,
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

    fn visit_unhandled_block(&mut self, block: &Block<'_>) -> Result<(), Self::Error> {
        self.warn_once_at(
            "unknown-parser-block",
            format!(
                "unknown parser block feature is not supported by the Markdown backend; skipping content: {block:?}"
            ),
            "Use a backend that supports this block feature, or replace it with a portable AsciiDoc construct.",
            block_location(block),
        );
        Ok(())
    }

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
        if !self.footnotes.is_empty() {
            if has_output {
                writeln!(self.writer)?;
            }
            let footnotes = std::mem::take(&mut self.footnotes);
            if self.variant() == MarkdownVariant::GitHubFlavored {
                for (id, _, content) in footnotes {
                    writeln!(self.writer, "[^{id}]: {content}")?;
                }
            } else {
                writeln!(self.writer, "<strong>Footnotes</strong>")?;
                writeln!(self.writer)?;
                for (id, number, content) in footnotes {
                    write!(self.writer, "{number}. ")?;
                    self.write_anchor(&format!("_footnote_{id}"))?;
                    writeln!(self.writer, "{content}")?;
                }
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
        if let Some(anchor) = header
            .metadata
            .id
            .as_ref()
            .or_else(|| header.metadata.anchors.last())
        {
            self.write_block_anchor(anchor.id)?;
        }
        if !header.title.is_empty() {
            write!(self.writer, "# ")?;
            self.visit_inline_nodes(header.title.as_ref())?;
            if let Some(subtitle) = &header.subtitle {
                write!(self.writer, ": ")?;
                self.visit_inline_nodes(subtitle)?;
            }
            writeln!(self.writer)?;
        }

        let has_revision = ["revnumber", "revdate", "revremark"].iter().any(|name| {
            self.processor
                .document_attributes()
                .get_string(name)
                .is_some()
        });
        if !header.authors.is_empty() || has_revision {
            writeln!(self.writer)?;
        }
        if !header.authors.is_empty() {
            write!(self.writer, "By ")?;
            for (index, author) in header.authors.iter().enumerate() {
                if index > 0 {
                    write!(self.writer, "; ")?;
                }
                self.write_author(author)?;
            }
            writeln!(self.writer)?;
        }
        self.write_revision()?;
        Ok(())
    }

    fn visit_section(&mut self, section: &Section) -> Result<(), Self::Error> {
        let section_title = InlineTextTransform::default()
            .references(&self.processor.references)
            .to_string(section.title.as_ref());
        let previous_section_title = self.current_section_title.replace(section_title);

        let result = (|| {
            self.write_block_anchor(&Section::generate_id_string(
                &section.metadata,
                &section.title,
            ))?;

            let effective_level = effective_section_level(section.level, section.kind);
            let level = effective_level + 1; // AsciiDoc levels are 0-indexed, Markdown uses 1-6
            let level = level.min(6); // Markdown only supports 6 heading levels

            if effective_level >= 6 {
                self.warn_once(
                "section-heading-depth",
                "section levels deeper than Markdown maximum 6 are capped at level 6",
                "Markdown only has six heading levels. Reduce the source section depth if the distinction matters.",
            );
            }

            if !section.metadata.options.contains(&"notitle") {
                let hashes = "#".repeat(level as usize);
                write!(self.writer, "{hashes} {}", self.section_prefix(section))?;
                self.visit_inline_nodes(section.title.as_ref())?;
                writeln!(self.writer)?;
            }

            // Render the section body or its generated index catalog.
            let prev_level = self.heading_level;
            self.heading_level = level as usize;

            if section.kind == SectionKind::Index && self.processor.generate_index() {
                let processor = self.processor.clone();
                crate::index::render(self, &processor, self.heading_level + 1)?;
            } else {
                self.visit_separated_blocks(&section.content, true)?;
            }

            self.heading_level = prev_level;
            Ok(())
        })();
        self.current_section_title = previous_section_title;
        result
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

        self.write_block_title(
            &paragraph.title,
            &paragraph.metadata,
            CaptionKind::for_style(paragraph.metadata.style),
        )?;
        match paragraph.metadata.style {
            Some("quote" | "verse") => {
                self.write_blockquote_inlines(&paragraph.content)?;
                self.write_attribution(&paragraph.metadata, "> ")?;
            }
            Some("abstract") => self.write_blockquote_inlines(&paragraph.content)?,
            Some("literal" | "listing" | "source") => {
                self.write_fenced_code_block(&paragraph.metadata, &paragraph.content)?;
            }
            Some("example") => {
                self.write_warning(
                    "example-blockquotes",
                    "example paragraphs",
                    "using blockquote",
                )?;
                self.write_blockquote_inlines(&paragraph.content)?;
            }
            _ => {
                self.visit_inline_nodes(&paragraph.content)?;
                writeln!(self.writer)?;
            }
        }
        Ok(())
    }

    fn visit_unordered_list(&mut self, list: &UnorderedList) -> Result<(), Self::Error> {
        self.write_metadata_anchor(&list.metadata)?;
        self.write_block_title(&list.title, &list.metadata, None)?;
        self.visit_list_items(
            &list.items,
            "-",
            1,
            false,
            suppresses_list_marker(list.metadata.style, false),
        )
    }

    fn visit_ordered_list(&mut self, list: &OrderedList) -> Result<(), Self::Error> {
        self.write_metadata_anchor(&list.metadata)?;
        self.write_block_title(&list.title, &list.metadata, None)?;

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
                "ordered-list-numbering",
                "non-numeric ordered list numbering styles",
                "rendering numerically",
            )?;
        }
        let (start, reversed) = list_sequence(&list.metadata, list.items.len());
        self.visit_list_items(
            &list.items,
            "1.",
            start,
            reversed,
            suppresses_list_marker(list.metadata.style, true),
        )
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
        self.write_warning("page-breaks", "page breaks", "using horizontal rule")?;
        writeln!(self.writer, "---")?;
        Ok(())
    }

    fn visit_table_of_contents(&mut self, toc: &TableOfContents) -> Result<(), Self::Error> {
        self.write_metadata_anchor(&toc.metadata)?;
        self.with_index_collection(false, |visitor| visitor.render_toc(Some(toc), "macro"))?;
        Ok(())
    }

    fn visit_delimited_block(&mut self, block: &DelimitedBlock) -> Result<(), Self::Error> {
        self.write_metadata_anchor(&block.metadata)?;

        let collapsible = matches!(block.inner, DelimitedBlockType::DelimitedExample(_))
            && block.metadata.options.contains(&"collapsible");
        if !collapsible
            && !matches!(block.inner, DelimitedBlockType::DelimitedComment(_))
            && shows_block_title(&block.inner)
        {
            self.write_block_title(
                &block.title,
                &block.metadata,
                CaptionKind::for_delimited(&block.inner, block.metadata.style),
            )?;
        }

        match &block.inner {
            DelimitedBlockType::DelimitedListing(content)
            | DelimitedBlockType::DelimitedLiteral(content) => {
                self.write_fenced_code_block(&block.metadata, content)?;
            }
            DelimitedBlockType::DelimitedQuote(blocks) => {
                self.visit_blockquote_blocks(blocks)?;
                self.write_attribution(&block.metadata, "> ")?;
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
                    self.write_warning(
                        "example-blockquotes",
                        "example blocks",
                        "using blockquote",
                    )?;
                    self.visit_blockquote_blocks(blocks)?;
                }
            }
            DelimitedBlockType::DelimitedSidebar(blocks) => {
                // Sidebars don't have a direct Markdown equivalent
                self.write_warning("sidebar-blocks", "sidebar blocks", "using blockquote")?;
                self.visit_blockquote_blocks(blocks)?;
            }
            DelimitedBlockType::DelimitedOpen(blocks) => {
                // Open blocks are just containers, render contents normally
                self.visit_separated_blocks(blocks, false)?;
            }
            DelimitedBlockType::DelimitedPass(content) => {
                self.write_raw_block_content(content)?;
            }
            DelimitedBlockType::DelimitedTable(table) => {
                self.visit_table_inner(table, &block.metadata)?;
            }
            DelimitedBlockType::DelimitedVerse(content) => {
                self.write_blockquote_inlines(content)?;
                self.write_attribution(&block.metadata, "> ")?;
            }
            DelimitedBlockType::DelimitedComment(_) => {
                // Comments don't get rendered
            }
            DelimitedBlockType::DelimitedStem(stem) => {
                self.warn_once(
                    "stem-blocks",
                    "block STEM is not supported by the Markdown backend; preserving the expression as fenced code",
                    "Use a Markdown renderer with a math extension, or use a backend that supports STEM rendering.",
                );
                let notation = stem.notation.to_string();
                self.write_raw_fenced_code_block(Some(&notation), stem.content)?;
            }
            _ => {
                self.warn_once_at(
                    "unknown-delimited-block",
                    format!(
                        "unknown delimited block feature is not supported by the Markdown backend; skipping content: {:?}",
                        block.inner
                    ),
                    "Use a backend that supports this block type, or replace it with a portable AsciiDoc construct.",
                    Some(&block.location),
                );
            }
        }
        Ok(())
    }

    fn visit_admonition(&mut self, admonition: &Admonition) -> Result<(), Self::Error> {
        self.write_metadata_anchor(&admonition.metadata)?;
        self.write_block_title(&admonition.title, &admonition.metadata, None)?;

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
                "commonmark-admonitions",
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
        let alt = block_image_alt(image);
        self.write_image(image, &alt)?;
        writeln!(self.writer)?;
        if !image.title.is_empty() {
            writeln!(self.writer)?;
            self.write_block_title_line(&image.title, &image.metadata, Some(CaptionKind::Figure))?;
        }
        Ok(())
    }

    fn visit_video(&mut self, video: &Video) -> Result<(), Self::Error> {
        self.write_metadata_anchor(&video.metadata)?;
        self.write_block_title(&video.title, &video.metadata, None)?;
        self.warn_static_media_fallback();

        let title = if video.title.is_empty() {
            video
                .sources
                .first()
                .and_then(Source::get_filename)
                .map_or_else(|| "video".to_owned(), str::to_owned)
        } else {
            InlineTextTransform::default().to_string(video.title.as_ref())
        };
        if let Some(poster) = video.metadata.attributes.get_string("poster") {
            let poster = resolve_target(&poster, self.processor.document_attributes());
            write!(
                self.writer,
                "![{}]({})",
                Self::escape_markdown(&format!("Video poster: {title}")),
                escape_link_destination(&poster)
            )?;
            writeln!(self.writer)?;
            if !video.sources.is_empty() {
                writeln!(self.writer)?;
            }
        }
        for (index, source) in video.sources.iter().enumerate() {
            let target = self.media_target(source);
            let source_count = video.sources.len();
            self.write_link(&target, |visitor| {
                if source_count == 1 {
                    write!(visitor.writer, "Video: {}", Self::escape_markdown(&title))?;
                } else {
                    write!(
                        visitor.writer,
                        "Video source {}/{}: {}",
                        index + 1,
                        source_count,
                        Self::escape_markdown(&title)
                    )?;
                }
                Ok(())
            })?;
            writeln!(self.writer)?;
        }
        Ok(())
    }

    fn visit_audio(&mut self, audio: &Audio) -> Result<(), Self::Error> {
        self.write_metadata_anchor(&audio.metadata)?;
        self.write_block_title(&audio.title, &audio.metadata, None)?;
        self.warn_static_media_fallback();
        let target = self.media_target(&audio.source);
        let title = if audio.title.is_empty() {
            audio
                .source
                .get_filename()
                .map_or_else(|| audio.source.to_string(), str::to_owned)
        } else {
            InlineTextTransform::default().to_string(audio.title.as_ref())
        };
        self.write_link(&target, |visitor| {
            write!(visitor.writer, "Audio: {}", Self::escape_markdown(&title))?;
            Ok(())
        })?;
        writeln!(self.writer)?;
        Ok(())
    }

    fn visit_description_list(&mut self, list: &DescriptionList) -> Result<(), Self::Error> {
        self.write_metadata_anchor(&list.metadata)?;
        self.write_block_title(&list.title, &list.metadata, None)?;
        match list.metadata.style {
            Some("horizontal") => self.visit_horizontal_description_list(list),
            Some("qanda") => self.visit_qanda_description_list(list),
            _ => self.visit_regular_description_list(list),
        }
    }

    fn visit_callout_list(&mut self, list: &CalloutList) -> Result<(), Self::Error> {
        self.write_metadata_anchor(&list.metadata)?;
        self.write_block_title(&list.title, &list.metadata, None)?;
        for item in &list.items {
            self.write_list_indent()?;
            write!(self.writer, "- **({})** ", item.callout.number)?;
            self.visit_inline_nodes(&item.principal)?;
            writeln!(self.writer)?;

            if !item.blocks.is_empty() {
                writeln!(self.writer)?;
                self.list_depth += 1;
                let result: Result<(), Error> = (|| {
                    for (index, block) in item.blocks.iter().enumerate() {
                        if index > 0 {
                            writeln!(self.writer)?;
                        }
                        self.write_list_indent()?;
                        self.visit_block(block)?;
                    }
                    Ok(())
                })();
                self.list_depth -= 1;
                result?;
            }
        }
        Ok(())
    }

    fn visit_inline_node(&mut self, node: &InlineNode) -> Result<(), Self::Error> {
        match node {
            InlineNode::PlainText(text) => {
                self.write_escaped_text(text.content)?;
            }
            InlineNode::BoldText(text) => {
                self.write_inline_anchor(text.id)?;
                let role = self.write_role_start(text.role)?;
                write!(self.writer, "**")?;
                self.visit_inline_nodes(&text.content)?;
                write!(self.writer, "**")?;
                self.write_role_end(role)?;
            }
            InlineNode::ItalicText(text) => {
                self.write_inline_anchor(text.id)?;
                let role = self.write_role_start(text.role)?;
                write!(self.writer, "*")?;
                self.visit_inline_nodes(&text.content)?;
                write!(self.writer, "*")?;
                self.write_role_end(role)?;
            }
            InlineNode::MonospaceText(text) => {
                self.write_inline_anchor(text.id)?;
                let role = self.write_role_start(text.role)?;
                let content = InlineTextTransform::default()
                    .line_break("\n")
                    .to_string(&text.content);
                self.write_code_span(&content)?;
                self.write_role_end(role)?;
            }
            InlineNode::HighlightText(text) => {
                self.write_inline_anchor(text.id)?;
                let role = self.write_role_start(text.role)?;
                if role.is_none() {
                    write!(self.writer, "<mark>")?;
                }
                self.visit_inline_nodes(&text.content)?;
                if role.is_none() {
                    write!(self.writer, "</mark>")?;
                }
                self.write_role_end(role)?;
            }
            InlineNode::SubscriptText(text) => {
                self.write_inline_anchor(text.id)?;
                let role = self.write_role_start(text.role)?;
                write!(self.writer, "<sub>")?;
                self.visit_inline_nodes(&text.content)?;
                write!(self.writer, "</sub>")?;
                self.write_role_end(role)?;
            }
            InlineNode::SuperscriptText(text) => {
                self.write_inline_anchor(text.id)?;
                let role = self.write_role_start(text.role)?;
                write!(self.writer, "<sup>")?;
                self.visit_inline_nodes(&text.content)?;
                write!(self.writer, "</sup>")?;
                self.write_role_end(role)?;
            }
            InlineNode::LineBreak(_) => writeln!(self.writer, "  ")?,
            InlineNode::RawText(text) => {
                self.write_passthrough(text.content, &text.subs)?;
            }
            InlineNode::VerbatimText(text) => self.write_code_span(text.content)?,
            InlineNode::StandaloneCurvedApostrophe(_) => {
                write!(self.writer, "'")?;
            }
            InlineNode::CurvedQuotationText(text) => {
                self.write_inline_anchor(text.id)?;
                let role = self.write_role_start(text.role)?;
                write!(self.writer, "\"")?;
                self.visit_inline_nodes(&text.content)?;
                write!(self.writer, "\"")?;
                self.write_role_end(role)?;
            }
            InlineNode::CurvedApostropheText(text) => {
                self.write_inline_anchor(text.id)?;
                let role = self.write_role_start(text.role)?;
                write!(self.writer, "'")?;
                self.visit_inline_nodes(&text.content)?;
                write!(self.writer, "'")?;
                self.write_role_end(role)?;
            }
            InlineNode::InlineAnchor(anchor) => {
                self.write_inline_anchor_node(anchor)?;
            }
            InlineNode::Macro(mac) => {
                self.visit_inline_macro_inner(mac)?;
            }
            InlineNode::CalloutRef(callout) => {
                write!(self.writer, "({})", callout.number)?;
            }
            _ => self.warn_unknown_inline_node(node),
        }
        Ok(())
    }

    fn visit_text(&mut self, text: &str) -> Result<(), Self::Error> {
        self.write_escaped_text(text)
    }
}

impl<W: Write> MarkdownVisitor<'_, '_, W> {
    fn visit_regular_description_list(&mut self, list: &DescriptionList<'_>) -> Result<(), Error> {
        self.write_list_indent()?;
        self.write_warning(
            "description-list-fallback",
            "description lists",
            "using regular list",
        )?;
        for item in &list.items {
            self.write_list_indent()?;
            write!(self.writer, "- **")?;
            self.visit_inline_nodes(&item.term)?;
            writeln!(self.writer, "**")?;

            if !item.principal_text.is_empty() {
                self.write_list_indent()?;
                write!(self.writer, "  ")?;
                self.visit_inline_nodes(&item.principal_text)?;
                writeln!(self.writer)?;
            }
            self.visit_description_blocks(&item.description)?;
        }
        Ok(())
    }

    fn visit_horizontal_description_list(
        &mut self,
        list: &DescriptionList<'_>,
    ) -> Result<(), Error> {
        for item in &list.items {
            self.write_list_indent()?;
            write!(self.writer, "**")?;
            self.visit_inline_nodes(&item.term)?;
            write!(self.writer, "**")?;
            if !item.principal_text.is_empty() {
                write!(self.writer, " — ")?;
                self.visit_inline_nodes(&item.principal_text)?;
            }
            writeln!(self.writer, "  ")?;

            for block in &item.description {
                if !matches!(
                    block,
                    Block::DescriptionList(_) | Block::OrderedList(_) | Block::UnorderedList(_)
                ) {
                    self.write_list_indent()?;
                }
                self.visit_block(block)?;
            }
        }
        Ok(())
    }

    fn visit_qanda_description_list(&mut self, list: &DescriptionList<'_>) -> Result<(), Error> {
        let (start, reversed) = list_sequence(&list.metadata, list.items.len());
        for (index, item) in list.items.iter().enumerate() {
            let number = if reversed {
                start.saturating_sub(index)
            } else {
                start.saturating_add(index)
            };
            self.write_list_indent()?;
            write!(self.writer, "{number}. <em>")?;
            self.visit_inline_nodes(&item.term)?;
            writeln!(self.writer, "</em>")?;

            if !item.principal_text.is_empty() {
                writeln!(self.writer)?;
                self.write_list_indent()?;
                write!(self.writer, "    ")?;
                self.visit_inline_nodes(&item.principal_text)?;
                writeln!(self.writer)?;
            }
            if !item.description.is_empty() {
                if item.principal_text.is_empty() {
                    writeln!(self.writer)?;
                }
                self.visit_description_blocks(&item.description)?;
            }
            if index + 1 < list.items.len() {
                writeln!(self.writer)?;
            }
        }
        Ok(())
    }

    fn visit_description_blocks(&mut self, blocks: &[Block<'_>]) -> Result<(), Error> {
        for block in blocks {
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
                write!(visitor.writer, "{}", Self::escape_markdown(default_label))?;
            } else {
                for node in label {
                    visitor.visit_inline_node(node)?;
                }
            }
            Ok(())
        })
    }

    fn write_link_macro_node(&mut self, link: &Link<'_>) -> Result<(), Error> {
        let target = link.target.to_string();
        if link.text.iter().any(is_linked_image) {
            for node in &link.text {
                if is_linked_image(node) {
                    self.visit_inline_node(node)?;
                } else {
                    self.write_link(&target, |visitor| visitor.visit_inline_node(node))?;
                }
            }
            return Ok(());
        }

        let fallback = link_fallback(&target, link.hides_uri_scheme());
        self.write_macro_link(&target, fallback, &link.text)
    }

    fn write_keyboard(&mut self, keys: &[&str]) -> Result<(), Error> {
        for (index, key) in keys.iter().enumerate() {
            if index > 0 {
                write!(self.writer, "+")?;
            }
            write!(self.writer, "<kbd>{}</kbd>", escape_html_text(key))?;
        }
        Ok(())
    }

    fn write_menu(&mut self, target: &str, items: &[&str]) -> Result<(), Error> {
        write!(self.writer, "{}", Self::escape_markdown(target))?;
        for item in items {
            write!(self.writer, " > {}", Self::escape_markdown(item))?;
        }
        Ok(())
    }

    fn visit_inline_macro_inner(&mut self, mac: &InlineMacro) -> Result<(), Error> {
        match mac {
            InlineMacro::Link(link) => self.write_link_macro_node(link)?,
            InlineMacro::Image(image) => {
                let alt = inline_image_alt(image);
                self.write_image(image, &alt)?;
            }
            InlineMacro::Icon(icon_macro) => {
                let alt = icon::alt(&icon_macro.target, &icon_macro.attributes);
                write!(self.writer, "[{}]", Self::escape_markdown(&alt))?;
            }
            InlineMacro::Keyboard(keyboard) => {
                self.write_keyboard(&keyboard.keys)?;
            }
            InlineMacro::Button(button) => {
                write!(self.writer, "**[{}]**", Self::escape_markdown(button.label))?;
            }
            InlineMacro::Menu(menu) => {
                self.write_menu(menu.target, &menu.items)?;
            }
            InlineMacro::Footnote(footnote) => {
                let id = self.record_footnote(footnote)?;
                if self.in_link_text {
                    write!(self.writer, "[{}]", footnote.number)?;
                } else if self.variant() == MarkdownVariant::GitHubFlavored {
                    write!(self.writer, "[^{id}]")?;
                } else {
                    write!(self.writer, "<sup>")?;
                    self.write_anchor_link(&format!("_footnote_{id}"), |visitor| {
                        write!(visitor.writer, "[{}]", footnote.number)?;
                        Ok(())
                    })?;
                    write!(self.writer, "</sup>")?;
                }
            }
            InlineMacro::Url(url) => {
                let target = url.target.to_string();
                let fallback = link_fallback(&target, url.hides_uri_scheme());
                self.write_macro_link(&target, fallback, &url.text)?;
            }
            InlineMacro::Mailto(mailto) => {
                let target = mailto.target.to_string();
                let destination = if target.starts_with("mailto:") {
                    target.clone()
                } else {
                    format!("mailto:{target}")
                };
                self.write_macro_link(&destination, mailto_fallback(&target), &mailto.text)?;
            }
            InlineMacro::Autolink(autolink) => {
                let target = autolink.url.to_string();
                let (fallback, angle_brackets) =
                    autolink_fallback(&target, autolink.bracketed, autolink.hides_uri_scheme());
                self.write_link(&target, |visitor| {
                    if angle_brackets {
                        write!(
                            visitor.writer,
                            "&lt;{}&gt;",
                            Self::escape_markdown(fallback)
                        )?;
                    } else {
                        write!(visitor.writer, "{}", Self::escape_markdown(fallback))?;
                    }
                    Ok(())
                })?;
            }
            InlineMacro::CrossReference(xref) => self.visit_cross_reference(xref)?,
            InlineMacro::IndexTerm(term) => self.visit_index_term(term)?,
            InlineMacro::Pass(pass) => {
                if let Some(text) = pass.text {
                    self.write_passthrough(text, &pass.substitutions)?;
                }
            }
            InlineMacro::Stem(stem) => {
                self.warn_once(
                    "inline-stem",
                    "inline STEM is not supported by the Markdown backend; preserving the expression as code",
                    "Use a Markdown renderer with a math extension, or use a backend that supports STEM rendering.",
                );
                self.write_code_span(stem.content)?;
            }
            _ => {
                self.warn_once_at(
                    "unknown-inline-macro",
                    format!(
                        "unknown inline macro feature is not supported by the Markdown backend; skipping content: {mac:?}"
                    ),
                    "Use a backend that supports this inline macro, or replace it with portable inline content.",
                    Some(mac.location()),
                );
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
        self.with_index_collection(false, |visitor| {
            match resolve_xref(references.get(target), xref, &guard) {
                XrefDisplay::Title(inlines, _scope) | XrefDisplay::Label(inlines, _scope) => {
                    visitor.write_anchor_link(target, |visitor| {
                        for node in inlines {
                            visitor.visit_inline_node(node)?;
                        }
                        Ok(())
                    })
                }
                XrefDisplay::ShortCaption(prefix) => visitor.write_anchor_link(target, |visitor| {
                    write!(visitor.writer, "{prefix}")?;
                    Ok(())
                }),
                XrefDisplay::FullCaption(prefix, inlines, _scope) => {
                    visitor.write_anchor_link(target, |visitor| {
                        write!(visitor.writer, "{prefix}, “")?;
                        for node in inlines {
                            visitor.visit_inline_node(node)?;
                        }
                        write!(visitor.writer, "”")?;
                        Ok(())
                    })
                }
                XrefDisplay::Fallback(text) | XrefDisplay::Unresolved(text) => visitor
                    .write_anchor_link(target, |visitor| {
                        write!(visitor.writer, "{text}")?;
                        Ok(())
                    }),
                XrefDisplay::External(target) => {
                    if let Some((destination, text)) = visitor.interdocument_xref(&target) {
                        visitor.write_link(&destination, |visitor| {
                            write!(visitor.writer, "{text}")?;
                            Ok(())
                        })
                    } else {
                        write!(visitor.writer, "{target}")?;
                        Ok(())
                    }
                }
                // Markdown links do not nest, so a reference inside another one's
                // text is text alone.
                XrefDisplay::Nested(text) => {
                    write!(visitor.writer, "{text}")?;
                    Ok(())
                }
            }
        })
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
        write!(self.writer, "]({})", escape_link_destination(destination))?;
        Ok(())
    }

    fn write_image(&mut self, image: &Image<'_>, alt: &str) -> Result<(), Error> {
        if let Some(link) = image.metadata.attributes.get_string("link") {
            self.write_link(&link, |visitor| visitor.write_image_markup(image, alt))
        } else {
            self.write_image_markup(image, alt)
        }
    }

    fn write_image_markup(&mut self, image: &Image<'_>, alt: &str) -> Result<(), Error> {
        let target = self.media_target(&image.source);
        let title = image.metadata.attributes.get_string("title");
        let width = image.metadata.attributes.get_string("width");
        let height = image.metadata.attributes.get_string("height");
        if width.is_some() || height.is_some() {
            write!(
                self.writer,
                "<img src=\"{}\" alt=\"{}\"",
                escape_html_attribute(&target),
                escape_html_attribute(alt)
            )?;
            if let Some(title) = title {
                write!(self.writer, " title=\"{}\"", escape_html_attribute(&title))?;
            }
            if let Some(width) = width {
                write!(self.writer, " width=\"{}\"", escape_html_attribute(&width))?;
            }
            if let Some(height) = height {
                write!(
                    self.writer,
                    " height=\"{}\"",
                    escape_html_attribute(&height)
                )?;
            }
            write!(self.writer, ">")?;
            return Ok(());
        }

        write!(
            self.writer,
            "![{}]({}",
            Self::escape_markdown(alt),
            escape_link_destination(&target)
        )?;
        if let Some(title) = title {
            write!(self.writer, " \"{}\"", escape_link_title(&title))?;
        }
        write!(self.writer, ")")?;
        Ok(())
    }

    fn warn_static_media_fallback(&mut self) {
        self.warn_once(
            "static-media-playback",
            "audio and video playback are not supported by the Markdown backend; rendering static links",
            "Use an HTML-capable backend when embedded playback controls are required.",
        );
    }

    fn interdocument_xref(&self, target: &str) -> Option<(String, String)> {
        let attributes = self.processor.document_attributes();
        let extension = attributes
            .get_string("relfilesuffix")
            .or_else(|| attributes.get_string("outfilesuffix"))
            .unwrap_or_else(|| Cow::Borrowed(MARKDOWN_BACKEND.outfilesuffix()));
        interdocument_xref(target, extension.strip_prefix('.').unwrap_or(&extension))
    }

    fn write_list_indent(&mut self) -> Result<(), Error> {
        for _ in 0..self.list_depth {
            write!(self.writer, "    ")?;
        }
        Ok(())
    }

    fn visit_list_items(
        &mut self,
        items: &[ListItem],
        marker: &str,
        start: usize,
        reversed: bool,
        markerless: bool,
    ) -> Result<(), Error> {
        for (index, item) in items.iter().enumerate() {
            self.write_list_indent()?;
            let number = if reversed {
                start.saturating_sub(index)
            } else {
                start.saturating_add(index)
            };
            let item_marker = if marker.ends_with('.') {
                format!("{number}.")
            } else {
                marker.to_owned()
            };

            let is_task = item.checked.is_some();
            let is_checked = matches!(item.checked, Some(ListItemCheckedStatus::Checked));

            if !markerless {
                write!(self.writer, "{item_marker} ")?;
            }
            if is_task && markerless {
                write!(self.writer, "{} ", if is_checked { '☑' } else { '☐' })?;
            } else if is_task && self.variant() == MarkdownVariant::GitHubFlavored {
                let checkbox = if is_checked { "[x]" } else { "[ ]" };
                write!(self.writer, "{checkbox} ")?;
            } else if is_task {
                let checkbox = if is_checked { "x" } else { " " };
                write!(self.writer, "\\[{checkbox}\\] ")?;
            }

            self.visit_inline_nodes(&item.principal)?;
            if markerless {
                writeln!(self.writer, "  ")?;
            } else {
                writeln!(self.writer)?;
            }

            for block in &item.blocks {
                if matches!(block, Block::OrderedList(_) | Block::UnorderedList(_)) {
                    if markerless {
                        writeln!(self.writer)?;
                    } else {
                        self.list_depth += 1;
                    }
                    let result = self.visit_block(block);
                    if markerless {
                        writeln!(self.writer)?;
                    } else {
                        self.list_depth -= 1;
                    }
                    result?;
                } else {
                    if markerless {
                        self.write_list_indent()?;
                    } else {
                        write!(self.writer, "    ")?;
                    }
                    self.visit_block(block)?;
                }
            }
        }
        Ok(())
    }

    fn visit_table_inner(
        &mut self,
        table: &Table,
        metadata: &BlockMetadata<'_>,
    ) -> Result<(), Error> {
        if self.variant() == MarkdownVariant::CommonMark {
            self.write_warning(
                "commonmark-tables",
                "tables",
                "not supported in CommonMark, skipping",
            )?;
            return Ok(());
        }

        self.warn_gfm_table_fallbacks(table, metadata);
        self.render_gfm_table(table)
    }

    fn warn_gfm_table_fallbacks(&mut self, table: &Table<'_>, metadata: &BlockMetadata<'_>) {
        if table.header.is_none() && (!table.rows.is_empty() || table.footer.is_some()) {
            self.warn_once(
                "gfm-headerless-tables",
                "headerless tables are not supported by GFM; adding an empty header row and preserving every source row as data",
                "Use an explicit header row when the destination requires strict GFM table semantics.",
            );
        }
        if table.footer.is_some() {
            self.warn_once(
                "gfm-table-footers",
                "table footers are not supported by GFM; rendering each footer as the final body row",
                "Use an HTML-capable backend when the footer must remain structurally distinct.",
            );
        }
        if table_has_spans(table) {
            self.warn_once(
                "gfm-table-spans",
                "table cell spans are not supported by GFM; leaving spanned positions empty while preserving cell text",
                "Use an HTML-capable backend when merged cells are required.",
            );
        }
        if table
            .columns
            .iter()
            .any(|column| column.style != ColumnStyle::Default)
            || table_cells(table).any(|cell| {
                cell.style
                    .is_some_and(|style| style != ColumnStyle::Default)
            })
        {
            self.warn_once(
                "gfm-table-cell-styles",
                "non-default table cell styles are not fully supported by GFM; preserving supported emphasis and code styling",
                "Use an HTML-capable backend when table cell styling must remain exact.",
            );
        }
        if table_cells(table).any(|cell| {
            cell.content.len() > 1
                || cell
                    .content
                    .iter()
                    .any(|block| !matches!(block, Block::Paragraph(_)))
        }) {
            self.warn_once(
                "gfm-nested-table-blocks",
                "nested table cell blocks are not supported by GFM; flattening block boundaries with HTML line breaks",
                "Use an HTML-capable backend when cells must retain nested block structure.",
            );
        }
        if metadata.attributes.get("width").is_some()
            || metadata.options.contains(&"autowidth")
            || table
                .columns
                .iter()
                .any(|column| column.width != ColumnWidth::default())
        {
            self.warn_once(
                "gfm-table-widths",
                "table width metadata is not supported by GFM; preserving the table without fixed widths",
                "Use an HTML-capable backend when table or column widths must remain exact.",
            );
        }
        if metadata.attributes.get("align").is_some()
            || metadata.attributes.get("float").is_some()
            || table
                .columns
                .iter()
                .any(|column| column.valign != VerticalAlignment::Top)
            || table_cells(table).any(|cell| cell.halign.is_some() || cell.valign.is_some())
        {
            self.warn_once(
                "gfm-table-local-alignment",
                "table-level and per-cell alignment are not supported by GFM; preserving column-level horizontal alignment",
                "Use column alignment for GFM, or an HTML-capable backend for table-level and per-cell alignment.",
            );
        }
    }

    fn render_gfm_table(&mut self, table: &Table) -> Result<(), Error> {
        if table.header.is_none() && table.rows.is_empty() && table.footer.is_none() {
            return Ok(());
        }

        let column_count = determine_column_count(table);
        let grid = build_grid(table, column_count);
        if let Some(header) = grid.iter().find(|row| row.is_header) {
            self.write_gfm_table_row(header, table)?;
        } else {
            write!(self.writer, "|")?;
            for _ in 0..column_count {
                write!(self.writer, "  |")?;
            }
            writeln!(self.writer)?;
        }

        write!(self.writer, "|")?;
        for column_index in 0..column_count {
            let alignment = table
                .columns
                .get(column_index)
                .map_or(HorizontalAlignment::Left, |column| column.halign);
            let marker = match alignment {
                HorizontalAlignment::Left => ":---",
                HorizontalAlignment::Center => ":---:",
                HorizontalAlignment::Right => "---:",
            };
            write!(self.writer, " {marker} |")?;
        }
        writeln!(self.writer)?;

        for row in grid.iter().filter(|row| !row.is_header) {
            self.write_gfm_table_row(row, table)?;
        }
        Ok(())
    }

    fn write_gfm_table_row(&mut self, row: &GridRow<'_>, table: &Table<'_>) -> Result<(), Error> {
        write!(self.writer, "|")?;
        for (column_index, cell_kind) in row.cells.iter().enumerate() {
            write!(self.writer, " ")?;
            if let CellKind::Content { cell_index } = cell_kind
                && let Some(cell) = row.ast_row.columns.get(*cell_index)
            {
                let style = cell.style.or_else(|| {
                    table.columns.get(column_index).and_then(|column| {
                        (column.style != ColumnStyle::Default).then_some(column.style)
                    })
                });
                self.write_gfm_table_cell(cell, style)?;
            }
            write!(self.writer, " |")?;
        }
        writeln!(self.writer)?;
        Ok(())
    }

    fn write_gfm_table_cell(
        &mut self,
        cell: &TableColumn<'_>,
        style: Option<ColumnStyle>,
    ) -> Result<(), Error> {
        let mut buffer = Vec::new();
        let (result, emitted_anchors, footnotes) = {
            let mut visitor = MarkdownVisitor::new(
                &mut buffer,
                self.processor.clone(),
                self.diagnostics.reborrow(),
            );
            visitor.heading_level = self.heading_level;
            visitor.emitted_anchors = std::mem::take(&mut self.emitted_anchors);
            visitor.collect_index_terms = self.collect_index_terms;
            visitor
                .current_section_title
                .clone_from(&self.current_section_title);
            visitor.footnotes = std::mem::take(&mut self.footnotes);
            let result = visitor.visit_separated_blocks(&cell.content, false);
            (result, visitor.emitted_anchors, visitor.footnotes)
        };
        self.emitted_anchors = emitted_anchors;
        self.footnotes = footnotes;
        let _ = result?;
        let content = flatten_table_cell(&String::from_utf8(buffer)?);
        match style {
            Some(ColumnStyle::Strong | ColumnStyle::Header) => {
                write!(self.writer, "<strong>{content}</strong>")?;
            }
            Some(ColumnStyle::Emphasis) => {
                write!(self.writer, "<em>{content}</em>")?;
            }
            Some(ColumnStyle::Monospace | ColumnStyle::Literal) => {
                write!(self.writer, "<code>{content}</code>")?;
            }
            None | Some(ColumnStyle::AsciiDoc | ColumnStyle::Default | _) => {
                write!(self.writer, "{content}")?;
            }
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
