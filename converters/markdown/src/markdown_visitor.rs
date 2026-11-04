//! Visitor implementation for Markdown conversion.

use std::{borrow::Cow, collections::HashSet, fmt::Write as _, io::Write, mem::take, rc::Rc};

use acdc_converters_core::{
    Converter, Diagnostics, TraversalContext, Warning,
    code::{SourceLineOptions, default_line_comment, source_language},
    document_attribute_text, icon,
    inline_text::InlineTextTransform,
    link::{autolink_fallback, link_fallback, mailto_fallback, mailto_target},
    list::OrderedListNumbering,
    media::{resolve_target, with_time_range_from_attributes},
    section::{
        appendix_number_prefix, book_chapter_signifier, effective_section_level,
        part_number_prefix, section_number_prefix,
    },
    substitutions::{Replacements, TextBoundaries, apply_passthrough_replacements_iter},
    table::{CellKind, GridRow, column_count, grid_rows, has_spans},
    toc::{Config as TocConfig, NumberingConfig, entry_level, has_parts, number_prefixes},
    visitor::{BlockVisitor, DocumentVisitor},
    xref::{XrefDisplay, interdocument_xref, resolve_reference},
};
use acdc_parser::{
    AdmonitionVariant, CaptionKind, ColumnStyle, ColumnWidth, HorizontalAlignment,
    ListItemCheckedStatus, Location, SectionKind, SourceLocation, Substitution, VerticalAlignment,
    ast::{
        AdmonitionRef, AnchorRef, AttributeValueRef, AttributesRef, AudioRef, AuthorRef,
        AutolinkRef, BlockKind, BlockRef, CalloutListRef, ColumnFormatRef, ContentRef,
        CrossReferenceRef, DescriptionListRef, DiscreteHeaderRef, DocumentRef, ElementIdRef,
        FootnoteRef, HeaderRef, IconRef, ImageRef, IndexTermRef, IndexTermRelationshipRef,
        InlineImageRef, InlineKind, InlineRef, KeyboardRef, LinkRef, ListItemRef, OrderedListRef,
        PageBreakRef, ParagraphRef, ReferenceRef, SectionRef, SourceRef, TableCellRef,
        TableOfContentsRef, TableRef, TableRowRef, ThematicBreakRef, TocEntryRef, UnorderedListRef,
        VideoRef,
    },
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

fn list_sequence(attributes: AttributesRef<'_>, item_count: usize) -> (usize, bool) {
    let reversed = has_option(attributes, "reversed");
    let start = attribute_text(attributes, "start")
        .and_then(|start| start.parse::<usize>().ok())
        .filter(|start| *start > 0)
        .unwrap_or(if reversed { item_count } else { 1 });
    (start, reversed)
}

fn table_cells(table: TableRef<'_>) -> impl Iterator<Item = TableCellRef<'_>> {
    table
        .header()
        .into_iter()
        .chain(table.rows())
        .chain(table.footer())
        .flat_map(TableRowRef::cells)
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

fn attribute_content<'doc>(
    attributes: AttributesRef<'doc>,
    name: &str,
) -> Option<ContentRef<'doc>> {
    match attributes.get(name) {
        Some(AttributeValueRef::Content(content)) => Some(content),
        _ => None,
    }
}

fn has_option(attributes: AttributesRef<'_>, name: &str) -> bool {
    matches!(attributes.get("options"), Some(AttributeValueRef::Tokens(tokens)) if tokens.contains(name))
}

fn block_style(attributes: AttributesRef<'_>) -> Option<&str> {
    match attributes.get("style") {
        Some(AttributeValueRef::Text(text)) => Some(text),
        _ => None,
    }
}

fn attribute_text<'doc>(attributes: AttributesRef<'doc>, name: &str) -> Option<&'doc str> {
    match attributes.get(name) {
        Some(AttributeValueRef::Text(text)) => Some(text),
        _ => None,
    }
}

fn is_linked_image(node: InlineRef<'_>) -> bool {
    matches!(node.kind(), InlineKind::Image(image) if attribute_text(image.attributes(), "link").is_some())
}

fn raw_content(content: ContentRef<'_>) -> (String, Option<InlineRef<'_>>) {
    let mut output = String::new();
    let mut unknown = None;
    for node in content.inlines() {
        match node.kind() {
            InlineKind::Verbatim(text) => output.push_str(text.text()),
            InlineKind::Raw(text) => output.push_str(text.text()),
            InlineKind::Plain(text) => output.push_str(text.text()),
            InlineKind::CharacterReference(reference) => output.push_str(
                &acdc_converters_core::character_reference_text(reference.kind()),
            ),
            InlineKind::LineBreak(_) => output.push('\n'),
            InlineKind::Callout(callout) => {
                let _ = write!(output, "({})", callout.number());
            }
            InlineKind::Bold(_)
            | InlineKind::Italic(_)
            | InlineKind::Monospace(_)
            | InlineKind::Highlight(_)
            | InlineKind::Subscript(_)
            | InlineKind::Superscript(_)
            | InlineKind::CurvedQuotation(_)
            | InlineKind::CurvedApostrophe(_)
            | InlineKind::StandaloneCurvedApostrophe(_)
            | InlineKind::Symbol(_)
            | InlineKind::Anchor(_)
            | InlineKind::Footnote(_)
            | InlineKind::Icon(_)
            | InlineKind::Image(_)
            | InlineKind::Keyboard(_)
            | InlineKind::Button(_)
            | InlineKind::Menu(_)
            | InlineKind::Url(_)
            | InlineKind::Link(_)
            | InlineKind::Mailto(_)
            | InlineKind::Autolink(_)
            | InlineKind::CrossReference(_)
            | InlineKind::Pass(_)
            | InlineKind::Stem(_)
            | InlineKind::IndexTerm(_) => {
                let _ = InlineTextTransform::default()
                    .line_break("\n")
                    .write_inlines(&mut output, std::iter::once(node));
            }
            _ => {
                unknown.get_or_insert(node);
            }
        }
    }
    (output, unknown)
}

fn source_content<'inline>(
    content: ContentRef<'inline>,
    language: Option<&str>,
) -> (String, Option<InlineRef<'inline>>) {
    let mut output = String::new();
    let mut unknown = None;
    let comment_prefix = default_line_comment(language);
    let mut nodes = content.inlines().peekable();
    let mut previous_callout = false;
    while let Some(node) = nodes.next() {
        match node.kind() {
            InlineKind::Verbatim(text) => {
                let mut text = text.text();
                if previous_callout {
                    text = text.strip_prefix("-->").unwrap_or(text);
                }
                let next_callout = nodes
                    .peek()
                    .is_some_and(|next| matches!(next.kind(), InlineKind::Callout(_)));
                let text = if next_callout {
                    let xml = text.ends_with("<!--") && nodes.clone().nth(1).is_some_and(|next| matches!(next.kind(), InlineKind::Verbatim(text) if text.text().starts_with("-->")));
                    if xml {
                        Cow::Borrowed(text.strip_suffix("<!--").unwrap_or(text))
                    } else {
                        strip_callout_guard(text, comment_prefix)
                    }
                } else {
                    Cow::Borrowed(text)
                };
                output.push_str(&text);
            }
            InlineKind::Raw(text) => output.push_str(text.text()),
            InlineKind::Plain(text) => output.push_str(text.text()),
            InlineKind::CharacterReference(reference) => output.push_str(
                &acdc_converters_core::character_reference_text(reference.kind()),
            ),
            InlineKind::LineBreak(_) => output.push('\n'),
            InlineKind::Callout(callout) => {
                let _ = write!(output, "({})", callout.number());
            }
            InlineKind::Bold(_)
            | InlineKind::Italic(_)
            | InlineKind::Monospace(_)
            | InlineKind::Highlight(_)
            | InlineKind::Subscript(_)
            | InlineKind::Superscript(_)
            | InlineKind::CurvedQuotation(_)
            | InlineKind::CurvedApostrophe(_)
            | InlineKind::StandaloneCurvedApostrophe(_)
            | InlineKind::Symbol(_)
            | InlineKind::Anchor(_)
            | InlineKind::Footnote(_)
            | InlineKind::Icon(_)
            | InlineKind::Image(_)
            | InlineKind::Keyboard(_)
            | InlineKind::Button(_)
            | InlineKind::Menu(_)
            | InlineKind::Url(_)
            | InlineKind::Link(_)
            | InlineKind::Mailto(_)
            | InlineKind::Autolink(_)
            | InlineKind::CrossReference(_)
            | InlineKind::Pass(_)
            | InlineKind::Stem(_)
            | InlineKind::IndexTerm(_) => {
                let _ = InlineTextTransform::default()
                    .line_break("\n")
                    .write_inlines(&mut output, std::iter::once(node));
            }
            _ => {
                unknown.get_or_insert(node);
            }
        }
        previous_callout = matches!(node.kind(), InlineKind::Callout(_));
    }
    (output, unknown)
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
    pub(crate) processor: &'d Processor<'a>,

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
    pub fn new(writer: W, processor: &'d Processor<'a>, diagnostics: Diagnostics<'d>) -> Self {
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

    fn section_prefix(traversal: &TraversalContext<'a>, section: SectionRef<'a>) -> String {
        section.number().map_or_else(String::new, |number| {
            if section.kind() == SectionKind::Appendix {
                let caption = traversal
                    .get("appendix-caption")
                    .and_then(|value| value.text());
                appendix_number_prefix(number, caption)
            } else if section.level() == 0 && section.kind() == SectionKind::Normal {
                let signifier = traversal
                    .get("part-signifier")
                    .and_then(|value| value.text());
                part_number_prefix(number, signifier)
            } else {
                let signifier = (section.level() == 1 && section.kind() == SectionKind::Normal)
                    .then(|| book_chapter_signifier(traversal, None))
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

    fn write_inline_anchor_node(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        anchor: AnchorRef<'_>,
    ) -> Result<(), Error> {
        self.write_inline_anchor(Some(anchor.id()))?;
        if !anchor.is_bibliography() {
            return Ok(());
        }

        if let Some(label) = anchor.bibliography_label() {
            write!(self.writer, "\\[")?;
            self.write_content(traversal, label)?;
            write!(self.writer, "\\]")?;
            return Ok(());
        }

        write!(self.writer, "\\[{}\\]", Self::escape_markdown(anchor.id()))?;
        Ok(())
    }

    fn write_metadata_anchor(&mut self, anchor: Option<AnchorRef<'_>>) -> Result<(), Error> {
        if let Some(anchor) = anchor {
            self.write_block_anchor(anchor.id())?;
        }
        Ok(())
    }

    fn write_block_title_line(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        title: Option<ContentRef<'_>>,
        attributes: AttributesRef<'_>,
        fallback: Option<CaptionKind>,
    ) -> Result<(), Error> {
        if title.is_none() {
            return Ok(());
        }
        write!(self.writer, "<strong>")?;
        if let Some(content) = attribute_content(attributes, "caption") {
            self.write_content(traversal, content)?;
        } else if let Some(caption) = self.processor.caption_prefix(attributes, fallback) {
            write!(self.writer, "{}", Self::escape_markdown(&caption))?;
        }
        if let Some(title) = title {
            self.write_content(traversal, title)?;
        }
        writeln!(self.writer, "</strong>")?;
        Ok(())
    }

    fn write_block_title(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        title: Option<ContentRef<'_>>,
        attributes: AttributesRef<'_>,
        fallback: Option<CaptionKind>,
    ) -> Result<(), Error> {
        if title.is_none() {
            return Ok(());
        }
        self.write_block_title_line(traversal, title, attributes, fallback)?;
        writeln!(self.writer)?;
        Ok(())
    }

    fn write_attribution(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        attributes: AttributesRef<'_>,
        line_prefix: &str,
    ) -> Result<(), Error> {
        let attribution = attribute_content(attributes, "attribution");
        let citation = attribute_content(attributes, "citetitle");
        if attribution.is_none() && citation.is_none() {
            return Ok(());
        }

        write!(self.writer, "{line_prefix}")?;
        if let Some(attribution) = attribution {
            write!(self.writer, "— ")?;
            self.write_content(traversal, attribution)?;
        }
        if let Some(citation) = citation {
            if attribution.is_some() {
                write!(self.writer, ", ")?;
            }
            write!(self.writer, "<cite>")?;
            self.write_content(traversal, citation)?;
            write!(self.writer, "</cite>")?;
        }
        writeln!(self.writer)?;
        Ok(())
    }

    fn write_blockquote_inlines(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        content: ContentRef<'_>,
    ) -> Result<(), Error> {
        write!(self.writer, "> ")?;
        for node in content.inlines() {
            match node.kind() {
                InlineKind::Plain(text) => {
                    self.write_blockquote_text(text.text(), true)?;
                }
                InlineKind::Raw(text) => {
                    self.write_blockquote_text(
                        &Self::passthrough_content(text.text(), || text.substitutions()),
                        false,
                    )?;
                }
                InlineKind::Verbatim(text) => {
                    self.write_blockquote_text(text.text(), false)?;
                }
                InlineKind::LineBreak(_) => self.write_blockquote_break()?,
                InlineKind::Bold(_)
                | InlineKind::Italic(_)
                | InlineKind::Monospace(_)
                | InlineKind::Highlight(_)
                | InlineKind::Subscript(_)
                | InlineKind::Superscript(_)
                | InlineKind::CurvedQuotation(_)
                | InlineKind::CurvedApostrophe(_)
                | InlineKind::StandaloneCurvedApostrophe(_)
                | InlineKind::CharacterReference(_)
                | InlineKind::Symbol(_)
                | InlineKind::Anchor(_)
                | InlineKind::Footnote(_)
                | InlineKind::Icon(_)
                | InlineKind::Image(_)
                | InlineKind::Keyboard(_)
                | InlineKind::Button(_)
                | InlineKind::Menu(_)
                | InlineKind::Url(_)
                | InlineKind::Link(_)
                | InlineKind::Mailto(_)
                | InlineKind::Autolink(_)
                | InlineKind::CrossReference(_)
                | InlineKind::Pass(_)
                | InlineKind::Stem(_)
                | InlineKind::IndexTerm(_)
                | InlineKind::Callout(_)
                | _ => self.write_inline(traversal, node)?,
            }
        }
        writeln!(self.writer)?;
        Ok(())
    }

    fn write_blockquote_text(&mut self, text: &str, escape: bool) -> Result<(), Error> {
        let mut lines = text.split('\n').peekable();
        while let Some(line) = lines.next() {
            if escape {
                write!(self.writer, "{}", Self::escape_markdown(line))?;
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
        traversal: &mut TraversalContext<'a>,
        attributes: AttributesRef<'_>,
        content: ContentRef<'_>,
    ) -> Result<(), Error> {
        acdc_converters_core::index::visit_index_inlines(content.inlines(), &mut |term, _| {
            self.register_index_term(traversal, term)
        })?;
        let language = source_language(attributes);
        let (content, unknown) = source_content(content, language);
        if let Some(node) = unknown {
            self.warn_unknown_inline_node_ref(node);
        }
        self.warn_source_options(attributes, &content, language);

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
        attributes: AttributesRef<'_>,
        content: &str,
        language: Option<&str>,
    ) {
        let options = SourceLineOptions::from_attributes(attributes, content);
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
        if block_style(attributes) == Some("source")
            && language == Some("php")
            && has_option(attributes, "mixed")
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

    fn warn_unknown_inline_node_ref(&mut self, node: InlineRef<'_>) {
        self.warn_once_at(
            "unknown-inline-node",
            format!("unknown inline feature is not supported by the Markdown backend; skipping content: {node:?}"),
            "Use a backend that supports this inline feature, or replace it with portable inline content.",
            Some(&node.location().into()),
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

    fn write_emphasis(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        content: ContentRef<'_>,
        marker: &str,
        html_tag: &str,
    ) -> Result<(), Error> {
        // Empty Markdown delimiters are literal text, so retain the empty
        // formatting with an HTML element.
        if content.is_empty() {
            write!(self.writer, "<{html_tag}></{html_tag}>")?;
        } else {
            write!(self.writer, "{marker}")?;
            self.write_content(traversal, content)?;
            write!(self.writer, "{marker}")?;
        }
        Ok(())
    }

    fn write_code_span(&mut self, content: &str) -> Result<(), Error> {
        if content.is_empty()
            || content.contains('\n')
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

    fn write_content(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        content: ContentRef<'_>,
    ) -> Result<(), Error> {
        acdc_converters_core::visitor::InlineVisitor::visit_inline_nodes(self, traversal, content)
    }

    fn write_inline(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        node: InlineRef<'_>,
    ) -> Result<(), Error> {
        acdc_converters_core::visitor::InlineVisitor::visit_inline_node(self, traversal, node)
    }

    fn write_escaped_text(&mut self, text: &str) -> Result<(), Error> {
        write!(
            self.writer,
            "{}",
            Self::escape_markdown(&text.replace('&', "&amp;"))
        )?;
        Ok(())
    }

    fn passthrough_content<I: Iterator<Item = Substitution>>(
        text: &str,
        substitutions: impl Fn() -> I,
    ) -> String {
        let mut output = text.to_owned();
        let mut special_chars = false;
        for substitution in substitutions() {
            match substitution {
                Substitution::SpecialChars => {
                    special_chars = true;
                    output = escape_html_text(&output);
                }
                Substitution::Replacements => {
                    output = apply_passthrough_replacements_iter(
                        &output,
                        substitutions(),
                        &Replacements::unicode(),
                        TextBoundaries::BOTH,
                    )
                    .into_owned();
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
        if special_chars {
            // Escape for Markdown after AsciiDoc substitutions to preserve literal
            // backslashes, brackets, and emphasis markers in rendered text.
            Self::escape_markdown(&output)
        } else {
            output
        }
    }

    fn write_passthrough<I: Iterator<Item = Substitution>>(
        &mut self,
        text: &str,
        substitutions: impl Fn() -> I,
    ) -> Result<(), Error> {
        write!(
            self.writer,
            "{}",
            Self::passthrough_content(text, substitutions)
        )?;
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

    fn record_footnote(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        footnote: FootnoteRef<'_>,
    ) -> Result<String, Error> {
        let id = footnote
            .id()
            .map_or_else(|| footnote.number().to_string(), str::to_owned);
        if footnote.content().is_empty()
            || self
                .footnotes
                .iter()
                .any(|(existing_id, _, _)| existing_id == &id)
        {
            return Ok(id);
        }

        let mut buffer = Vec::new();
        {
            let mut visitor =
                MarkdownVisitor::new(&mut buffer, self.processor, self.diagnostics.reborrow());
            visitor.heading_level = self.heading_level;
            visitor.list_depth = self.list_depth;
            visitor.collect_index_terms = self.collect_index_terms;
            visitor
                .current_section_title
                .clone_from(&self.current_section_title);
            visitor.write_content(traversal, footnote.content())?;
        }
        let rendered = String::from_utf8(buffer).unwrap_or_default();
        self.footnotes
            .push((id.clone(), footnote.number(), rendered));
        Ok(id)
    }

    fn render_index_term_label(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        inlines: ContentRef<'_>,
    ) -> Result<IndexTermLabel, Error> {
        let plain = InlineTextTransform::default()
            .references(&self.processor.references)
            .content_to_string(inlines);
        let mut output = Vec::new();
        {
            let mut visitor =
                MarkdownVisitor::new(&mut output, self.processor, self.diagnostics.reborrow());
            visitor.collect_index_terms = false;
            visitor.in_link_text = true;
            visitor
                .current_section_title
                .clone_from(&self.current_section_title);
            visitor.write_content(traversal, inlines)?;
        }
        Ok(IndexTermLabel {
            plain,
            rendered: String::from_utf8(output)?,
        })
    }

    fn register_index_term(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        term: IndexTermRef<'_>,
    ) -> Result<(), Error> {
        let term = term.catalog();
        if self.collect_index_terms && self.processor.generate_index() {
            let primary = self.render_index_term_label(traversal, term.term())?;
            let secondary = term
                .secondary()
                .map(|inlines| self.render_index_term_label(traversal, inlines))
                .transpose()?;
            let tertiary = term
                .tertiary()
                .map(|inlines| self.render_index_term_label(traversal, inlines))
                .transpose()?;
            let relationship = match term.relationship() {
                Some(IndexTermRelationshipRef::See { target }) => {
                    IndexCatalogRelationship::See(self.render_index_term_label(traversal, target)?)
                }
                Some(IndexTermRelationshipRef::SeeAlso { targets }) => {
                    IndexCatalogRelationship::SeeAlso(
                        targets
                            .iter()
                            .map(|target| self.render_index_term_label(traversal, target))
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

        Ok(())
    }

    fn visit_index_term(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        term: IndexTermRef<'_>,
    ) -> Result<(), Error> {
        self.register_index_term(traversal, term)?;
        if term.is_visible() {
            self.write_content(traversal, term.term())?;
        }
        Ok(())
    }

    fn write_raw_block_content(&mut self, content: ContentRef<'_>) -> Result<(), Error> {
        let (content, unknown) = raw_content(content);
        if let Some(node) = unknown {
            self.warn_unknown_inline_node_ref(node);
        }
        write!(self.writer, "{content}")?;
        if !content.ends_with('\n') {
            writeln!(self.writer)?;
        }
        Ok(())
    }

    fn write_author(&mut self, author: AuthorRef<'_>) -> Result<(), Error> {
        write!(
            self.writer,
            "{} ",
            Self::escape_markdown(author.first_name())
        )?;
        if let Some(middle_name) = author.middle_name() {
            write!(self.writer, "{} ", Self::escape_markdown(middle_name))?;
        }
        write!(self.writer, "{}", Self::escape_markdown(author.last_name()))?;
        if let Some(email) = author.email() {
            write!(self.writer, " <{}>", Self::escape_markdown(email))?;
        }
        Ok(())
    }

    fn write_revision(&mut self) -> Result<(), Error> {
        let attributes = self.processor.document_attributes();
        let number = document_attribute_text((attributes).get("revnumber")).map(str::to_owned);
        let date = document_attribute_text((attributes).get("revdate")).map(str::to_owned);
        let remark = document_attribute_text((attributes).get("revremark")).map(str::to_owned);
        let label = document_attribute_text((attributes).get("version-label"))
            .filter(|label| !label.is_empty())
            .map(str::to_owned);
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
        traversal: &mut TraversalContext<'a>,
        title: Option<ContentRef<'_>>,
        is_open: bool,
        write_body: F,
    ) -> Result<(), Error>
    where
        F: FnOnce(&mut Self, &mut TraversalContext<'a>) -> Result<bool, Error>,
    {
        if is_open {
            writeln!(self.writer, "<details open>")?;
        } else {
            writeln!(self.writer, "<details>")?;
        }
        write!(self.writer, "<summary>")?;
        if let Some(title) = title {
            self.write_content(traversal, title)?;
        } else {
            write!(self.writer, "Details")?;
        }
        writeln!(self.writer, "</summary>")?;
        // Blank line so inner content is rendered as Markdown inside <details>.
        writeln!(self.writer)?;
        if write_body(self, traversal)? {
            writeln!(self.writer)?;
        }
        writeln!(self.writer, "</details>")?;
        Ok(())
    }

    fn toc_will_render(&self, toc_macro: Option<TableOfContentsRef<'_>>, placement: &str) -> bool {
        if self
            .processor
            .document
            .is_none_or(|document| document.toc_entries().next().is_none())
        {
            return false;
        }
        let config = TocConfig::resolve(
            toc_macro,
            &TraversalContext::new(self.processor.document_attributes()),
        );
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
        traversal: &mut TraversalContext<'a>,
        entries: &[TocEntryRef<'_>],
        config: &TocRenderConfig<'_>,
        position: TocRenderPosition,
    ) -> Result<(), Error> {
        if position.current_level > config.max_level {
            return Ok(());
        }

        let first_real_part = if position.parts_at_current_level {
            entries
                .iter()
                .position(|entry| entry.level() == 0 && entry.kind() == SectionKind::Normal)
        } else {
            None
        };
        let current_entries = entries
            .iter()
            .enumerate()
            .filter(|(index, entry)| {
                let level = entry_level(**entry, config.has_real_parts);
                if level == position.current_level {
                    first_real_part
                        .is_none_or(|part| *index < part || entry.level() != position.current_level)
                } else {
                    position.parts_at_current_level && entry.level() == 0 && level == 0
                }
            })
            .collect::<Vec<_>>();

        for (entry_position, (entry_index, entry)) in current_entries.iter().enumerate() {
            write!(self.writer, "{:indent$}- ", "", indent = position.indent)?;
            self.write_anchor_link(entry.id(), |visitor| {
                if let Some(Some(number)) = config
                    .section_numbers
                    .get(position.base_index + entry_index)
                {
                    write!(visitor.writer, "{number}")?;
                }
                if let Some(title) = entry.title() {
                    visitor.write_content(traversal, title)?;
                }
                Ok(())
            })?;
            writeln!(self.writer)?;

            let child_start = entry_index + 1;
            let child_end = current_entries
                .get(entry_position + 1)
                .map_or(entries.len(), |next| next.0);
            let child_level = if entry.level() == 0 && entry.kind().is_special() {
                2
            } else {
                entry_level(**entry, config.has_real_parts) + 1
            };
            if let Some(children) = entries.get(child_start..child_end)
                && child_level <= config.max_level
                && children.iter().any(|child| child.level() == child_level)
            {
                self.render_toc_entries(
                    traversal,
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
        traversal: &mut TraversalContext<'a>,
        toc_macro: Option<TableOfContentsRef<'_>>,
        placement: &str,
    ) -> Result<(), Error> {
        if !self.toc_will_render(toc_macro, placement) {
            return Ok(());
        }

        let processor = self.processor;
        let config = TocConfig::resolve(
            toc_macro,
            &TraversalContext::new(processor.document_attributes()),
        );
        let title = config.title().or_else(|| {
            (processor.document_attributes().get("toc-title").is_none())
                .then_some("Table of Contents")
        });
        if let Some(title) = title.filter(|title| !title.is_empty()) {
            writeln!(self.writer, "## {}", Self::escape_markdown(title))?;
            writeln!(self.writer)?;
        }

        let part_signifier = traversal
            .get("part-signifier")
            .and_then(|value| value.text());
        let chapter_signifier = book_chapter_signifier(traversal, None);
        let numbering_config = NumberingConfig::new(traversal, part_signifier, chapter_signifier);
        let entries: Vec<_> = processor
            .document
            .into_iter()
            .flat_map(DocumentRef::toc_entries)
            .collect();
        let numbers = number_prefixes(entries.iter().copied(), &numbering_config);
        let real_parts = has_parts(entries.iter().copied());
        let first_level = entries
            .first()
            .map_or(1, |entry| entry_level(*entry, real_parts));
        let parts_at_current_level = first_level > 0 && real_parts;
        let start_level = if parts_at_current_level {
            1
        } else {
            first_level
        };
        self.render_toc_entries(
            traversal,
            &entries,
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
        traversal: &mut TraversalContext<'a>,
        toc_macro: Option<TableOfContentsRef<'_>>,
        placement: &str,
        has_output: bool,
    ) -> Result<bool, Error> {
        if !self.toc_will_render(toc_macro, placement) {
            return Ok(has_output);
        }
        if has_output {
            writeln!(self.writer)?;
        }
        self.with_index_collection(false, |visitor| {
            visitor.render_toc(traversal, toc_macro, placement)
        })?;
        Ok(true)
    }

    fn block_has_output(&self, block: BlockRef<'a>) -> bool {
        match block.kind() {
            BlockKind::Open(node) => node.blocks().any(|block| self.block_has_output(block)),
            BlockKind::Quote(node) => node.blocks().any(|block| self.block_has_output(block)),
            BlockKind::Table(table) => {
                self.variant() == MarkdownVariant::CommonMark
                    || table.header().is_some()
                    || table.rows().next().is_some()
                    || table.footer().is_some()
            }
            BlockKind::TableOfContents(toc) => {
                toc.primary_anchor().is_some() || self.toc_will_render(Some(toc), "macro")
            }
            BlockKind::Comment(_)
            | BlockKind::DocumentAttribute(_)
            | BlockKind::DelimitedComment(_) => false,
            BlockKind::Admonition(_)
            | BlockKind::DiscreteHeader(_)
            | BlockKind::ThematicBreak(_)
            | BlockKind::PageBreak(_)
            | BlockKind::UnorderedList(_)
            | BlockKind::OrderedList(_)
            | BlockKind::CalloutList(_)
            | BlockKind::DescriptionList(_)
            | BlockKind::Section(_)
            | BlockKind::Paragraph(_)
            | BlockKind::Image(_)
            | BlockKind::Audio(_)
            | BlockKind::Video(_)
            | BlockKind::Example(_)
            | BlockKind::Listing(_)
            | BlockKind::Literal(_)
            | BlockKind::Sidebar(_)
            | BlockKind::Passthrough(_)
            | BlockKind::Verse(_)
            | BlockKind::Stem(_)
            | _ => true,
        }
    }

    fn visit_separated_blocks(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        blocks: impl Iterator<Item = BlockRef<'a>>,
        mut has_output: bool,
    ) -> Result<bool, Error> {
        for block in blocks {
            if matches!(block.kind(), BlockKind::DocumentAttribute(_)) {
                traversal.visit_block(self, block)?;
                continue;
            }
            if !self.block_has_output(block) {
                continue;
            }
            if has_output {
                writeln!(self.writer)?;
            }
            traversal.visit_block(self, block)?;
            has_output = true;
        }
        Ok(has_output)
    }

    fn visit_blockquote_blocks(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        blocks: impl Iterator<Item = BlockRef<'a>>,
    ) -> Result<(), Error> {
        let mut has_output = false;
        for block in blocks {
            if matches!(block.kind(), BlockKind::DocumentAttribute(_)) {
                traversal.visit_block(self, block)?;
                continue;
            }
            if !self.block_has_output(block) {
                continue;
            }
            if has_output {
                writeln!(self.writer, ">")?;
            }
            write!(self.writer, "> ")?;
            traversal.visit_block(self, block)?;
            has_output = true;
        }
        Ok(())
    }
}

impl<'a, W: Write> MarkdownVisitor<'a, '_, W> {
    /// Skip a block and report its source range once per conversion.
    ///
    /// # Errors
    ///
    /// This fallback always succeeds.
    pub fn visit_unhandled_block(
        &mut self,
        _traversal: &mut TraversalContext<'a>,
        block: BlockRef<'a>,
    ) -> Result<(), Error> {
        self.warn_once_at(
            "unknown-parser-block",
            format!(
                "unknown parser block feature is not supported by the Markdown backend; skipping content: {block:?}"
            ),
            "Use a backend that supports this block feature, or replace it with a portable AsciiDoc construct.",
            Some(&block.location().into()),
        );
        Ok(())
    }

    fn render_document(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        doc: DocumentRef<'a>,
    ) -> Result<(), Error> {
        let mut has_output = false;
        if let Some(header) = doc
            .header()
            .filter(|header| header.title().is_some_and(|title| !title.is_empty()))
        {
            self.visit_header(traversal, header)?;
            has_output = true;
        }
        has_output = self.render_placed_toc(traversal, None, "auto", has_output)?;
        let first_section = doc
            .blocks()
            .position(|block| matches!(block.kind(), BlockKind::Section(_)));
        let split = first_section.unwrap_or_else(|| doc.blocks().count());
        let emit_preamble = doc.header().is_some()
            && first_section.is_some()
            && doc.blocks().take(split).any(|block| {
                !matches!(
                    block.kind(),
                    BlockKind::Comment(_) | BlockKind::DocumentAttribute(_)
                )
            });
        has_output =
            self.visit_separated_blocks(traversal, doc.blocks().take(split), has_output)?;
        if emit_preamble {
            has_output = self.render_placed_toc(traversal, None, "preamble", has_output)?;
        }
        has_output =
            self.visit_separated_blocks(traversal, doc.blocks().skip(split), has_output)?;
        if !self.footnotes.is_empty() {
            if has_output {
                writeln!(self.writer)?;
            }
            let footnotes = take(&mut self.footnotes);
            if self.variant() == MarkdownVariant::GitHubFlavored {
                for (id, _, content) in footnotes {
                    writeln!(self.writer, "[^{id}]: {content}")?;
                }
            } else {
                writeln!(self.writer, "<strong>Footnotes</strong>\n")?;
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
        Ok(())
    }

    fn visit_header(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        header: HeaderRef<'_>,
    ) -> Result<(), Error> {
        self.write_metadata_anchor(header.primary_anchor())?;
        if let Some(title) = header.title().filter(|title| !title.is_empty()) {
            write!(self.writer, "# ")?;
            self.write_content(traversal, title)?;
            if let Some(subtitle) = header.subtitle() {
                write!(self.writer, ": ")?;
                self.write_content(traversal, subtitle)?;
            }
            writeln!(self.writer)?;
        }
        let has_revision = ["revnumber", "revdate", "revremark"]
            .iter()
            .any(|name| document_attribute_text(traversal.get(name)).is_some());
        if header.authors().next().is_some() || has_revision {
            writeln!(self.writer)?;
        }
        if header.authors().next().is_some() {
            write!(self.writer, "By ")?;
            for (index, author) in header.authors().enumerate() {
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

    fn visit_section(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        section: SectionRef<'a>,
    ) -> Result<(), Error> {
        let section_title = section.title().map_or_else(String::new, |title| {
            InlineTextTransform::default()
                .references(&self.processor.references)
                .content_to_string(title)
        });
        let previous_section_title = self.current_section_title.replace(section_title);

        let result = (|| {
            self.write_block_anchor(&section.resolve_id())?;

            let effective_level = effective_section_level(section.level(), section.kind());
            let level = effective_level + 1; // AsciiDoc levels are 0-indexed, Markdown uses 1-6
            let level = level.min(6); // Markdown only supports 6 heading levels

            if effective_level >= 6 {
                self.warn_once(
                "section-heading-depth",
                "section levels deeper than Markdown maximum 6 are capped at level 6",
                "Markdown only has six heading levels. Reduce the source section depth if the distinction matters.",
            );
            }

            if !section.options().contains("notitle") {
                let hashes = "#".repeat(level as usize);
                write!(
                    self.writer,
                    "{hashes} {}",
                    Self::section_prefix(traversal, section)
                )?;
                if let Some(title) = section.title() {
                    self.write_content(traversal, title)?;
                }
                writeln!(self.writer)?;
            }

            // Render the section body or its generated index catalog.
            let prev_level = self.heading_level;
            self.heading_level = level as usize;

            // Keep authored content before the generated catalog.
            self.visit_separated_blocks(traversal, section.blocks(), true)?;
            if section.kind() == SectionKind::Index && self.processor.generate_index() {
                let processor = self.processor;
                crate::index::render(&mut self.writer, processor, self.heading_level + 1)?;
            }

            self.heading_level = prev_level;
            Ok(())
        })();
        self.current_section_title = previous_section_title;
        result
    }

    fn visit_paragraph(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        paragraph: ParagraphRef<'_>,
    ) -> Result<(), Error> {
        self.write_metadata_anchor(paragraph.primary_anchor())?;
        if paragraph.style() == Some("pass") {
            return self.write_raw_block_content(paragraph.content());
        }

        if paragraph.style() == Some("example") && paragraph.options().contains("collapsible") {
            let is_open = paragraph.options().contains("open");
            return self.write_collapsible(
                traversal,
                paragraph.title(),
                is_open,
                |v, traversal| {
                    v.write_content(traversal, paragraph.content())?;
                    writeln!(v.writer)?;
                    Ok(true)
                },
            );
        }

        self.write_block_title(
            traversal,
            paragraph.title(),
            paragraph.attributes(),
            CaptionKind::for_style(paragraph.style()),
        )?;
        match paragraph.style() {
            Some("quote" | "verse") => {
                self.write_blockquote_inlines(traversal, paragraph.content())?;
                self.write_attribution(traversal, paragraph.attributes(), "> ")?;
            }
            Some("abstract") => self.write_blockquote_inlines(traversal, paragraph.content())?,
            Some("sidebar") => {
                self.write_warning("sidebar-blocks", "sidebar blocks", "using blockquote")?;
                self.write_blockquote_inlines(traversal, paragraph.content())?;
            }
            Some("literal" | "listing" | "source") => {
                self.write_fenced_code_block(
                    traversal,
                    paragraph.attributes(),
                    paragraph.content(),
                )?;
            }
            Some("example") => {
                self.write_warning(
                    "example-blockquotes",
                    "example paragraphs",
                    "using blockquote",
                )?;
                self.write_blockquote_inlines(traversal, paragraph.content())?;
            }
            _ => {
                self.write_content(traversal, paragraph.content())?;
                writeln!(self.writer)?;
            }
        }
        Ok(())
    }

    fn visit_unordered_list(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        list: UnorderedListRef<'a>,
    ) -> Result<(), Error> {
        self.write_metadata_anchor(list.primary_anchor())?;
        self.write_block_title(traversal, list.title(), list.attributes(), None)?;
        self.visit_list_items(
            traversal,
            list.items(),
            "-",
            1,
            false,
            suppresses_list_marker(list.style(), false),
        )
    }

    fn visit_ordered_list(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        list: OrderedListRef<'a>,
    ) -> Result<(), Error> {
        self.write_metadata_anchor(list.primary_anchor())?;
        self.write_block_title(traversal, list.title(), list.attributes(), None)?;

        // Markdown (CommonMark/GFM) can only express numeric ordered markers.
        // An explicit numbering style that isn't already numeric (arabic/decimal)
        // is rendered numerically, with a warning.
        if let Some(numbering) = list
            .style()
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
        let (start, reversed) = list_sequence(list.attributes(), list.items().count());
        self.visit_list_items(
            traversal,
            list.items(),
            "1.",
            start,
            reversed,
            suppresses_list_marker(list.style(), true),
        )
    }

    fn visit_thematic_break(
        &mut self,
        _traversal: &mut TraversalContext<'a>,
        br: ThematicBreakRef<'_>,
    ) -> Result<(), Error> {
        if let Some(anchor) = br.anchors().next() {
            self.write_block_anchor(anchor.id())?;
        }
        writeln!(self.writer, "---")?;
        Ok(())
    }

    fn visit_page_break(
        &mut self,
        _traversal: &mut TraversalContext<'a>,
        page_break: PageBreakRef<'_>,
    ) -> Result<(), Error> {
        self.write_metadata_anchor(page_break.primary_anchor())?;
        // Page breaks don't exist in Markdown; use thematic break as fallback
        self.write_warning("page-breaks", "page breaks", "using horizontal rule")?;
        writeln!(self.writer, "---")?;
        Ok(())
    }

    fn visit_table_of_contents(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        toc: TableOfContentsRef<'_>,
    ) -> Result<(), Error> {
        self.write_metadata_anchor(toc.primary_anchor())?;
        self.with_index_collection(false, |visitor| {
            visitor.render_toc(traversal, Some(toc), "macro")
        })?;
        Ok(())
    }

    fn visit_compound_block(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        block: BlockRef<'a>,
    ) -> Result<(), Error> {
        self.write_metadata_anchor(block.primary_anchor())?;
        let kind = block.kind();
        let fallback = if matches!(kind, BlockKind::Example(_)) {
            Some(CaptionKind::Example)
        } else if let BlockKind::Listing(node) = kind {
            (node.style() != Some("literal")).then_some(CaptionKind::Listing)
        } else if let BlockKind::Literal(node) = kind {
            matches!(node.style(), Some("listing" | "source")).then_some(CaptionKind::Listing)
        } else if let BlockKind::Open(node) = kind {
            CaptionKind::for_style(node.style())
        } else if matches!(kind, BlockKind::Table(_)) {
            Some(CaptionKind::Table)
        } else {
            None
        };
        let collapsible =
            matches!(kind, BlockKind::Example(node) if node.options().contains("collapsible"));
        if !collapsible
            && !matches!(
                kind,
                BlockKind::Passthrough(_) | BlockKind::DelimitedComment(_)
            )
            && let Some(attributes) = block.attributes()
        {
            self.write_block_title(traversal, block.title(), attributes, fallback)?;
        }
        match kind {
            BlockKind::Listing(node) => {
                self.write_fenced_code_block(traversal, node.attributes(), node.content())?;
            }
            BlockKind::Literal(node) => {
                self.write_fenced_code_block(traversal, node.attributes(), node.content())?;
            }
            BlockKind::Quote(node) => {
                self.visit_blockquote_blocks(traversal, node.blocks())?;
                self.write_attribution(traversal, node.attributes(), "> ")?;
            }
            BlockKind::Example(node) => {
                if collapsible {
                    self.write_collapsible(
                        traversal,
                        node.title(),
                        node.options().contains("open"),
                        |v, traversal| v.visit_separated_blocks(traversal, node.blocks(), false),
                    )?;
                } else {
                    self.write_warning(
                        "example-blockquotes",
                        "example blocks",
                        "using blockquote",
                    )?;
                    self.visit_blockquote_blocks(traversal, node.blocks())?;
                }
            }
            BlockKind::Sidebar(node) => {
                self.write_warning("sidebar-blocks", "sidebar blocks", "using blockquote")?;
                self.visit_blockquote_blocks(traversal, node.blocks())?;
            }
            BlockKind::Open(node) => {
                self.visit_separated_blocks(traversal, node.blocks(), false)?;
            }
            BlockKind::Passthrough(node) => self.write_raw_block_content(node.content())?,
            BlockKind::Table(node) => self.visit_table_inner(traversal, node, node.attributes())?,
            BlockKind::Verse(node) => {
                self.write_blockquote_inlines(traversal, node.content())?;
                self.write_attribution(traversal, node.attributes(), "> ")?;
            }
            BlockKind::Stem(node) => {
                self.warn_once("stem-blocks", "block STEM is not supported by the Markdown backend; preserving the expression as fenced code", "Use a Markdown renderer with a math extension, or use a backend that supports STEM rendering.");
                self.write_raw_fenced_code_block(Some(&node.notation().to_string()), node.text())?;
            }
            BlockKind::DelimitedComment(_) => {}
            BlockKind::TableOfContents(_)
            | BlockKind::Admonition(_)
            | BlockKind::DiscreteHeader(_)
            | BlockKind::DocumentAttribute(_)
            | BlockKind::ThematicBreak(_)
            | BlockKind::PageBreak(_)
            | BlockKind::UnorderedList(_)
            | BlockKind::OrderedList(_)
            | BlockKind::CalloutList(_)
            | BlockKind::DescriptionList(_)
            | BlockKind::Section(_)
            | BlockKind::Paragraph(_)
            | BlockKind::Image(_)
            | BlockKind::Audio(_)
            | BlockKind::Video(_)
            | BlockKind::Comment(_)
            | _ => self.visit_unhandled_block(traversal, block)?,
        }
        Ok(())
    }

    fn visit_admonition(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        admonition: AdmonitionRef<'a>,
    ) -> Result<(), Error> {
        self.write_metadata_anchor(admonition.primary_anchor())?;
        self.write_block_title(traversal, admonition.title(), admonition.attributes(), None)?;

        // GitHub Flavored Markdown supports Alerts syntax (> [!TYPE])
        // CommonMark falls back to blockquote with bold label
        let alert_type = match admonition.variant() {
            AdmonitionVariant::Note => "NOTE",
            AdmonitionVariant::Tip => "TIP",
            AdmonitionVariant::Important => "IMPORTANT",
            AdmonitionVariant::Warning => "WARNING",
            AdmonitionVariant::Caution => "CAUTION",
        };

        if let Some(content) = admonition.caption_content() {
            write!(self.writer, "> **")?;
            self.write_content(traversal, content)?;
            writeln!(self.writer, "**")?;
        } else if self.variant() == MarkdownVariant::GitHubFlavored {
            // Use GitHub Alerts syntax (native support, no warning needed)
            writeln!(self.writer, "> [!{alert_type}]")?;
        } else {
            // CommonMark: use blockquote with bold label
            let label = match admonition.variant() {
                AdmonitionVariant::Note => "Note",
                AdmonitionVariant::Tip => "Tip",
                AdmonitionVariant::Important => "Important",
                AdmonitionVariant::Warning => "Warning",
                AdmonitionVariant::Caution => "Caution",
            };
            self.write_warning(
                "commonmark-admonitions",
                &format!("{label} admonitions"),
                "using blockquote with label",
            )?;
            writeln!(self.writer, "> **{label}**")?;
        }

        let mut has_body = false;
        for block in admonition.blocks() {
            if !self.block_has_output(block) {
                continue;
            }
            if has_body {
                writeln!(self.writer, ">")?;
            }
            write!(self.writer, "> ")?;
            traversal.visit_block(self, block)?;
            has_body = true;
        }
        Ok(())
    }

    fn visit_discrete_header(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        header: DiscreteHeaderRef<'_>,
    ) -> Result<(), Error> {
        self.write_block_anchor(&header.resolve_id())?;

        // Discrete headers are just headings without section structure
        let level = (header.level() + 1).min(6);
        let hashes = "#".repeat(level as usize);
        write!(self.writer, "{hashes} ")?;
        if let Some(title) = header.title() {
            self.write_content(traversal, title)?;
        }
        writeln!(self.writer)?;
        Ok(())
    }

    fn visit_image(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        image: ImageRef<'_>,
    ) -> Result<(), Error> {
        self.write_metadata_anchor(image.primary_anchor())?;
        let alt = acdc_converters_core::media::image_alt_from_content(image.source(), image.alt());
        self.write_image(traversal, image.source(), image.attributes(), &alt, None)?;
        writeln!(self.writer)?;
        if image.title().is_some() {
            writeln!(self.writer)?;
            self.write_block_title_line(
                traversal,
                image.title(),
                image.attributes(),
                Some(CaptionKind::Figure),
            )?;
        }
        Ok(())
    }

    fn visit_video(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        video: VideoRef<'_>,
    ) -> Result<(), Error> {
        self.write_metadata_anchor(video.primary_anchor())?;
        self.write_block_title(traversal, video.title(), video.attributes(), None)?;
        self.warn_static_media_fallback();

        let title = if let Some(title) = video.title() {
            InlineTextTransform::default().content_to_string(title)
        } else {
            video
                .source()
                .file_name()
                .map_or_else(|| "video".to_owned(), str::to_owned)
        };
        if let Some(poster) = attribute_text(video.attributes(), "poster")
            .filter(|value| !matches!(*value, "youtube" | "vimeo"))
        {
            let poster = resolve_target(poster, traversal);
            write!(
                self.writer,
                "![{}]({})",
                Self::escape_markdown(&format!("Video poster: {title}")),
                escape_link_destination(&poster)
            )?;
            writeln!(self.writer)?;
            if !video.source().to_string().is_empty() {
                writeln!(self.writer)?;
            }
        }
        let target = resolve_target(
            &acdc_converters_core::video::TryUrl::try_url(&video, false)?,
            traversal,
        );
        self.write_link(&target, |visitor| {
            write!(visitor.writer, "Video: {}", Self::escape_markdown(&title))?;
            Ok(())
        })?;
        writeln!(self.writer)?;
        Ok(())
    }

    fn visit_audio(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        audio: AudioRef<'_>,
    ) -> Result<(), Error> {
        self.write_metadata_anchor(audio.primary_anchor())?;
        self.write_block_title(traversal, audio.title(), audio.attributes(), None)?;
        self.warn_static_media_fallback();
        let target = with_time_range_from_attributes(
            &resolve_target(&audio.source().to_string(), traversal),
            audio.attributes(),
        );
        let title = if let Some(title) = audio.title() {
            InlineTextTransform::default().content_to_string(title)
        } else {
            audio
                .source()
                .file_name()
                .map_or_else(|| audio.source().to_string(), str::to_owned)
        };
        self.write_link(&target, |visitor| {
            write!(visitor.writer, "Audio: {}", Self::escape_markdown(&title))?;
            Ok(())
        })?;
        writeln!(self.writer)?;
        Ok(())
    }

    fn visit_description_list(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        list: DescriptionListRef<'a>,
    ) -> Result<(), Error> {
        self.write_metadata_anchor(list.primary_anchor())?;
        self.write_block_title(traversal, list.title(), list.attributes(), None)?;
        match list.style() {
            Some("horizontal") => self.visit_horizontal_description_list(traversal, list),
            Some("qanda") => self.visit_qanda_description_list(traversal, list),
            _ => self.visit_regular_description_list(traversal, list),
        }
    }

    fn visit_callout_list(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        list: CalloutListRef<'a>,
    ) -> Result<(), Error> {
        self.write_metadata_anchor(list.primary_anchor())?;
        self.write_block_title(traversal, list.title(), list.attributes(), None)?;
        for item in list.items() {
            self.write_list_indent()?;
            write!(self.writer, "- **({})** ", item.callout().number())?;
            self.write_content(traversal, item.principal())?;
            writeln!(self.writer)?;

            if item.blocks().next().is_some() {
                writeln!(self.writer)?;
                self.list_depth += 1;
                let result: Result<(), Error> = (|| {
                    for (index, block) in item.blocks().enumerate() {
                        if index > 0 {
                            writeln!(self.writer)?;
                        }
                        self.write_list_indent()?;
                        traversal.visit_block(self, block)?;
                    }
                    Ok(())
                })();
                self.list_depth -= 1;
                result?;
            }
        }
        Ok(())
    }
}

impl<'a, W: Write> MarkdownVisitor<'a, '_, W> {
    fn visit_regular_description_list(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        list: DescriptionListRef<'a>,
    ) -> Result<(), Error> {
        self.write_list_indent()?;
        self.write_warning(
            "description-list-fallback",
            "description lists",
            "using regular list",
        )?;
        for item in list.items() {
            self.write_list_indent()?;
            write!(self.writer, "- **")?;
            self.write_content(traversal, item.term())?;
            writeln!(self.writer, "**")?;

            if !item.principal().is_empty() {
                self.write_list_indent()?;
                write!(self.writer, "  ")?;
                self.write_content(traversal, item.principal())?;
                writeln!(self.writer)?;
            }
            self.visit_description_blocks(traversal, item.blocks())?;
        }
        Ok(())
    }

    fn visit_horizontal_description_list(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        list: DescriptionListRef<'a>,
    ) -> Result<(), Error> {
        for item in list.items() {
            self.write_list_indent()?;
            write!(self.writer, "**")?;
            self.write_content(traversal, item.term())?;
            write!(self.writer, "**")?;
            if !item.principal().is_empty() {
                write!(self.writer, " — ")?;
                self.write_content(traversal, item.principal())?;
            }
            writeln!(self.writer, "  ")?;

            for block in item.blocks() {
                if !matches!(
                    block.kind(),
                    BlockKind::DescriptionList(_)
                        | BlockKind::OrderedList(_)
                        | BlockKind::UnorderedList(_)
                ) {
                    self.write_list_indent()?;
                }
                traversal.visit_block(self, block)?;
            }
        }
        Ok(())
    }

    fn visit_qanda_description_list(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        list: DescriptionListRef<'a>,
    ) -> Result<(), Error> {
        let item_count = list.items().count();
        let (start, reversed) = list_sequence(list.attributes(), item_count);
        for (index, item) in list.items().enumerate() {
            let number = if reversed {
                start.saturating_sub(index)
            } else {
                start.saturating_add(index)
            };
            self.write_list_indent()?;
            write!(self.writer, "{number}. <em>")?;
            self.write_content(traversal, item.term())?;
            writeln!(self.writer, "</em>")?;

            if !item.principal().is_empty() {
                writeln!(self.writer)?;
                self.write_list_indent()?;
                write!(self.writer, "    ")?;
                self.write_content(traversal, item.principal())?;
                writeln!(self.writer)?;
            }
            if item.blocks().next().is_some() {
                if item.principal().is_empty() {
                    writeln!(self.writer)?;
                }
                self.visit_description_blocks(traversal, item.blocks())?;
            }
            if index + 1 < item_count {
                writeln!(self.writer)?;
            }
        }
        Ok(())
    }

    fn visit_description_blocks(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        blocks: impl Iterator<Item = BlockRef<'a>>,
    ) -> Result<(), Error> {
        for block in blocks {
            self.list_depth += 1;
            let result = (|| {
                if !matches!(
                    block.kind(),
                    BlockKind::DescriptionList(_)
                        | BlockKind::OrderedList(_)
                        | BlockKind::UnorderedList(_)
                ) {
                    self.write_list_indent()?;
                }
                traversal.visit_block(self, block)
            })();
            self.list_depth -= 1;
            result?;
        }
        Ok(())
    }

    fn write_macro_link(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        destination: &str,
        default_label: &str,
        label: ContentRef<'_>,
        tooltip: Option<ContentRef<'_>>,
    ) -> Result<(), Error> {
        self.write_link_with_tooltip(destination, tooltip, |visitor| {
            if label.is_empty() {
                write!(visitor.writer, "{}", Self::escape_markdown(default_label))?;
            } else {
                visitor.write_content(traversal, label)?;
            }
            Ok(())
        })
    }

    fn write_link_macro_node(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        link: LinkRef<'_>,
    ) -> Result<(), Error> {
        let target = link.target().to_string();
        if link.text().inlines().any(is_linked_image) {
            for node in link.text().inlines() {
                if is_linked_image(node) {
                    self.write_inline(traversal, node)?;
                } else {
                    self.write_link_with_tooltip(&target, link.tooltip(), |visitor| {
                        visitor.write_inline(traversal, node)
                    })?;
                }
            }
            return Ok(());
        }

        let fallback = link_fallback(&target, link.hides_uri_scheme());
        self.write_macro_link(traversal, &target, fallback, link.text(), link.tooltip())
    }

    fn write_keyboard(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        keyboard: KeyboardRef<'_>,
    ) -> Result<(), Error> {
        for (index, key) in keyboard.keys().enumerate() {
            if index > 0 {
                write!(self.writer, "+")?;
            }
            write!(self.writer, "<kbd>")?;
            self.write_content(traversal, key)?;
            write!(self.writer, "</kbd>")?;
        }
        Ok(())
    }

    fn write_inline_image(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        image: InlineImageRef<'_>,
    ) -> Result<(), Error> {
        let alt = acdc_converters_core::media::image_alt_from_content(image.source(), image.alt());
        self.write_image(
            traversal,
            image.source(),
            image.attributes(),
            &alt,
            image
                .tooltip()
                .map(|value| {
                    InlineTextTransform::default()
                        .rendered()
                        .decode_char_refs(true)
                        .content_to_string(value)
                })
                .as_deref(),
        )?;
        Ok(())
    }

    fn write_inline_icon(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        icon_macro: IconRef<'_>,
    ) -> Result<(), Error> {
        let render = |visitor: &mut Self| {
            write!(visitor.writer, "[")?;
            if let Some(alt) = icon_macro.alt() {
                visitor.write_content(traversal, alt)?;
            } else {
                write!(
                    visitor.writer,
                    "{}",
                    Self::escape_markdown(&icon::alt_from_content(icon_macro.target(), None))
                )?;
            }
            write!(visitor.writer, "]")?;
            Ok(())
        };
        if let Some(link) = attribute_text(icon_macro.attributes(), "link") {
            self.write_link_with_tooltip(link, icon_macro.tooltip(), render)?;
        } else {
            let mut render = render;
            render(self)?;
        }
        Ok(())
    }

    fn write_formatting_span(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        node: InlineRef<'_>,
    ) -> Result<(), Error> {
        if let InlineKind::Bold(text) = node.kind() {
            self.write_inline_anchor(text.id().map(ElementIdRef::as_str))?;
            let role = self.write_role_start(text.role())?;
            self.write_emphasis(traversal, text.content(), "**", "strong")?;
            self.write_role_end(role)?;
        } else if let InlineKind::Italic(text) = node.kind() {
            self.write_inline_anchor(text.id().map(ElementIdRef::as_str))?;
            let role = self.write_role_start(text.role())?;
            self.write_emphasis(traversal, text.content(), "*", "em")?;
            self.write_role_end(role)?;
        } else if let InlineKind::Monospace(text) = node.kind() {
            self.write_inline_anchor(text.id().map(ElementIdRef::as_str))?;
            let role = self.write_role_start(text.role())?;
            let content = InlineTextTransform::default()
                .line_break("\n")
                .decode_char_refs(true)
                .content_to_string(text.content());
            self.write_code_span(&content)?;
            self.write_role_end(role)?;
        } else if let InlineKind::Highlight(text) = node.kind() {
            self.write_inline_anchor(text.id().map(ElementIdRef::as_str))?;
            let role = self.write_role_start(text.role())?;
            if role.is_none() {
                write!(self.writer, "<mark>")?;
            }
            self.write_content(traversal, text.content())?;
            if role.is_none() {
                write!(self.writer, "</mark>")?;
            }
            self.write_role_end(role)?;
        } else if let InlineKind::Subscript(text) = node.kind() {
            self.write_inline_anchor(text.id().map(ElementIdRef::as_str))?;
            let role = self.write_role_start(text.role())?;
            write!(self.writer, "<sub>")?;
            self.write_content(traversal, text.content())?;
            write!(self.writer, "</sub>")?;
            self.write_role_end(role)?;
        } else if let InlineKind::Superscript(text) = node.kind() {
            self.write_inline_anchor(text.id().map(ElementIdRef::as_str))?;
            let role = self.write_role_start(text.role())?;
            write!(self.writer, "<sup>")?;
            self.write_content(traversal, text.content())?;
            write!(self.writer, "</sup>")?;
            self.write_role_end(role)?;
        } else if let InlineKind::CurvedQuotation(text) = node.kind() {
            self.write_inline_anchor(text.id().map(ElementIdRef::as_str))?;
            let role = self.write_role_start(text.role())?;
            write!(self.writer, "“")?;
            self.write_content(traversal, text.content())?;
            write!(self.writer, "”")?;
            self.write_role_end(role)?;
        } else if let InlineKind::CurvedApostrophe(text) = node.kind() {
            self.write_inline_anchor(text.id().map(ElementIdRef::as_str))?;
            let role = self.write_role_start(text.role())?;
            write!(self.writer, "‘")?;
            self.write_content(traversal, text.content())?;
            write!(self.writer, "’")?;
            self.write_role_end(role)?;
        } else {
            self.warn_unknown_inline_node_ref(node);
        }
        Ok(())
    }

    fn write_footnote(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        footnote: FootnoteRef<'_>,
    ) -> Result<(), Error> {
        let id = self.record_footnote(traversal, footnote)?;
        if self.in_link_text {
            write!(self.writer, "[{}]", footnote.number())?;
        } else if self.variant() == MarkdownVariant::GitHubFlavored {
            write!(self.writer, "[^{id}]")?;
        } else {
            write!(self.writer, "<sup>")?;
            self.write_anchor_link(&format!("_footnote_{id}"), |visitor| {
                write!(visitor.writer, "[{}]", footnote.number())?;
                Ok(())
            })?;
            write!(self.writer, "</sup>")?;
        }
        Ok(())
    }

    fn write_autolink(&mut self, autolink: AutolinkRef<'_>) -> Result<(), Error> {
        let target = autolink.target().to_string();
        let (fallback, angle_brackets) = autolink_fallback(
            &target,
            autolink.is_bracketed(),
            autolink.hides_uri_scheme(),
        );
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
        Ok(())
    }

    fn write_inline_node_content(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        node: InlineRef<'_>,
    ) -> Result<(), Error> {
        match node.kind() {
            InlineKind::Plain(text) => {
                self.write_escaped_text(text.text())?;
            }
            InlineKind::Symbol(symbol) => self.write_escaped_text(symbol.text())?,
            InlineKind::CharacterReference(reference) => {
                write!(self.writer, "{}", reference.kind())?;
            }
            InlineKind::LineBreak(_) => writeln!(self.writer, "  ")?,
            InlineKind::Raw(text) => {
                self.write_passthrough(text.text(), || text.substitutions())?;
            }
            InlineKind::Verbatim(text) => self.write_code_span(text.text())?,
            InlineKind::StandaloneCurvedApostrophe(_) => {
                write!(self.writer, "’")?;
            }
            InlineKind::Anchor(anchor) => {
                self.write_inline_anchor_node(traversal, anchor)?;
            }
            InlineKind::Callout(callout) => {
                write!(self.writer, "({})", callout.number())?;
            }
            InlineKind::Bold(_)
            | InlineKind::Italic(_)
            | InlineKind::Monospace(_)
            | InlineKind::Highlight(_)
            | InlineKind::Subscript(_)
            | InlineKind::Superscript(_)
            | InlineKind::CurvedQuotation(_)
            | InlineKind::CurvedApostrophe(_) => {
                self.write_formatting_span(traversal, node)?;
            }
            InlineKind::Link(link) => self.write_link_macro_node(traversal, link)?,
            InlineKind::Image(image) => self.write_inline_image(traversal, image)?,
            InlineKind::Icon(icon_macro) => self.write_inline_icon(traversal, icon_macro)?,
            InlineKind::Keyboard(keyboard) => {
                self.write_keyboard(traversal, keyboard)?;
            }
            InlineKind::Button(button) => {
                write!(self.writer, "<strong>[")?;
                self.write_content(traversal, button.label())?;
                write!(self.writer, "]</strong>")?;
            }
            InlineKind::Menu(menu) => {
                self.write_content(traversal, menu.target())?;
                for item in menu.items() {
                    write!(self.writer, " > ")?;
                    self.write_content(traversal, item)?;
                }
            }
            InlineKind::Footnote(footnote) => self.write_footnote(traversal, footnote)?,
            InlineKind::Url(url) => {
                let target = url.target().to_string();
                let fallback = link_fallback(&target, url.hides_uri_scheme());
                self.write_macro_link(traversal, &target, fallback, url.text(), url.tooltip())?;
            }
            InlineKind::Mailto(mailto) => {
                let target = mailto.target().to_string();
                let destination = mailto_target(mailto);
                self.write_macro_link(
                    traversal,
                    &destination,
                    mailto_fallback(&target),
                    mailto.text(),
                    mailto.tooltip(),
                )?;
            }
            InlineKind::Autolink(autolink) => self.write_autolink(autolink)?,
            InlineKind::CrossReference(xref) => self.visit_cross_reference(traversal, xref)?,
            InlineKind::IndexTerm(term) => self.visit_index_term(traversal, term)?,
            InlineKind::Pass(pass) => {
                if let Some(text) = pass.text() {
                    self.write_passthrough(text, || pass.substitutions())?;
                }
            }
            InlineKind::Stem(stem) => {
                self.warn_once(
                    "inline-stem",
                    "inline STEM is not supported by the Markdown backend; preserving the expression as code",
                    "Use a Markdown renderer with a math extension, or use a backend that supports STEM rendering.",
                );
                self.write_code_span(stem.text())?;
            }
            _ => {
                self.warn_unknown_inline_node_ref(node);
            }
        }
        Ok(())
    }

    /// Render a cross-reference as a local anchor or interdocument link.
    ///
    /// Local references link to the `#id` fragment. Interdocument references
    /// link to the corresponding Markdown output. The text is the reference's
    /// own text when it has one, otherwise the target's reference text.
    fn visit_cross_reference(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        xref: CrossReferenceRef<'_>,
    ) -> Result<(), Error> {
        let target = xref.target();
        if !xref.text().is_empty() {
            if !xref.target_is_local()
                && let Some((destination, _)) = Self::interdocument_xref(traversal, target)
            {
                return self.write_link(&destination, |visitor| {
                    visitor.write_content(traversal, xref.text())?;
                    Ok(())
                });
            }
            return self.write_anchor_link(target, |visitor| {
                visitor.write_content(traversal, xref.text())?;
                Ok(())
            });
        }

        // Clone the handles so the borrowed reference text and the resolution
        // guard both outlive the `&mut self` render calls.
        let references = Rc::clone(&self.processor.references);
        let guard = self.processor.xref_guard.clone();
        self.with_index_collection(false, |visitor| {
            match resolve_reference(references.get(target).map(ReferenceRef::from), xref, &guard) {
                XrefDisplay::Title(inlines, _scope) | XrefDisplay::Label(inlines, _scope) => {
                    visitor.write_anchor_link(target, |visitor| {
                        visitor.write_content(traversal, inlines)?;
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
                        visitor.write_content(traversal, inlines)?;
                        write!(visitor.writer, "”")?;
                        Ok(())
                    })
                }
                XrefDisplay::Emphasized(prefix, inlines, _scope) => {
                    visitor.write_anchor_link(target, |visitor| {
                        if let Some(prefix) = prefix {
                            write!(visitor.writer, "{prefix}, ")?;
                        }
                        write!(visitor.writer, "*")?;
                        visitor.write_content(traversal, inlines)?;
                        write!(visitor.writer, "*")?;
                        Ok(())
                    })
                }
                XrefDisplay::Fallback(text) | XrefDisplay::Unresolved(text) => visitor
                    .write_anchor_link(target, |visitor| {
                        write!(visitor.writer, "{text}")?;
                        Ok(())
                    }),
                XrefDisplay::External(target) => {
                    if let Some((destination, text)) = Self::interdocument_xref(traversal, &target)
                    {
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
        self.write_link_with_tooltip(destination, None, text)
    }

    fn write_link_with_tooltip(
        &mut self,
        destination: &str,
        tooltip: Option<ContentRef<'_>>,
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
        write!(self.writer, "]({}", escape_link_destination(destination))?;
        if let Some(tooltip) = tooltip {
            let title = InlineTextTransform::default()
                .rendered()
                .decode_char_refs(true)
                .content_to_string(tooltip);
            write!(self.writer, " \"{}\"", escape_link_title(&title))?;
        }
        write!(self.writer, ")")?;
        Ok(())
    }

    fn write_image(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        source: SourceRef<'_>,
        attributes: AttributesRef<'_>,
        alt: &str,
        title: Option<&str>,
    ) -> Result<(), Error> {
        if let Some(link) = attribute_text(attributes, "link") {
            self.write_link(link, |visitor| {
                visitor.write_image_markup(traversal, source, attributes, alt, title)
            })
        } else {
            self.write_image_markup(traversal, source, attributes, alt, title)
        }
    }

    fn write_image_markup(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        source: SourceRef<'_>,
        attributes: AttributesRef<'_>,
        alt: &str,
        title: Option<&str>,
    ) -> Result<(), Error> {
        let target = resolve_target(&source.to_string(), traversal);
        let width = attribute_text(attributes, "width");
        let height = attribute_text(attributes, "height");
        if width.is_some() || height.is_some() {
            write!(
                self.writer,
                "<img src=\"{}\" alt=\"{}\"",
                escape_html_attribute(&target),
                escape_html_attribute(alt)
            )?;
            if let Some(title) = title {
                write!(self.writer, " title=\"{}\"", escape_html_attribute(title))?;
            }
            if let Some(width) = width {
                write!(self.writer, " width=\"{}\"", escape_html_attribute(width))?;
            }
            if let Some(height) = height {
                write!(self.writer, " height=\"{}\"", escape_html_attribute(height))?;
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
            write!(self.writer, " \"{}\"", escape_link_title(title))?;
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

    fn interdocument_xref(
        traversal: &TraversalContext<'a>,
        target: &str,
    ) -> Option<(String, String)> {
        let attributes = &traversal;
        let extension = attributes
            .get("relfilesuffix")
            .and_then(|value| value.text())
            .or_else(|| {
                attributes
                    .get("outfilesuffix")
                    .and_then(|value| value.text())
            })
            .unwrap_or_else(|| MARKDOWN_BACKEND.outfilesuffix());
        interdocument_xref(target, extension.strip_prefix('.').unwrap_or(extension))
    }

    fn write_list_indent(&mut self) -> Result<(), Error> {
        for _ in 0..self.list_depth {
            write!(self.writer, "    ")?;
        }
        Ok(())
    }

    fn visit_list_items(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        items: impl Iterator<Item = ListItemRef<'a>>,
        marker: &str,
        start: usize,
        reversed: bool,
        markerless: bool,
    ) -> Result<(), Error> {
        for (index, item) in items.enumerate() {
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

            let is_task = item.checked().is_some();
            let is_checked = matches!(item.checked(), Some(ListItemCheckedStatus::Checked));

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

            self.write_content(traversal, item.principal())?;
            if markerless {
                writeln!(self.writer, "  ")?;
            } else {
                writeln!(self.writer)?;
            }

            for block in item.blocks() {
                if matches!(
                    block.kind(),
                    BlockKind::OrderedList(_) | BlockKind::UnorderedList(_)
                ) {
                    if markerless {
                        writeln!(self.writer)?;
                    } else {
                        self.list_depth += 1;
                    }
                    let result = traversal.visit_block(self, block);
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
                    traversal.visit_block(self, block)?;
                }
            }
        }
        Ok(())
    }

    fn visit_table_inner(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        table: TableRef<'a>,
        attributes: AttributesRef<'_>,
    ) -> Result<(), Error> {
        if self.variant() == MarkdownVariant::CommonMark {
            self.write_warning(
                "commonmark-tables",
                "tables",
                "not supported in CommonMark, skipping",
            )?;
            return Ok(());
        }

        self.warn_gfm_table_fallbacks(table, attributes);
        self.render_gfm_table(traversal, table)
    }

    fn warn_gfm_table_fallbacks(&mut self, table: TableRef<'a>, attributes: AttributesRef<'_>) {
        if table.header().is_none() && (table.rows().next().is_some() || table.footer().is_some()) {
            self.warn_once(
                "gfm-headerless-tables",
                "headerless tables are not supported by GFM; adding an empty header row and preserving every source row as data",
                "Use an explicit header row when the destination requires strict GFM table semantics.",
            );
        }
        if table.footer().is_some() {
            self.warn_once(
                "gfm-table-footers",
                "table footers are not supported by GFM; rendering each footer as the final body row",
                "Use an HTML-capable backend when the footer must remain structurally distinct.",
            );
        }
        if has_spans(table) {
            self.warn_once(
                "gfm-table-spans",
                "table cell spans are not supported by GFM; leaving spanned positions empty while preserving cell text",
                "Use an HTML-capable backend when merged cells are required.",
            );
        }
        if table
            .columns()
            .any(|column| column.style() != ColumnStyle::Default)
            || table_cells(table).any(|cell| {
                cell.style()
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
            cell.blocks().count() > 1
                || cell
                    .blocks()
                    .any(|block| !matches!(block.kind(), BlockKind::Paragraph(_)))
        }) {
            self.warn_once(
                "gfm-nested-table-blocks",
                "nested table cell blocks are not supported by GFM; flattening block boundaries with HTML line breaks",
                "Use an HTML-capable backend when cells must retain nested block structure.",
            );
        }
        if attributes.get("width").is_some()
            || has_option(attributes, "autowidth")
            || table
                .columns()
                .any(|column| column.width() != ColumnWidth::default())
        {
            self.warn_once(
                "gfm-table-widths",
                "table width metadata is not supported by GFM; preserving the table without fixed widths",
                "Use an HTML-capable backend when table or column widths must remain exact.",
            );
        }
        if attributes.get("align").is_some()
            || attributes.get("float").is_some()
            || table
                .columns()
                .any(|column| column.valign() != VerticalAlignment::Top)
            || table_cells(table).any(|cell| cell.halign().is_some() || cell.valign().is_some())
        {
            self.warn_once(
                "gfm-table-local-alignment",
                "table-level and per-cell alignment are not supported by GFM; preserving column-level horizontal alignment",
                "Use column alignment for GFM, or an HTML-capable backend for table-level and per-cell alignment.",
            );
        }
    }

    fn render_gfm_table(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        table: TableRef<'a>,
    ) -> Result<(), Error> {
        if table.header().is_none() && table.rows().next().is_none() && table.footer().is_none() {
            return Ok(());
        }

        let column_count = column_count(table);
        let grid = grid_rows(table, column_count);
        if let Some(header) = grid.iter().find(|row| row.is_header) {
            self.write_gfm_table_row(traversal, header, table)?;
        } else {
            write!(self.writer, "|")?;
            for _ in 0..column_count {
                write!(self.writer, "  |")?;
            }
            writeln!(self.writer)?;
        }

        write!(self.writer, "|")?;
        let mut columns = table.columns();
        for _ in 0..column_count {
            let alignment = columns
                .next()
                .map_or(HorizontalAlignment::Left, ColumnFormatRef::halign);
            let marker = match alignment {
                HorizontalAlignment::Left => ":---",
                HorizontalAlignment::Center => ":---:",
                HorizontalAlignment::Right => "---:",
            };
            write!(self.writer, " {marker} |")?;
        }
        writeln!(self.writer)?;

        for row in grid.iter().filter(|row| !row.is_header) {
            self.write_gfm_table_row(traversal, row, table)?;
        }
        Ok(())
    }

    fn write_gfm_table_row(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        row: &GridRow<TableRowRef<'a>>,
        table: TableRef<'a>,
    ) -> Result<(), Error> {
        write!(self.writer, "|")?;
        let mut columns = table.columns();
        let mut cells = row.ast_row.cells().enumerate();
        for cell_kind in &row.cells {
            let column = columns.next();
            write!(self.writer, " ")?;
            if let CellKind::Content { cell_index } = cell_kind
                && let Some((_, cell)) = cells.find(|(index, _)| index == cell_index)
            {
                let style = cell.style().or_else(|| {
                    column.and_then(|column| {
                        (column.style() != ColumnStyle::Default).then_some(column.style())
                    })
                });
                self.write_gfm_table_cell(traversal, cell, style)?;
            }
            write!(self.writer, " |")?;
        }
        writeln!(self.writer)?;
        Ok(())
    }

    fn write_gfm_table_cell(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        cell: TableCellRef<'a>,
        style: Option<ColumnStyle>,
    ) -> Result<(), Error> {
        let mut buffer = Vec::new();
        let (result, emitted_anchors, footnotes) = {
            let mut visitor =
                MarkdownVisitor::new(&mut buffer, self.processor, self.diagnostics.reborrow());
            visitor.heading_level = self.heading_level;
            visitor.emitted_anchors = take(&mut self.emitted_anchors);
            visitor.collect_index_terms = self.collect_index_terms;
            visitor
                .current_section_title
                .clone_from(&self.current_section_title);
            visitor.footnotes = take(&mut self.footnotes);
            let scoped = style == Some(ColumnStyle::AsciiDoc);
            let result = if scoped {
                traversal.with_table_cell(cell, |traversal| {
                    visitor.visit_separated_blocks(traversal, cell.blocks(), false)
                })
            } else {
                visitor.visit_separated_blocks(traversal, cell.blocks(), false)
            };
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

    /// Preserve literal text in Markdown prose.
    fn escape_markdown(text: &str) -> String {
        let mut result = String::with_capacity(text.len());
        for ch in text.chars() {
            match ch {
                '\\' | '`' | '*' | '_' | '[' | ']' | '|' | '<' | '>' => {
                    result.push('\\');
                    result.push(ch);
                }
                _ => result.push(ch),
            }
        }
        result
    }
}

impl<'a, W: Write> acdc_converters_core::visitor::InlineVisitor<'a> for MarkdownVisitor<'a, '_, W> {
    type Error = Error;

    fn visit_inline_node(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        node: InlineRef<'_>,
    ) -> Result<(), Error> {
        self.write_inline_node_content(traversal, node)
    }
}

impl<'a, W: Write> BlockVisitor<'a> for MarkdownVisitor<'a, '_, W> {
    type Error = Error;

    fn visit_block(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        block: BlockRef<'a>,
    ) -> Result<(), Error> {
        match block.kind() {
            BlockKind::Section(node) => self.visit_section(traversal, node),
            BlockKind::Paragraph(node) => self.visit_paragraph(traversal, node),
            BlockKind::UnorderedList(node) => self.visit_unordered_list(traversal, node),
            BlockKind::OrderedList(node) => self.visit_ordered_list(traversal, node),
            BlockKind::DescriptionList(node) => self.visit_description_list(traversal, node),
            BlockKind::CalloutList(node) => self.visit_callout_list(traversal, node),
            BlockKind::Admonition(node) => self.visit_admonition(traversal, node),
            BlockKind::DiscreteHeader(node) => self.visit_discrete_header(traversal, node),
            BlockKind::Image(node) => self.visit_image(traversal, node),
            BlockKind::Audio(node) => self.visit_audio(traversal, node),
            BlockKind::Video(node) => self.visit_video(traversal, node),
            BlockKind::ThematicBreak(node) => self.visit_thematic_break(traversal, node),
            BlockKind::PageBreak(node) => self.visit_page_break(traversal, node),
            BlockKind::TableOfContents(node) => self.visit_table_of_contents(traversal, node),
            BlockKind::DocumentAttribute(_) | BlockKind::Comment(_) => Ok(()),
            BlockKind::DelimitedComment(_)
            | BlockKind::Example(_)
            | BlockKind::Listing(_)
            | BlockKind::Literal(_)
            | BlockKind::Open(_)
            | BlockKind::Sidebar(_)
            | BlockKind::Table(_)
            | BlockKind::Passthrough(_)
            | BlockKind::Quote(_)
            | BlockKind::Verse(_)
            | BlockKind::Stem(_) => self.visit_compound_block(traversal, block),
            _ => self.visit_unhandled_block(traversal, block),
        }
    }
}

impl<'a, W: Write> DocumentVisitor<'a> for MarkdownVisitor<'a, '_, W> {
    fn visit_document(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        document: DocumentRef<'a>,
    ) -> Result<(), Error> {
        self.render_document(traversal, document)
    }
}
