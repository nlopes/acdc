//! Visitor implementation for manpage (roff/troff) conversion.

use std::{io::Write, mem::replace};

use acdc_converters_core::{
    Diagnostics, TraversalContext, document_attribute_text,
    substitutions::TextBoundaries,
    visitor::{BlockVisitor, DocumentVisitor},
};
use acdc_parser::ast::{
    AttributeValueRef, AttributesRef, BlockKind, BlockRef, CaptionRef, ContentRef, DocumentRef,
    InlineKind, InlineRef,
};

use crate::{
    Error, Processor,
    escape::{EscapeMode, escape_roff_macro_argument, manify},
    inlines::LinkLabel,
};

pub(super) fn attribute_text<'doc>(
    attributes: AttributesRef<'doc>,
    name: &str,
) -> Option<&'doc str> {
    match attributes.get(name) {
        Some(AttributeValueRef::Text(value)) => Some(acdc_parser::strip_quotes(value)),
        _ => None,
    }
}

#[derive(Clone, Copy)]
pub(crate) enum TextCase {
    Preserve,
    Uppercase,
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum IndexCollection {
    Enabled,
    Disabled,
}

/// Manpage visitor that generates roff/troff output from `AsciiDoc` AST.
pub struct ManpageVisitor<'a, 'd, W: Write> {
    pub(crate) writer: W,
    pub(crate) processor: &'d Processor<'a>,

    /// Per-conversion diagnostics handle.
    pub(crate) diagnostics: Diagnostics<'d>,
    /// Current nesting depth for lists (used for .RS/.RE indentation).
    pub(crate) list_depth: usize,
    /// Whether the current section supplies the manpage name metadata.
    pub(crate) in_name_section: bool,
    /// Strip ASCII indentation from the next text node after a hard break.
    pub(crate) strip_next_leading_space: bool,
    /// Whether we are inside an inline formatting span (bold, italic, etc.).
    /// When true, em-dash boundary replacement at string start/end is suppressed.
    pub(crate) in_inline_span: bool,
    pub(crate) index_collection: IndexCollection,
    pub(crate) text_boundaries: TextBoundaries,
    /// Text casing applied while preserving inline markup.
    pub(crate) text_case: TextCase,
    pub(crate) text_escape_mode: EscapeMode,
    /// Buffer the current label so nested links can be emitted as separate commands.
    pub(crate) link_label: Option<LinkLabel>,
    /// Font selected by the enclosing generated formatting scope.
    pub(crate) current_font: &'static str,
    /// Title of the first level-1 section for name-section validation.
    first_section_title: Option<String>,
    /// Title of the second level-1 section (for SYNOPSIS validation).
    second_section_title: Option<String>,
}

impl<'a, 'd, W: Write> ManpageVisitor<'a, 'd, W> {
    /// Create a new manpage visitor.
    pub fn new(writer: W, processor: &'d Processor<'a>, diagnostics: Diagnostics<'d>) -> Self {
        Self {
            writer,
            processor,

            diagnostics,
            list_depth: 0,
            in_name_section: false,
            strip_next_leading_space: false,
            in_inline_span: false,
            index_collection: IndexCollection::Enabled,
            text_boundaries: TextBoundaries::BOTH,
            text_case: TextCase::Preserve,
            text_escape_mode: EscapeMode::Normalize,
            link_label: None,
            current_font: "\\fR",
            first_section_title: None,
            second_section_title: None,
        }
    }

    pub(crate) fn writer_mut(&mut self) -> &mut dyn Write {
        match self.link_label.as_mut() {
            Some(label) => &mut label.content,
            None => &mut self.writer,
        }
    }

    /// Warn that a block could not be rendered.
    ///
    /// # Errors
    ///
    /// This callback always succeeds after recording its warning.
    pub fn visit_unhandled_block(
        &mut self,
        _traversal: &mut TraversalContext<'a>,
        _block: BlockRef<'a>,
    ) -> Result<(), Error> {
        self.warn_unsupported_parser_variant("block");
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
        let context_nodes = nodes.clone();
        let result = (|| {
            let mut after_hard_break = false;
            for (i, node) in nodes.enumerate() {
                let boundaries = if self.in_inline_span {
                    TextBoundaries::NONE
                } else {
                    previous_boundaries
                };
                self.text_boundaries = boundaries
                    .with_ordinary_replacements(false)
                    .for_inline_refs(
                        Some(node),
                        context_nodes.clone().take(i).rev(),
                        context_nodes.clone().skip(i + 1),
                    );
                if after_hard_break
                    && !matches!(self.text_escape_mode, EscapeMode::Preserve)
                    && matches!(node.kind(), InlineKind::Plain(_) | InlineKind::Raw(_))
                {
                    self.strip_next_leading_space = true;
                }
                let invisible = matches!(node.kind(), InlineKind::Anchor(_))
                    || matches!(node.kind(), InlineKind::IndexTerm(term) if !term.is_visible());
                after_hard_break = matches!(node.kind(), InlineKind::LineBreak(_))
                    || (after_hard_break && invisible);

                self.write_inline(traversal, node)?;
            }
            Ok(())
        })();
        self.text_boundaries = previous_boundaries;
        result
    }

    /// Render `content` with the given casing, restoring the previous casing
    /// afterwards.
    pub(crate) fn with_text_case(
        &mut self,
        text_case: TextCase,
        content: impl FnOnce(&mut Self) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let previous = self.text_case;
        self.text_case = text_case;
        let result = content(self);
        self.text_case = previous;
        result
    }

    /// Create a visitor that renders into `writer` with this visitor's inline
    /// context.
    ///
    /// Copied text retains its casing, whitespace and index-registration policy, but has
    /// its own output and link-label state.
    pub(crate) fn nested_visitor<'w, W2: Write>(
        &mut self,
        writer: &'w mut W2,
    ) -> ManpageVisitor<'a, '_, &'w mut W2> {
        let processor = self.processor;
        let mut visitor = ManpageVisitor::new(writer, processor, self.diagnostics.reborrow());
        visitor.text_case = self.text_case;
        visitor.text_escape_mode = self.text_escape_mode;
        visitor.index_collection = self.index_collection;
        visitor.current_font = self.current_font;
        visitor
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

    pub(crate) fn warn_unsupported_parser_variant(&mut self, kind: &str) {
        self.diagnostics.warn_with_advice(
            format!("an unsupported parser {kind} variant was omitted from manpage output"),
            "Use another backend for this document and report the unsupported construct.",
        );
    }

    /// Write a blank line for spacing.
    pub(crate) fn write_sp(&mut self) -> Result<(), Error> {
        writeln!(self.writer, ".sp")?;
        Ok(())
    }

    pub(crate) fn render_title_inlines(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        title: Option<ContentRef<'_>>,
    ) -> Result<(), Error> {
        // Titles use collapsed whitespace; their bodies retain authored spacing.
        let previous = replace(&mut self.text_escape_mode, EscapeMode::Collapse);
        let result = title.map_or(Ok(()), |title| self.write_content(traversal, title));
        self.text_escape_mode = previous;
        result
    }

    /// Render a block title with the caption resolved by the parser.
    pub(crate) fn render_captioned_title(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        title: Option<ContentRef<'_>>,
        attributes: AttributesRef<'_>,
    ) -> Result<(), Error> {
        if title.is_none() {
            return Ok(());
        }

        let prefix = match CaptionRef::resolve(
            attributes,
            self.processor.parser_options.document_attributes().into(),
            None,
        ) {
            Some(CaptionRef::Numbered {
                label,
                number: Some(number),
                ..
            }) => Some(format!("{label} {number}. ")),
            Some(CaptionRef::Custom(prefix)) => Some(prefix.to_string()),
            Some(_) | None => None,
        };
        self.render_font("\\fB", false, |visitor| {
            if let Some(AttributeValueRef::Content(content)) = attributes.get("caption") {
                visitor.write_content(traversal, content)?;
            } else if let Some(prefix) = prefix {
                write!(
                    visitor.writer_mut(),
                    "{}",
                    manify(&prefix, EscapeMode::Collapse)
                )?;
            }
            visitor.render_title_inlines(traversal, title)
        })?;
        writeln!(self.writer_mut())?;
        writeln!(self.writer_mut(), ".br")?;
        Ok(())
    }
}

impl<'a, W: Write> BlockVisitor<'a> for ManpageVisitor<'a, '_, W> {
    type Error = Error;

    fn visit_block(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        block: BlockRef<'a>,
    ) -> Result<(), Error> {
        match block.kind() {
            BlockKind::Section(node) => self.render_section(traversal, node),
            BlockKind::Paragraph(node) => self.render_paragraph(traversal, node),
            BlockKind::OrderedList(node) => self.render_ordered_list(traversal, node),
            BlockKind::UnorderedList(node) => self.render_unordered_list(traversal, node),
            BlockKind::DescriptionList(node) => self.render_description_list(traversal, node),
            BlockKind::CalloutList(node) => self.render_callout_list(traversal, node),
            BlockKind::Admonition(node) => self.render_admonition(traversal, node),
            BlockKind::Image(node) => self.render_image(traversal, node),
            BlockKind::Audio(node) => self.render_audio(traversal, node),
            BlockKind::Video(node) => self.render_video(traversal, node),
            BlockKind::DiscreteHeader(node) => {
                self.write_sp()?;
                self.render_font("\\fB", false, |visitor| {
                    visitor.render_title_inlines(traversal, node.title())
                })?;
                writeln!(self.writer)?;
                Ok(())
            }
            BlockKind::ThematicBreak(_) => {
                self.write_sp()?;
                writeln!(self.writer, ".ce")?;
                writeln!(self.writer, "* * *")?;
                writeln!(self.writer, ".ce 0")?;
                Ok(())
            }
            BlockKind::PageBreak(_) => {
                writeln!(self.writer, ".bp")?;
                Ok(())
            }
            BlockKind::TableOfContents(_)
            | BlockKind::DocumentAttribute(_)
            | BlockKind::Comment(_) => Ok(()),
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
            _ => self.visit_unhandled_block(traversal, block),
        }
    }
}

impl<'a, W: Write> DocumentVisitor<'a> for ManpageVisitor<'a, '_, W> {
    fn visit_document_start(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        doc: DocumentRef<'a>,
    ) -> Result<(), Self::Error> {
        self.render_document_start(traversal, doc)
    }

    fn visit_document_supplements(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        doc: DocumentRef<'a>,
    ) -> Result<(), Self::Error> {
        // In embedded mode, skip NOTES and AUTHOR(S) sections (matches asciidoctor --embedded)
        if self.processor.options.embedded() {
            return Ok(());
        }

        // Render footnotes as NOTES section (matching asciidoctor)
        if doc.footnotes().next().is_some() {
            let w = self.writer_mut();
            writeln!(w, ".SH \"NOTES\"")?;

            for footnote in doc.footnotes() {
                let w = self.writer_mut();
                writeln!(w, ".IP [{}] 4", footnote.number())?;
                self.write_content(traversal, footnote.content())?;
                let w = self.writer_mut();
                writeln!(w)?;
            }
        }

        // Render AUTHOR(S) section if document has authors
        if let Some(header) = doc.header()
            && header.authors().next().is_some()
        {
            let w = self.writer_mut();
            if header.authors().count() == 1 {
                writeln!(w, ".SH \"AUTHOR\"")?;
            } else {
                writeln!(w, ".SH \"AUTHORS\"")?;
            }
            for author in header.authors() {
                let w = self.writer_mut();
                writeln!(w, ".sp")?;
                let name = crate::document::format_author_name(author);
                write!(w, "\\fB{}\\fP", manify(&name, EscapeMode::Normalize))?;
                if let Some(email) = author.email() {
                    let escaped_email = escape_roff_macro_argument(email).replace('@', "\\(at");
                    writeln!(w, " \\c\n.MTO \"{escaped_email}\" \"\" \"\"")?;
                } else {
                    writeln!(w)?;
                }
            }
        }

        Ok(())
    }

    fn visit_document_end(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        _doc: DocumentRef<'a>,
    ) -> Result<(), Self::Error> {
        // Validate manpage section order conventions
        const SECTION_ORDER_ADVICE: &str =
            "Manpage output conventionally starts with the name section followed by SYNOPSIS.";

        let name_section_title =
            document_attribute_text((traversal).get("manname-title")).unwrap_or("Name");
        if let Some(ref first) = self.first_section_title
            && !first.eq_ignore_ascii_case(name_section_title)
        {
            self.diagnostics.warn_with_advice(
                format!("manpage convention: name section should be first, got `{first}`"),
                SECTION_ORDER_ADVICE,
            );
        }
        if let Some(ref second) = self.second_section_title
            && !second.eq_ignore_ascii_case("SYNOPSIS")
        {
            self.diagnostics.warn_with_advice(
                format!(
                    "manpage convention: SYNOPSIS should be the second section, got `{second}`"
                ),
                SECTION_ORDER_ADVICE,
            );
        }
        Ok(())
    }
}

impl<'a, W: Write> acdc_converters_core::visitor::InlineVisitor<'a> for ManpageVisitor<'a, '_, W> {
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
    ) -> Result<(), Self::Error> {
        let saved = self.in_inline_span;
        if acdc_converters_core::visitor::is_formatting_span(node) {
            self.in_inline_span = true;
        }

        let result = self.render_inline_node(traversal, node);

        self.in_inline_span = saved;
        result
    }
}
