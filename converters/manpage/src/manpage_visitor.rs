//! Visitor implementation for manpage (roff/troff) conversion.

use std::io::Write;

#[cfg(feature = "pre-spec-subs")]
use acdc_converters_core::substitutions::{SubsFlags, effective_subs_flags};

use acdc_converters_core::{
    Diagnostics, TraversalContext, document_attribute_text,
    substitutions::TextBoundaries,
    visitor::{Visitor, WritableVisitor},
};
use acdc_parser::{
    Admonition, Audio, Block, BlockMetadata, CalloutList, Caption, DelimitedBlock, DescriptionList,
    DiscreteHeader, Document, Header, Image, InlineMacro, InlineNode, ListItem, OrderedList,
    PageBreak, Paragraph, Section, TableOfContents, ThematicBreak, UnorderedList, Video,
};

use crate::{
    Error, Processor,
    escape::{EscapeMode, escape_roff_macro_argument, manify},
    inlines::LinkLabel,
};

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
            first_section_title: None,
            second_section_title: None,
        }
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

    /// Render a block title with the caption resolved by the parser.
    pub(crate) fn render_captioned_title(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        title: &[InlineNode<'_>],
        metadata: &BlockMetadata<'_>,
    ) -> Result<(), Error> {
        if title.is_empty() {
            return Ok(());
        }

        // Inline formatting changes roff's previous font, so save the enclosing font.
        writeln!(self.writer_mut(), ".nr acdc-title-font \\n[.f]")?;
        write!(self.writer_mut(), "\\fB")?;
        let prefix = match metadata.caption.as_ref() {
            Some(Caption::Numbered {
                label,
                number: Some(number),
                ..
            }) => Some(format!("{label} {number}. ")),
            Some(Caption::Custom(prefix)) => Some(prefix.to_string()),
            Some(_) | None => None,
        };
        if let Some(prefix) = prefix {
            write!(
                self.writer_mut(),
                "{}",
                manify(&prefix, EscapeMode::Normalize)
            )?;
        }
        // A title uses normal substitutions even when its block's body disables them.
        #[cfg(feature = "pre-spec-subs")]
        let previous_subs = self
            .processor
            .current_subs
            .replace(effective_subs_flags(None, false));
        let result = self.visit_inline_nodes(traversal, title);
        #[cfg(feature = "pre-spec-subs")]
        self.processor.current_subs.set(previous_subs);
        result?;
        writeln!(self.writer_mut(), "\\f[\\n[acdc-title-font]]")?;
        writeln!(self.writer_mut(), ".br")?;
        writeln!(self.writer_mut(), ".rr acdc-title-font")?;
        Ok(())
    }
}

impl<'a, W: Write> Visitor<'a> for ManpageVisitor<'a, '_, W> {
    type Error = Error;

    fn visit_unhandled_block(
        &mut self,
        _traversal: &mut TraversalContext<'a>,
        _block: &'a Block<'a>,
    ) -> Result<(), Self::Error> {
        self.warn_unsupported_parser_variant("block");
        Ok(())
    }

    fn visit_document_start(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        doc: &'a Document<'a>,
    ) -> Result<(), Self::Error> {
        self.render_document_start(traversal, doc)
    }

    fn visit_document_supplements(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        doc: &'a Document<'a>,
    ) -> Result<(), Self::Error> {
        // In embedded mode, skip NOTES and AUTHOR(S) sections (matches asciidoctor --embedded)
        if self.processor.options.embedded() {
            return Ok(());
        }

        // Render footnotes as NOTES section (matching asciidoctor)
        if !doc.footnotes.is_empty() {
            let w = self.writer_mut();
            writeln!(w, ".SH \"NOTES\"")?;

            for footnote in &doc.footnotes {
                let w = self.writer_mut();
                writeln!(w, ".IP [{}] 4", footnote.number)?;
                self.visit_inline_nodes(traversal, &footnote.content)?;
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
        _doc: &'a Document<'a>,
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

    fn visit_header(
        &mut self,
        _traversal: &mut TraversalContext<'a>,
        _header: &Header,
    ) -> Result<(), Self::Error> {
        // Header is handled in visit_document_start for manpages
        // The .TH macro contains all header information
        Ok(())
    }

    fn visit_section(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        section: &'a Section<'a>,
    ) -> Result<(), Self::Error> {
        self.render_section(traversal, section)
    }

    fn visit_paragraph(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        para: &Paragraph,
    ) -> Result<(), Self::Error> {
        self.render_paragraph(traversal, para)
    }

    fn visit_delimited_block(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        block: &'a DelimitedBlock<'a>,
    ) -> Result<(), Self::Error> {
        self.render_delimited_block(traversal, block)
    }

    fn visit_ordered_list(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        list: &'a OrderedList<'a>,
    ) -> Result<(), Self::Error> {
        self.render_ordered_list(traversal, list)
    }

    fn visit_unordered_list(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        list: &'a UnorderedList<'a>,
    ) -> Result<(), Self::Error> {
        self.render_unordered_list(traversal, list)
    }

    fn visit_description_list(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        list: &'a DescriptionList<'a>,
    ) -> Result<(), Self::Error> {
        self.render_description_list(traversal, list)
    }

    fn visit_callout_list(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        list: &'a CalloutList<'a>,
    ) -> Result<(), Self::Error> {
        self.render_callout_list(traversal, list)
    }

    fn visit_list_item(
        &mut self,
        _traversal: &mut TraversalContext<'a>,
        _item: &'a ListItem<'a>,
    ) -> Result<(), Self::Error> {
        // List items are handled by their parent list visitors
        Ok(())
    }

    fn visit_admonition(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        admon: &'a Admonition<'a>,
    ) -> Result<(), Self::Error> {
        self.render_admonition(traversal, admon)
    }

    fn visit_image(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        img: &Image,
    ) -> Result<(), Self::Error> {
        self.render_image(traversal, img)
    }

    fn visit_video(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        video: &Video,
    ) -> Result<(), Self::Error> {
        self.render_video(traversal, video)
    }

    fn visit_audio(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        audio: &Audio,
    ) -> Result<(), Self::Error> {
        self.render_audio(traversal, audio)
    }

    fn visit_thematic_break(
        &mut self,
        _traversal: &mut TraversalContext<'a>,
        _br: &ThematicBreak,
    ) -> Result<(), Self::Error> {
        // Thematic break as a centered line of dashes
        self.write_sp()?;
        writeln!(self.writer, ".ce")?;
        writeln!(self.writer, "* * *")?;
        writeln!(self.writer, ".ce 0")?;
        Ok(())
    }

    fn visit_page_break(
        &mut self,
        _traversal: &mut TraversalContext<'a>,
        _br: &PageBreak,
    ) -> Result<(), Self::Error> {
        // Page break in roff
        writeln!(self.writer, ".bp")?;
        Ok(())
    }

    fn visit_table_of_contents(
        &mut self,
        _traversal: &mut TraversalContext<'a>,
        _toc: &TableOfContents,
    ) -> Result<(), Self::Error> {
        // Man pages do not include a table of contents.
        Ok(())
    }

    fn visit_discrete_header(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        header: &DiscreteHeader,
    ) -> Result<(), Self::Error> {
        // Discrete headers are rendered as bold text, not as sections
        self.write_sp()?;
        write!(self.writer, "\\fB")?;
        self.visit_inline_nodes(traversal, &header.title)?;
        writeln!(self.writer, "\\fP")?;
        Ok(())
    }

    fn visit_inline_nodes(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        nodes: &[InlineNode],
    ) -> Result<(), Self::Error> {
        let previous_boundaries = self.text_boundaries;
        #[cfg(feature = "pre-spec-subs")]
        let replacements = self
            .processor
            .current_subs
            .get()
            .contains(SubsFlags::REPLACEMENTS);
        #[cfg(not(feature = "pre-spec-subs"))]
        let replacements = true;

        let result = (|| {
            let mut after_hard_break = false;
            for (i, node) in nodes.iter().enumerate() {
                let boundaries = if self.in_inline_span {
                    TextBoundaries::NONE
                } else {
                    previous_boundaries
                };
                self.text_boundaries = boundaries
                    .with_ordinary_replacements(replacements)
                    .for_inline(nodes, i);
                if after_hard_break
                    && !matches!(self.text_escape_mode, EscapeMode::Preserve)
                    && matches!(node, InlineNode::PlainText(_) | InlineNode::RawText(_))
                {
                    self.strip_next_leading_space = true;
                }
                let invisible = matches!(node, InlineNode::InlineAnchor(_))
                    || matches!(node, InlineNode::Macro(InlineMacro::IndexTerm(term)) if !term.is_visible());
                after_hard_break =
                    matches!(node, InlineNode::LineBreak(_)) || (after_hard_break && invisible);

                self.visit_inline_node(traversal, node)?;
            }
            Ok(())
        })();
        self.text_boundaries = previous_boundaries;
        result
    }

    fn visit_inline_node(
        &mut self,
        traversal: &mut TraversalContext<'a>,
        node: &InlineNode,
    ) -> Result<(), Self::Error> {
        let saved = self.in_inline_span;
        if acdc_converters_core::visitor::is_formatting_span(node) {
            self.in_inline_span = true;
        }

        let result = self.render_inline_node(traversal, node);

        self.in_inline_span = saved;
        result
    }

    fn visit_text(
        &mut self,
        _traversal: &mut TraversalContext<'a>,
        text: &str,
    ) -> Result<(), Self::Error> {
        let escaped = manify(text, EscapeMode::Normalize);
        write!(self.writer, "{escaped}")?;
        Ok(())
    }
}

impl<'a, W: Write> WritableVisitor<'a> for ManpageVisitor<'a, '_, W> {
    fn writer_mut(&mut self) -> &mut dyn Write {
        match self.link_label.as_mut() {
            Some(label) => &mut label.content,
            None => &mut self.writer,
        }
    }
}
