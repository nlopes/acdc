//! Visitor implementation for terminal output.

use std::io::Write;

use acdc_converters_core::{
    Diagnostics, TraversalContext,
    substitutions::TextBoundaries,
    visitor::{BlockVisitor, DocumentVisitor},
};
use acdc_parser::{
    CaptionKind, SectionKind,
    ast::{
        AttributeValueRef, AttributesRef, BlockKind, BlockRef, ContentRef, DocumentRef,
        FootnoteDefinitionRef, HeaderRef, InlineKind, InlineRef,
    },
};
use crossterm::{
    QueueableCommand,
    style::{Color, PrintStyledContent, SetBackgroundColor, SetForegroundColor, Stylize},
};

use crate::{Converter, Error};

pub(crate) fn has_token(attributes: AttributesRef<'_>, name: &str, token: &str) -> bool {
    matches!(attributes.get(name), Some(AttributeValueRef::Tokens(tokens)) if tokens.iter().any(|value| value == token))
}

/// Terminal visitor that generates terminal output from `AsciiDoc` AST
pub(crate) struct TerminalVisitor<'a, 'd, W: Write> {
    pub(crate) writer: W,
    pub(crate) converter: &'d Converter<'a>,

    /// Per-conversion diagnostics handle.
    pub(crate) diagnostics: Diagnostics<'d>,
    /// Whether we are inside an inline formatting span (bold, italic, etc.).
    /// When true, em-dash boundary replacement at string start/end is suppressed.
    in_inline_span: bool,
    pub(crate) text_boundaries: TextBoundaries,
    /// Colors of the inline spans currently open, innermost last. A span
    /// restores the colors of the span around it on exit instead of resetting
    /// the terminal, so text after a nested span keeps the outer color.
    colors: Vec<SpanColors>,
    /// Colors already written to the writer, so only a channel that changes
    /// produces an escape sequence.
    applied_colors: SpanColors,
}

/// Foreground and background color of an inline span. `None` means the span
/// leaves that channel to the span around it.
type SpanColors = (Option<Color>, Option<Color>);

impl<'a, 'd, W: Write> TerminalVisitor<'a, 'd, W> {
    pub(crate) fn render_footnotes(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        footnotes: impl Iterator<Item = FootnoteDefinitionRef<'a>>,
    ) -> Result<(), Error> {
        let mut footnotes = footnotes.peekable();
        if footnotes.peek().is_some() {
            writeln!(self.writer)?;
            writeln!(self.writer, "─────")?;
            for footnote in footnotes {
                self.writer.queue(PrintStyledContent(
                    format!("[{}]", footnote.number())
                        .with(self.converter.appearance.colors.footnote)
                        .bold(),
                ))?;
                write!(self.writer, " ")?;

                self.write_content(traversal, footnote.content())?;
                writeln!(self.writer)?;
            }
        }
        Ok(())
    }

    pub(super) fn new(
        writer: W,
        converter: &'d Converter<'a>,
        diagnostics: Diagnostics<'d>,
    ) -> Self {
        Self {
            writer,
            converter,

            diagnostics,
            in_inline_span: false,
            text_boundaries: TextBoundaries::BOTH,
            colors: Vec::new(),
            applied_colors: (None, None),
        }
    }

    pub(crate) fn writer_mut(&mut self) -> &mut dyn Write {
        &mut self.writer
    }

    /// Warn that a block could not be rendered.
    fn visit_unhandled_block(
        &mut self,
        _traversal: &mut TraversalContext<'a>,
        _block: BlockRef<'a>,
    ) {
        self.warn_unsupported_parser_variant("block");
    }

    pub(crate) fn render_title_with_wrapper(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        title: Option<ContentRef<'_>>,
        prefix: &str,
        suffix: &str,
    ) -> Result<(), Error> {
        if let Some(title) = title {
            write!(self.writer_mut(), "{prefix}")?;
            self.write_content(traversal, title)?;
            write!(self.writer_mut(), "{suffix}")?;
        }
        Ok(())
    }

    pub(crate) fn write_content(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        content: ContentRef<'_>,
    ) -> Result<(), Error> {
        acdc_converters_core::visitor::InlineVisitor::visit_inline_nodes(self, traversal, content)
    }

    pub(crate) fn write_inline(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        node: InlineRef<'_>,
    ) -> Result<(), Error> {
        acdc_converters_core::visitor::InlineVisitor::visit_inline_node(self, traversal, node)
    }

    pub(crate) fn write_inlines<'inline>(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        nodes: impl DoubleEndedIterator<Item = InlineRef<'inline>> + ExactSizeIterator + Clone,
    ) -> Result<(), Error> {
        let previous_boundaries = self.text_boundaries;
        let siblings = nodes.clone();
        let result = (|| {
            for (index, node) in nodes.enumerate() {
                let boundaries = if self.in_inline_span {
                    TextBoundaries::NONE
                } else {
                    previous_boundaries
                };
                self.text_boundaries = boundaries
                    .with_ordinary_replacements(false)
                    .for_inline_refs(
                        Some(node),
                        siblings.clone().take(index).rev(),
                        siblings.clone().skip(index + 1),
                    );
                self.write_inline(traversal, node)?;
            }
            Ok(())
        })();
        self.text_boundaries = previous_boundaries;
        result
    }

    pub(crate) fn render_captioned_title_with_wrapper(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        title: Option<ContentRef<'_>>,
        attributes: AttributesRef<'_>,
        fallback: Option<CaptionKind>,
        prefix: &str,
        suffix: &str,
    ) -> Result<(), Error> {
        if title.is_none() {
            return Ok(());
        }
        if let Some(AttributeValueRef::Content(caption)) = attributes.get("caption") {
            write!(self.writer_mut(), "{prefix}")?;
            self.write_content(traversal, caption)?;
            return self.render_title_with_wrapper(traversal, title, "", suffix);
        }
        let caption = self
            .converter
            .caption_prefix(attributes, fallback)
            .unwrap_or_default();
        self.render_title_with_wrapper(traversal, title, &format!("{prefix}{caption}"), suffix)
    }

    /// Open an inline span that sets colors.
    ///
    /// Pair every call with [`Self::pop_colors`].
    pub(crate) fn push_colors(
        &mut self,
        fg: Option<Color>,
        bg: Option<Color>,
    ) -> Result<(), Error> {
        let outer = self.current_colors();
        let colors = (fg.or(outer.0), bg.or(outer.1));
        self.colors.push(colors);
        self.apply_colors(colors)
    }

    /// Close the span opened by [`Self::push_colors`], restoring the colors of
    /// the span around it.
    pub(crate) fn pop_colors(&mut self) -> Result<(), Error> {
        self.colors.pop();
        let colors = self.current_colors();
        self.apply_colors(colors)
    }

    fn current_colors(&self) -> SpanColors {
        self.colors.last().copied().unwrap_or((None, None))
    }

    /// Write the channels that change, using the terminal default for a channel
    /// no open span sets. `Color::Reset` addresses one channel only (`SGR 39` /
    /// `SGR 49`), unlike a full reset, so bold and italic survive.
    fn apply_colors(&mut self, colors: SpanColors) -> Result<(), Error> {
        let (fg, bg) = colors;
        let applied = self.applied_colors;
        self.applied_colors = colors;
        let w = self.writer_mut();
        if fg != applied.0 {
            w.queue(SetForegroundColor(fg.unwrap_or(Color::Reset)))?;
        }
        if bg != applied.1 {
            w.queue(SetBackgroundColor(bg.unwrap_or(Color::Reset)))?;
        }
        Ok(())
    }

    /// Consume the visitor and return the writer
    pub(super) fn into_writer(self) -> W {
        self.writer
    }

    pub(crate) fn warn_unsupported_parser_variant(&mut self, kind: &str) {
        self.diagnostics.warn_with_advice(
            format!("an unsupported parser {kind} variant was omitted from terminal output"),
            "Use another backend for this document and report the unsupported construct.",
        );
    }
}

impl<'a, W: Write> BlockVisitor<'a> for TerminalVisitor<'a, '_, W> {
    type Error = Error;
    fn visit_block(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        block: BlockRef<'a>,
    ) -> Result<(), Error> {
        match block.kind() {
            BlockKind::Section(section) => {
                let render_catalog = section.kind() == SectionKind::Index
                    && self.converter.has_valid_index_section();
                self.render_section(traversal, section)?;
                traversal.visit_blocks(self, section.blocks())?;
                if render_catalog {
                    let converter = self.converter;
                    crate::index::render(self.writer_mut(), converter)?;
                }
                Ok(())
            }
            BlockKind::Paragraph(node) => self.render_paragraph(traversal, node),
            BlockKind::OrderedList(node) => self.render_ordered_list(traversal, node),
            BlockKind::UnorderedList(node) => self.render_unordered_list(traversal, node),
            BlockKind::DescriptionList(node) => self.render_description_list(traversal, node),
            BlockKind::CalloutList(node) => self.render_callout_list(traversal, node),
            BlockKind::Admonition(node) => self.render_admonition(traversal, node),
            BlockKind::Image(node) => self.render_image(traversal, node),
            BlockKind::Audio(node) => self.render_audio(traversal, node),
            BlockKind::Video(node) => self.render_video(traversal, node),
            BlockKind::DiscreteHeading(node) => self.render_discrete_header(node),
            BlockKind::TableOfContents(node) => self.render_toc(traversal, Some(node), "macro"),
            BlockKind::ThematicBreak(_) => {
                writeln!(self.writer, "{}", "─".repeat(self.converter.terminal_width))?;
                Ok(())
            }
            BlockKind::PageBreak(_) => {
                writeln!(
                    self.writer,
                    "\n{}\n",
                    "═".repeat(self.converter.terminal_width)
                )?;
                Ok(())
            }
            BlockKind::DocumentAttribute(_) | BlockKind::Comment(_) => Ok(()),
            BlockKind::Example(_)
            | BlockKind::Open(_)
            | BlockKind::Sidebar(_)
            | BlockKind::Quote(_)
            | BlockKind::Verse(_)
            | BlockKind::Listing(_)
            | BlockKind::Literal(_)
            | BlockKind::Passthrough(_)
            | BlockKind::Table(_)
            | BlockKind::Stem(_)
            | BlockKind::DelimitedComment(_) => self.render_delimited_block(traversal, block),
            BlockKind::Extension(_) | _ => {
                self.visit_unhandled_block(traversal, block);
                Ok(())
            }
        }
    }
}

impl<'a, W: Write> DocumentVisitor<'a> for TerminalVisitor<'a, '_, W> {
    fn visit_header(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        header: HeaderRef<'a>,
    ) -> Result<(), Self::Error> {
        // In embedded mode, skip header output (title, authors, revision info)
        if self.converter.options.embedded() {
            return Ok(());
        }
        self.render_header(traversal, header)
    }

    fn visit_body_content_start(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        _doc: DocumentRef<'a>,
    ) -> Result<(), Self::Error> {
        self.render_toc(traversal, None, "auto")?;
        Ok(())
    }

    fn visit_preamble_end(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        _doc: DocumentRef<'a>,
    ) -> Result<(), Self::Error> {
        self.render_toc(traversal, None, "preamble")?;
        Ok(())
    }

    fn visit_document_supplements(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        doc: DocumentRef<'a>,
    ) -> Result<(), Self::Error> {
        self.render_footnotes(traversal, doc.footnotes())?;
        Ok(())
    }
}

impl<'a, W: Write> acdc_converters_core::visitor::InlineVisitor<'a> for TerminalVisitor<'a, '_, W> {
    type Error = Error;
    fn visit_inline_nodes(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        content: ContentRef<'_>,
    ) -> Result<(), Error> {
        self.write_inlines(traversal, content.inlines())
    }
    fn visit_inline_node(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        node: InlineRef<'_>,
    ) -> Result<(), Error> {
        let saved = self.in_inline_span;
        if matches!(
            node.kind(),
            InlineKind::Bold(_)
                | InlineKind::Italic(_)
                | InlineKind::Monospace(_)
                | InlineKind::Highlight(_)
                | InlineKind::Superscript(_)
                | InlineKind::Subscript(_)
                | InlineKind::DoubleQuotedSpan(_)
                | InlineKind::SingleQuotedSpan(_)
        ) {
            self.in_inline_span = true;
        }
        let result = self.render_inline_node(traversal, node);
        self.in_inline_span = saved;
        result
    }
}

#[cfg(test)]
mod tests;
