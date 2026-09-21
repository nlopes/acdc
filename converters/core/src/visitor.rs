//! Visitor pattern for traversing `AsciiDoc` AST.
//!
//! This module provides a Visitor trait that defines methods for visiting each type of AST node.
//! Converters implement this trait to define how to process each node type.
//!
//! # Naming Conventions
//!
//! The codebase follows a two-tier naming pattern for converter functions:
//!
//! - **`visit_*` functions**: High-level orchestration functions that accept a visitor reference
//!   and traverse the AST structure. Nested blocks go through
//!   [`TraversalContext::visit_block`] so attribute events are applied before their callbacks.
//!   They are entry points for processing complete AST nodes.
//!
//! - **`render_*` functions**: Low-level helper functions that generate specific markup output.
//!   These functions typically write directly to a `Write` trait object (not a full visitor)
//!   and handle specific formatting concerns. They are implementation details focused purely
//!   on output generation.
//!
//! This distinction keeps the visitor pattern semantics clear: `visit_*` functions orchestrate
//! traversal and structure, while `render_*` functions focus on output formatting.

use std::io::Write;

use acdc_parser::{
    Admonition, Audio, Block, CalloutList, CalloutListItem, CalloutRef, DelimitedBlock,
    DelimitedBlockType, DescriptionList, DiscreteHeader, Document, DocumentAttribute, Header,
    Image, InlineNode, ListItem, OrderedList, PageBreak, Paragraph, Section, TableOfContents,
    ThematicBreak, UnorderedList, Video,
};

use crate::TraversalContext;

/// The Visitor trait defines methods for visiting each type of AST node.
///
/// Converters implement this trait to define how to process each node type.
///
/// Defaults traverse structural blocks, list items, and `AsciiDoc` table-cell scopes.
/// Leaf callbacks do nothing. Override inline callbacks to inspect or render inline
/// content, and structural callbacks to control output or which children are visited.
///
/// Structural nodes borrow the document for `'doc` so the active attribute view can
/// retain references to accepted assignments. Inline nodes use shorter borrows so
/// converters can also visit temporary text, such as normalized source indentation.
///
/// # Document Structure
///
/// The `visit_document` method calls visitors in this order (per `AsciiDoc` spec):
///
/// 1. `visit_document_start()` - document setup
/// 2. `visit_header()` - if header present
/// 3. Walk preamble blocks (blocks before first section)
/// 4. `visit_preamble_end()` - after preamble blocks walked
/// 5. Walk remaining blocks (sections and top-level blocks)
/// 6. `visit_document_supplements()` - footnotes, bibliography, etc.
/// 7. `visit_document_end()` - document cleanup
///
/// All structural hooks (`visit_document_*`, `visit_preamble_end`) have default
/// no-op implementations. Attribute replay belongs to [`TraversalContext`], not
/// to the visitor: dispatch nested blocks through the context, and use
/// [`TraversalContext::with_table_cell`] when overriding `AsciiDoc` cell traversal.
pub trait Visitor<'doc> {
    /// The error type that can be returned during visiting
    type Error;

    /// Called before any document processing begins.
    ///
    /// Use for: document setup, inspecting structure, opening document wrappers.
    /// Example: HTML converter writes `<!DOCTYPE html><html><head>` here.
    ///
    /// # Errors
    ///
    /// The default implementation never returns an error, but custom implementations
    /// may return errors during document processing.
    fn visit_document_start(
        &mut self,
        _traversal: &mut TraversalContext<'doc>,
        _doc: &'doc Document<'doc>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Called before any body content blocks are walked.
    ///
    /// Called after header (if present), before any blocks (preamble or sections).
    /// Use for: opening content wrappers that contain all body blocks.
    /// Example: HTML converter opens `<div id="content">`.
    ///
    /// # Errors
    ///
    /// The default implementation never returns an error, but custom implementations
    /// may return errors during document processing.
    fn visit_body_content_start(
        &mut self,
        _traversal: &mut TraversalContext<'doc>,
        _doc: &'doc Document<'doc>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Called before preamble blocks are walked (if preamble exists).
    ///
    /// Use for: opening preamble wrappers, special preamble setup.
    /// Example: HTML converter opens preamble divs.
    /// Note: Only called if preamble blocks exist (blocks before first section).
    ///
    /// # Errors
    ///
    /// The default implementation never returns an error, but custom implementations
    /// may return errors during document processing.
    fn visit_preamble_start(
        &mut self,
        _traversal: &mut TraversalContext<'doc>,
        _doc: &'doc Document<'doc>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Called after preamble blocks are walked (if preamble existed).
    ///
    /// Use for: closing preamble wrappers, adding content after preamble.
    /// Example: HTML converter closes preamble divs and adds TOC if configured.
    /// Note: Only called if preamble blocks existed.
    ///
    /// # Errors
    ///
    /// The default implementation never returns an error, but custom implementations
    /// may return errors during document processing.
    fn visit_preamble_end(
        &mut self,
        _traversal: &mut TraversalContext<'doc>,
        _doc: &'doc Document<'doc>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Called after all blocks processed, before document end.
    ///
    /// Use for: document supplements like footnotes, bibliography, appendices.
    /// Example: HTML converter renders footnotes and footer.
    ///
    /// # Errors
    ///
    /// The default implementation never returns an error, but custom implementations
    /// may return errors during document processing.
    fn visit_document_supplements(
        &mut self,
        _traversal: &mut TraversalContext<'doc>,
        _doc: &'doc Document<'doc>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Called at document end, after all processing complete.
    ///
    /// Use for: closing document wrappers, final cleanup.
    /// Example: HTML converter writes `</body></html>`.
    ///
    /// # Errors
    ///
    /// The default implementation never returns an error, but custom implementations
    /// may return errors during document processing.
    fn visit_document_end(
        &mut self,
        _traversal: &mut TraversalContext<'doc>,
        _doc: &'doc Document<'doc>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Visit a complete document.
    ///
    /// This is the main entry point for traversing an `AsciiDoc` document.
    /// It orchestrates the document structure per `AsciiDoc` spec:
    /// - Document start (setup)
    /// - Header (optional)
    /// - Preamble (blocks before first section)
    /// - Sections and top-level blocks
    /// - Document supplements (footnotes, etc.)
    /// - Document end (cleanup)
    ///
    /// The default implementation calls visitor hooks at appropriate structural points,
    /// allowing converters to handle document framing, metadata, and supplements.
    ///
    /// # Errors
    ///
    /// Returns an error if any visitor method fails during document traversal.
    fn visit_document(
        &mut self,
        traversal: &mut TraversalContext<'doc>,
        doc: &'doc Document<'doc>,
    ) -> Result<(), Self::Error> {
        // 1. Document start - setup, document wrappers
        self.visit_document_start(traversal, doc)?;

        // 2. Header - title, authors, metadata (if present)
        if let Some(header) = &doc.header {
            self.visit_header(traversal, header)?;
        }

        // 3. Body content start - before any blocks
        self.visit_body_content_start(traversal, doc)?;

        // 4. Identify preamble (blocks before first section) per AsciiDoc spec
        let first_section_idx = doc
            .blocks
            .iter()
            .position(|b| matches!(b, Block::Section(_)));

        let (preamble, remaining) = match first_section_idx {
            Some(idx) => doc.blocks.split_at(idx),
            None => (doc.blocks.as_slice(), &[][..]),
        };

        // Check if preamble has substantive content (not just comments/attributes)
        let has_substantive_preamble = preamble
            .iter()
            .any(|b| !matches!(b, Block::Comment(_) | Block::DocumentAttribute(_)));

        // Preamble wrapper is only emitted when ALL conditions are met:
        // 1. Document has a header (title)
        // 2. There is at least one section
        // 3. There's substantive content before that section
        let emit_preamble =
            doc.header.is_some() && first_section_idx.is_some() && has_substantive_preamble;

        // 5. Walk preamble blocks
        if emit_preamble {
            self.visit_preamble_start(traversal, doc)?;
        }

        for block in preamble {
            traversal.visit_block(self, block)?;
        }

        if emit_preamble {
            self.visit_preamble_end(traversal, doc)?;
        }

        // 6. Walk remaining blocks (sections and other top-level blocks)
        for block in remaining {
            traversal.visit_block(self, block)?;
        }

        // 7. Document supplements - footnotes, bibliography, etc.
        self.visit_document_supplements(traversal, doc)?;

        // 8. Document end - close wrappers, final cleanup
        self.visit_document_end(traversal, doc)?;

        Ok(())
    }

    /// Called before a block is dispatched, including attribute and comment blocks.
    ///
    /// # Errors
    ///
    /// Returns a backend-specific error if preparation fails.
    fn before_block(
        &mut self,
        _traversal: &mut TraversalContext<'doc>,
        _block: &'doc Block<'doc>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Observe an attribute assignment after the context has applied it.
    ///
    /// # Errors
    ///
    /// The default implementation never returns an error.
    fn visit_document_attribute(
        &mut self,
        _traversal: &mut TraversalContext<'doc>,
        _attribute: &'doc DocumentAttribute<'doc>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Visit a block variant that this version of the visitor does not handle.
    ///
    /// The default implementation records a tracing event and omits the block. Converters that
    /// return user-facing diagnostics should override this method.
    ///
    /// # Errors
    ///
    /// The default implementation never returns an error, but custom implementations may return
    /// an error while reporting or rendering the block.
    fn visit_unhandled_block(
        &mut self,
        _traversal: &mut TraversalContext<'doc>,
        block: &'doc Block<'doc>,
    ) -> Result<(), Self::Error> {
        tracing::warn!(?block, "Unexpected block");
        Ok(())
    }

    /// Visit the document header (title, authors, metadata).
    ///
    /// Called after `visit_document_start()`, before any blocks.
    ///
    /// # Errors
    ///
    /// Returns an error if conversion of the header fails.
    fn visit_header(
        &mut self,
        traversal: &mut TraversalContext<'doc>,
        header: &Header,
    ) -> Result<(), Self::Error> {
        self.visit_inline_nodes(traversal, &header.title)
    }

    /// Visit a section (heading with nested content)
    ///
    /// # Errors
    ///
    /// Returns an error if conversion of this section fails.
    fn visit_section(
        &mut self,
        traversal: &mut TraversalContext<'doc>,
        section: &'doc Section<'doc>,
    ) -> Result<(), Self::Error> {
        self.visit_inline_nodes(traversal, &section.title)?;
        traversal.visit_blocks(self, &section.content)
    }

    /// Visit a paragraph
    ///
    /// # Errors
    ///
    /// Returns an error if conversion of this paragraph fails.
    fn visit_paragraph(
        &mut self,
        traversal: &mut TraversalContext<'doc>,
        para: &Paragraph,
    ) -> Result<(), Self::Error> {
        self.visit_inline_nodes(traversal, &para.title)?;
        self.visit_inline_nodes(traversal, &para.content)
    }

    /// Visit a delimited block (listing, example, sidebar, table, etc.)
    ///
    /// # Errors
    ///
    /// Returns an error if conversion of this delimited block fails.
    fn visit_delimited_block(
        &mut self,
        traversal: &mut TraversalContext<'doc>,
        block: &'doc DelimitedBlock<'doc>,
    ) -> Result<(), Self::Error> {
        match &block.inner {
            DelimitedBlockType::DelimitedExample(blocks)
            | DelimitedBlockType::DelimitedOpen(blocks)
            | DelimitedBlockType::DelimitedQuote(blocks)
            | DelimitedBlockType::DelimitedSidebar(blocks) => traversal.visit_blocks(self, blocks),
            DelimitedBlockType::DelimitedTable(table) => {
                for row in table.header.iter().chain(&table.rows).chain(&table.footer) {
                    for (index, column) in row.columns.iter().enumerate() {
                        let style = column.style.unwrap_or_else(|| {
                            table
                                .columns
                                .get(index)
                                .map_or(acdc_parser::ColumnStyle::Default, |format| format.style)
                        });
                        if style == acdc_parser::ColumnStyle::AsciiDoc {
                            traversal.with_table_cell(column, |context| {
                                context.visit_blocks(self, &column.content)
                            })?;
                        } else {
                            traversal.visit_blocks(self, &column.content)?;
                        }
                    }
                }
                Ok(())
            }
            DelimitedBlockType::DelimitedComment(_)
            | DelimitedBlockType::DelimitedListing(_)
            | DelimitedBlockType::DelimitedLiteral(_)
            | DelimitedBlockType::DelimitedPass(_)
            | DelimitedBlockType::DelimitedVerse(_)
            | DelimitedBlockType::DelimitedStem(_)
            | _ => Ok(()),
        }
    }

    /// Visit an ordered (numbered) list
    ///
    /// # Errors
    ///
    /// Returns an error if conversion of this list fails.
    fn visit_ordered_list(
        &mut self,
        traversal: &mut TraversalContext<'doc>,
        list: &'doc OrderedList<'doc>,
    ) -> Result<(), Self::Error> {
        self.visit_inline_nodes(traversal, &list.title)?;
        for item in &list.items {
            self.visit_list_item(traversal, item)?;
        }
        Ok(())
    }

    /// Visit an unordered (bulleted) list
    ///
    /// # Errors
    ///
    /// Returns an error if conversion of this list fails.
    fn visit_unordered_list(
        &mut self,
        traversal: &mut TraversalContext<'doc>,
        list: &'doc UnorderedList<'doc>,
    ) -> Result<(), Self::Error> {
        self.visit_inline_nodes(traversal, &list.title)?;
        for item in &list.items {
            self.visit_list_item(traversal, item)?;
        }
        Ok(())
    }

    /// Visit a description list (term/definition pairs)
    ///
    /// # Errors
    ///
    /// Returns an error if conversion of this list fails.
    fn visit_description_list(
        &mut self,
        traversal: &mut TraversalContext<'doc>,
        list: &'doc DescriptionList<'doc>,
    ) -> Result<(), Self::Error> {
        for item in &list.items {
            self.visit_inline_nodes(traversal, &item.term)?;
            self.visit_inline_nodes(traversal, &item.principal_text)?;
            traversal.visit_blocks(self, &item.description)?;
        }
        Ok(())
    }

    /// Visit a callout list
    ///
    /// # Errors
    ///
    /// Returns an error if conversion of this list fails.
    fn visit_callout_list(
        &mut self,
        traversal: &mut TraversalContext<'doc>,
        list: &'doc CalloutList<'doc>,
    ) -> Result<(), Self::Error> {
        for item in &list.items {
            self.visit_callout_list_item(traversal, item)?;
        }
        Ok(())
    }

    /// Visit a list item
    ///
    /// # Errors
    ///
    /// Returns an error if conversion of this list item fails.
    fn visit_list_item(
        &mut self,
        traversal: &mut TraversalContext<'doc>,
        item: &'doc ListItem<'doc>,
    ) -> Result<(), Self::Error> {
        self.visit_inline_nodes(traversal, &item.principal)?;
        traversal.visit_blocks(self, &item.blocks)
    }

    /// Visit an admonition (NOTE, TIP, WARNING, etc.)
    ///
    /// # Errors
    ///
    /// Returns an error if conversion of this admonition fails.
    fn visit_admonition(
        &mut self,
        traversal: &mut TraversalContext<'doc>,
        admon: &'doc Admonition<'doc>,
    ) -> Result<(), Self::Error> {
        traversal.visit_blocks(self, &admon.blocks)
    }

    /// Visit an image
    ///
    /// # Errors
    ///
    /// Returns an error if conversion of this image fails.
    fn visit_image(
        &mut self,
        _traversal: &mut TraversalContext<'doc>,
        _img: &Image,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Visit a video
    ///
    /// # Errors
    ///
    /// Returns an error if conversion of this video fails.
    fn visit_video(
        &mut self,
        _traversal: &mut TraversalContext<'doc>,
        _video: &Video,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Visit an audio element
    ///
    /// # Errors
    ///
    /// Returns an error if conversion of this audio element fails.
    fn visit_audio(
        &mut self,
        _traversal: &mut TraversalContext<'doc>,
        _audio: &Audio,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Visit a thematic break (horizontal rule)
    ///
    /// # Errors
    ///
    /// Returns an error if conversion of this thematic break fails.
    fn visit_thematic_break(
        &mut self,
        _traversal: &mut TraversalContext<'doc>,
        _br: &ThematicBreak,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Visit a page break
    ///
    /// # Errors
    ///
    /// Returns an error if conversion of this page break fails.
    fn visit_page_break(
        &mut self,
        _traversal: &mut TraversalContext<'doc>,
        _br: &PageBreak,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Visit a table of contents
    ///
    /// # Errors
    ///
    /// Returns an error if conversion of this table of contents fails.
    fn visit_table_of_contents(
        &mut self,
        _traversal: &mut TraversalContext<'doc>,
        _toc: &TableOfContents,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Visit a discrete header (not part of document structure)
    ///
    /// # Errors
    ///
    /// Returns an error if conversion of this discrete header fails.
    fn visit_discrete_header(
        &mut self,
        traversal: &mut TraversalContext<'doc>,
        header: &DiscreteHeader,
    ) -> Result<(), Self::Error> {
        self.visit_inline_nodes(traversal, &header.title)
    }

    /// Visit a sequence of inline nodes
    ///
    /// # Errors
    ///
    /// Returns an error if conversion of any inline node fails.
    fn visit_inline_nodes(
        &mut self,
        traversal: &mut TraversalContext<'doc>,
        nodes: &[InlineNode],
    ) -> Result<(), Self::Error> {
        for node in nodes {
            self.visit_inline_node(traversal, node)?;
        }
        Ok(())
    }

    /// Visit a single inline node
    ///
    /// # Errors
    ///
    /// Returns an error if conversion of this inline node fails.
    fn visit_inline_node(
        &mut self,
        _traversal: &mut TraversalContext<'doc>,
        _node: &InlineNode,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Visit plain text
    ///
    /// # Errors
    ///
    /// Returns an error if writing the text fails.
    fn visit_text(
        &mut self,
        _traversal: &mut TraversalContext<'doc>,
        _text: &str,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Visit a callout reference inline node.
    ///
    /// Default implementation does nothing. Override to render callout markers.
    ///
    /// # Errors
    ///
    /// Returns an error if writing the callout reference fails.
    fn visit_callout_ref(
        &mut self,
        _traversal: &mut TraversalContext<'doc>,
        _callout: &CalloutRef,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Visit a callout list item.
    ///
    /// The default visits principal text followed by attached blocks.
    ///
    /// # Errors
    ///
    /// Returns an error if writing the callout list item fails.
    fn visit_callout_list_item(
        &mut self,
        traversal: &mut TraversalContext<'doc>,
        item: &'doc CalloutListItem<'doc>,
    ) -> Result<(), Self::Error> {
        self.visit_inline_nodes(traversal, &item.principal)?;
        traversal.visit_blocks(self, &item.blocks)
    }
}

/// Returns true if the inline node is a formatting span whose children
/// should suppress em-dash boundary replacement at string start/end.
#[must_use]
pub fn is_formatting_span(node: &InlineNode) -> bool {
    matches!(
        node,
        InlineNode::BoldText(_)
            | InlineNode::ItalicText(_)
            | InlineNode::MonospaceText(_)
            | InlineNode::HighlightText(_)
            | InlineNode::SuperscriptText(_)
            | InlineNode::SubscriptText(_)
            | InlineNode::CurvedQuotationText(_)
            | InlineNode::CurvedApostropheText(_)
    )
}

/// A writable visitor that outputs to a writer.
///
/// This is a common pattern for converters that generate text output (HTML, terminal, etc.)
pub trait WritableVisitor<'doc>: Visitor<'doc> {
    /// Get a mutable reference to the writer
    fn writer_mut(&mut self) -> &mut dyn Write;

    /// Render a title with wrapper markup (prefix and suffix).
    ///
    /// This helper handles the common pattern of:
    /// 1. Write opening markup
    /// 2. Drop the writer borrow
    /// 3. Visit inline nodes (which needs mutable visitor)
    /// 4. Get writer again
    /// 5. Write closing markup
    ///
    /// Does nothing if title is empty.
    ///
    /// # Errors
    ///
    /// Returns an error if writing or visiting fails.
    fn render_title_with_wrapper(
        &mut self,
        traversal: &mut TraversalContext<'doc>,
        title: &[InlineNode],
        prefix: &str,
        suffix: &str,
    ) -> Result<(), Self::Error>
    where
        Self::Error: From<std::io::Error>,
    {
        if !title.is_empty() {
            let w = self.writer_mut();
            write!(w, "{prefix}")?;
            let _ = w;
            self.visit_inline_nodes(traversal, title)?;
            let w = self.writer_mut();
            write!(w, "{suffix}")?;
        }
        Ok(())
    }
}
